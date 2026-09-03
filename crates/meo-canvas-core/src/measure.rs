//! The bridge between taffy's questions and Skia's answers.
//!
//! taffy calls a measure function for every leaf whose size it cannot derive
//! from style alone, handing over the space available and expecting an extent
//! back. For text that answer comes from shaping the run at the offered width,
//! which is Skia's paragraph layout; for images it is the decoded bitmap's
//! intrinsic size scaled to fit.
//!
//! # The baseline goes two ways
//!
//! [`MeasuredLeaf`] carries a `first_baseline`, and it is read twice.
//! [`crate::layout`] puts it in the `LayoutOutput` it hands taffy
//! (`taffy-0.14.0/src/tree/taffy_tree.rs:904`), which is what
//! `align-items: baseline` reads, and keeps it in
//! [`crate::layout::LayoutResult`] for the paint pass, which is what lets
//! glyphs sit correctly inside a box placed by some other rule.
//!
//! The first of those is worth stating exactly, because the failure it avoids
//! is visible rather than subtle: taffy reads a *missing* baseline as the
//! node's own height (`taffy-0.14.0/src/compute/flexbox.rs:1921`), so a row of
//! measured text with no baseline reported lines up on the bottom edges of the
//! runs, and the largest text then sits highest — the trend inverted, not
//! merely offset.
//!
//! The arrangement that tells the two apart is `align-items: baseline` over
//! mixed font **sizes**, and nothing else does:
//! `fixtures/baseline-alignment` is that scene, measured against Chrome. A row
//! of boxes with no text agrees with itself either way, because a box's
//! baseline *is* its bottom margin edge.

use std::collections::HashMap;

// The paragraph path is test-only now: text is measured and drawn through
// `crate::lines`, and what remains of Skia's own text stack is the
// comparison report those tests run. See `build_paragraph`.
#[cfg(test)]
use meo_canvas_scene::style::{
    effect::TextShadow,
    text::{
        FontStyle, LineHeight, Spacing, TextAlign, TextDecoration, TextSegment,
    },
};
use meo_canvas_scene::{
    Size,
    node::{NodeId, NodeKind},
    style::text::ParagraphStyle,
};
#[cfg(test)]
use meo_skia_canvas::{
    Paragraph, RgbaLinear, TextAlign as SkiaTextAlign,
    TextDecoration as SkiaTextDecoration, TextEngine,
    TextShadow as SkiaTextShadow, TextSlant, TextStyle as SkiaTextStyle,
};

#[cfg(test)]
use crate::resolve::ResolvedText;
use crate::{
    lines::{self, Block, Metrics, TextMeasurer},
    resolve::{Fonts, Resolved},
};

/// The name a text node falls back to when it names no family of its own.
///
/// Empty rather than a face name: Skia's font collection reads an empty family
/// list as "any registered face", which is the behaviour a caller wants from an
/// unstyled run. Naming a real family here would make a scene that never asked
/// for one fail on a machine that does not have it.
pub const DEFAULT_FONT_FAMILY: &str = "";

/// The em size a text node is measured at when nothing sets one.
///
/// CSS's initial `font-size` is `medium`, which every browser resolves to 16
/// pixels. Matching it means a scene ported from a web design measures the same
/// before either side sets a size explicitly.
pub const DEFAULT_FONT_SIZE: f32 = 16.0;

/// What measuring one leaf produced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredLeaf {
    /// The extent taffy is told about.
    pub size: Size,
    /// Distance from the top edge to the first baseline, when the leaf has
    /// one.
    ///
    /// taffy never sees this. It exists so the paint pass does not reshape the
    /// run a second time to find out where the glyphs sit.
    pub first_baseline: Option<f32>,
}

impl MeasuredLeaf {
    /// No extent and no baseline.
    ///
    /// What an implementor answers for a node it was never prepared for. Not a
    /// sentinel a caller tests against: an empty run measures to this too, and
    /// the two cases are indistinguishable on purpose, because neither draws
    /// anything.
    pub const EMPTY: Self = Self {
        size: Size::ZERO,
        first_baseline: None,
    };

    /// A leaf of the given extent with no baseline, which is every leaf that
    /// is not text.
    #[must_use]
    pub const fn sized(size: Size) -> Self {
        Self {
            size,
            first_baseline: None,
        }
    }
}

/// The space taffy offers a leaf on one axis.
///
/// Mirrors `taffy::AvailableSpace` in this crate's own vocabulary so an
/// implementor of [`Measure`] need not name taffy. Note that this is not Yoga's
/// `YGMeasureMode`: Yoga distinguishes `Exactly`/`AtMost`/`Undefined`, taffy
/// distinguishes definite from the two intrinsic sizes, and `MinContent` has no
/// Yoga counterpart at all. A measure function ported from Yoga is rewritten
/// here, not translated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Available {
    /// A known extent.
    Definite(f32),
    /// The smallest extent the content fits in.
    MinContent,
    /// The extent the content takes when nothing constrains it.
    MaxContent,
}

/// Whether a layout applies the paragraph's `max_lines` and ellipsis.
///
/// A `bool` at the call site reads as `laid(node, width, false)`, which says
/// nothing about which half is which. The two questions this distinguishes --
/// what the text *is* and what is *drawn* of it -- are far enough apart to be
/// worth naming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Truncate {
    /// Apply them. The used value: what the paint pass draws.
    Yes,
    /// Ignore them. The intrinsic value: what the content can do.
    No,
}

/// Answers the layout pass's questions about how large a leaf is.
///
/// The seam between [`crate::layout`] and this module. Layout owns the taffy
/// tree and never names a font; this module owns the fonts and never names
/// taffy. A test of layout supplies a measurer that returns fixed sizes and so
/// needs no font on the machine running it.
///
/// # Why it cannot fail
///
/// There is no `Result` here, and that is a constraint taffy imposes rather
/// than a simplification: its measure closure returns a size, and a solve in
/// progress has nowhere to put an error. So every failure that can be seen in
/// advance is raised in advance -- [`SceneMeasurer::prepare`] resolves each
/// text node's family and builds its paragraph before layout starts, and
/// reports [`crate::Error::UnknownFont`] there. By the time `measure` is
/// called, every leaf it will be asked about already has an answer.
///
/// # Contract
///
/// `measure` is called many times for one leaf during a single solve, at
/// different widths, and must be a function of its arguments: the same `node`
/// with the same `known` and `available` returns the same
/// [`MeasuredLeaf`]. Caching between calls is expected -- that is what `&mut
/// self` is for -- but the cache must not change the answer.
///
/// A `node` the measurer has never been prepared for is a caller error rather
/// than a runtime one; an implementor returns [`MeasuredLeaf::EMPTY`] rather
/// than panicking, so a mismatch between the tree and the measurer costs a
/// misdrawn node instead of the process.
pub trait Measure {
    /// The extent of `node`, given what is already fixed and what is offered.
    ///
    /// `known` carries an axis whose size layout has already settled; an axis
    /// that is `Some` is not for the measurer to choose, and the returned size
    /// on that axis is expected to match. `available` describes the space on
    /// each axis for the axes that are still open.
    fn measure(
        &mut self,
        node: NodeId,
        known: (Option<f32>, Option<f32>),
        available: (Available, Available),
    ) -> MeasuredLeaf;
}

/// Everything a solve needs to answer size questions about one scene's leaves.
///
/// Named for the scene rather than for text because it answers for images too:
/// they are the other leaf kind whose extent is not in the style, and an image
/// measurer separate from this one would be a second table keyed the same way.
///
/// Built by [`SceneMeasurer::prepare`], which is where every foreseeable
/// failure is raised. After that the [`Measure`] implementation cannot fail,
/// because taffy's closure has nowhere to put an error.
///
/// # Where the shaping is cached
///
/// Wrapping asks the same question relentlessly -- taffy calls a leaf several
/// times per pass while it searches for a width that fits -- and shaping is
/// the expensive half of answering. [`TextMeasurer`] holds the answers for the
/// life of this measurer, which is one render. See [`crate::lines`].
pub struct SceneMeasurer<'resolved> {
    resolved: &'resolved Resolved<'resolved>,
    /// The shaping cache every line box is built through.
    text: TextMeasurer,
    /// Answers already given, keyed by the question.
    ///
    /// A solve asks about one leaf at several widths and repeats questions
    /// between passes; a laid-out paragraph is not free to interrogate, and
    /// the key is 24 bytes.
    answers: HashMap<Question, MeasuredLeaf>,
}

/// One measurement request, in a form that can be a map key.
///
/// `f32` is not `Eq` or `Hash`, so the widths are keyed by their bit patterns.
/// That is exact rather than approximate: two calls that pass the same `f32`
/// have the same bits, and two that pass different ones must not share an
/// answer. The only value that would misbehave is `NaN`, which never reaches
/// here -- taffy offers a definite space or an intrinsic one, never a `NaN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Question {
    node: NodeId,
    known: (Option<u32>, Option<u32>),
    available: (AvailableKey, AvailableKey),
}

/// [`Available`] reduced to something hashable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AvailableKey {
    Definite(u32),
    MinContent,
    MaxContent,
}

impl From<Available> for AvailableKey {
    fn from(value: Available) -> Self {
        match value {
            Available::Definite(extent) => Self::Definite(extent.to_bits()),
            Available::MinContent => Self::MinContent,
            Available::MaxContent => Self::MaxContent,
        }
    }
}

impl core::fmt::Debug for SceneMeasurer<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `Paragraph` does not implement `Debug`, and a dump of shaped glyph
        // runs would not help a reader anyway. The counts are what says whether
        // preparation found what it expected.
        f.debug_struct("SceneMeasurer")
            .field("shaped", &self.text.cached())
            .field("answers", &self.answers.len())
            .finish_non_exhaustive()
    }
}

impl<'resolved> SceneMeasurer<'resolved> {
    /// Shapes every text node in the scene and returns a measurer for it.
    ///
    /// This is the pass that can fail. Everything after it -- every call taffy
    /// makes during a solve -- is answered from what was built here.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::UnknownFont`] when a text node names a family
    /// neither registered nor installed. [`Resolved::new`] checks the same
    /// thing, so a measurer built from a resolved scene reaches this only if
    /// the font registry changed between the two.
    pub fn prepare(
        resolved: &'resolved Resolved<'resolved>,
        fonts: &Fonts,
    ) -> Result<Self, crate::Error> {
        // Every family a text node names, checked here so that the failure is
        // raised in the one pass that can raise one. Nothing is shaped yet:
        // shaping happens against a width, and no width is known until taffy
        // asks.
        for (index, node) in resolved.scene.nodes.iter().enumerate() {
            // The cast is exact: the arena is bounded by `MAX_NODES`, a `u32`.
            let id = NodeId::new(index as u32);
            if !matches!(node.kind, NodeKind::Text { .. }) {
                continue;
            }
            let Some(style) = resolved.text(id) else {
                continue;
            };
            if !style.family.is_empty() && !fonts.has(&style.family) {
                return Err(crate::Error::UnknownFont(style.family.clone()));
            }
        }

        Ok(Self {
            resolved,
            text: TextMeasurer::new(),
            answers: HashMap::new(),
        })
    }

    /// Lays a text node's content out at `width`, for the paint pass.
    ///
    /// Crate-internal, and re-done rather than remembered: the width layout
    /// settles on is not always the width it last asked about, and a block
    /// carried over from the wrong question is the defect that made the old
    /// painter lay every paragraph out a second time.
    pub(crate) fn block(&mut self, node: NodeId, width: f32) -> Option<Block> {
        self.laid(node, width, Truncate::Yes)
    }

    /// Lays a text node out at `width`, with or without its truncation.
    ///
    /// # Why the caller chooses
    ///
    /// Because `max_lines` and the ellipsis are **used-value** behaviour: they
    /// say what is drawn once the width is already settled, and CSS Sizing 3
    /// §5.1 derives an intrinsic size from the content alone. Applying them
    /// while answering `MinContent` makes the answer circular -- the marker is
    /// laid out at a width of zero, the run comes back as `…`, and the
    /// paragraph reports the marker's width as the narrowest it can be.
    ///
    /// That is not a flexbox defect downstream of here. Flexbox 1 §4.5 floors
    /// a flex item at exactly what this function reports, so the item shrank
    /// precisely to the minimum it was handed. Measured in Chrome
    /// (`crates/meo-canvas/tests/assets/chrome/min-content.tsv`):
    /// `-webkit-line-clamp: 1` leaves `Flower of Paradise` at its plain
    /// min-content of 49.91, and `text-overflow: ellipsis` matches its own
    /// `nowrap` control at 106.50 rather than dropping below it. **Neither
    /// truncation lowers an intrinsic width.**
    fn laid(
        &mut self,
        node: NodeId,
        width: f32,
        truncate: Truncate,
    ) -> Option<Block> {
        let style = self.resolved.text(node)?;
        let scene_node = self.resolved.scene.get(node)?;
        let NodeKind::Text {
            segments,
            paragraph,
        } = &scene_node.kind
        else {
            return None;
        };
        // A truncation-free view of the same paragraph. Cloned rather than
        // threaded through `lines::layout` as a flag because the truncation is
        // *the whole of* what a paragraph style carries: an empty one is the
        // untruncated question, exactly.
        let untruncated = ParagraphStyle::default();
        let paragraph = match truncate {
            Truncate::Yes => paragraph,
            Truncate::No => &untruncated,
        };
        let laid = lines::layout(
            &mut self.text,
            style,
            segments,
            width,
            paragraph,
            Metrics::of(style),
        );

        // **A box sized from this text's own measurement must not break it.**
        //
        // taffy rounds a rect to whole pixels, deliberately -- `rounding_drift`
        // has why, and turning it off makes adjacent boxes antialias against
        // each other at every shared edge. But the paint pass then re-flows the
        // paragraph at that rounded width, and a natural width of `43.43`
        // arrives as `43`: the break falls at the space and the second line
        // spills out of a box that is one line tall.
        //
        // Measured, Poppins 12 at weight 500: `Max HP` is `43.43` and wraps,
        // `Elemental Mastery` is exactly `112.0` and does not, `ATK` is `22.73`
        // and rounds **up** to `23` and does not. **Rounding down wraps and
        // rounding up fits**, and because taffy rounds cumulative coordinates
        // the same string wraps or not depending on what sits above it.
        //
        // So: when one more pixel would have been enough, the box was rounded
        // down from this paragraph's own width and the break is an artefact.
        // When it would not, the box is genuinely narrower than the text and
        // the break is real -- `Elemental Mastery` in a 43-pixel box still
        // wraps, because 112 is nowhere near 44.
        //
        // The cost is one extra wrap, only for paragraphs that broke, and the
        // shaping underneath is cached. What it trades away: a box deliberately
        // a fraction narrower than its text now overflows by that fraction
        // instead of breaking, which is invisible where a spurious line break
        // is not.
        // **`wrapped_lines`, not `lines.len()`, and that distinction is the
        // whole of the defect this once had.**
        //
        // The test is "did the wrap break this paragraph". With `max_lines`,
        // `lines.len()` cannot answer it: a paragraph that broke into two comes
        // back as one line carrying a marker, identical by this test to one
        // that never broke -- so the rescue below was skipped for
        // exactly the case it was needed most, and a label that fitted
        // rendered as its own ellipsis.
        //
        // Measured in the report that found it: `HP` in Oswald 12 is `12.49`
        // wide, its box rounds to `12` at any offset whose fraction is at or
        // above `.5`, the wrap breaks, and `maxLines: 1` truncates the break to
        // `…` in a box with room for the whole word.
        // Both ways a rounded-down box shows: the wrap breaking a paragraph
        // that fitted, and a single line judged too wide to keep. A word with
        // no space in it cannot break, so `HP` at `12.49` in a box of `12`
        // never raises `wrapped_lines` and reaches the marker by the second
        // route -- which is the case the report was actually about.
        if (laid.wrapped_lines > 1 || laid.truncated) && width.is_finite() {
            let loose = lines::layout(
                &mut self.text,
                style,
                segments,
                f32::INFINITY,
                paragraph,
                Metrics::of(style),
            );
            if loose.width - width < 1.0 {
                return Some(loose);
            }
        }
        Some(laid)
    }

    /// The width of one inter-word space in a node's own face.
    ///
    /// The painter needs it for the same reason the wrap does: a space is a
    /// run of no width, and the gap it stands for is arithmetic. Answered
    /// through the same cache, so asking here costs nothing the wrap has not
    /// already paid.
    pub(crate) fn space(
        &mut self,
        base: &lines::RunStyle,
        letter_spacing: f32,
    ) -> f32 {
        self.text.space_width(base, letter_spacing)
    }

    /// The scene this measurer answers for.
    #[must_use]
    pub const fn resolved(&self) -> &'resolved Resolved<'resolved> {
        self.resolved
    }

    /// Measures a text node by wrapping it at the width on offer.
    ///
    /// # Which width
    ///
    /// The space offered, never the content's own measure of itself. Laying
    /// out at exactly what the content occupies loses the last word to a float
    /// comparison, and that boundary is not a corner case: flexbox settles an
    /// auto-sized item at precisely its max-content width and asks again with
    /// that as a known dimension, so every text node that fits is asked this
    /// question.
    ///
    /// So an open axis wraps at infinity -- the one width with no boundary to
    /// land on -- and narrows only when the budget is genuinely less than what
    /// the content occupies. `MinContent` is the exception that is not one:
    /// wrapping at every space *is* what min-content means.
    fn measure_text(
        &mut self,
        node: NodeId,
        known: (Option<f32>, Option<f32>),
        available: (Available, Available),
    ) -> MeasuredLeaf {
        let budget = match (known.0, available.0) {
            (Some(fixed), _) => fixed,
            (None, Available::Definite(budget)) => budget,
            (None, Available::MinContent) => 0.0,
            (None, Available::MaxContent) => f32::INFINITY,
        };

        // **An intrinsic question is answered from the content alone.**
        // `MinContent` and `MaxContent` ask what this text *can* do, and a
        // clamp describes what is drawn once that is already decided -- so
        // answering them through the truncation reports the marker's width as
        // the narrowest a paragraph can be, and flexbox then floors the item
        // there. See `laid`, and the Chrome table it names.
        let truncate = if known.0.is_none()
            && matches!(
                available.0,
                Available::MinContent | Available::MaxContent
            ) {
            Truncate::No
        } else {
            Truncate::Yes
        };

        let Some(loose) = self.laid(node, f32::INFINITY, truncate) else {
            return MeasuredLeaf::EMPTY;
        };
        let block = if budget < loose.width {
            self.laid(node, budget, truncate).unwrap_or(loose)
        } else {
            loose
        };

        MeasuredLeaf {
            size: Size::new(
                known.0.unwrap_or(block.width),
                known.1.unwrap_or(block.height),
            ),
            // The first line's own baseline, which is what places its glyphs.
            // taffy discards it -- `compute/leaf.rs` returns `Point::NONE` for
            // every measured leaf -- but this crate keeps it, and the painter
            // reads it from `LayoutResult`.
            first_baseline: block
                .lines
                .first()
                .map(lines::Line::baseline_from_top),
        }
    }
}

impl Measure for SceneMeasurer<'_> {
    fn measure(
        &mut self,
        node: NodeId,
        known: (Option<f32>, Option<f32>),
        available: (Available, Available),
    ) -> MeasuredLeaf {
        let question = Question {
            node,
            known: (known.0.map(f32::to_bits), known.1.map(f32::to_bits)),
            available: (available.0.into(), available.1.into()),
        };
        if let Some(answer) = self.answers.get(&question) {
            return *answer;
        }

        let answer = if self.resolved.text(node).is_some() {
            self.measure_text(node, known, available)
        } else if let Some(image) = self.resolved.image(node) {
            MeasuredLeaf::sized(fit_intrinsic(
                image.intrinsic_size(),
                known,
                available,
            ))
        } else {
            // A node this measurer was never prepared for. The trait says
            // `EMPTY` rather than a panic, because a mismatch between the tree
            // and the measurer should cost a misdrawn node, not the process.
            MeasuredLeaf::EMPTY
        };

        self.answers.insert(question, answer);
        answer
    }
}

/// Fits a leaf with an intrinsic size into what layout has offered.
///
/// The rule CSS gives a replaced element: a fixed axis wins, an open axis takes
/// the intrinsic extent scaled to preserve the ratio when the other axis is
/// fixed, and neither being fixed leaves the intrinsic size, clamped to a
/// definite budget.
fn fit_intrinsic(
    intrinsic: Size,
    known: (Option<f32>, Option<f32>),
    available: (Available, Available),
) -> Size {
    let ratio = if intrinsic.height > 0.0 {
        Some(intrinsic.width / intrinsic.height)
    } else {
        None
    };

    match (known.0, known.1, ratio) {
        (Some(width), Some(height), _) => Size::new(width, height),
        (Some(width), None, Some(ratio)) if ratio > 0.0 => {
            Size::new(width, width / ratio)
        }
        (None, Some(height), Some(ratio)) => Size::new(height * ratio, height),
        (Some(width), None, _) => Size::new(width, intrinsic.height),
        (None, Some(height), None) => Size::new(intrinsic.width, height),
        (None, None, _) => Size::new(
            clamp_to(intrinsic.width, available.0),
            clamp_to(intrinsic.height, available.1),
        ),
    }
}

/// An intrinsic extent narrowed to a definite budget.
///
/// The intrinsic sizes are the same on both intrinsic questions: an image does
/// not wrap, so its minimum and maximum content extents are one number.
const fn clamp_to(intrinsic: f32, available: Available) -> f32 {
    match available {
        Available::Definite(budget) => intrinsic.min(budget),
        Available::MinContent | Available::MaxContent => intrinsic,
    }
}

/// Shapes one text node into a paragraph, ready to be laid out at any width.
///
/// **Nothing in the renderer uses this any more.** Text is measured and drawn
/// through [`crate::lines`], which computes its own line boxes the way v1 and
/// a browser do. What keeps this alive is the comparison report in that
/// module's tests: two independent statements of one layout, with their
/// disagreements enumerated rather than accepted. It is the evidence the port
/// was equivalent where it meant to be and deliberate where it was not, and it
/// goes when that stops being worth re-running.
#[cfg(test)]
pub(crate) fn build_paragraph(
    engine: &TextEngine,
    style: &ResolvedText,
    segments: &[TextSegment],
    paragraph: &ParagraphStyle,
    shadows: &[TextShadow],
) -> Paragraph {
    let base = skia_style(style, paragraph, shadows);
    let mut builder = engine.paragraph_builder(&base);
    for segment in segments {
        let run = style.inherit(&segment.style);
        builder.push_style(&skia_style(&run, paragraph, shadows));
        builder.add_text(&segment.text);
        builder.pop();
    }
    // Built at an unconstrained width so the intrinsic extents are available
    // before layout has offered anything. Every later call lays it out again.
    builder.build(f32::INFINITY)
}

/// Translates a resolved style into the backend's own.
///
/// Test-only, with [`build_paragraph`], and for the same reason.
#[cfg(test)]
fn skia_style(
    style: &ResolvedText,
    paragraph: &ParagraphStyle,
    shadows: &[TextShadow],
) -> SkiaTextStyle {
    SkiaTextStyle {
        font_families: if style.family.is_empty() {
            Vec::new()
        } else {
            vec![style.family.clone()]
        },
        font_size: style.size,
        font_weight: i32::from(style.weight.get()),
        slant: match style.style {
            FontStyle::Normal => TextSlant::Upright,
            FontStyle::Italic => TextSlant::Italic,
        },
        color: RgbaLinear::from_srgb8(
            style.color.r,
            style.color.g,
            style.color.b,
            f32::from(style.color.a) / 255.0,
        ),
        align: match style.align {
            TextAlign::Start => SkiaTextAlign::Start,
            TextAlign::End => SkiaTextAlign::End,
            TextAlign::Left => SkiaTextAlign::Left,
            TextAlign::Center => SkiaTextAlign::Center,
            TextAlign::Right => SkiaTextAlign::Right,
            TextAlign::Justify => SkiaTextAlign::Justify,
        },
        // The decoration was resolved and then dropped: `ResolvedText` has
        // carried it since the resolve pass and nothing passed it on, so
        // `underline` and `line-through` painted a paragraph identical to the
        // pixel with `none`. The property crossed both wire formats correctly
        // and was lost here, which is why a byte comparison could not see it.
        decoration: match style.decoration {
            TextDecoration::None => SkiaTextDecoration::default(),
            TextDecoration::Underline => SkiaTextDecoration::underline(),
            TextDecoration::Overline => SkiaTextDecoration::overline(),
            TextDecoration::LineThrough => SkiaTextDecoration::line_through(),
        },
        // Node-level rather than inherited, so they come from `Effects` and
        // not from `ResolvedText` — which is why nothing was reading them: the
        // scene carried them and the paragraph was built without ever being
        // shown that field.
        shadows: shadows
            .iter()
            .map(|shadow| SkiaTextShadow {
                color: RgbaLinear::from_srgb8(
                    shadow.color.r,
                    shadow.color.g,
                    shadow.color.b,
                    f32::from(shadow.color.a) / 255.0,
                ),
                offset_x: shadow.offset_x,
                offset_y: shadow.offset_y,
                // CSS gives a blur *radius* and Skia takes a Gaussian sigma.
                // Half is the conversion every CSS engine uses.
                blur_sigma: shadow.blur / 2.0,
            })
            .collect(),
        // **The sentinel is correct here and wrong upstream of it.** Skia
        // takes a multiplier and spells "use the face's metrics" as `1.0`, so
        // `None` converts to exactly that. The bug was carrying Skia's
        // spelling further in than Skia.
        // **Skia takes a multiplier, so a length divides here and nowhere
        // else.** That is the whole cost of storing what the author wrote: a
        // caller's `24px` used to be divided by the font size at every call
        // site that wanted to write one, and now it is divided once, at the
        // boundary that actually needs a ratio.
        //
        // `None` is CSS's `normal` and Skia spells that `1.0` -- the sentinel
        // is correct here and was wrong upstream of it.
        line_height_multiplier: match style.line_height {
            Some(LineHeight::Number(multiple)) => multiple,
            Some(LineHeight::Length(pixels)) if style.size > 0.0 => {
                pixels / style.size
            }
            Some(LineHeight::Percent(share)) => share,
            Some(LineHeight::Length(_)) | None => {
                ResolvedText::NORMAL_LINE_HEIGHT
            }
        },
        letter_spacing: spacing_pixels(style.letter_spacing, style.size),
        word_spacing: spacing_pixels(style.word_spacing, style.size),
        max_lines: paragraph.max_lines.map(|lines| lines as usize),
        ellipsis: paragraph.ellipsis.clone(),
        ..SkiaTextStyle::default()
    }
}

/// A [`Spacing`] as the absolute pixel count the backend takes.
///
/// Test-only: the live path resolves spacing in [`crate::lines::Metrics`].
#[cfg(test)]
fn spacing_pixels(spacing: Spacing, font_size: f32) -> f32 {
    match spacing {
        Spacing::Normal => 0.0,
        Spacing::Points(points) => points,
        Spacing::Em(em) => em * font_size,
    }
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::{
        Length, Scene, Size,
        node::{ImageSource, Node, NodeId, NodeKind},
        style::{paint::ObjectFit, text::Spacing},
    };

    use super::{
        Available, AvailableKey, DEFAULT_FONT_FAMILY, DEFAULT_FONT_SIZE,
        Measure, MeasuredLeaf, SceneMeasurer, clamp_to, fit_intrinsic,
        spacing_pixels,
    };
    use crate::resolve::{
        Fonts, Resolved, ResolvedText,
        tests::{RED_PNG, TEST_FAMILY, test_fonts},
    };

    /// Wide enough that the test string fits on one line at the test font's
    /// default size, so a `Definite` measurement is not silently a wrap test.
    const ROOMY: f32 = 4_000.0;

    fn text_scene(content: &str) -> (Scene, NodeId) {
        let mut scene = Scene::new(Size::new(200.0, 100.0));
        let leaf = scene
            .push(NodeId::ROOT, Node::text(content))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(leaf) {
            node.text.font_family = Some(TEST_FAMILY.to_owned());
            node.text.font_size = Some(20.0);
        }
        (scene, leaf)
    }

    #[test]
    fn a_text_leaf_measures_wider_unwrapped_than_wrapped() {
        let (scene, leaf) = text_scene("the quick brown fox jumps over it");
        let fonts = test_fonts();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let max = measurer.measure(
            leaf,
            (None, None),
            (Available::MaxContent, Available::MaxContent),
        );
        let min = measurer.measure(
            leaf,
            (None, None),
            (Available::MinContent, Available::MinContent),
        );

        assert!(max.size.width > 0.0, "unwrapped text has width");
        assert!(
            min.size.width < max.size.width,
            "the longest word is narrower than the whole line: {} vs {}",
            min.size.width,
            max.size.width
        );
        // Narrower means more lines, so taller.
        assert!(min.size.height >= max.size.height);
        assert!(max.first_baseline.is_some_and(|baseline| baseline > 0.0));
    }

    #[test]
    fn a_definite_budget_never_widens_the_answer_past_the_content() {
        let (scene, leaf) = text_scene("short");
        let fonts = test_fonts();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let roomy = measurer.measure(
            leaf,
            (None, None),
            (Available::Definite(ROOMY), Available::MaxContent),
        );
        // The answer is the content's own width, not the budget it was offered.
        assert!(roomy.size.width < ROOMY);
    }

    #[test]
    fn a_known_axis_is_returned_unchanged() {
        let (scene, leaf) = text_scene("some text here");
        let fonts = test_fonts();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let fixed = measurer.measure(
            leaf,
            (Some(120.0), Some(48.0)),
            (Available::MaxContent, Available::MaxContent),
        );
        assert_eq!(fixed.size, Size::new(120.0, 48.0));
    }

    #[test]
    fn the_same_question_gets_the_same_answer() {
        let (scene, leaf) = text_scene("cached");
        let fonts = test_fonts();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let question = (
            (None, None),
            (Available::Definite(150.0), Available::MaxContent),
        );
        let first = measurer.measure(leaf, question.0, question.1);
        let second = measurer.measure(leaf, question.0, question.1);
        assert_eq!(first, second);

        // A different question is a different answer, so the key discriminates.
        let narrower = measurer.measure(
            leaf,
            (None, None),
            (Available::Definite(10.0), Available::MaxContent),
        );
        assert!(narrower.size.height >= first.size.height);
        assert!(!format!("{measurer:?}").is_empty());
        assert_eq!(measurer.resolved().scene.len(), scene.len());
    }

    #[test]
    fn an_image_leaf_measures_to_its_intrinsic_size() {
        let mut scene = Scene::new(Size::ZERO);
        let leaf = scene
            .push(
                NodeId::ROOT,
                Node::new(NodeKind::Image {
                    source: ImageSource::Bytes(RED_PNG.to_vec()),
                    fit: ObjectFit::Contain,
                    position: (Length::ZERO, Length::ZERO),
                    frame: None,
                }),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        let fonts = Fonts::new();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let measured = measurer.measure(
            leaf,
            (None, None),
            (Available::MaxContent, Available::MaxContent),
        );
        assert_eq!(measured.size, Size::new(4.0, 2.0));
        assert!(measured.first_baseline.is_none());
    }

    #[test]
    fn a_node_the_measurer_never_prepared_measures_to_nothing() {
        let scene = Scene::new(Size::ZERO);
        let fonts = Fonts::new();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));

        // The root is a container, so it is neither text nor an image.
        assert_eq!(
            measurer.measure(
                NodeId::ROOT,
                (None, None),
                (Available::MaxContent, Available::MaxContent)
            ),
            MeasuredLeaf::EMPTY
        );
    }

    #[test]
    fn fitting_an_intrinsic_size_follows_the_replaced_element_rule() {
        let intrinsic = Size::new(4.0, 2.0);
        let open = (Available::MaxContent, Available::MaxContent);

        // Both axes fixed: the fixed size wins outright.
        assert_eq!(
            fit_intrinsic(intrinsic, (Some(10.0), Some(20.0)), open),
            Size::new(10.0, 20.0)
        );
        // One axis fixed: the other follows the 2:1 ratio.
        assert_eq!(
            fit_intrinsic(intrinsic, (Some(10.0), None), open),
            Size::new(10.0, 5.0)
        );
        assert_eq!(
            fit_intrinsic(intrinsic, (None, Some(5.0)), open),
            Size::new(10.0, 5.0)
        );
        // Neither fixed: the intrinsic size, narrowed to a definite budget.
        assert_eq!(fit_intrinsic(intrinsic, (None, None), open), intrinsic);
        assert_eq!(
            fit_intrinsic(
                intrinsic,
                (None, None),
                (Available::Definite(3.0), Available::Definite(1.0))
            ),
            Size::new(3.0, 1.0)
        );
        // A budget wider than the image does not stretch it.
        assert_eq!(
            fit_intrinsic(
                intrinsic,
                (None, None),
                (Available::Definite(400.0), Available::MinContent)
            ),
            intrinsic
        );
    }

    #[test]
    fn a_zero_height_image_has_no_ratio_to_preserve() {
        let degenerate = Size::new(4.0, 0.0);
        let open = (Available::MaxContent, Available::MaxContent);
        assert_eq!(
            fit_intrinsic(degenerate, (Some(10.0), None), open),
            Size::new(10.0, 0.0)
        );
        assert_eq!(
            fit_intrinsic(degenerate, (None, Some(7.0)), open),
            Size::new(4.0, 7.0)
        );
    }

    #[test]
    fn spacing_resolves_against_the_font_size() {
        assert!(
            (spacing_pixels(Spacing::Normal, 16.0) - 0.0).abs() < f32::EPSILON
        );
        assert!(
            (spacing_pixels(Spacing::Points(3.0), 16.0) - 3.0).abs()
                < f32::EPSILON
        );
        // An em is a multiple of the size, so the same value scales with it.
        assert!(
            (spacing_pixels(Spacing::Em(0.5), 16.0) - 8.0).abs() < f32::EPSILON
        );
        assert!(
            (spacing_pixels(Spacing::Em(0.5), 32.0) - 16.0).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn clamping_only_narrows_against_a_definite_budget() {
        assert!(
            (clamp_to(10.0, Available::Definite(4.0)) - 4.0).abs()
                < f32::EPSILON
        );
        assert!(
            (clamp_to(10.0, Available::Definite(40.0)) - 10.0).abs()
                < f32::EPSILON
        );
        assert!(
            (clamp_to(10.0, Available::MinContent) - 10.0).abs() < f32::EPSILON
        );
        assert!(
            (clamp_to(10.0, Available::MaxContent) - 10.0).abs() < f32::EPSILON
        );
    }

    #[test]
    fn the_measured_leaf_constructors_say_what_they_mean() {
        assert_eq!(MeasuredLeaf::EMPTY.size, Size::ZERO);
        assert!(MeasuredLeaf::EMPTY.first_baseline.is_none());
        let sized = MeasuredLeaf::sized(Size::new(3.0, 4.0));
        assert_eq!(sized.size, Size::new(3.0, 4.0));
        assert!(sized.first_baseline.is_none());
        assert!(!format!("{sized:?}").is_empty());
    }

    #[test]
    fn the_cache_key_distinguishes_every_kind_of_offered_space() {
        assert_eq!(
            AvailableKey::from(Available::Definite(1.0)),
            AvailableKey::Definite(1.0_f32.to_bits())
        );
        assert_ne!(
            AvailableKey::from(Available::Definite(1.0)),
            AvailableKey::from(Available::Definite(2.0))
        );
        assert_eq!(
            AvailableKey::from(Available::MinContent),
            AvailableKey::MinContent
        );
        assert_eq!(
            AvailableKey::from(Available::MaxContent),
            AvailableKey::MaxContent
        );
        assert!(!format!("{:?}", Available::MinContent).is_empty());
    }

    #[test]
    fn the_defaults_are_the_css_initial_values() {
        assert_eq!(DEFAULT_FONT_FAMILY, "");
        assert!((DEFAULT_FONT_SIZE - 16.0).abs() < f32::EPSILON);
    }
    /// The keyword translation is a table, and a table with an arm nobody runs
    /// is a table with an arm nobody checked.
    #[test]
    fn every_style_keyword_translates_to_the_backend_s_own() {
        use meo_canvas_scene::style::text::{
            FontStyle, ParagraphStyle, TextAlign,
        };

        let paragraph = ParagraphStyle {
            max_lines: Some(2),
            ellipsis: Some("...".to_owned()),
        };

        for align in TextAlign::ALL {
            let mut style = ResolvedText::initial();
            style.align = *align;
            style.style = FontStyle::Italic;
            style.family = TEST_FAMILY.to_owned();
            style.letter_spacing = Spacing::Em(0.1);
            let translated = super::skia_style(&style, &paragraph, &[]);
            assert_eq!(translated.font_families, vec![TEST_FAMILY.to_owned()]);
            assert_eq!(translated.max_lines, Some(2));
            assert_eq!(translated.ellipsis.as_deref(), Some("..."));
            assert!((translated.font_size - style.size).abs() < f32::EPSILON);
        }

        // The empty family is a request for any registered face, which the
        // backend spells as an empty list rather than a name.
        let anonymous = ResolvedText::initial();
        assert!(
            super::skia_style(&anonymous, &ParagraphStyle::default(), &[])
                .font_families
                .is_empty()
        );
    }
    /// A `MaxContent` question means "unconstrained", and the answer must be
    /// one line for a run that fits on one.
    ///
    /// Pins the fix for laying out at `max_intrinsic_width()`: that value is
    /// the width the content needs, and a budget of exactly it loses the last
    /// word to a float comparison. "Body text" at 16px reported an intrinsic
    /// 55.010 and wrapped to two lines when laid out at 55.010, so every
    /// unconstrained measurement came back a whole line too tall. Anyone
    /// replacing the unconstrained layout with the intrinsic width fails here.
    #[test]
    fn a_max_content_measurement_does_not_wrap() {
        let (scene, leaf) = text_scene("Body text");
        let fonts = test_fonts();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let unconstrained = measurer.measure(
            leaf,
            (None, None),
            (Available::MaxContent, Available::MaxContent),
        );
        // A budget one pixel under the content's width does wrap, which is
        // what makes the comparison above meaningful rather than vacuous.
        let narrow = measurer.measure(
            leaf,
            (None, None),
            (
                Available::Definite(unconstrained.size.width - 1.0),
                Available::MaxContent,
            ),
        );
        assert!(
            narrow.size.height > unconstrained.size.height,
            "a narrower budget must wrap: {} vs {}",
            narrow.size.height,
            unconstrained.size.height
        );

        // One line, stated as a height rather than a line count because the
        // measurer reports extents: the wrapped answer is a whole line taller,
        // so anything at or above it means the unconstrained case wrapped too.
        let one_line = narrow.size.height - unconstrained.size.height;
        assert!(
            unconstrained.size.height < narrow.size.height - one_line / 2.0,
            "the unconstrained measurement wrapped: {} against a wrapped {}",
            unconstrained.size.height,
            narrow.size.height
        );
    }

    /// A definite budget wider than the content must not wrap it either, which
    /// is the same defect reached through the other arm.
    #[test]
    fn a_roomy_definite_budget_does_not_wrap() {
        let (scene, leaf) = text_scene("Body text");
        let fonts = test_fonts();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let unconstrained = measurer.measure(
            leaf,
            (None, None),
            (Available::MaxContent, Available::MaxContent),
        );
        let roomy = measurer.measure(
            leaf,
            (None, None),
            (Available::Definite(ROOMY), Available::MaxContent),
        );
        assert!(
            (roomy.size.height - unconstrained.size.height).abs()
                < f32::EPSILON,
            "a roomy budget wrapped: {} against unconstrained {}",
            roomy.size.height,
            unconstrained.size.height
        );

        // The budget that actually breaks: exactly the content's own width.
        // Flexbox settles an auto-sized item at precisely its max-content
        // width and then re-asks with that as a known dimension, so this is
        // the question every fitting text node gets rather than a corner case.
        // A budget merely "roomy" never reaches the boundary, which is why the
        // assertion above passed while the bug was live.
        let exact = measurer.measure(
            leaf,
            (Some(unconstrained.size.width), None),
            (
                Available::Definite(unconstrained.size.width),
                Available::MaxContent,
            ),
        );
        assert!(
            (exact.size.height - unconstrained.size.height).abs()
                < f32::EPSILON,
            "a budget of exactly the content width wrapped: {} against {}",
            exact.size.height,
            unconstrained.size.height
        );
    }
}
