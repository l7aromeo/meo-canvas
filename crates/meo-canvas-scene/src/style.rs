//! Everything a node is styled with, grouped the way the JavaScript surface
//! groups it.
//!
//! Four modules, because `canvas.type.ts` mixes four concerns into one
//! `BoxProps` and a reader of that file has to know which is which: [`layout`]
//! is what the solver reads, [`paint`] is what fills the box, [`text`] is what
//! inherits down to a run of glyphs, and [`effect`] is what is applied to the
//! result. Splitting them is what lets a text node carry [`text::TextStyle`]
//! without also carrying grid track sizes.
//!
//! # Enums, not numbers
//!
//! Every keyword here is a Rust enum whose variants map one-to-one onto a
//! TypeScript string-literal union -- `'row' | 'column'`, not Yoga's
//! `FlexDirection.Row = 0`. A numeric enum gives a JavaScript caller no
//! autocomplete and no error for a wrong number, and the numbers are Yoga's
//! rather than CSS's, which is a second vocabulary to learn. The byte each
//! variant crosses the wire as is declared at its definition and is unrelated
//! to the order the variants are written in.
//!
//! # Colours are resolved, not strings
//!
//! [`paint::Color`] is four bytes of sRGB. `canvas.type.ts` types every colour
//! as `string`, and that string is parsed at the surface that produced it --
//! the addon, the CLI, or a Rust caller -- rather than here. A scene carrying
//! `"rebeccapurple"` would need a CSS colour parser in a crate that has no
//! dependencies, and would let two surfaces disagree about what a name means.

pub mod effect;
pub mod layout;
pub mod paint;
pub mod text;

use crate::wire::wire_enum;

/// A length that is either an absolute measure or a share of something else.
///
/// The two forms `canvas.type.ts` spells as a number or a percentage string.
/// There is no `auto` here; a property that admits it uses [`Dimension`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// A count of logical pixels.
    Points(f32),
    /// A fraction of the reference extent, where `1.0` is 100%.
    ///
    /// Stored as a fraction rather than as the percentage a caller wrote,
    /// because every consumer multiplies by it and none of them prints it.
    Percent(f32),
}

/// A length, or an instruction to derive one.
///
/// [`Length`] plus the `auto` that sizes, margins and flex bases admit.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Dimension {
    /// Derived from content or from the parent, depending on the property.
    #[default]
    Auto,
    /// A count of logical pixels.
    Points(f32),
    /// A fraction of the reference extent, where `1.0` is 100%.
    Percent(f32),
}

impl Length {
    /// Zero pixels.
    pub const ZERO: Self = Self::Points(0.0);
}

impl Default for Length {
    fn default() -> Self {
        Self::ZERO
    }
}

impl From<Length> for Dimension {
    fn from(value: Length) -> Self {
        match value {
            Length::Points(points) => Self::Points(points),
            Length::Percent(fraction) => Self::Percent(fraction),
        }
    }
}

wire_enum! {
    /// Which of a glyph's fill and stroke is painted on top.
    ///
    /// CSS `paint-order`. A stroke is centred on the outline, so half of it
    /// falls inside the glyph: painted over the fill, a thick stroke eats into
    /// the letterform and thins it; painted under, the fill stays whole and the
    /// stroke only widens the glyph outward.
    pub enum PaintOrder {
        /// Fill over stroke, so the stroke only widens the glyph outward.
        Fill = 0,
        /// Stroke over fill, which is what a canvas does by default.
        Stroke = 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{Dimension, Length, PaintOrder};

    #[test]
    fn length_defaults_to_zero_points() {
        assert_eq!(Length::default(), Length::ZERO);
        assert_eq!(Length::ZERO, Length::Points(0.0));
    }

    #[test]
    fn dimension_defaults_to_auto() {
        assert_eq!(Dimension::default(), Dimension::Auto);
    }

    #[test]
    fn length_widens_into_dimension() {
        assert_eq!(
            Dimension::from(Length::Points(12.0)),
            Dimension::Points(12.0)
        );
        assert_eq!(
            Dimension::from(Length::Percent(0.5)),
            Dimension::Percent(0.5)
        );
    }

    #[test]
    fn paint_order_has_both_css_values() {
        assert_eq!(PaintOrder::ALL, &[PaintOrder::Fill, PaintOrder::Stroke]);
    }
    #[test]
    fn the_derived_traits_work_on_the_length_types() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        for rendered in [
            format!("{:?}", Length::Percent(0.5)),
            format!("{:?}", Dimension::Points(1.0)),
            format!("{:?}", PaintOrder::Stroke),
        ] {
            assert!(!rendered.is_empty());
        }

        let mut hasher = DefaultHasher::new();
        PaintOrder::Fill.hash(&mut hasher);
        assert_ne!(hasher.finish(), 0);
        assert_eq!(PaintOrder::Fill, PaintOrder::Fill.clone());
    }
}
