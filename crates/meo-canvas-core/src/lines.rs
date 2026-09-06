//! Line boxes, wrapping and truncation, computed here rather than by Skia's
//! paragraph.
//!
//! # Why this exists
//!
//! A browser canvas has no paragraph. v1 draws text with `fillText` and
//! `strokeText` and computes everything around them by hand: where the lines
//! break, how tall a line box is, where its baseline sits, which characters
//! survive a truncation and where the ellipsis goes. That is the model this
//! renderer is being moved onto, because it is the model whose answers the
//! project already trusts -- and because a paragraph decides several of those
//! for us in ways nothing here can reach.
//!
//! This module is the arithmetic. Nothing in it draws.
//!
//! # What a line box is
//!
//! A line's **content height** is the face's own ascent plus descent, taken as
//! the maximum over the runs on that line. Its **box height** is what
//! `line_height` asks for, or the content height when it asks for nothing.
//! The difference is **leading**, split half above and half below -- and it is
//! allowed to be negative: a `line_height` tighter than the face needs makes
//! the glyphs overlap their neighbours rather than moving the baseline, which
//! is what CSS says and what v1 does.
//!
//! So the baseline of a line is `top + leading / 2 + ascent`, and a line's
//! ascent comes from the **face** rather than from the string. That is why
//! [`METRICS_STRING`] exists and why its contents do not matter.
//!
//! # Spaces are runs of no width
//!
//! A space between two words is kept as a run carrying a width of zero, and
//! the gap is added by the arithmetic instead: `space_width + word_spacing`.
//! v1 does this so that justification and word spacing have one place to
//! change the gap, rather than having to reach inside a measured run.

use std::collections::{HashMap, VecDeque};

use meo_canvas_scene::style::{
    paint::Color,
    text::{
        FontStyle, FontVariant, LineHeight, ParagraphStyle, Spacing,
        TextDecoration, TextSegment,
    },
};
use meo_skia_canvas::{
    Canvas, CanvasOptions, Font, FontFeature, FontStretch, FontVariantCaps,
};

use crate::resolve::ResolvedText;

/// The string every face measurement is taken with.
///
/// Its contents do not matter: what is read back is
/// `font_bounding_box_ascent` and `font_bounding_box_descent`, which describe
/// the **face** and are the same for every string in it. One constant is kept
/// so the cache answers it once per face rather than once per line, and so
/// that a line's height cannot depend on what is written on it -- the property
/// CSS gets from a strut, and the reason a line does not move when it gains a
/// descender.
///
/// v1's own string, kept character for character so that a face reporting
/// something odd reports it identically in both.
pub const METRICS_STRING: &str = "Ag|``";

/// Entries the measurement cache holds before the oldest is dropped.
///
/// v1's number. Sized for the strings one render draws rather than the strings
/// that exist: a card's labels, its words, and the per-character measurements
/// a truncation makes.
const MEASUREMENT_LIMIT: usize = 4096;

/// The ascent a face is assumed to have when it reports nothing, as a fraction
/// of the em size.
const FALLBACK_ASCENT: f32 = 0.8;

/// The descent a face is assumed to have when it reports nothing.
const FALLBACK_DESCENT: f32 = 0.2;

/// The width a space is assumed to have when the face reports zero.
const FALLBACK_SPACE: f32 = 0.3;

/// A font selection, as a key a cache can hold.
///
/// [`Font`] is not `Hash` -- it carries an `f32` size -- so the parts that
/// change a measurement are keyed by their bits.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontKey {
    /// The family name.
    family: String,
    /// The em size, by its bits, because a size is an `f32`.
    size: u32,
    /// The CSS numeric weight.
    weight: u16,
    /// Whether an italic face was asked for.
    italic: bool,
    /// The OpenType features asked for, by their wire values, since a
    /// `FontVariant` is not `Hash` and a feature changes the advance.
    variant: Vec<u8>,
}

/// What one measurement answers.
///
/// Snapshotted rather than held as a backend object: the `actual` pair
/// describes the string and the `font` pair describes the face, and a line box
/// is built from the second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    /// The advance width of the string.
    pub width: f32,
    /// The face's ascent above the baseline, **rounded to a whole pixel**.
    ///
    /// Chrome rounds `fontBoundingBoxAscent` and `fontBoundingBoxDescent`
    /// deliberately: measured on this face at 16px it reports exactly 19 and
    /// 5, while returning a fractional `actualBoundingBoxAscent` from the same
    /// call -- so the integers are a decision and not a loss of precision.
    ///
    /// **Rounding here is not a tolerance, it is what stops a drift.** The
    /// face reports 19.088 and 4.624, whose sum is 23.712 against Chrome's
    /// line box of 24. Every line would be 0.288px short and the error would
    /// accumulate down the paragraph -- nearly three pixels of baseline by the
    /// tenth line.
    pub ascent: f32,
    /// The face's descent below the baseline, rounded with the ascent.
    pub descent: f32,
}

/// One run's style, resolved down to what a measurement needs.
#[derive(Debug, Clone, PartialEq)]
pub struct RunStyle {
    /// Family name.
    pub family: String,
    /// Em size in pixels.
    pub size: f32,
    /// CSS numeric weight.
    pub weight: u16,
    /// Whether the face is italic.
    pub italic: bool,
    /// The OpenType features the run asks for.
    ///
    /// **Part of the measurement, not only of the drawing.** `frac` alone
    /// takes a nineteen-character sample from 220.61 to 211.04 in the
    /// repository's own face, so a width measured without the features is the
    /// wrong width.
    pub variant: Vec<FontVariant>,
    /// The colour the glyphs are filled with.
    ///
    /// **Here rather than on the node, because a run is what carries it.**
    /// A segment declaring a colour changes its own ink and nothing else,
    /// measured against Chrome 151: the paragraph's height and the span's box
    /// are unmoved and only the pixels differ. The fields above are the ones
    /// that were already here, and they are exactly the properties a segment
    /// could carry before this -- everything else a caller set on a segment
    /// had nowhere to travel and was silently discarded.
    pub color: Color,
    /// Underline, overline or strike, drawn with the run.
    ///
    /// Per-run for the same reason as [`RunStyle::color`] and by the same
    /// measurement: a span's decoration marks that span's ink alone.
    pub decoration: TextDecoration,
    /// Extra space after each of this run's characters, **already in pixels**.
    ///
    /// **Resolved here rather than carried as a `Spacing`, because an `em`
    /// resolves against the size of the element that declares it.** A segment
    /// that sets both a font size and an `em` spacing must resolve the spacing
    /// against its own size, not the paragraph's -- so the resolution happens
    /// in [`RunStyle::of`], where the merged size is already known. Passing
    /// the paragraph's already-resolved pixels down would be right only
    /// for a segment that does not change its size.
    ///
    /// **Inter-word spaces keep the paragraph's value**, which is why
    /// [`Metrics::letter_spacing`] still exists: a space between two runs
    /// belongs to neither of them.
    pub letter_spacing: f32,
}

impl RunStyle {
    /// The style a segment draws in, on top of the node's resolved style.
    #[must_use]
    pub fn of(base: &ResolvedText, segment: &TextSegment) -> Self {
        let style = &segment.style;
        Self {
            family: style
                .font_family
                .clone()
                .unwrap_or_else(|| base.family.clone()),
            size: style.font_size.unwrap_or(base.size),
            weight: style.font_weight.unwrap_or(base.weight).get(),
            italic: matches!(
                style.font_style.unwrap_or(base.style),
                FontStyle::Italic
            ),
            variant: style
                .font_variant
                .clone()
                .unwrap_or_else(|| base.font_variant.clone()),
            color: style.color.unwrap_or(base.color),
            decoration: style.text_decoration.unwrap_or(base.decoration),
            letter_spacing: spacing_pixels(
                style.letter_spacing.unwrap_or(base.letter_spacing),
                // The run's own size, for the reason the field documents.
                style.font_size.unwrap_or(base.size),
            ),
        }
    }

    /// The node's own style, for the measurements a run does not own: the
    /// strut of an empty line, and the width of an inter-word space.
    #[must_use]
    pub fn base(base: &ResolvedText) -> Self {
        Self {
            family: base.family.clone(),
            size: base.size,
            weight: base.weight.get(),
            italic: matches!(base.style, FontStyle::Italic),
            variant: base.font_variant.clone(),
            color: base.color,
            decoration: base.decoration,
            letter_spacing: spacing_pixels(base.letter_spacing, base.size),
        }
    }

    /// This style as the backend's font selection.
    #[must_use]
    pub fn to_font(&self) -> Font {
        Font {
            families: vec![self.family.clone()],
            size: self.size,
            weight: self.weight,
            italic: self.italic,
            stretch: FontStretch::Normal,
            line_height: None,
        }
    }

    /// This style as a cache key.
    fn key(&self) -> FontKey {
        FontKey {
            family: self.family.clone(),
            size: self.size.to_bits(),
            weight: self.weight,
            italic: self.italic,
            variant: self.variant.iter().map(|v| v.to_wire()).collect(),
        }
    }

    /// This style's features as the backend takes them: a caps keyword and a
    /// list of OpenType tags.
    ///
    /// CSS spells all of these as `font-variant` keywords; the backend splits
    /// them, because the caps forms select a face's alternate glyphs and the
    /// rest are feature tags. A keyword the backend has no place for is
    /// dropped rather than approximated.
    #[must_use]
    pub fn to_variant(&self) -> (FontVariantCaps, Vec<FontFeature>) {
        let mut caps = FontVariantCaps::Normal;
        let mut features = Vec::new();
        let mut tag = |name: &str, value: i32| {
            features.push(FontFeature {
                name: name.to_owned(),
                value,
            });
        };
        #[expect(
            clippy::match_same_arms,
            reason = "`Normal` asks for nothing on purpose; the \
                      `#[non_exhaustive]` arm asks for nothing because it has \
                      no name to ask with. Same body, different reasons."
        )]
        for variant in &self.variant {
            match variant {
                FontVariant::Normal => {}
                FontVariant::SmallCaps => caps = FontVariantCaps::SmallCaps,
                FontVariant::AllSmallCaps => {
                    caps = FontVariantCaps::AllSmallCaps;
                }
                FontVariant::PetiteCaps => caps = FontVariantCaps::PetiteCaps,
                FontVariant::AllPetiteCaps => {
                    caps = FontVariantCaps::AllPetiteCaps;
                }
                FontVariant::Unicase => caps = FontVariantCaps::Unicase,
                FontVariant::TitlingCaps => {
                    caps = FontVariantCaps::TitlingCaps;
                }
                FontVariant::HistoricalForms => tag("hist", 1),
                FontVariant::LiningNums => tag("lnum", 1),
                FontVariant::OldstyleNums => tag("onum", 1),
                FontVariant::ProportionalNums => tag("pnum", 1),
                FontVariant::TabularNums => tag("tnum", 1),
                FontVariant::DiagonalFractions => tag("frac", 1),
                FontVariant::StackedFractions => tag("afrc", 1),
                FontVariant::Ordinal => tag("ordn", 1),
                FontVariant::SlashedZero => tag("zero", 1),
                FontVariant::CommonLigatures => tag("liga", 1),
                FontVariant::NoCommonLigatures => tag("liga", 0),
                FontVariant::DiscretionaryLigatures => tag("dlig", 1),
                FontVariant::NoDiscretionaryLigatures => tag("dlig", 0),
                FontVariant::HistoricalLigatures => tag("hlig", 1),
                FontVariant::NoHistoricalLigatures => tag("hlig", 0),
                FontVariant::Contextual => tag("calt", 1),
                FontVariant::NoContextual => tag("calt", 0),
                FontVariant::Simplified => tag("smpl", 1),
                FontVariant::Traditional => tag("trad", 1),
                FontVariant::Jis78 => tag("jp78", 1),
                FontVariant::Jis83 => tag("jp83", 1),
                FontVariant::Jis90 => tag("jp90", 1),
                FontVariant::Jis04 => tag("jp04", 1),
                FontVariant::FullWidth => tag("fwid", 1),
                FontVariant::ProportionalWidth => tag("pwid", 1),
                FontVariant::Ruby => tag("ruby", 1),
                FontVariant::Super => tag("sups", 1),
                FontVariant::Sub => tag("subs", 1),
                // `FontVariant` is `#[non_exhaustive]`, and OpenType has more
                // features than this list names. One this build does not know
                // asks the shaper for nothing, which leaves the text as it
                // would have been drawn without it.
                _ => {}
            }
        }
        (caps, features)
    }
}

/// Measures strings, answering from memory when it has been asked before.
///
/// # Why the cache is not optional
///
/// Shaping is the expensive half of laying text out, and this asks the same
/// question relentlessly: layout calls a measure function several times per
/// pass while it searches for a width that fits, and a truncation walks a
/// string one character at a time. v1 measured the ratio on a twenty-four line
/// card -- **720 calls resolving to 58 distinct questions**, so ninety-nine
/// parts in a hundred of the work was a repeat.
///
/// # Why there is no epoch
///
/// v1's cache is global to the process and needs a counter to invalidate it
/// when a font is registered, since the same `12px Roboto` measures
/// differently before and after Roboto exists. This one lives for one render:
/// it is built with the measurer and dropped with it, so a registration
/// between renders cannot be seen by a cache that no longer exists. The
/// guarantee v1 buys with the epoch, this gets from its lifetime.
pub struct TextMeasurer {
    /// A one-pixel canvas, which is where a measuring context comes from. The
    /// font library is process-wide, so this sees every registered face.
    canvas: Canvas,
    /// Answers, keyed by the font and the string.
    answers: HashMap<(FontKey, u32, String), Measurement>,
    /// Keys in the order they were inserted, so the oldest can be dropped.
    ///
    /// First in, first out, where v1 is least-recently-used. The difference is
    /// which entry survives a full cache and not what any of them say.
    order: VecDeque<(FontKey, u32, String)>,
}

impl core::fmt::Debug for TextMeasurer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextMeasurer")
            .field("cached", &self.answers.len())
            .finish_non_exhaustive()
    }
}

impl Default for TextMeasurer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextMeasurer {
    /// A measurer with an empty cache.
    #[must_use]
    pub fn new() -> Self {
        // Nothing is drawn on it, and asking for a backend that has to be
        // created would cost a device for arithmetic.
        let options = CanvasOptions {
            gpu: false,
            ..CanvasOptions::default()
        };
        Self {
            canvas: Canvas::with_options(1.0, 1.0, options)
                .unwrap_or_else(|_| Canvas::new(1.0, 1.0)),
            answers: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// How many answers are held, for a test that wants to see the bound hold.
    #[must_use]
    pub fn cached(&self) -> usize {
        self.answers.len()
    }

    /// Measures `text` in `style`, at `letter_spacing`.
    ///
    /// The spacing is part of the key because the backend applies it, so two
    /// spacings are two different widths for one string.
    pub fn measure(
        &mut self,
        style: &RunStyle,
        letter_spacing: f32,
        text: &str,
    ) -> Measurement {
        let key = (style.key(), letter_spacing.to_bits(), text.to_owned());
        if let Some(hit) = self.answers.get(&key) {
            return *hit;
        }

        let font = style.to_font();
        let (caps, features) = style.to_variant();
        let context = self.canvas.context();
        context.set_font(&font);
        // After `set_font`, which resets the variant axes as assigning the
        // CSS `font` shorthand does.
        context.set_font_variant(caps, &features);
        context.set_letter_spacing(letter_spacing);
        let metrics = context.measure_text(text, None);
        let measurement = Measurement {
            width: metrics.width,
            ascent: if metrics.font_bounding_box_ascent > 0.0 {
                metrics.font_bounding_box_ascent.round()
            } else {
                (style.size * FALLBACK_ASCENT).round()
            },
            descent: if metrics.font_bounding_box_descent > 0.0 {
                metrics.font_bounding_box_descent.round()
            } else {
                (style.size * FALLBACK_DESCENT).round()
            },
        };

        if self.answers.len() >= MEASUREMENT_LIMIT
            && let Some(oldest) = self.order.pop_front()
        {
            self.answers.remove(&oldest);
        }
        self.order.push_back(key.clone());
        self.answers.insert(key, measurement);
        measurement
    }

    /// The width of a run, with the letter spacing CSS adds and the backend
    /// does not.
    ///
    /// **One unit, not one per character.** The backend applies the spacing
    /// *between* characters, so an `n`-character run comes back `n - 1`
    /// spacings wide where CSS gives it `n` -- one after every character,
    /// including the last. Every run is short by exactly one unit, however
    /// long it is.
    ///
    /// v1 carries a comment saying that an earlier version added `n - 1` here
    /// on the premise that the backend applied none of it, and that a line
    /// came out roughly a third too wide at 2px on a sixteen-character string.
    /// The premise had stopped being true; the arithmetic had not caught up.
    pub fn run_width(
        &mut self,
        style: &RunStyle,
        letter_spacing: f32,
        text: &str,
    ) -> f32 {
        let measured = self.measure(style, letter_spacing, text).width;
        if letter_spacing == 0.0 || text.is_empty() {
            measured
        } else {
            measured + letter_spacing
        }
    }

    /// The width of one inter-word space, spacing included.
    ///
    /// Measured in the node's **base** font rather than in any run's, which is
    /// v1's choice: the gap between two differently-styled words is one gap,
    /// and taking it from whichever run happens to precede it would make a
    /// line's width depend on the order its styles appear in.
    ///
    /// A single character comes back with no letter spacing at all -- there is
    /// nothing for the backend to put it between -- so the one unit CSS gives
    /// it is added here like any other run.
    pub fn space_width(&mut self, base: &RunStyle, letter_spacing: f32) -> f32 {
        let measured = self.measure(base, letter_spacing, " ").width;
        let resolved = if measured > 0.0 {
            measured
        } else {
            base.size * FALLBACK_SPACE
        };
        if letter_spacing == 0.0 {
            resolved
        } else {
            resolved + letter_spacing
        }
    }
}

/// One run of text on a line, in one style.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// The characters, which for a space run is the whitespace itself.
    pub text: String,
    /// What it is drawn in.
    pub style: RunStyle,
    /// Its advance width. **Zero for a space run**, whose gap is arithmetic
    /// rather than a measurement -- see this module's own documentation.
    pub width: f32,
}

impl Run {
    /// Whether this run is inter-word whitespace rather than a word.
    #[must_use]
    pub fn is_space(&self) -> bool {
        !self.text.is_empty()
            && self.text.chars().all(|c| c.is_whitespace() && c != '\n')
    }
}

/// One line box: its runs, and the three heights that place it.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The runs on the line, in order, spaces included.
    pub runs: Vec<Run>,
    /// The face's ascent, taken as the maximum over the line's words.
    pub ascent: f32,
    /// Ascent plus descent: what the line needs.
    pub content_height: f32,
    /// What the line box occupies, which `line_height` may make smaller.
    pub height: f32,
    /// Whether this line ended at a newline the caller wrote, rather than at
    /// a wrap. Truncation may pull text up across a wrap and never across
    /// one of these.
    pub hard_break: bool,
}

impl Line {
    /// The distance from the line box's top to its baseline.
    ///
    /// Half the leading, then the ascent. **The leading goes negative** for a
    /// line box shorter than the face needs, which is the case where CSS lets
    /// the glyphs escape the box rather than moving the baseline inside it.
    #[must_use]
    pub fn baseline_from_top(&self) -> f32 {
        (self.height - self.content_height) / 2.0 + self.ascent
    }
}

/// Everything one text node's content occupies.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The lines, after any `max_lines` limit.
    pub lines: Vec<Line>,
    /// The widest line.
    pub width: f32,
    /// Every line box plus the gaps between them.
    pub height: f32,
    /// How many lines the **wrap** produced, before `max_lines` dropped any.
    ///
    /// `lines.len()` cannot answer this and that is the whole reason the field
    /// exists. A paragraph with `max_lines: 1` that broke into two comes back
    /// as one line carrying a marker, which is indistinguishable by any later
    /// test from a paragraph that never broke -- and
    /// [`crate::measure::SceneMeasurer`] has a rescue that needs exactly that
    /// distinction, because a break caused by a box rounded down from the
    /// text's own width is an artefact rather than a break.
    ///
    /// Recorded here, where the untruncated wrap is still in hand, rather than
    /// reconstructed afterwards by wrapping a second time.
    pub wrapped_lines: usize,
    /// Whether a marker replaced any of the text.
    ///
    /// Both triggers set it, and they are different failures: `max_lines`
    /// dropping lines, and a single line too wide to break -- an unbreakable
    /// word does not raise `wrapped_lines` above one, so that field alone
    /// cannot see the second. The rescue in [`crate::measure`] needs both,
    /// because a box rounded down from the text's own width produces either
    /// depending only on whether the text has a space in it.
    pub truncated: bool,
}

/// The paragraph-level inputs a wrap needs, resolved to pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Extra space after every character, in pixels.
    pub letter_spacing: f32,
    /// Extra space in every inter-word gap, in pixels.
    pub word_spacing: f32,
    /// The line box height, or `None` for the face's own.
    pub line_height: Option<f32>,
    /// Space added between line boxes, and never after the last.
    pub line_gap: f32,
}

impl Metrics {
    /// The metrics a resolved style asks for.
    #[must_use]
    pub fn of(base: &ResolvedText) -> Self {
        Self {
            letter_spacing: spacing_pixels(base.letter_spacing, base.size),
            word_spacing: spacing_pixels(base.word_spacing, base.size),
            // `None` is CSS's `normal`, and it arrives as `None` rather
            // than as a magic number: **`1.0` is a line box exactly one em
            // tall, which a caller can legitimately ask for.** This used to
            // exclude it as a sentinel, so every `line-height: 1` in a
            // document got the face's metrics instead -- twenty of them in
            // one card, six pixels apiece.
            //
            // The zero-and-below guard stays: a non-positive height has no
            // line box to give and is not what this change is about.
            //
            // **This is where a stated height becomes pixels, and it is the
            // only place a number is multiplied.** A length is already
            // pixels; a number is a multiple of *this* element's size, which
            // is why it survives resolution unresolved. A percentage cannot
            // arrive -- `ResolvedText` never holds one.
            line_height: match base.line_height {
                Some(LineHeight::Number(multiple)) if multiple > 0.0 => {
                    Some(multiple * base.size)
                }
                Some(LineHeight::Length(pixels)) if pixels > 0.0 => {
                    Some(pixels)
                }
                Some(LineHeight::Percent(share)) => {
                    // Resolution turns every percentage into a length, so
                    // one here is a scene that skipped it rather than a value
                    // to interpret. Treated as the length it would have been
                    // against this element's size, which is the same answer
                    // resolution would have produced.
                    (share > 0.0).then_some(share * base.size)
                }
                _ => None,
            },
            line_gap: base.line_gap,
        }
    }
}

/// A [`Spacing`] as the pixel count it stands for.
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
        // `Spacing` is `#[non_exhaustive]`; an unknown spelling adds nothing,
        // which is what `Normal` already means.
        _ => 0.0,
    }
}

/// Splits a segment into words and the whitespace between them.
///
/// Newlines are their own pieces, because a wrap has to know a break was asked
/// for rather than chosen. Every other whitespace run collapses into one gap,
/// as CSS's `white-space: normal` does.
fn pieces(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let kind = |c: char| {
            if c == '\n' {
                0
            } else if c.is_whitespace() {
                1
            } else {
                2
            }
        };
        let first = kind(rest.chars().next().unwrap_or(' '));
        // A newline is always alone, so two of them are two breaks.
        let end = if first == 0 {
            rest.chars().next().map_or(1, char::len_utf8)
        } else {
            rest.char_indices()
                .find(|(_, c)| kind(*c) != first)
                .map_or(rest.len(), |(at, _)| at)
        };
        out.push(&rest[..end]);
        rest = &rest[end..];
    }
    out
}

/// Wraps segments into lines that fit `max_width`.
///
/// Word level, with a word wider than the line broken between characters. The
/// first word on a line is placed whatever its width, so a line is never empty
/// because nothing fits.
#[must_use]
pub fn wrap(
    measurer: &mut TextMeasurer,
    base: &ResolvedText,
    segments: &[TextSegment],
    max_width: f32,
    metrics: Metrics,
) -> Vec<Line> {
    let base_style = RunStyle::base(base);
    let space = measurer.space_width(&base_style, metrics.letter_spacing);
    let gap = space + metrics.word_spacing;

    let mut lines: Vec<Line> = Vec::new();
    let mut runs: Vec<Run> = Vec::new();
    let mut used = 0.0_f32;

    for segment in segments {
        let style = RunStyle::of(base, segment);
        for piece in pieces(&segment.text) {
            if piece == "\n" {
                finish(&mut lines, &mut runs, &mut used, true);
                continue;
            }
            if piece.chars().all(char::is_whitespace) {
                // Kept, at no width, so the gap it stands for can be added
                // once when the next word arrives.
                if !runs.is_empty() {
                    runs.push(Run {
                        text: piece.to_owned(),
                        style: style.clone(),
                        width: 0.0,
                    });
                }
                continue;
            }

            let width = measurer.run_width(&style, style.letter_spacing, piece);
            let advance = if runs.last().is_some_and(Run::is_space) {
                gap + width
            } else {
                width
            };

            if used + advance <= max_width || runs.is_empty() {
                used += advance;
                runs.push(Run {
                    text: piece.to_owned(),
                    style: style.clone(),
                    width,
                });
                continue;
            }

            finish(&mut lines, &mut runs, &mut used, false);

            // **A word too wide for the line overflows it, on a line of its
            // own.** Measured in Chrome: a 278px word in a 100px box is one
            // line 278px wide when it stands alone, and the second of two
            // lines when a short word precedes it. Neither is broken.
            // `overflow-wrap: break-word` is what asks for breaking, and this
            // scene does not have that property yet -- see [`break_word`].
            //
            // v1 breaks such a word between characters, but only when it
            // arrives at a line that already has something on it, because its
            // place-it-anyway branch is tested before the width. That
            // arrangement is v1's, not the browser's.
            used = width;
            runs.push(Run {
                text: piece.to_owned(),
                style: style.clone(),
                width,
            });
        }
    }
    finish(&mut lines, &mut runs, &mut used, false);

    lines
}

/// Closes the line being built and starts the next one.
///
/// Trailing whitespace goes with it: a space at the end of a line is the one
/// the wrap consumed when it broke there, and CSS does not draw it. `hard`
/// records **why** the line ended, which truncation needs and nothing else
/// does -- text may be pulled up across a wrap and never across a newline the
/// caller wrote.
fn finish(
    lines: &mut Vec<Line>,
    runs: &mut Vec<Run>,
    used: &mut f32,
    hard: bool,
) {
    while runs.last().is_some_and(Run::is_space) {
        runs.pop();
    }
    lines.push(Line {
        runs: core::mem::take(runs),
        ascent: 0.0,
        content_height: 0.0,
        height: 0.0,
        hard_break: hard,
    });
    *used = 0.0;
}

/// Breaks one word into pieces that each fit `max_width`.
///
/// Character by character, because there is nothing narrower to break at. A
/// single character wider than the line is given a piece of its own rather
/// than being dropped.
///
/// **Nothing calls this yet, and that is the correct state.** Chrome does not
/// break a long word under the default `overflow-wrap: normal` -- measured, in
/// both arrangements: alone in a box a third its width it is one overflowing
/// line, and after a short word it is the second line, still whole. Only
/// `overflow-wrap: break-word` breaks it, and the scene has no such property.
/// The arithmetic is kept, tested, and waiting for that property rather than
/// deleted, because the day it is added this is what it needs and rewriting it
/// then would be rewriting something already measured against the browser.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "waiting for `overflow-wrap: break-word`, see the docs above"
    )
)]
fn break_word(
    measurer: &mut TextMeasurer,
    style: &RunStyle,
    word: &str,
    max_width: f32,
    letter_spacing: f32,
) -> Vec<Run> {
    let mut parts = Vec::new();
    let mut part = String::new();

    for character in word.chars() {
        let mut candidate = part.clone();
        candidate.push(character);
        let width = measurer.run_width(style, letter_spacing, &candidate);
        if width <= max_width {
            part = candidate;
            continue;
        }

        if !part.is_empty() {
            let width = measurer.run_width(style, letter_spacing, &part);
            parts.push(Run {
                text: core::mem::take(&mut part),
                style: style.clone(),
                width,
            });
        }
        part.push(character);
        let alone = measurer.run_width(style, letter_spacing, &part);
        if alone > max_width {
            parts.push(Run {
                text: core::mem::take(&mut part),
                style: style.clone(),
                width: alone,
            });
        }
    }

    if !part.is_empty() {
        let width = measurer.run_width(style, letter_spacing, &part);
        parts.push(Run {
            text: part,
            style: style.clone(),
            width,
        });
    }
    parts
}

/// Rebuilds the last visible line so it fills the room an ellipsis leaves it.
///
/// # Why the line has to be rebuilt at all
///
/// Wrapping breaks at words, so the word that overflowed has already moved to
/// a line `max_lines` is about to throw away -- leaving the last line ending
/// at whatever word fitted, with an ellipsis tacked on. **CSS does the
/// opposite**: the last line takes as many characters as fit, mid-word if that
/// is where the room runs out, and the ellipsis follows. `Flower of Paradise
/// Lost` in 140px is `Flower of Par…` in a browser where breaking at the word
/// gives `Flower of…`.
///
/// That is the whole reason `max_lines` and the ellipsis cannot be left to the
/// backend's paragraph: a paragraph-level limit cannot express it.
///
/// Text is pulled up from the lines that follow, which is where the rest went
/// -- but **never across a newline the caller wrote**, since that is a break
/// they asked for rather than one wrapping introduced.
fn truncate_with(
    measurer: &mut TextMeasurer,
    base: &ResolvedText,
    all: &[Line],
    from: usize,
    max_width: f32,
    marker: &str,
    metrics: Metrics,
) -> Line {
    let base_style = RunStyle::base(base);
    let space = measurer.space_width(&base_style, metrics.letter_spacing);
    let gap = space + metrics.word_spacing;

    // The ellipsis takes the style of the last thing written before it, which
    // is v1's rule and the one that keeps a bold last word's marker bold.
    let style = all[from]
        .runs
        .iter()
        .rev()
        .find(|run| !run.is_space())
        .map_or(base_style, |run| run.style.clone());
    let marker_width =
        measurer.run_width(&style, metrics.letter_spacing, marker);
    let budget = max_width - marker_width;

    // This line, then each soft-wrapped continuation, with the space the wrap
    // consumed put back between them.
    let mut source: Vec<Run> = Vec::new();
    for (offset, line) in all[from..].iter().enumerate() {
        if offset > 0
            && let Some(previous) = source.last()
        {
            source.push(Run {
                text: " ".to_owned(),
                style: previous.style.clone(),
                width: 0.0,
            });
        }
        source.extend(line.runs.iter().cloned());
        if line.hard_break {
            break;
        }
    }

    let mut runs: Vec<Run> = Vec::new();
    let mut used = 0.0_f32;
    let mut pending = false;
    let mut started = false;
    for run in source {
        if run.is_space() {
            pending = started;
            runs.push(run);
            continue;
        }
        let ahead = if pending { gap } else { 0.0 };
        if used + ahead + run.width <= budget {
            used += ahead + run.width;
            runs.push(run);
            pending = false;
            started = true;
            continue;
        }

        // The word does not fit whole: keep the characters that do, and stop.
        let mut kept = String::new();
        let mut kept_width = 0.0;
        for character in run.text.chars() {
            let mut candidate = kept.clone();
            candidate.push(character);
            let width = measurer.run_width(
                &run.style,
                metrics.letter_spacing,
                &candidate,
            );
            if used + ahead + width > budget {
                break;
            }
            kept = candidate;
            kept_width = width;
        }
        if !kept.is_empty() {
            runs.push(Run {
                text: kept,
                style: run.style,
                width: kept_width,
            });
        }
        break;
    }

    runs.push(Run {
        text: marker.to_owned(),
        style,
        width: marker_width,
    });

    // **A space before the marker is kept if it fits.** v1 strips trailing
    // whitespace here, reasoning that it pushes the marker away from the text
    // it belongs to; Chrome does not, because it keeps the longest prefix of
    // the string that fits and a space is part of the string. Measured:
    // `Flower of Paradise` at 22px in 90 is drawn as `Flower of …`, 89.98
    // wide, space and all.
    //
    // It only survives while it fits, which is the same rule and not a second
    // one: the gap a space stands for is arithmetic the marker's own width
    // competes with, so the line is measured with the marker on it and the
    // space goes if that is what does not fit.
    while runs.len() > 1
        && line_width(
            &Line {
                runs: runs.clone(),
                ascent: 0.0,
                content_height: 0.0,
                height: 0.0,
                hard_break: true,
            },
            space,
            metrics.word_spacing,
        ) > max_width
        && runs.get(runs.len() - 2).is_some_and(Run::is_space)
    {
        runs.remove(runs.len() - 2);
    }

    Line {
        runs,
        ascent: 0.0,
        content_height: 0.0,
        height: 0.0,
        hard_break: true,
    }
}

/// Fills in every line's three heights.
///
/// The ascent and descent come from the **face**, per run, taken as maxima
/// over the line's words; a line with no words takes the node's own face, so
/// an empty line between two paragraphs is as tall as a line of text.
pub fn measure_lines(
    measurer: &mut TextMeasurer,
    base: &ResolvedText,
    lines: &mut [Line],
    metrics: Metrics,
) {
    let base_style = RunStyle::base(base);
    for line in lines {
        let mut ascent = 0.0_f32;
        let mut descent = 0.0_f32;
        for run in line.runs.iter().filter(|run| !run.is_space()) {
            let face = measurer.measure(
                &run.style,
                metrics.letter_spacing,
                METRICS_STRING,
            );
            ascent = ascent.max(face.ascent);
            descent = descent.max(face.descent);
        }
        if ascent == 0.0 && descent == 0.0 {
            let face = measurer.measure(
                &base_style,
                metrics.letter_spacing,
                METRICS_STRING,
            );
            ascent = face.ascent;
            descent = face.descent;
        }
        line.ascent = ascent;
        line.content_height = ascent + descent;
        // What `line_height` asks for, **even when it is less than the face
        // needs**: CSS lets a tight line box overlap its neighbours rather
        // than quietly growing.
        line.height = metrics.line_height.unwrap_or(line.content_height);
    }
}

/// The width one line occupies, gaps included.
///
/// A gap is added for **a space run that is there**, not between every pair of
/// words: two runs can meet with nothing between them, which is what
/// `<b>a</b><b>b</b>` produces, and inserting a space there would draw one
/// that the text does not contain.
#[must_use]
pub fn line_width(line: &Line, space: f32, word_spacing: f32) -> f32 {
    let mut width = 0.0;
    let mut pending = false;
    let mut started = false;
    for run in &line.runs {
        if run.is_space() {
            pending = started;
            continue;
        }
        if pending {
            width += space + word_spacing;
        }
        width += run.width;
        pending = false;
        started = true;
    }
    width
}

/// Lays a node's text out at `max_width`, applying `max_lines`.
///
/// The lines beyond the limit are dropped here. Rebuilding the last one to
/// carry an ellipsis is a separate step, because it needs the lines that were
/// dropped and this returns only the ones that survive.
#[must_use]
pub fn layout(
    measurer: &mut TextMeasurer,
    base: &ResolvedText,
    segments: &[TextSegment],
    max_width: f32,
    paragraph: &ParagraphStyle,
    metrics: Metrics,
) -> Block {
    let all = wrap(measurer, base, segments, max_width, metrics);
    let mut lines = all.clone();
    let mut truncated = false;
    let marker = paragraph
        .ellipsis
        .as_deref()
        .filter(|marker| !marker.is_empty());
    if let Some(limit) = paragraph.max_lines.map(|lines| lines as usize)
        && lines.len() > limit
    {
        lines.truncate(limit);
        if let Some(marker) = marker
            && let Some(last) = limit.checked_sub(1)
        {
            lines[last] = truncate_with(
                measurer, base, &all, last, max_width, marker, metrics,
            );
            truncated = true;
        }
    }

    let base_style = RunStyle::base(base);
    let space = measurer.space_width(&base_style, metrics.letter_spacing);

    // **A line can overflow with no line after it.** Wrapping breaks at
    // spaces, so a word with no space in it is placed whole however wide it
    // is -- `Antidisestablishmentarianism` occupies 171.81 in a box of 90 --
    // and the truncation above never runs, because its trigger is the *line
    // count*. Chrome cuts such a line at the last **letter** that fits and
    // draws the marker after it: `Antidisestabli…`, 88.00 wide. Measured in
    // `crates/meo-canvas/tests/assets/chrome/ellipsis.tsv`.
    //
    // The rule is the one `truncate_with` already implements. Only the
    // trigger was missing.
    if let Some(marker) = marker
        && let Some(last) = lines.len().checked_sub(1)
        && line_width(&lines[last], space, metrics.word_spacing) > max_width
    {
        lines[last] = truncate_with(
            measurer, base, &all, last, max_width, marker, metrics,
        );
        truncated = true;
    }

    measure_lines(measurer, base, &mut lines, metrics);
    let width = lines
        .iter()
        .map(|line| line_width(line, space, metrics.word_spacing))
        .fold(0.0_f32, f32::max);
    let boxes: f32 = lines.iter().map(|line| line.height).sum();
    let gaps = metrics.line_gap * (lines.len().saturating_sub(1)) as f32;

    Block {
        lines,
        width,
        height: boxes + gaps,
        wrapped_lines: all.len(),
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::style::text::{
        LineHeight, ParagraphStyle, Spacing, TextSegment, TextStyle,
    };
    use meo_skia_canvas::TextEngine;

    use super::{
        Line, METRICS_STRING, Metrics, Run, RunStyle, TextMeasurer, layout,
        line_width, pieces, wrap,
    };
    use crate::{
        measure::build_paragraph,
        resolve::{
            ResolvedText,
            tests::{TEST_FAMILY, test_fonts},
        },
    };

    /// A resolved style in the repository's own face.
    fn style() -> ResolvedText {
        let _fonts = test_fonts();
        ResolvedText {
            family: TEST_FAMILY.to_owned(),
            size: 16.0,
            ..ResolvedText::initial()
        }
    }

    fn plain(text: &str) -> Vec<TextSegment> {
        vec![TextSegment {
            text: text.to_owned(),
            style: TextStyle::default(),
        }]
    }

    /// One scene the two line-box models are both asked about.
    struct Case {
        /// What the table calls it.
        name: &'static str,
        /// The text, newlines included.
        text: &'static str,
        /// The width both models lay it out at.
        width: f32,
        /// The em size.
        size: f32,
        /// The line height as a multiple of the size; `1.0` is the face's own.
        line_height: f32,
        /// Letter spacing in pixels.
        letter_spacing: f32,
    }

    /// The scenes worth asking about, each isolating one decision.
    fn comparison_cases() -> Vec<Case> {
        let case =
            |name, text, width, size, line_height, letter_spacing| Case {
                name,
                text,
                width,
                size,
                line_height,
                letter_spacing,
            };
        vec![
            case("one word", "Hxgp", 400.0, 16.0, 1.0, 0.0),
            case("one line", "Hxgp quick brown", 400.0, 16.0, 1.0, 0.0),
            case("wraps once", "Hxgp quick brown fox", 90.0, 16.0, 1.0, 0.0),
            case(
                "wraps often",
                "Hxgp quick brown fox jumps over the lazy dog",
                70.0,
                16.0,
                1.0,
                0.0,
            ),
            case("hard break", "Hxgp\nquick", 400.0, 16.0, 1.0, 0.0),
            case("blank line", "Hxgp\n\nquick", 400.0, 16.0, 1.0, 0.0),
            case("long word alone", "Hxgpquickbrown", 60.0, 16.0, 1.0, 0.0),
            case("long word after", "a Hxgpquickbrown", 60.0, 16.0, 1.0, 0.0),
            case("tall line box", "Hxgp quick brown", 90.0, 16.0, 2.0, 0.0),
            case("tight line box", "Hxgp quick brown", 90.0, 16.0, 0.5, 0.0),
            case("letter spacing", "Hxgp quick brown", 90.0, 16.0, 1.0, 2.0),
            case("large", "Hxgp quick brown", 200.0, 34.0, 1.0, 0.0),
        ]
    }

    /// Prints this crate's line boxes beside the paragraph's, for every case.
    ///
    /// **Ignored on purpose: the output is a table for a person to read rather
    /// than an assertion.** The first round of the text port is two
    /// independent statements of one layout with their disagreements
    /// enumerated; deciding which of the two is right, case by case, is what
    /// the browser measurements are for. An assertion written before those
    /// land would pin whichever model happened to be written second, which is
    /// the failure this whole comparison exists to avoid.
    ///
    /// `cargo test -p meo-canvas-core --lib -- --ignored --nocapture line_box`
    ///
    /// On stderr, because `print_stdout` is denied outside the binary whose
    /// stdout is its deliverable -- the same reason the fixture harness
    /// reports there.
    #[test]
    #[ignore = "prints a comparison table rather than asserting one"]
    fn report_line_box_disagreements() {
        let fonts = test_fonts();
        let engine = TextEngine::new(fonts.library());
        let mut measurer = TextMeasurer::new();

        eprintln!(
            "case\tmodel\tlines\tline\twidth\tascent\tdescent\tbaseline\tbox\ttext"
        );
        for case in comparison_cases() {
            let base = ResolvedText {
                family: TEST_FAMILY.to_owned(),
                size: case.size,
                line_height: Some(LineHeight::Number(case.line_height)),
                letter_spacing: Spacing::Points(case.letter_spacing),
                ..ResolvedText::initial()
            };
            let segments = plain(case.text);
            let metrics = Metrics::of(&base);
            let space = measurer
                .space_width(&RunStyle::base(&base), metrics.letter_spacing);

            let ours = layout(
                &mut measurer,
                &base,
                &segments,
                case.width,
                &ParagraphStyle::default(),
                metrics,
            );
            for (index, line) in ours.lines.iter().enumerate() {
                let text: String =
                    line.runs.iter().map(|run| run.text.as_str()).collect();
                eprintln!(
                    "{}\tlines\t{}\t{index}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{text:?}",
                    case.name,
                    ours.lines.len(),
                    line_width(line, space, metrics.word_spacing),
                    line.ascent,
                    line.content_height - line.ascent,
                    line.baseline_from_top(),
                    line.height,
                );
            }

            let mut paragraph = build_paragraph(
                &engine,
                &base,
                &segments,
                &ParagraphStyle::default(),
                &[],
            );
            paragraph.layout(case.width);
            let skia = paragraph.line_metrics();
            for line in &skia {
                let text = case
                    .text
                    .get(line.start_index..line.end_excluding_whitespaces)
                    .unwrap_or("<not a boundary>");
                eprintln!(
                    "{}\tskia\t{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{text:?}",
                    case.name,
                    skia.len(),
                    line.line_number,
                    line.width,
                    line.ascent,
                    line.descent,
                    line.baseline,
                    line.ascent + line.descent,
                );
            }
        }
    }

    #[test]
    fn a_newline_is_its_own_piece_and_a_run_of_spaces_is_one() {
        assert_eq!(pieces("a b"), vec!["a", " ", "b"]);
        assert_eq!(pieces("a  b"), vec!["a", "  ", "b"]);
        // Two newlines are two breaks, which is what makes an empty line
        // between paragraphs possible.
        assert_eq!(pieces("a\n\nb"), vec!["a", "\n", "\n", "b"]);
        assert_eq!(pieces("a \n b"), vec!["a", " ", "\n", " ", "b"]);
    }

    #[test]
    fn the_cache_answers_the_same_question_once() {
        let mut measurer = TextMeasurer::new();
        let base = RunStyle::base(&style());
        let first = measurer.measure(&base, 0.0, METRICS_STRING);
        let cached = measurer.cached();
        let again = measurer.measure(&base, 0.0, METRICS_STRING);
        assert_eq!(first, again);
        assert_eq!(measurer.cached(), cached);
        // A different spacing is a different width for the same string, so it
        // has to be a different entry rather than a hit.
        let _ = measurer.measure(&base, 2.0, METRICS_STRING);
        assert_eq!(measurer.cached(), cached + 1);
    }

    #[test]
    fn a_font_variant_changes_the_width_it_is_measured_at() {
        use meo_canvas_scene::style::text::FontVariant;

        let mut measurer = TextMeasurer::new();
        let base = style();
        let plain = RunStyle::base(&base);
        let mut fractions = plain.clone();
        fractions.variant = vec![FontVariant::DiagonalFractions];

        // **A fraction, because that is the one feature this face answers
        // to.** Measured across seventeen OpenType tags on the repository's
        // own Oswald: `frac` moves a nineteen-character sample from 220.61 to
        // 211.04 and every other tag moves nothing, `smcp` included -- the
        // face has no small-caps glyphs and nothing synthesises them. A test
        // written with `SmallCaps` would report this property as dead however
        // well it worked.
        let sample = "about 1/2 of it";
        let without = measurer.run_width(&plain, 0.0, sample);
        let with = measurer.run_width(&fractions, 0.0, sample);
        assert!(
            (without - with).abs() > 0.5,
            "the feature did not reach the measurement: {without} against \
             {with}"
        );

        // And the two are separate cache entries rather than one answer
        // served twice, which is the way this would fail silently.
        assert_eq!(measurer.cached(), 2);
    }

    #[test]
    fn a_run_gains_exactly_one_letter_spacing() {
        let mut measurer = TextMeasurer::new();
        let base = RunStyle::base(&style());
        let plain = measurer.run_width(&base, 0.0, "Hxgp");
        let spaced = measurer.run_width(&base, 2.0, "Hxgp");
        let measured = measurer.measure(&base, 2.0, "Hxgp").width;
        // One unit on top of what the backend reports, whatever the backend
        // put between the characters.
        assert!((spaced - measured - 2.0).abs() < 0.001);
        assert!(spaced > plain);
    }

    #[test]
    fn a_newline_breaks_a_line_that_would_have_fitted() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let metrics = Metrics::of(&base);
        let lines = wrap(&mut measurer, &base, &plain("a\nb"), 1000.0, metrics);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].hard_break);
        assert!(!lines[1].hard_break);
    }

    #[test]
    fn two_newlines_leave_an_empty_line_that_is_still_a_line_box() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let metrics = Metrics::of(&base);
        let block = layout(
            &mut measurer,
            &base,
            &plain("a\n\nb"),
            1000.0,
            &ParagraphStyle::default(),
            metrics,
        );
        assert_eq!(block.lines.len(), 3);
        assert!(block.lines[1].runs.is_empty());
        // The empty line takes the node's own face rather than nothing, which
        // is what keeps a blank line as tall as a written one.
        assert!(block.lines[1].height > 0.0);
        assert!((block.lines[1].height - block.lines[0].height).abs() < 0.001);
    }

    #[test]
    fn the_first_word_is_placed_however_wide_it_is() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let metrics = Metrics::of(&base);
        // Narrow enough that nothing fits, and wide enough that breaking
        // between characters is not what happens: the word is one character.
        let lines = wrap(&mut measurer, &base, &plain("M M"), 1.0, metrics);
        assert!(lines.iter().all(|line| !line.runs.is_empty()));
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn a_word_too_wide_for_the_line_overflows_it_whole() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let metrics = Metrics::of(&base);

        // Alone: one line, wider than the box it was given.
        let alone =
            wrap(&mut measurer, &base, &plain("Hxgpquick"), 30.0, metrics);
        assert_eq!(alone.len(), 1);
        assert_eq!(alone[0].runs.len(), 1);
        assert!(alone[0].runs[0].width > 30.0);

        // After a short word: two lines, the second still whole. Measured in
        // Chrome, where a 278px word in a 100px box gives lines of 6.8 and
        // 278.1 -- the long one is never broken, only moved.
        let after =
            wrap(&mut measurer, &base, &plain("a Hxgpquick"), 30.0, metrics);
        assert_eq!(after.len(), 2);
        assert_eq!(after[1].runs.len(), 1);
        assert_eq!(after[1].runs[0].text, "Hxgpquick");
    }

    #[test]
    fn breaking_a_word_keeps_every_character_in_order() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        // The arithmetic `overflow-wrap: break-word` will need. Nothing calls
        // it, so this is the only thing keeping it honest until the property
        // exists.
        let parts = super::break_word(
            &mut measurer,
            &RunStyle::base(&base),
            "Hxgpquick",
            30.0,
            0.0,
        );
        assert!(parts.len() > 1);
        let rebuilt: String =
            parts.iter().map(|part| part.text.as_str()).collect();
        assert_eq!(rebuilt, "Hxgpquick");
        // Every piece but possibly a single over-wide character fits.
        assert!(
            parts.iter().all(
                |part| part.width <= 30.0 || part.text.chars().count() == 1
            )
        );
    }

    #[test]
    fn the_strut_is_whole_pixels_because_chrome_rounds_it() {
        let mut measurer = TextMeasurer::new();
        let base = RunStyle::base(&style());
        let face = measurer.measure(&base, 0.0, METRICS_STRING);
        assert!((face.ascent - face.ascent.round()).abs() < f32::EPSILON);
        assert!((face.descent - face.descent.round()).abs() < f32::EPSILON);
        // The identity that matters more than either value: Chrome's `normal`
        // line box is exactly the sum, and an unrounded pair misses it by
        // 0.288 every line.
        let base_style = style();
        let block = layout(
            &mut measurer,
            &base_style,
            &plain("Hxgp"),
            1000.0,
            &ParagraphStyle::default(),
            Metrics::of(&base_style),
        );
        assert!(
            (block.lines[0].height - (face.ascent + face.descent)).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn a_tight_line_height_gives_a_negative_leading() {
        let mut measurer = TextMeasurer::new();
        let mut base = style();
        // Half the face's own, so the box is shorter than the content.
        base.line_height = Some(LineHeight::Number(0.5));
        let metrics = Metrics::of(&base);
        let block = layout(
            &mut measurer,
            &base,
            &plain("Hxgp"),
            1000.0,
            &ParagraphStyle::default(),
            metrics,
        );
        let line = &block.lines[0];
        assert!(line.height < line.content_height);
        // The baseline moves *up* past the ascent rather than staying inside
        // the box, which is the case CSS lets the glyphs overlap.
        assert!(line.baseline_from_top() < line.ascent);
    }

    #[test]
    fn the_gap_falls_between_lines_and_not_after_the_last() {
        let mut measurer = TextMeasurer::new();
        let mut base = style();
        base.line_gap = 10.0;
        let metrics = Metrics::of(&base);
        let one = layout(
            &mut measurer,
            &base,
            &plain("a"),
            1000.0,
            &ParagraphStyle::default(),
            metrics,
        );
        let two = layout(
            &mut measurer,
            &base,
            &plain("a\nb"),
            1000.0,
            &ParagraphStyle::default(),
            metrics,
        );
        assert!(one.height.mul_add(-2.0, two.height - 10.0).abs() < 0.001);
    }

    /// What `wrapped_lines` and `truncated` report, in each of the four shapes.
    ///
    /// **They exist because `lines.len()` cannot answer either question after
    /// the fact**, and the rescue in [`crate::measure`] needs both: a box
    /// rounded down from the text's own width breaks a paragraph that fitted,
    /// and whether that shows as a dropped line or as a marker on a single line
    /// depends only on whether the text has a space in it.
    ///
    /// The third row is the one worth having. A word with no break opportunity
    /// never raises the line count, so it reaches the marker through the
    /// overflow trigger rather than the `max_lines` one -- and a fix reading
    /// only the line count repaired the spaced case and left this one broken.
    #[test]
    fn a_block_reports_what_the_wrap_did_before_max_lines_touched_it() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let metrics = Metrics::of(&base);
        let clamp = |lines| ParagraphStyle {
            max_lines: Some(lines),
            ellipsis: Some("\u{2026}".to_owned()),
        };

        // Neither wrapped nor truncated: the plain case, and the row that makes
        // the others mean something.
        let whole = layout(
            &mut measurer,
            &base,
            &plain("a b"),
            1000.0,
            &ParagraphStyle::default(),
            metrics,
        );
        assert_eq!(whole.wrapped_lines, 1);
        assert!(!whole.truncated);

        // Wrapped, nothing dropped: `wrapped_lines` counts what the wrap did
        // even when `max_lines` is absent.
        let wrapped = layout(
            &mut measurer,
            &base,
            &plain("Hxgp quick brown fox"),
            90.0,
            &ParagraphStyle::default(),
            metrics,
        );
        assert!(wrapped.wrapped_lines > 1, "the text was meant to wrap");
        assert!(!wrapped.truncated);

        // Wrapped and truncated: one line survives, and `lines.len()` is now 1
        // -- which is exactly the reading that made the rescue skip this case.
        let clamped = layout(
            &mut measurer,
            &base,
            &plain("Hxgp quick brown fox"),
            90.0,
            &clamp(1),
            metrics,
        );
        assert_eq!(clamped.lines.len(), 1);
        assert!(clamped.wrapped_lines > 1, "the wrap still broke it");
        assert!(clamped.truncated);

        // A single word too wide to break: the marker arrives through the
        // overflow trigger, and the line count never rises at all.
        let unbreakable = layout(
            &mut measurer,
            &base,
            &plain("Hxgpquickbrownfox"),
            40.0,
            &clamp(1),
            metrics,
        );
        assert_eq!(unbreakable.wrapped_lines, 1, "there is nowhere to break");
        assert!(
            unbreakable.truncated,
            "a line too wide for its box is truncated even though nothing wrapped"
        );
    }

    #[test]
    fn max_lines_drops_the_lines_beyond_it() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let metrics = Metrics::of(&base);
        let block = layout(
            &mut measurer,
            &base,
            &plain("a\nb\nc"),
            1000.0,
            &ParagraphStyle {
                max_lines: Some(2),
                ellipsis: None,
            },
            metrics,
        );
        assert_eq!(block.lines.len(), 2);
    }

    /// The text of a line, with a space wherever a space run sits.
    ///
    /// The runs carry the space as a run of no width -- the gap is arithmetic
    /// -- so joining their texts alone would run two words together.
    fn read(line: &Line) -> String {
        line.runs
            .iter()
            .map(|run| {
                if run.is_space() {
                    " "
                } else {
                    run.text.as_str()
                }
            })
            .collect()
    }

    #[test]
    fn the_last_line_is_rebuilt_around_the_ellipsis() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let metrics = Metrics::of(&base);
        let text = "Flower of Paradise Lost";
        // Oswald is a narrow face, so the width that makes this overflow is
        // smaller than the 140px a browser needs for the same words.
        let width = 60.0;

        let wrapped = wrap(&mut measurer, &base, &plain(text), width, metrics);
        assert!(wrapped.len() > 1, "the scene has to overflow one line");

        let block = layout(
            &mut measurer,
            &base,
            &plain(text),
            width,
            &ParagraphStyle {
                max_lines: Some(1),
                ellipsis: Some("\u{2026}".to_owned()),
            },
            metrics,
        );
        assert_eq!(block.lines.len(), 1);
        let last = read(&block.lines[0]);
        assert!(
            last.ends_with('\u{2026}'),
            "the marker is missing: {last:?}"
        );

        // **The point of the rebuild.** Wrapping breaks at words; truncation
        // does not. The line is rebuilt against a budget the marker has
        // already been taken out of, and it takes as many *characters* as fit
        // -- pulling text up from the lines `max_lines` discards when there is
        // room, and stopping mid-word when there is not. Here it stops
        // mid-word: `Flower o…` rather than the wrap's own `Flower of`, whose
        // marker would not have fitted beside it.
        //
        // Either way the line is not the wrap's line with a marker stuck on,
        // and that is the difference a paragraph-level line limit cannot
        // express.
        let kept = last.trim_end_matches('\u{2026}');
        assert_ne!(
            kept,
            read(&wrapped[0]),
            "the last line was left as the wrap made it and the marker \
             appended: {last:?}"
        );
        assert!(
            text.starts_with(kept),
            "the rebuilt line is not a prefix of the text: {kept:?}"
        );

        // And it must still fit: the marker's own width comes out of the
        // budget before a character is kept.
        let space = measurer.space_width(&RunStyle::base(&base), 0.0);
        assert!(line_width(&block.lines[0], space, 0.0) <= width);
    }

    #[test]
    fn the_rebuild_stops_at_a_newline_the_caller_wrote() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let metrics = Metrics::of(&base);
        // Two paragraphs, one visible line. Everything that could fill it
        // sits past a hard break -- a break the caller asked for rather than
        // one wrapping introduced -- so nothing may be pulled across it, and
        // the marker goes on the short line as it stands.
        let block = layout(
            &mut measurer,
            &base,
            &plain("aa\nbb cc dd ee ff"),
            80.0,
            &ParagraphStyle {
                max_lines: Some(1),
                ellipsis: Some("\u{2026}".to_owned()),
            },
            metrics,
        );
        let last = read(&block.lines[0]);
        assert_eq!(
            last, "aa\u{2026}",
            "text was pulled across a newline the caller wrote"
        );
    }

    #[test]
    fn a_space_run_carries_no_width_and_a_word_run_does() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let metrics = Metrics::of(&base);
        let lines = wrap(&mut measurer, &base, &plain("a b"), 1000.0, metrics);
        let runs = &lines[0].runs;
        assert_eq!(runs.len(), 3);
        assert!(runs[1].is_space());
        assert!(runs[1].width.abs() < f32::EPSILON);
        assert!(runs[0].width > 0.0);
    }

    #[test]
    fn a_line_of_only_spaces_is_a_line_with_no_runs() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let metrics = Metrics::of(&base);
        // Trailing whitespace is dropped when the line finalises, so this is
        // one empty line rather than one holding a space.
        let lines = wrap(&mut measurer, &base, &plain("   "), 1000.0, metrics);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].runs.is_empty());
    }

    #[test]
    fn a_run_and_a_line_report_what_they_hold() {
        let run = Run {
            text: " ".to_owned(),
            style: RunStyle::base(&style()),
            width: 0.0,
        };
        assert!(run.is_space());
        let line = Line {
            runs: vec![run],
            ascent: 10.0,
            content_height: 12.0,
            height: 20.0,
            hard_break: false,
        };
        // Four of leading, two above, then the ascent.
        assert!((line.baseline_from_top() - 14.0).abs() < f32::EPSILON);
    }
}
