//! Line boxes, wrapping and truncation, computed here rather than by Skia's
//! paragraph.
//!
//! # Why this exists
//!
//! A canvas has no paragraph, so v9 draws text with `fillText` and computes
//! everything around it by hand: where the lines break, how tall a line box is,
//! where its baseline sits, which characters survive a truncation and where the
//! ellipsis goes. This renderer does the same, because a paragraph decides
//! several of those in ways nothing here can reach.
//!
//! This module is the arithmetic. Nothing in it draws.
//!
//! # What a line box is
//!
//! A line's **content height** is the face's own ascent plus descent, taken as
//! the maximum over the runs on that line. Its **box height** is what
//! `line_height` asks for. The difference is **leading**, split half above and
//! half below -- and it is allowed to be negative: a `line_height` tighter than
//! the face needs makes the glyphs overlap their neighbours rather than moving
//! the baseline, which is what CSS says. So the baseline of a line is
//! `top + floor(leading / 2) + ascent`, floored as Chrome floors it.
//!
//! When `line_height` asks for nothing, which is CSS's `normal`, each face on
//! the line asks for its own ascent, descent and line gap, with the gap as its
//! leading: `floor(gap / 2) + ascent` above the baseline and the rest of the
//! gap plus the descent below it. The line box is the largest of each, which is
//! what Chrome builds when faces with different gaps share a line, and it is
//! also the content height, so the leading is zero and the baseline sits at the
//! largest reach above.
//!
//! Every one of those numbers comes from the **face** rather than from the
//! string. That is why [`METRICS_STRING`] exists and why its contents do not
//! matter.
//!
//! # Spaces are runs of no width
//!
//! A space between two words is kept as a run carrying a width of zero, and the
//! gap is added by the arithmetic instead: `space_width + word_spacing`. So
//! justification and word spacing have one place to change the gap, rather than
//! reaching inside a measured run.
//!
//! # Where a line breaks
//!
//! At Chrome's break opportunities, which are not only spaces, found over the
//! paragraph's whole text so a segment boundary is never one in itself. Blink's
//! line breaker takes three rules in order, and so does this module:
//!
//! 1. **After a run of spaces, always**, and never before one, so a space hangs
//!    at the end of the line it ends. `( a` and `a )` both break at the space,
//!    where UAX 14 keeps an opening bracket with what follows it and a closing
//!    one with what precedes it.
//! 2. **Between two printable ASCII characters, a table of Blink's own**: a
//!    break after `-` and `?`, and before `(`, `<`, `[` and `{` after closing
//!    and other punctuation. `a/b` and `C++` do not break, where UAX 14 breaks
//!    both. A hyphen before a digit breaks only after a letter or digit, so
//!    `x86-64` breaks and `-5` does not.
//! 3. **Anywhere else, UAX 14**, version 17, as ICU computes it: between two
//!    ideographs or kana, never before a closing mark such as `。` or `」` or
//!    after an opening one such as `「`, with `line-break: normal`'s tailoring,
//!    where small kana and the prolonged sound mark (`ぁ`, `ッ`, `ー`) may
//!    begin a line, which `strict` forbids. Thai, Lao, Khmer and Burmese break
//!    by dictionary, as ICU breaks them.
//!
//! A soft hyphen is not an opportunity, since nothing here draws the hyphen
//! Chrome adds there. `chrome_line_break.rs` holds every pair of printable
//! ASCII characters, every pair of a set from each class in running text, and
//! wrapped strings against Chrome, and names the few rows that differ.
//!
//! What lies between two opportunities is placed whole. Two pieces with an
//! opportunity and no space between them meet with no gap, and the kerning
//! between them is kept: in 16px Noto Sans SC, `アイ` is 0.8px narrower than
//! `ア` and `イ` apart.

use std::collections::{HashMap, VecDeque};

use meo_canvas_scene::style::{
    paint::Color,
    text::{
        FontStyle, FontVariant, LineHeight, ParagraphStyle, Spacing,
        TextDecoration, TextSegment,
    },
};
use meo_skia_canvas::{
    Canvas, CanvasOptions, Font, FontFeature, FontSlant, FontStretch,
    FontVariantCaps,
};

use crate::{FINITE_CEILING, resolve::ResolvedText};

mod breaks;

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
/// v9's own string, kept character for character so that a face reporting
/// something odd reports it identically in both.
pub const METRICS_STRING: &str = "Ag|``";

/// Entries the measurement cache holds before the oldest is dropped: v9's
/// number, sized for the strings one render draws, a card's labels, its words
/// and a truncation's per-character measurements.
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
/// Snapshotted rather than held as a backend object: the width describes the
/// string, and the ascent and descent describe the face, which is what a line
/// box is built from.
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

/// A measurement with the face's own line gap beside it.
///
/// Kept apart from [`Measurement`], whose fields are public, so the gap reaches
/// a line box without changing what a measurement is to a caller.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Answer {
    /// What [`TextMeasurer::measure`] returns.
    measurement: Measurement,
    /// The line gap `line-height: normal` adds, rounded apart from the ascent
    /// and descent as Chrome rounds it: 13px Poppins has 13.65, 4.55 and 1.3,
    /// a 20px box from 14 + 5 + 1 where the sum rounds to 19.
    line_gap: f32,
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
    /// Here rather than on the node, because a run is what carries it: a
    /// segment declaring a colour changes its own ink and nothing else,
    /// measured against Chrome 151, where the paragraph's height and the
    /// span's box are unmoved.
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
            // Through `usable`, as `line_gap` is: an infinite size would reach
            // the measurement and collapse the box around the paragraph, and a
            // `NaN` one, refused at the writer, is neutralised here. `1e30`
            // measures, and the canvas clips it.
            size: usable(style.font_size.unwrap_or(base.size)),
            weight: style.font_weight.unwrap_or(base.weight).get(),
            italic: matches!(
                style.font_style.unwrap_or(base.style),
                FontStyle::Italic
            ),
            variant: style
                .font_variant
                .clone()
                .unwrap_or_else(|| base.font_variant.clone()),
            color: style
                .color
                .map_or(base.color, |color| color.resolve(base.color)),
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
            size: usable(base.size),
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
            families: vec![self.family.as_str().into()],
            size: self.size,
            weight: self.weight,
            // 0.16 replaces the boolean with the three slants CSS has. The
            // boolean chose between upright and the family's italic, which is
            // what these two arms say; `Oblique` is the third and nothing here
            // asks for it, so no face selection moves.
            slant: if self.italic {
                FontSlant::Italic
            } else {
                FontSlant::Normal
            },
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
/// string one character at a time. Measured on v9 with a twenty-four line card,
/// **720 calls resolved to 58 distinct questions**.
///
/// # Why there is no epoch
///
/// This cache lives for one render: it is built with the measurer and dropped
/// with it, so a font registered between renders cannot be seen by a cache that
/// no longer exists. A process-wide cache would need a counter to invalidate it
/// on every registration, since `12px Roboto` measures differently before and
/// after Roboto exists.
pub struct TextMeasurer {
    /// A one-pixel canvas, which is where a measuring context comes from. The
    /// font library is the thread's, so this sees every face registered on it.
    canvas: Canvas,
    /// Answers, keyed by the font and the string.
    answers: HashMap<(FontKey, u32, String), Answer>,
    /// Keys in the order they were inserted, so the oldest can be dropped:
    /// first in, first out, which decides only which entry survives a full
    /// cache and not what any of them say.
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
        self.answer(style, letter_spacing, text).measurement
    }

    /// [`Self::measure`], with the face's line gap, through the same cache.
    fn answer(
        &mut self,
        style: &RunStyle,
        letter_spacing: f32,
        text: &str,
    ) -> Answer {
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
        let face = context.font_metrics();
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
        let answer = Answer {
            measurement,
            line_gap: face.line_gap.round(),
        };

        if self.answers.len() >= MEASUREMENT_LIMIT
            && let Some(oldest) = self.order.pop_front()
        {
            self.answers.remove(&oldest);
        }
        self.order.push_back(key.clone());
        self.answers.insert(key, answer);
        answer
    }

    /// The width of a run, which is what the backend measures and nothing more.
    ///
    /// CSS adds one letter-spacing unit after every character including the
    /// last, so an `n`-character run is `n` spacings wide, and
    /// meo-skia-canvas's `set_letter_spacing` adds exactly that. No correction
    /// is applied here: one written against a backend's arithmetic is right
    /// only while that arithmetic holds, and `chrome_text_truth` is what
    /// catches it moving.
    pub fn run_width(
        &mut self,
        style: &RunStyle,
        letter_spacing: f32,
        text: &str,
    ) -> f32 {
        self.measure(style, letter_spacing, text).width
    }

    /// The width of one inter-word space, spacing included.
    ///
    /// Measured in the node's **base** font rather than in any run's: the gap
    /// between two differently-styled words is one gap, and taking it from
    /// whichever run precedes it would make a line's width depend on the order
    /// its styles appear in.
    ///
    /// The spacing is the backend's, as in `run_width`, so a measured space
    /// already carries its unit. The fallback adds its own: a face that reports
    /// **no** space width gives a measurement the backend never made, with no
    /// spacing in it.
    pub fn space_width(&mut self, base: &RunStyle, letter_spacing: f32) -> f32 {
        let measured = self.measure(base, letter_spacing, " ").width;
        if measured > 0.0 {
            measured
        } else {
            base.size.mul_add(FALLBACK_SPACE, letter_spacing)
        }
    }

    /// The kerning between `left` and the first character of `right`: what
    /// the two set together lose against each set alone. Letter spacing is in
    /// all three measurements, so it cancels.
    fn kerning(&mut self, style: &RunStyle, left: char, right: &str) -> f32 {
        let Some(first) = right.chars().next() else {
            return 0.0;
        };
        let mut pair = String::with_capacity(8);
        pair.push(left);
        let alone = self.run_width(style, style.letter_spacing, &pair);
        pair.push(first);
        let together = self.run_width(style, style.letter_spacing, &pair);
        let second = self.run_width(
            style,
            style.letter_spacing,
            &right[..first.len_utf8()],
        );
        together - alone - second
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
    /// Whether this run is inter-word whitespace rather than a word: spaces,
    /// tabs, carriage returns and no-break spaces, which are drawn as a gap.
    /// Other Unicode spaces are text, measured in their face, as Chrome sets
    /// them: an ideographic space is an em wide, and one that begins a
    /// paragraph is drawn rather than dropped.
    #[must_use]
    pub fn is_space(&self) -> bool {
        !self.text.is_empty() && self.text.chars().all(is_gap)
    }
}

/// Whether `character` is drawn as an inter-word gap rather than measured:
/// the white space `white-space: normal` collapses, and the no-break space,
/// which `word-spacing` widens as it widens a space.
const fn is_gap(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\r' | '\u{a0}')
}

/// One line box: its runs, and the three heights that place it.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The runs on the line, in order, spaces included.
    pub runs: Vec<Run>,
    /// How far the baseline sits below the top of the content area.
    ///
    /// For a stated `line_height`, the largest ascent among the line's faces.
    /// At `normal`, the most any face reaches above the baseline: its ascent
    /// and the upper half of its line gap, floored. That is not the largest
    /// ascent plus the largest half gap: 25px Oswald beside 25px Poppins
    /// reaches 30, from Oswald's ascent, where those give 31.
    pub ascent: f32,
    /// The height the leading is split around.
    ///
    /// For a stated `line_height`, the largest ascent plus the largest
    /// descent. At `normal`, the box itself: the most any face reaches above
    /// the baseline plus the most any reaches below it, each with its share
    /// of its line gap, so the leading is zero.
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
    /// Half the leading, floored, then the ascent. **The leading goes
    /// negative** for a line box shorter than the face needs, which is the
    /// case where CSS lets the glyphs escape the box rather than moving the
    /// baseline inside it. At `normal` the leading is zero and this is the
    /// ascent.
    ///
    /// Floored as Chrome floors it, for either sign and for a box that is not
    /// a whole pixel: Oswald at 13px and `line-height: 1.5` is a 19.5px box
    /// over 20px of content, and Chrome puts the baseline at 16 + floor(-0.25),
    /// 15, where half the leading unfloored gives 15.75. Pinned by every row
    /// of `line-height-normal.tsv` in `tests/chrome_line_height.rs`.
    #[must_use]
    pub fn baseline_from_top(&self) -> f32 {
        ((self.height - self.content_height) / 2.0).floor() + self.ascent
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
    /// `lines.len()` cannot answer this: a paragraph with `max_lines: 1` that
    /// broke into two comes back as one line carrying a marker, which is
    /// indistinguishable by any later test from a paragraph that never broke.
    ///
    /// Recorded here, where the untruncated wrap is still in hand, rather than
    /// reconstructed afterwards by wrapping a second time.
    pub wrapped_lines: usize,
    /// Whether a marker replaced any of the text.
    ///
    /// Both triggers set it, and they are different failures: `max_lines`
    /// dropping lines, and a single line too wide to break -- an unbreakable
    /// word does not raise `wrapped_lines` above one, so that field alone
    /// cannot see the second.
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
            // The one place a stated height becomes pixels: a length already
            // is, a number multiplies this element's size, and `None` is
            // `normal`. `> 0.0` refuses `NaN` and negatives but not infinity,
            // so `usable` checks each product.
            line_height: match base.line_height {
                Some(LineHeight::Number(multiple)) if multiple > 0.0 => {
                    Some(usable(multiple * usable(base.size)))
                }
                Some(LineHeight::Length(pixels)) if pixels > 0.0 => {
                    Some(usable(pixels))
                }
                Some(LineHeight::Percent(share)) => {
                    // Resolution turns every percentage into a length, so one
                    // here is a scene that skipped it: treated as the length
                    // resolution would have produced.
                    (share > 0.0).then_some(usable(share * usable(base.size)))
                }
                _ => None,
            },
            // Through `usable`, as the spacing fields are: a non-finite gap
            // times the line count is a non-finite height, and a
            // shrink-wrapping box lays out at 0 with only the first line drawn.
            // `line-gap` is ours, not CSS's.
            line_gap: usable(base.line_gap),
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
    let pixels = match spacing {
        Spacing::Normal => 0.0,
        Spacing::Points(points) => points,
        Spacing::Em(em) => em * font_size,
        // `Spacing` is `#[non_exhaustive]`; an unknown spelling adds nothing,
        // which is what `Normal` already means.
        _ => 0.0,
    };
    usable(pixels)
}

/// A resolved spacing no layout can use, replaced: a non-finite one poisons the
/// measurement and a shrink-wrapping box lays out at zero. Finite values pass
/// unchanged, `1e30` included. Infinity clamps, as `calc` does, and `NaN` is
/// zero, `normal`, as Chrome 151 renders `calc(0/0 * 1px)`.
const fn usable(pixels: f32) -> f32 {
    if pixels.is_nan() {
        return 0.0;
    }
    if pixels.is_infinite() {
        return FINITE_CEILING.copysign(pixels);
    }
    pixels
}

/// One piece of a paragraph, as the wrap places it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece<'text> {
    /// A newline the caller wrote, which ends the line whatever fits.
    Newline,
    /// A run of the characters [`is_gap`] names, collapsed into one gap as
    /// `white-space: normal` collapses it.
    Space(&'text str),
    /// Characters with no break opportunity inside. `opens` says whether a
    /// line may begin with it, which is a property of the paragraph's text
    /// around it and not of the piece alone.
    Word {
        /// The characters.
        text: &'text str,
        /// Whether a break opportunity precedes it.
        opens: bool,
    },
}

/// Splits a segment into the pieces the wrap places: newlines, gaps, and
/// words cut at the paragraph's opportunities `breaks`, `from` being the
/// segment's offset in the paragraph. Its edges are not opportunities in
/// themselves: `<b>a</b>b` is one word in two styles.
fn pieces<'text>(
    text: &'text str,
    from: usize,
    breaks: &[usize],
) -> Vec<Piece<'text>> {
    let opens = |at: usize| breaks.binary_search(&(from + at)).is_ok();
    let mut out = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let rest = &text[start..];
        let kind = |c: char| {
            if c == '\n' {
                0
            } else if is_gap(c) {
                1
            } else {
                2
            }
        };
        let first = rest.chars().next().map_or(2, kind);
        // A newline is always alone, so two of them are two breaks.
        let end = if first == 0 {
            1
        } else {
            rest.char_indices()
                .find(|(_, c)| kind(*c) != first)
                .map_or(rest.len(), |(at, _)| at)
        };
        match first {
            0 => out.push(Piece::Newline),
            1 => out.push(Piece::Space(&rest[..end])),
            _ => {
                let mut cut = start;
                let stop = start + end;
                let inside = breaks
                    .iter()
                    .filter(|&&offset| {
                        offset > from + start && offset < from + stop
                    })
                    .map(|offset| offset - from);
                for at in inside.chain([stop]) {
                    out.push(Piece::Word {
                        text: &text[cut..at],
                        opens: opens(cut),
                    });
                    cut = at;
                }
            }
        }
        start += end;
    }
    out
}

/// A line under construction: its runs, the segment each came from, and the
/// width they occupy with their gaps.
#[derive(Debug, Default)]
struct Builder {
    /// The runs so far, spaces included.
    runs: Vec<Run>,
    /// The segment each run came from, which decides whether a word joins the
    /// run before it.
    origins: Vec<usize>,
    /// The width so far, as [`line_width`] reckons it.
    used: f32,
}

impl Builder {
    /// Whether the line holds a word yet. Spaces before the first word are not
    /// kept, so this is whether it holds a run at all.
    const fn started(&self) -> bool {
        !self.runs.is_empty()
    }

    /// Adds `items` to the line if `commit`, and returns the width they add
    /// either way. A word directly after one of the same segment joins its run,
    /// at the two widths plus the kerning where they meet, so the run is drawn
    /// as the text it is.
    fn extend(
        &mut self,
        measurer: &mut TextMeasurer,
        styles: &[RunStyle],
        items: &[(usize, Piece<'_>)],
        gap: f32,
        commit: bool,
    ) -> f32 {
        let mut added = 0.0;
        let mut started = self.started();
        let mut spaced = self.runs.last().is_some_and(Run::is_space);
        let mut last = self
            .runs
            .last()
            .filter(|run| !run.is_space())
            .and_then(|run| run.text.chars().next_back())
            .zip(self.origins.last().copied());
        for &(segment, piece) in items {
            let style = &styles[segment];
            match piece {
                Piece::Newline => {}
                Piece::Space(text) => {
                    // Kept at no width, so the gap it stands for is added
                    // once, when the next word arrives; dropped before any.
                    if started {
                        spaced = true;
                        last = None;
                        if commit {
                            self.push(text, style, 0.0, segment);
                        }
                    }
                }
                Piece::Word { text, .. } => {
                    let width =
                        measurer.run_width(style, style.letter_spacing, text);
                    let joins = last.filter(|&(_, origin)| origin == segment);
                    let advance = match (spaced, joins) {
                        (true, _) => gap + width,
                        (false, Some((left, _))) => {
                            width + measurer.kerning(style, left, text)
                        }
                        (false, None) => width,
                    };
                    added += advance;
                    if commit {
                        if joins.is_some()
                            && let Some(run) = self.runs.last_mut()
                        {
                            run.text.push_str(text);
                            run.width += advance;
                        } else {
                            self.push(text, style, width, segment);
                        }
                        self.used += advance;
                    }
                    started = true;
                    spaced = false;
                    last = text.chars().next_back().zip(Some(segment));
                }
            }
        }
        added
    }

    /// Appends one run.
    fn push(
        &mut self,
        text: &str,
        style: &RunStyle,
        width: f32,
        segment: usize,
    ) {
        self.runs.push(Run {
            text: text.to_owned(),
            style: style.clone(),
            width,
        });
        self.origins.push(segment);
    }

    /// Closes the line, dropping the trailing space the wrap consumed there,
    /// which CSS does not draw. `hard` records why the line ended: truncation
    /// pulls text up across a wrap, never across a written newline. Returns
    /// whether a space was dropped, which is whether the break fell at one.
    fn finish(&mut self, lines: &mut Vec<Line>, hard: bool) -> bool {
        let mut dropped = false;
        while self.runs.last().is_some_and(Run::is_space) {
            self.runs.pop();
            dropped = true;
        }
        lines.push(Line {
            runs: core::mem::take(&mut self.runs),
            ascent: 0.0,
            content_height: 0.0,
            height: 0.0,
            hard_break: hard,
        });
        self.origins.clear();
        self.used = 0.0;
        dropped
    }
}

/// What a wrap produced: the lines, and for each whether the break after it
/// fell at a space, which a truncation pulling the next line up puts back.
#[derive(Debug)]
struct Wrapped {
    /// The lines, without their heights.
    lines: Vec<Line>,
    /// Per line, whether the wrap dropped a space where it ended.
    spaced: Vec<bool>,
}

/// Wraps segments into lines that fit `max_width`.
///
/// A line breaks only at a break opportunity, found over the paragraph's
/// whole text as Chrome finds them -- after a space, between ideographs, after
/// a hyphen; this module's documentation has the rules -- so a segment
/// boundary inside a word is not one.
/// What lies between two opportunities is placed whole: a line takes it if it
/// fits, and a line holding nothing yet takes it whatever its width, so a line
/// is never empty because nothing fits.
#[must_use]
pub fn wrap(
    measurer: &mut TextMeasurer,
    base: &ResolvedText,
    segments: &[TextSegment],
    max_width: f32,
    metrics: Metrics,
) -> Vec<Line> {
    wrap_marked(measurer, base, segments, max_width, metrics).lines
}

/// [`wrap`], with where each break fell.
fn wrap_marked(
    measurer: &mut TextMeasurer,
    base: &ResolvedText,
    segments: &[TextSegment],
    max_width: f32,
    metrics: Metrics,
) -> Wrapped {
    let base_style = RunStyle::base(base);
    let space = measurer.space_width(&base_style, metrics.letter_spacing);
    let gap = space + metrics.word_spacing;

    let paragraph: String = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    let opportunities = breaks::opportunities(&paragraph);
    let styles: Vec<RunStyle> = segments
        .iter()
        .map(|segment| RunStyle::of(base, segment))
        .collect();
    let mut items = Vec::new();
    let mut from = 0;
    for (index, segment) in segments.iter().enumerate() {
        items.extend(
            pieces(&segment.text, from, &opportunities)
                .into_iter()
                .map(|piece| (index, piece)),
        );
        from += segment.text.len();
    }

    let mut wrapped = Wrapped {
        lines: Vec::new(),
        spaced: Vec::new(),
    };
    let mut line = Builder::default();
    let mut rest = items.as_slice();
    while let Some(&(_, first)) = rest.first() {
        if first == Piece::Newline {
            line.finish(&mut wrapped.lines, true);
            wrapped.spaced.push(false);
            rest = &rest[1..];
            continue;
        }
        // Up to the next piece a line may begin with, or a newline.
        let end = rest[1..]
            .iter()
            .position(|(_, piece)| {
                matches!(
                    piece,
                    Piece::Newline | Piece::Word { opens: true, .. }
                )
            })
            .map_or(rest.len(), |at| at + 1);
        let (unit, after) = rest.split_at(end);
        rest = after;

        let advance = line.extend(measurer, &styles, unit, gap, false);
        if line.started() && line.used + advance > max_width {
            // A unit too wide for the line overflows it on a line of its own,
            // as Chrome does with a 278px word in a 100px box, alone or after
            // a short word. Only `overflow-wrap: break-word` breaks it; see
            // [`break_word`].
            let dropped = line.finish(&mut wrapped.lines, false);
            wrapped.spaced.push(dropped);
        }
        line.extend(measurer, &styles, unit, gap, true);
    }
    line.finish(&mut wrapped.lines, false);
    wrapped.spaced.push(false);
    wrapped
}

/// Breaks what lies between two break opportunities into pieces that each fit
/// `max_width`, a character wider than the line taking a piece of its own.
/// Chrome breaks there only under
/// `overflow-wrap: break-word`, which the scene lacks, so nothing outside the
/// tests calls this.
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

/// Rebuilds the last visible line to fill the room an ellipsis leaves: CSS
/// takes as many characters as fit, so `Flower of Paradise Lost` in 140px is
/// `Flower of Par…`, not the wrap's `Flower of…`. Text is pulled up from the
/// lines after it, never across a newline the caller wrote.
fn truncate_with(
    measurer: &mut TextMeasurer,
    base: &ResolvedText,
    all: &Wrapped,
    from: usize,
    max_width: f32,
    marker: &str,
    metrics: Metrics,
) -> Line {
    let base_style = RunStyle::base(base);
    let space = measurer.space_width(&base_style, metrics.letter_spacing);
    let gap = space + metrics.word_spacing;

    // The ellipsis takes the style of the last thing written before it, which
    // is v9's rule and the one that keeps a bold last word's marker bold.
    let style = all.lines[from]
        .runs
        .iter()
        .rev()
        .find(|run| !run.is_space())
        .map_or(base_style, |run| run.style.clone());
    let marker_width =
        measurer.run_width(&style, metrics.letter_spacing, marker);
    let budget = max_width - marker_width;

    // This line, then each soft-wrapped continuation, with the space the wrap
    // consumed put back between them where it consumed one: `漢字` broken
    // between its ideographs had none.
    let mut source: Vec<Run> = Vec::new();
    for (offset, line) in all.lines[from..].iter().enumerate() {
        if offset > 0
            && all.spaced[from + offset - 1]
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

    // A space before the marker is kept while it fits, as Chrome keeps the
    // longest prefix that fits: `Flower of Paradise` at 22px in 90 draws
    // `Flower of …`, 89.98 wide. The line is measured with the marker on it,
    // and the space goes if that does not fit.
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

/// The faces on one line, as the maxima its box is built from.
#[derive(Debug, Default)]
struct Extent {
    /// The largest ascent.
    ascent: f32,
    /// The largest descent.
    descent: f32,
    /// The most any face reaches above the baseline at `normal`: its ascent
    /// and the upper half of its line gap.
    above: f32,
    /// The most any face reaches below it: its descent and the rest of its
    /// gap.
    below: f32,
}

impl Extent {
    /// Adds one face. The gap's upper half is floored, as Chrome floors every
    /// half-leading, so an odd gap puts its spare pixel below the baseline.
    fn add(&mut self, face: Answer) {
        let Answer {
            measurement:
                Measurement {
                    ascent, descent, ..
                },
            line_gap,
        } = face;
        let half = (line_gap / 2.0).floor();
        self.ascent = self.ascent.max(ascent);
        self.descent = self.descent.max(descent);
        self.above = self.above.max(ascent + half);
        self.below = self.below.max(descent + line_gap - half);
    }

    /// Sets `line`'s three heights for a stated height or `normal`, which
    /// [`Line::baseline_from_top`] turns into the baseline.
    fn place(&self, line: &mut Line, line_height: Option<f32>) {
        if let Some(height) = line_height {
            // What `line_height` asks for, **even when it is less than the
            // face needs**: CSS lets a tight line box overlap its neighbours
            // rather than quietly growing.
            line.ascent = self.ascent;
            line.content_height = self.ascent + self.descent;
            line.height = height;
        } else {
            line.ascent = self.above;
            line.content_height = self.above + self.below;
            line.height = line.content_height;
        }
    }
}

/// Fills in every line's three heights.
///
/// The ascent, descent and line gap come from the **face**, per run, taken as
/// maxima over the line's words; a line with no words takes the node's own
/// face, so an empty line between two paragraphs is as tall as a line of text.
pub fn measure_lines(
    measurer: &mut TextMeasurer,
    base: &ResolvedText,
    lines: &mut [Line],
    metrics: Metrics,
) {
    let base_style = RunStyle::base(base);
    for line in lines {
        let mut extent = Extent::default();
        for run in line.runs.iter().filter(|run| !run.is_space()) {
            extent.add(measurer.answer(
                &run.style,
                metrics.letter_spacing,
                METRICS_STRING,
            ));
        }
        if extent.ascent == 0.0 && extent.descent == 0.0 {
            extent = Extent::default();
            extent.add(measurer.answer(
                &base_style,
                metrics.letter_spacing,
                METRICS_STRING,
            ));
        }
        extent.place(line, metrics.line_height);
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
    let all = wrap_marked(measurer, base, segments, max_width, metrics);
    let mut lines = all.lines.clone();
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

    // A line can overflow with no line after it: an unbreakable word is placed
    // whole, and Chrome cuts it at the last letter that fits, `Antidisestabli…`
    // at 88.00 in 90 (`chrome/ellipsis.tsv`). `truncate_with` is the rule; this
    // is its second trigger.
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
        wrapped_lines: all.lines.len(),
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
        Answer, Extent, Line, METRICS_STRING, Measurement, Metrics, Piece, Run,
        RunStyle, TextMeasurer, breaks, layout, line_width, pieces,
        spacing_pixels, wrap,
    };
    use crate::{
        FINITE_CEILING,
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

    /// A spacing no arithmetic can carry never reaches a measurement. The
    /// equalities are exact: `0.0` is what `normal` resolves to, and the
    /// pass-through row asserts a number arrived unchanged.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the point of these rows is that a value is exactly what it \
                  was, or exactly what `normal` resolves to; a tolerance would \
                  pass a repair that shifted every spacing slightly"
    )]
    fn a_spacing_that_is_not_a_number_resolves_to_normal() {
        assert_eq!(spacing_pixels(Spacing::Points(f32::NAN), 16.0), 0.0);
        assert_eq!(spacing_pixels(Spacing::Em(f32::NAN), 16.0), 0.0);
        // Which is what `normal` already resolves to, so the two agree rather
        // than merely both being zero.
        assert_eq!(
            spacing_pixels(Spacing::Points(f32::NAN), 16.0),
            spacing_pixels(Spacing::Normal, 16.0),
        );
    }

    /// An infinite spacing is clamped: `calc` is CSS's only spelling of
    /// infinity and it clamps, and Chrome 151 gives `calc(-infinity * 1px)` as
    /// `-3.35544e+07px`.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the two directions are the same magnitude exactly, which is \
                  what says the sign is carried rather than recomputed"
    )]
    fn an_infinite_spacing_is_clamped_and_keeps_its_direction() {
        let up = spacing_pixels(Spacing::Points(f32::INFINITY), 16.0);
        let down = spacing_pixels(Spacing::Points(f32::NEG_INFINITY), 16.0);
        assert!(up.is_finite() && down.is_finite());
        assert!(up > 0.0, "an infinite spacing pushes glyphs apart: {up}");
        assert!(down < 0.0, "a negative one pulls them together: {down}");
        assert_eq!(up, -down, "the two directions are the same distance");
    }

    /// The product is checked, not the numbers that made it: `Em` multiplies by
    /// the run's own font size, so two finite values can overflow to infinity.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the clamped value is the ceiling itself, exactly"
    )]
    fn a_spacing_that_overflows_while_resolving_is_clamped_too() {
        let resolved = spacing_pixels(Spacing::Em(1.0e30), 1.0e30);
        assert!(resolved.is_finite(), "{resolved} reached a measurement");
        assert_eq!(resolved, FINITE_CEILING);
    }

    /// An absurd finite spacing is left alone, to the bit: nothing here judges
    /// a spacing too large to have meant, and a `clamp` would pull `1e30` down.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "unchanged means unchanged, to the bit"
    )]
    fn a_finite_spacing_is_carried_through_unchanged() {
        // The row says nothing unless the list crosses the ceiling, so that is
        // checked where it cannot rot: a `1e30` edited down to `1e3` one day
        // stops compiling rather than quietly making this vacuous.
        const _: () = assert!(1.0e30 > FINITE_CEILING);
        for points in [0.0, 4.0, -4.0, 1.0e6, -1.0e6, 1.0e30, -1.0e30] {
            assert_eq!(spacing_pixels(Spacing::Points(points), 16.0), points);
        }
    }

    /// The line gap is checked too, though it is a plain `f32` rather than a
    /// `Spacing`: a non-finite gap times the line count is a non-finite height.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "zero is the value, not an approximation of it"
    )]
    fn a_line_gap_that_is_not_a_number_is_replaced_as_well() {
        let mut base = style();
        base.line_gap = f32::NAN;
        assert_eq!(Metrics::of(&base).line_gap, 0.0);

        base.line_gap = f32::INFINITY;
        assert!(Metrics::of(&base).line_gap.is_finite());

        // And a finite gap, however large, is still exactly what was asked for.
        base.line_gap = 1.0e30;
        assert_eq!(Metrics::of(&base).line_gap, 1.0e30);
    }

    /// An infinite line height and size are bounded too: both are plain `f32`s
    /// outside `spacing_pixels`, and `line_height`'s `> 0.0` guard lets
    /// `Infinity` through.
    #[test]
    fn an_infinite_line_height_or_size_never_reaches_the_measurement() {
        let mut base = style();
        base.line_height = Some(LineHeight::Number(f32::INFINITY));
        assert!(Metrics::of(&base).line_height.is_none_or(f32::is_finite));

        // A finite multiple against an infinite size is infinite, so the
        // product is checked and not only the two numbers behind it.
        base.line_height = Some(LineHeight::Number(2.0));
        base.size = f32::INFINITY;
        assert!(Metrics::of(&base).line_height.is_none_or(f32::is_finite));
        assert!(RunStyle::base(&base).size.is_finite());
    }

    /// A `NaN` size is neutralised: refused at the writer, where a refusal can
    /// be reported, and neutralised here, where it cannot.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "zero is the value, not an approximation of it"
    )]
    fn a_size_that_is_not_a_number_is_neutralised() {
        let mut base = style();
        base.size = f32::NAN;
        assert_eq!(RunStyle::base(&base).size, 0.0);
    }

    /// And the metrics a paragraph is laid out with carry none of it either:
    /// the call site a scene takes, where the rows above test `spacing_pixels`
    /// alone.
    #[test]
    #[expect(clippy::float_cmp, reason = "`normal` resolves to exactly zero")]
    fn resolved_metrics_are_finite_whatever_the_style_asked_for() {
        let mut base = style();
        base.letter_spacing = Spacing::Points(f32::NAN);
        base.word_spacing = Spacing::Points(f32::NEG_INFINITY);
        let metrics = Metrics::of(&base);
        assert_eq!(metrics.letter_spacing, 0.0);
        assert!(metrics.word_spacing.is_finite());
        assert!(metrics.word_spacing < 0.0);
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

    /// Prints this crate's line boxes beside the paragraph's, on stderr, for a
    /// person: `cargo test -p meo-canvas-core --lib -- --ignored --nocapture
    /// line_box`. Ignored, since the browser measurements decide which is
    /// right.
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

    /// A word that opens, or one that does not, for the piece assertions.
    fn word(text: &str, opens: bool) -> Piece<'_> {
        Piece::Word { text, opens }
    }

    #[test]
    fn a_newline_is_its_own_piece_and_a_run_of_spaces_is_one() {
        fn at(text: &str) -> Vec<Piece<'_>> {
            pieces(text, 0, &breaks::opportunities(text))
        }
        assert_eq!(
            at("a  b"),
            [word("a", false), Piece::Space("  "), word("b", true)]
        );
        // Two newlines are two breaks, which is what makes an empty line
        // between paragraphs possible.
        assert_eq!(
            at("a\n\nb"),
            [
                word("a", false),
                Piece::Newline,
                Piece::Newline,
                word("b", true)
            ]
        );
    }

    #[test]
    fn a_word_is_cut_at_every_opportunity_inside_it() {
        let text = "漢字well-known";
        assert_eq!(
            pieces(text, 0, &breaks::opportunities(text)),
            [
                word("漢", false),
                word("字", true),
                word("well-", true),
                word("known", true),
            ]
        );
    }

    /// The paragraph's opportunities decide, not the segment's: `b` continues
    /// the word `a` began in the segment before, and `字` does not.
    #[test]
    fn a_segment_edge_opens_only_where_the_paragraph_breaks() {
        let paragraph = "ab漢字";
        let opportunities = breaks::opportunities(paragraph);
        assert_eq!(pieces("b", 1, &opportunities), [word("b", false)]);
        assert_eq!(pieces("字", 5, &opportunities), [word("字", true)]);
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

        // A fraction, the one feature this face answers to: across seventeen
        // tags on the repository's Oswald, `frac` moves a nineteen-character
        // sample from 220.61 to 211.04 and every other tag, `smcp` included,
        // moves nothing.
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
    fn a_run_carries_the_backends_spacing_and_no_correction() {
        let mut measurer = TextMeasurer::new();
        let base = RunStyle::base(&style());
        let plain = measurer.run_width(&base, 0.0, "Hxgp");
        let spaced = measurer.run_width(&base, 2.0, "Hxgp");
        let measured = measurer.measure(&base, 2.0, "Hxgp").width;
        // **Nothing on top of what the backend reports.** This asserted one
        // unit until meo-skia-canvas 0.16, which added a unit per character
        // rather than `n - 1` and turned the compensation into a double count.
        assert!((spaced - measured).abs() < f32::EPSILON);
        // And four characters' worth of it, so a backend that stopped applying
        // spacing altogether fails here rather than passing the equality
        // above.
        assert!(
            (spaced - plain - 8.0).abs() < 0.001,
            "{spaced} against {plain}"
        );
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
        let answer = measurer.answer(&base, 0.0, METRICS_STRING);
        let face = answer.measurement;
        assert!((face.ascent - face.ascent.round()).abs() < f32::EPSILON);
        assert!((face.descent - face.descent.round()).abs() < f32::EPSILON);
        assert!(
            (answer.line_gap - answer.line_gap.round()).abs() < f32::EPSILON
        );
        // The identity that matters more than any one value: Chrome's
        // `normal` line box is exactly the sum, and an unrounded pair misses
        // it by 0.288 every line.
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
            (block.lines[0].height
                - (face.ascent + face.descent + answer.line_gap))
                .abs()
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

    /// What `wrapped_lines` and `truncated` report in four shapes, which
    /// `lines.len()` cannot. The third matters: a word with no break
    /// opportunity reaches the marker by overflow, never raising the line
    /// count.
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
        // -- which is why `wrapped_lines` is a field of its own.
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

    /// Every line a wrap at 0px makes, which is one per break opportunity.
    fn at_zero(segments: &[TextSegment]) -> Vec<String> {
        let mut measurer = TextMeasurer::new();
        let base = style();
        wrap(&mut measurer, &base, segments, 0.0, Metrics::of(&base))
            .iter()
            .map(read)
            .collect()
    }

    /// `parts` as segments, every other one at another size.
    fn alternating(parts: &[&str]) -> Vec<TextSegment> {
        parts
            .iter()
            .enumerate()
            .map(|(index, text)| TextSegment {
                text: (*text).to_owned(),
                style: TextStyle {
                    font_size: (index % 2 == 1).then_some(20.0),
                    ..TextStyle::default()
                },
            })
            .collect()
    }

    /// The opportunities are the paragraph's: a segment edge inside a word is
    /// not one, and one between two ideographs, after a space or after a
    /// hyphen still is.
    #[test]
    fn a_segment_edge_neither_makes_nor_removes_a_break() {
        assert_eq!(at_zero(&alternating(&["ab", "cd"])), ["abcd"]);
        assert_eq!(at_zero(&alternating(&["漢", "字"])), ["漢", "字"]);
        assert_eq!(at_zero(&alternating(&["a ", "b"])), ["a", "b"]);
        assert_eq!(
            at_zero(&alternating(&["well-", "known"])),
            ["well-", "known"]
        );
        assert_eq!(
            at_zero(&alternating(&["漢字", "かな"])),
            ["漢", "字", "か", "な"]
        );
        // `」` may not begin a line, wherever its segment starts.
        assert_eq!(at_zero(&alternating(&["漢", "」漢"])), ["漢」", "漢"]);
    }

    /// Pieces with no space between them join one run, which is drawn as the
    /// text it is, kerning included.
    #[test]
    fn pieces_that_meet_without_a_space_join_one_run() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let lines = wrap(
            &mut measurer,
            &base,
            &plain("漢字well-known"),
            1e4,
            Metrics::of(&base),
        );
        assert_eq!(lines.len(), 1);
        let texts: Vec<&str> =
            lines[0].runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(texts, ["漢字well-known"]);
    }

    /// A truncation pulling the next line up puts back the space the wrap
    /// consumed, and only that: `漢字` broke from `Tokyo…` at no space.
    #[test]
    fn a_truncation_adds_no_space_where_the_break_had_none() {
        let mut measurer = TextMeasurer::new();
        let base = style();
        let metrics = Metrics::of(&base);
        let text = "漢字Tokyoooooooooooo";
        let width = 70.0;
        let wrapped = wrap(&mut measurer, &base, &plain(text), width, metrics);
        assert_eq!(read(&wrapped[0]), "漢字", "the scene has to break there");

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
        let last = read(&block.lines[0]);
        assert!(
            last.starts_with("漢字To"),
            "nothing was pulled up: {last:?}"
        );
        assert!(!last.contains(' '), "a space appeared: {last:?}");
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

        // The line is rebuilt against a budget without the marker and takes as
        // many characters as fit, pulling text up when there is room and
        // stopping mid-word when not: `Flower o…` here, not the wrap's `Flower
        // of` with a marker on.
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
        // Eight of leading, four above, then the ascent.
        assert!((line.baseline_from_top() - 14.0).abs() < f32::EPSILON);
    }

    /// Asserts the height and baseline `extent` places a line at.
    fn assert_places(
        extent: &Extent,
        line_height: Option<f32>,
        (height, baseline): (f32, f32),
    ) {
        let mut line = Line {
            runs: Vec::new(),
            ascent: 0.0,
            content_height: 0.0,
            height: 0.0,
            hard_break: false,
        };
        extent.place(&mut line, line_height);
        let placed = (line.height, line.baseline_from_top());
        assert!(
            (placed.0 - height).abs() < f32::EPSILON
                && (placed.1 - baseline).abs() < f32::EPSILON,
            "{placed:?}, not ({height}, {baseline})"
        );
    }

    /// A face's rounded parts, as a measurement carries them.
    fn face(ascent: f32, descent: f32, line_gap: f32) -> Answer {
        Answer {
            measurement: Measurement {
                width: 0.0,
                ascent,
                descent,
            },
            line_gap,
        }
    }

    #[test]
    fn a_stated_height_splits_its_leading_around_the_largest_face() {
        let mut extent = Extent::default();
        extent.add(face(10.0, 2.0, 0.0));
        // Eight of leading, four above, then the ascent.
        assert_places(&extent, Some(20.0), (20.0, 14.0));
    }

    #[test]
    fn normal_puts_an_odd_gap_s_spare_pixel_below_the_baseline() {
        let mut extent = Extent::default();
        extent.add(face(26.0, 9.0, 3.0));
        assert_places(&extent, None, (38.0, 27.0));
    }

    #[test]
    fn normal_takes_each_face_s_half_gaps_rather_than_the_largest_gap() {
        // 25px Oswald and Poppins, as Chrome's `line-height-normal.tsv` row
        // gives them: 41 and 30, where the largest of each part gives 42, 31.
        let mut extent = Extent::default();
        extent.add(face(30.0, 7.0, 0.0));
        extent.add(face(26.0, 9.0, 3.0));
        assert_places(&extent, None, (41.0, 30.0));
    }
}
