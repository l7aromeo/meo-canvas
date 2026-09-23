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

/// Whether a layout applies the paragraph's `max_lines` and ellipsis. Named
/// because `laid(node, width, false)` does not say whether it asks what the
/// text is or what is drawn of it.
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
    /// Answers keyed by the question, kept across the two solves of
    /// `compensate_ratio_direction`. Right while a measurement is a pure
    /// function of its key; one reading the tree or a previous pass would go
    /// stale, and only in two-pass renders.
    answers: HashMap<Question, MeasuredLeaf>,
}

/// One measurement request as a map key. Widths are keyed by their bit
/// patterns, which is exact, since the same `f32` has the same bits; only `NaN`
/// would misbehave, and taffy never offers one.
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

    /// Lays a text node's content out at `width`, for the paint pass. Re-done
    /// rather than remembered: the width layout settles on is not always the
    /// last one it asked about.
    pub(crate) fn block(&mut self, node: NodeId, width: f32) -> Option<Block> {
        self.laid(node, width, Truncate::Yes)
    }

    /// Lays a text node out at `width`, with or without its truncation, which
    /// is used-value behaviour: an intrinsic size comes from the content alone
    /// (CSS Sizing 3 §5.1), and in Chrome's `chrome/min-content.tsv` neither
    /// truncation lowers a min-content width.
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

        // A box rounded down from this paragraph's own width must not break it:
        // taffy rounds to whole pixels, so `Max HP` at `43.43` gets `43`. Where
        // one more pixel fits, lay it out loose. `truncated` counts too, since
        // `HP` at `12.49` cannot break and reaches the marker directly.
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

    /// The width of one inter-word space in a node's own face, which the
    /// painter needs for the gap a space stands for. Answered through the
    /// wrap's cache.
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

    /// Measures a text node by wrapping at the width on offer: an open axis at
    /// infinity, since at exactly the max-content width, which flexbox re-asks
    /// with, a float comparison loses the last word. `MinContent` wraps at
    /// every space.
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

        // An intrinsic question is answered from the content alone: a clamp
        // describes what is drawn, and through it the marker's width would
        // become the narrowest the paragraph can be. See `laid`.
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
            MeasuredLeaf::sized(fit_intrinsic(image.intrinsic_size(), known))
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

/// Fits a leaf with an intrinsic size into what layout has fixed, by CSS's rule
/// for a replaced element: a fixed axis wins, an open axis scales to keep the
/// ratio when the other is fixed, and with neither fixed the intrinsic size
/// stands however little space is offered, as in Chrome.
fn fit_intrinsic(intrinsic: Size, known: (Option<f32>, Option<f32>)) -> Size {
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
        // Neither axis fixed: the intrinsic size. Chrome overflows a 60x40
        // `<img>` in a 200x30, a 200x100 and a 30-wide block alike; a flex
        // line's stretch is `align_self`, not a clamp here.
        (None, None, _) => intrinsic,
    }
}

/// Shapes one text node into a Skia paragraph. Test-only: text is measured and
/// drawn through [`crate::lines`], and this is the independent layout that
/// module's comparison report checks it against.
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
        // Passed on from `ResolvedText`: dropped here, `underline` would paint
        // exactly as `none` does, and no comparison of the wire formats could
        // see it.
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
        // Skia takes a multiplier, so a length divides by the font size here
        // and nowhere else, and `None`, CSS's `normal`, is Skia's `1.0`.
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
#[expect(
    clippy::match_same_arms,
    reason = "the named arm and the `#[non_exhaustive]` arm agree today and \
              mean different things: one is the value this build knows, the \
              other is one it has never heard of."
)]
fn spacing_pixels(spacing: Spacing, font_size: f32) -> f32 {
    match spacing {
        Spacing::Normal => 0.0,
        Spacing::Points(points) => points,
        Spacing::Em(em) => em * font_size,
        // As `lines::spacing_pixels`: `Spacing` is `#[non_exhaustive]`.
        _ => 0.0,
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
        Measure, MeasuredLeaf, SceneMeasurer, fit_intrinsic, spacing_pixels,
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

    /// The rescue undoes a break one more pixel would have prevented, since
    /// taffy rounds a rect as `round(x + w) - round(x)`, and leaves a real one.
    /// Both rows, because a rescue that reached every paragraph would pass the
    /// first alone.
    #[test]
    fn a_break_within_a_pixel_of_fitting_is_undone_and_a_real_one_is_not() {
        let (scene, leaf) = text_scene("ab cd");
        let fonts = test_fonts();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let natural = measurer
            .measure(
                leaf,
                (None, None),
                (Available::MaxContent, Available::MaxContent),
            )
            .size
            .width;

        // A hair under its own width: the rounding case, and the whole
        // paragraph comes back on one line.
        let pinched = measurer
            .block(leaf, natural - 0.5)
            .unwrap_or_else(|| unreachable!("the leaf is text"));
        assert_eq!(
            pinched.lines.len(),
            1,
            "a box half a pixel short is a rounded box, not a narrower one"
        );

        // Genuinely narrower, by more than the guard allows: the break stands.
        let narrow = measurer
            .block(leaf, natural / 2.0)
            .unwrap_or_else(|| unreachable!("the leaf is text"));
        assert!(
            narrow.lines.len() > 1,
            "a box half the width of the text really is too narrow, and the \
             rescue must not reach that far"
        );
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

    /// Chrome's broken `<img>`, with Playwright aborting the load: auto 0x0,
    /// `width:100px` 100x0, `height:60px` 0x60, attributes 80x40, where a
    /// loaded one measured 64x32. Per axis: zero on an `auto` axis, every
    /// explicit extent kept.
    #[test]
    fn a_source_that_never_decoded_measures_as_chrome_measures_it() {
        // No bitmap arrived, so there is no intrinsic size to fit.
        let none = Size::ZERO;

        assert_eq!(
            fit_intrinsic(none, (None, None)),
            Size::new(0.0, 0.0),
            "auto on both axes should collapse, as Chrome does"
        );
        assert_eq!(
            fit_intrinsic(none, (Some(100.0), None)),
            Size::new(100.0, 0.0),
            "an explicit width should survive and the auto height collapse"
        );
        assert_eq!(
            fit_intrinsic(none, (None, Some(60.0))),
            Size::new(0.0, 60.0),
            "an explicit height should survive and the auto width collapse"
        );
        assert_eq!(
            fit_intrinsic(none, (Some(80.0), Some(40.0))),
            Size::new(80.0, 40.0),
            "both stated should be honoured untouched"
        );

        // A zero intrinsic height must not reach the ratio arm, where 0/0 is
        // NaN and every later comparison is silently false. The guard is
        // `intrinsic.height > 0.0`; this asserts its consequence.
        for size in [
            fit_intrinsic(none, (Some(100.0), None)),
            fit_intrinsic(none, (None, Some(60.0))),
            fit_intrinsic(none, (None, None)),
        ] {
            assert!(
                size.width.is_finite() && size.height.is_finite(),
                "a zero intrinsic size produced {size:?}"
            );
        }
    }

    #[test]
    fn fitting_an_intrinsic_size_follows_the_replaced_element_rule() {
        let intrinsic = Size::new(4.0, 2.0);

        // Both axes fixed: the fixed size wins outright.
        assert_eq!(
            fit_intrinsic(intrinsic, (Some(10.0), Some(20.0))),
            Size::new(10.0, 20.0)
        );
        // One axis fixed: the other follows the 2:1 ratio.
        assert_eq!(
            fit_intrinsic(intrinsic, (Some(10.0), None)),
            Size::new(10.0, 5.0)
        );
        assert_eq!(
            fit_intrinsic(intrinsic, (None, Some(5.0))),
            Size::new(10.0, 5.0)
        );
        // Neither fixed: the intrinsic size, whatever space is offered, as
        // `an_intrinsic_size_is_not_narrowed_by_the_space_offered` measures in
        // Chrome.
        assert_eq!(fit_intrinsic(intrinsic, (None, None)), intrinsic);
    }

    #[test]
    fn a_zero_height_image_has_no_ratio_to_preserve() {
        let degenerate = Size::new(4.0, 0.0);
        assert_eq!(
            fit_intrinsic(degenerate, (Some(10.0), None)),
            Size::new(10.0, 0.0)
        );
        assert_eq!(
            fit_intrinsic(degenerate, (None, Some(7.0))),
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

    /// A 60x40 image is 60x40 whatever space is offered: Chrome overflows it in
    /// a 200x30, a 200x100 and a 30-wide block alike, so the offered space is
    /// not a parameter of `fit_intrinsic`.
    #[test]
    fn an_intrinsic_size_is_not_narrowed_by_the_space_offered() {
        let intrinsic = Size::new(60.0, 40.0);
        assert_eq!(fit_intrinsic(intrinsic, (None, None)), intrinsic);
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
    /// A `MaxContent` question is unconstrained, so a run that fits on one line
    /// answers one line. At the content's own width a float comparison loses
    /// the last word: `Body text` at 16px measures 55.010 and wraps at 55.010.
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

        // The budget that breaks: exactly the content's own width, which
        // flexbox re-asks with for every fitting text node. A merely roomy
        // budget never reaches the boundary.
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
