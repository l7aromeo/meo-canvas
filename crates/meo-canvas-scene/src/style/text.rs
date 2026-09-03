//! What inherits down to a run of glyphs.
//!
//! `canvas.type.ts` puts these on `BoxProps` rather than on `TextProps`, so a
//! container can set a family and a size that every text node beneath it picks
//! up. They are a separate struct here for the same reason: a [`TextStyle`] on
//! a box is an inheritance source, and a [`TextStyle`] on a text node is what
//! that node draws with. The resolve stage of `meo-canvas-core` folds one into
//! the other.
//!
//! Inheritance is why every field is an `Option`. A `None` means "take the
//! parent's", which is a different statement from "use the initial value" --
//! a box that set nothing must not overwrite what its own parent said.

use crate::{
    style::{PaintOrder, paint::Color},
    wire::wire_enum,
};

wire_enum! {
    /// Where a line of text sits across its width.
    pub enum TextAlign {
        /// At the inline start, which flips under a right-to-left direction.
        Start = 0,
        /// At the inline end.
        End = 1,
        /// At the left edge regardless of direction.
        Left = 2,
        /// Centred.
        Center = 3,
        /// At the right edge regardless of direction.
        Right = 4,
        /// Stretched so both edges are flush, except on the last line.
        Justify = 5,
    }
}

wire_enum! {
    /// A line drawn through, over or under text.
    pub enum TextDecoration {
        /// No line.
        None = 0,
        /// A line below the baseline.
        Underline = 1,
        /// A line above the ascent.
        Overline = 2,
        /// A line through the middle.
        LineThrough = 3,
    }
}

wire_enum! {
    /// Where the text sits within the node's box.
    ///
    /// CSS's `vertical-align` places one inline box on its line; this places
    /// the **whole paragraph** in the box that holds it, which is what v1
    /// does and what a scene with one paragraph per node can express. A node
    /// sized to its own text has nothing left over, so the three agree there.
    pub enum VerticalAlign {
        /// Against the top of the box.
        Top = 0,
        /// Centred in the box.
        Middle = 1,
        /// Against the bottom of the box.
        Bottom = 2,
    }
}

wire_enum! {
    /// Upright or slanted glyphs.
    pub enum FontStyle {
        /// Upright.
        Normal = 0,
        /// Slanted, using the family's italic face where it has one.
        Italic = 1,
    }
}

wire_enum! {
    /// One CSS `font-variant` keyword.
    ///
    /// An enum rather than the string `canvas.type.ts` types this as, for the
    /// reason every keyword in this crate is an enum: the TypeScript surface
    /// spells it as a string-literal union, and an enum is what keeps that
    /// union's autocomplete and its rejection of a misspelling. The keywords
    /// are CSS's own, so `FontVariant::SmallCaps` is `'small-caps'` on the
    /// other side and needs no lookup table between them.
    #[non_exhaustive]
    pub enum FontVariant {
        /// CSS `normal`. No variant features beyond the font's defaults.
        Normal = 0,
        /// CSS `historical-forms`. Historical glyph forms, where the font has them.
        HistoricalForms = 1,
        /// CSS `small-caps`. Lowercase drawn as small capitals.
        SmallCaps = 2,
        /// CSS `all-small-caps`. Both cases drawn as small capitals.
        AllSmallCaps = 3,
        /// CSS `petite-caps`. Lowercase drawn as petite capitals, which are shorter than small caps.
        PetiteCaps = 4,
        /// CSS `all-petite-caps`. Both cases drawn as petite capitals.
        AllPetiteCaps = 5,
        /// CSS `unicase`. Uppercase drawn at small-capital height beside ordinary lowercase.
        Unicase = 6,
        /// CSS `titling-caps`. Capitals drawn for all-capital titling, lighter than the text capitals.
        TitlingCaps = 7,
        /// CSS `lining-nums`. Digits sharing one height, aligned to the cap height.
        LiningNums = 8,
        /// CSS `oldstyle-nums`. Digits with ascenders and descenders, sized to the lowercase.
        OldstyleNums = 9,
        /// CSS `proportional-nums`. Digits with per-glyph widths.
        ProportionalNums = 10,
        /// CSS `tabular-nums`. Digits sharing one width, so columns line up.
        TabularNums = 11,
        /// CSS `diagonal-fractions`. Fractions set on a diagonal bar.
        DiagonalFractions = 12,
        /// CSS `stacked-fractions`. Fractions set on a horizontal bar.
        StackedFractions = 13,
        /// CSS `ordinal`. Ordinal markers, as in `1st`, drawn as their own glyphs.
        Ordinal = 14,
        /// CSS `slashed-zero`. A zero with a slash through it.
        SlashedZero = 15,
        /// CSS `common-ligatures`. Ligatures the font marks as always appropriate.
        CommonLigatures = 16,
        /// CSS `no-common-ligatures`. Common ligatures suppressed.
        NoCommonLigatures = 17,
        /// CSS `discretionary-ligatures`. Ligatures the font offers as optional.
        DiscretionaryLigatures = 18,
        /// CSS `no-discretionary-ligatures`. Discretionary ligatures suppressed.
        NoDiscretionaryLigatures = 19,
        /// CSS `historical-ligatures`. Ligatures that were once common and now read as archaic.
        HistoricalLigatures = 20,
        /// CSS `no-historical-ligatures`. Historical ligatures suppressed.
        NoHistoricalLigatures = 21,
        /// CSS `contextual`. Glyph substitutions that depend on neighbouring characters.
        Contextual = 22,
        /// CSS `no-contextual`. Contextual substitutions suppressed.
        NoContextual = 23,
        /// CSS `jis78`. Japanese glyphs from the JIS78 standard.
        Jis78 = 24,
        /// CSS `jis83`. Japanese glyphs from the JIS83 standard.
        Jis83 = 25,
        /// CSS `jis90`. Japanese glyphs from the JIS90 standard.
        Jis90 = 26,
        /// CSS `jis04`. Japanese glyphs from the JIS2004 standard.
        Jis04 = 27,
        /// CSS `simplified`. Simplified Han forms.
        Simplified = 28,
        /// CSS `traditional`. Traditional Han forms.
        Traditional = 29,
        /// CSS `full-width`. East Asian glyphs at full width.
        FullWidth = 30,
        /// CSS `proportional-width`. East Asian glyphs at their natural widths.
        ProportionalWidth = 31,
        /// CSS `ruby`. Glyph forms designed for ruby annotation, which is set small.
        Ruby = 32,
        /// CSS `super`. Superscript forms.
        Super = 33,
        /// CSS `sub`. Subscript forms.
        Sub = 34,
    }
}

/// A font weight on the CSS numeric scale.
///
/// A number rather than an enum: the scale is open, variable fonts interpolate
/// between its stops, and `canvas.type.ts` already accepts a bare number
/// alongside its keywords. The keywords are resolved to numbers at the surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontWeight(u16);

impl FontWeight {
    /// CSS `bold`.
    pub const BOLD: Self = Self(700);
    /// The heaviest weight CSS defines.
    pub const MAX: u16 = 1000;
    /// The lightest weight CSS defines.
    ///
    /// The bound is CSS's own range rather than a limit this crate invents: a
    /// value outside it names no face, and clamping is the behaviour a browser
    /// has.
    pub const MIN: u16 = 1;
    /// CSS `normal`.
    pub const NORMAL: Self = Self(400);

    /// Creates a weight, clamped to the range CSS defines.
    #[must_use]
    pub const fn new(weight: u16) -> Self {
        if weight < Self::MIN {
            Self(Self::MIN)
        } else if weight > Self::MAX {
            Self(Self::MAX)
        } else {
            Self(weight)
        }
    }

    /// The weight as a number.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Spacing between glyphs or words.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub enum Spacing {
    /// Whatever the font specifies.
    #[default]
    Normal,
    /// An absolute adjustment in logical pixels.
    Points(f32),
    /// An adjustment as a multiple of the font size.
    Em(f32),
}

/// How tall a line box is, in the four spellings CSS gives it.
///
/// # Three variants for four kinds
///
/// `normal` is the **absence** of a stated value and is spelled `None` where
/// this appears, which is what `Option` has meant here since the line-height
/// sentinel: an explicit `1.0` and an inherited `normal` are different things
/// and were once indistinguishable.
///
/// # Why a percentage is here and not in the resolved form
///
/// **This is what the author wrote; resolution is where it stops being that.**
/// A percentage resolves against the font size of the element that *declares*
/// it, and the resulting **length** is what descendants inherit -- so
/// `crates/meo-canvas-core/src/resolve.rs` turns a `Percent` into a `Length`
/// as it merges, and nothing downstream ever sees one.
///
/// A [`Number`](Self::Number) is not resolved there, and the difference is the
/// whole of CSS's rule. Measured in Chrome, a parent at `16px` declaring for a
/// child at `32px`:
///
/// ```text
///                declared   inherited by a 32px child
/// number 1.5       24              48   <- recomputed against the child
/// length 24px      24              24
/// percent 150%     24              24   <- resolved at the parent, not 48
/// ```
///
/// **Both mistakes are invisible to a test that only declares.** Resolve a
/// percentage late and the third row reads 48; resolve a number early and the
/// first reads 24. Every directly-declared row passes either way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    /// A multiple of the font size, recomputed by whoever inherits it.
    Number(f32),
    /// An absolute height in logical pixels.
    Length(f32),
    /// A share of the declaring element's own font size, as a fraction --
    /// `1.5` is CSS's `150%`. Never survives resolution.
    Percent(f32),
}

/// An outline drawn around glyphs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStroke {
    /// Stroke width in logical pixels.
    pub width: f32,
    /// Stroke colour.
    pub color: Color,
}

/// Everything that styles glyphs, all of it inheritable.
///
/// A `None` field takes its value from the nearest ancestor that set one, and
/// from the initial value if no ancestor did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextStyle {
    /// Family name, resolved against the renderer's registered fonts.
    pub font_family: Option<String>,
    /// Em size in logical pixels.
    pub font_size: Option<f32>,
    /// Weight on the CSS numeric scale.
    pub font_weight: Option<FontWeight>,
    /// Upright or slanted.
    pub font_style: Option<FontStyle>,
    /// Glyph fill colour.
    pub color: Option<Color>,
    /// Horizontal placement within the line box.
    pub text_align: Option<TextAlign>,
    /// A line through, over or under the text.
    pub text_decoration: Option<TextDecoration>,
    /// Vertical placement within the line box.
    pub vertical_align: Option<VerticalAlign>,
    /// Which of fill and stroke is drawn on top.
    pub paint_order: Option<PaintOrder>,
    /// How tall a line box is. `None` is CSS's `normal`.
    pub line_height: Option<LineHeight>,
    /// Extra space added to every line box, in logical pixels.
    pub line_gap: Option<f32>,
    /// Space between glyphs.
    pub letter_spacing: Option<Spacing>,
    /// Space between words.
    pub word_spacing: Option<Spacing>,
    /// OpenType feature keywords applied to the run.
    ///
    /// A list because CSS's `font-variant` is a space-separated shorthand and
    /// a caller routinely wants two at once -- `small-caps tabular-nums`
    /// is one setting, not a choice between them. `None` inherits; an
    /// empty list means the same as [`FontVariant::Normal`].
    pub font_variant: Option<Vec<FontVariant>>,
    /// An outline around the glyphs.
    pub text_stroke: Option<TextStroke>,
}

/// A run of text with its own overrides, for markup inside one paragraph.
///
/// Mirrors `canvas.type.ts`'s `TextSegment`. The overrides are a full
/// [`TextStyle`] rather than that file's five-field subset, because a segment
/// that can set a colour and a weight has no principled reason not to set a
/// family, and the wire format costs the same either way.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSegment {
    /// The characters of this run.
    pub text: String,
    /// Overrides applied on top of the paragraph's style.
    pub style: TextStyle,
}

/// The marker a truncated line ends with when the caller does not name one.
///
/// U+2026 HORIZONTAL ELLIPSIS, one glyph rather than three full stops.
///
/// **Measured rather than assumed.** Chrome's `text-overflow: ellipsis` was
/// read in Helvetica at 40px -- deliberately not the repository's own Oswald,
/// where `…` and `...` rasterise to identical ink runs with advances 0.36px
/// apart and cannot tell the two answers apart. The marker Chrome drew has its
/// three dots 10px apart across a 31px span, which is exactly a literal `…`;
/// three full stops sit 7px apart across 26px. Advances 40.00 against 33.34.
///
/// v1 draws the same character for `ellipsis: true`
/// (`src/canvas/text.canvas.ts:1244`), so the API reference and the
/// behavioural one agree and there was nothing to choose between.
///
/// It is a constant rather than a variant of an enum because **nothing
/// downstream reads the difference**: a marker named by default and the same
/// marker written out reach the measurer, the line-breaker and the painter as
/// the same string, so a scene that recorded which one the caller wrote would
/// carry a distinction only to round-trip it. The JavaScript surface spells the
/// same thing `ellipsis: true`, which is that language's idiom for it.
pub const DEFAULT_ELLIPSIS: &str = "\u{2026}";

/// Paragraph-level properties, which do not inherit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParagraphStyle {
    /// Maximum lines before the text is truncated. `None` means unlimited.
    pub max_lines: Option<u32>,
    /// String appended to a truncated last line. `None` truncates without a
    /// marker, and so does `Some("")` -- an empty marker and no marker are the
    /// same picture, which is what v1's truthiness guard produced and what a
    /// caller writing `ellipsis: ''` still gets.
    ///
    /// [`DEFAULT_ELLIPSIS`] is what to write for the marker CSS uses.
    pub ellipsis: Option<String>,
}

#[cfg(test)]
mod tests {

    #[test]
    fn every_spacing_is_named_here_so_a_new_one_cannot_be_ignored_elsewhere() {
        // **The compile error `#[non_exhaustive]` moved out of the other
        // crates.** `meo-canvas-core` now has a wildcard arm for this enum, so
        // a variant added here would take that arm and draw nothing rather
        // than fail to build. This match has no wildcard and lives in the
        // crate that owns the type, which is where the attribute leaves
        // exhaustiveness intact: adding a variant fails to compile here, and
        // whoever adds it goes and looks at the arms that need it.
        //
        // `cargo test` rather than `cargo build`, which is the cost of putting
        // it in a test; the gate runs both.
        use super::Spacing;
        let witness = |value: &Spacing| match value {
            Spacing::Normal => "normal",
            Spacing::Points { .. } => "points",
            Spacing::Em { .. } => "em",
        };
        let _ = witness(&Spacing::Normal);
    }
    use super::{
        DEFAULT_ELLIPSIS, FontStyle, FontVariant, FontWeight, ParagraphStyle,
        Spacing, TextAlign, TextDecoration, TextStyle, VerticalAlign,
    };

    /// The default marker is the one character CSS names, not three stops.
    ///
    /// Measured in Chrome rather than picked -- see [`DEFAULT_ELLIPSIS`]. The
    /// second assertion is the one worth having: the two look alike in prose
    /// and in several faces, and the whole reason this constant exists is that
    /// they are different characters with different advances.
    #[test]
    fn the_default_marker_is_one_horizontal_ellipsis() {
        assert_eq!(DEFAULT_ELLIPSIS, "\u{2026}");
        assert_eq!(DEFAULT_ELLIPSIS.chars().count(), 1);
        assert_ne!(DEFAULT_ELLIPSIS, "...");
    }

    /// The marker is a value the paragraph carries, not a mode it is put in.
    ///
    /// The JavaScript surface spells the same thing `ellipsis: true` and
    /// resolves it before the scene is built, so both surfaces reach the
    /// painter with an identical [`ParagraphStyle`]. This is the Rust half of
    /// that sentence.
    #[test]
    fn the_default_marker_is_what_a_paragraph_carries() {
        let paragraph = ParagraphStyle {
            max_lines: Some(1),
            ellipsis: Some(DEFAULT_ELLIPSIS.to_owned()),
        };
        assert_eq!(paragraph.ellipsis.as_deref(), Some("\u{2026}"));
    }

    #[test]
    fn font_weight_clamps_to_the_css_range() {
        assert_eq!(FontWeight::new(0).get(), FontWeight::MIN);
        assert_eq!(FontWeight::new(5000).get(), FontWeight::MAX);
        assert_eq!(FontWeight::new(500).get(), 500);
        assert_eq!(FontWeight::default(), FontWeight::NORMAL);
        assert_eq!(FontWeight::NORMAL.get(), 400);
        assert_eq!(FontWeight::BOLD.get(), 700);
        assert!(FontWeight::NORMAL < FontWeight::BOLD);
    }

    #[test]
    fn every_text_field_starts_unset_so_nothing_overrides_a_parent() {
        let style = TextStyle::default();
        assert!(style.font_family.is_none());
        assert!(style.font_size.is_none());
        assert!(style.font_weight.is_none());
        assert!(style.color.is_none());
        assert!(style.text_stroke.is_none());
        assert_eq!(ParagraphStyle::default().max_lines, None);
    }

    #[test]
    fn spacing_defaults_to_the_font_s_own() {
        assert_eq!(Spacing::default(), Spacing::Normal);
    }

    #[test]
    fn every_text_enum_lists_its_variants() {
        assert_eq!(TextAlign::ALL.len(), 6);
        assert_eq!(TextDecoration::ALL.len(), 4);
        assert_eq!(VerticalAlign::ALL.len(), 3);
        assert_eq!(FontStyle::ALL.len(), 2);
    }
    #[test]
    fn font_variant_covers_every_css_keyword_the_backend_accepts() {
        // The set `meo-skia-canvas` types as `FontVariantSetting`: one
        // `normal`, plus the alternates, caps, numeric, ligature, east-asian
        // and position groups.
        assert_eq!(FontVariant::ALL.len(), 35);
        assert_eq!(FontVariant::ALL[0], FontVariant::Normal);
        assert_eq!(FontVariant::from_wire(0), Some(FontVariant::Normal));
        assert_eq!(FontVariant::from_wire(35), None);
        assert!(TextStyle::default().font_variant.is_none());
    }
}
