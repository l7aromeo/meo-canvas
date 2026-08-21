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
    /// Where a line of text sits within its line box.
    pub enum VerticalAlign {
        /// Against the top of the line box.
        Top = 0,
        /// Centred in the line box.
        Middle = 1,
        /// Against the bottom of the line box.
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
pub enum Spacing {
    /// Whatever the font specifies.
    #[default]
    Normal,
    /// An absolute adjustment in logical pixels.
    Points(f32),
    /// An adjustment as a multiple of the font size.
    Em(f32),
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
    /// Line box height as a multiple of the font size.
    pub line_height: Option<f32>,
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

/// Paragraph-level properties, which do not inherit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParagraphStyle {
    /// Maximum lines before the text is truncated. `None` means unlimited.
    pub max_lines: Option<u32>,
    /// String appended to a truncated last line. `None` truncates without a
    /// marker.
    pub ellipsis: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        FontStyle, FontVariant, FontWeight, ParagraphStyle, Spacing, TextAlign,
        TextDecoration, TextStyle, VerticalAlign,
    };

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
