//! What is applied to the drawn result: transforms, shadows and masks.
//!
//! These run after the box is filled and its children are drawn, which is why
//! they are not part of [`crate::style::paint`]: a shadow is cast by the
//! composited node, and a transform moves the node and everything inside it.
//!
//! Filters are strings here rather than a parsed operation list. A CSS filter
//! chain is a small language, `meo-skia-canvas` already parses it, and
//! reproducing that grammar in a dependency-free crate would give the workspace
//! two parsers that can disagree.

use crate::{
    style::{
        Length,
        paint::{Color, Gradient},
    },
    wire::wire_enum,
};

/// A 2D transform applied to the node and its subtree.
///
/// Applied about [`Transform::origin`], which is a fraction of the node's own
/// box rather than an absolute point, so a transform written once behaves the
/// same on a box of any size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Horizontal translation.
    pub translate_x: Length,
    /// Vertical translation.
    pub translate_y: Length,
    /// Rotation in degrees, clockwise.
    ///
    /// Degrees rather than radians because that is what the CSS property and
    /// `canvas.type.ts` both use, and converting at the surface would put a
    /// rounding step between what a caller wrote and what is stored.
    pub rotate_degrees: f32,
    /// Horizontal scale factor.
    pub scale_x: f32,
    /// Vertical scale factor.
    pub scale_y: f32,
    /// The point the transform is applied about, as a fraction of the box.
    pub origin: (Length, Length),
}

impl Transform {
    /// The centre of the box, which is what CSS's `transform-origin` defaults
    /// to.
    pub const ORIGIN_CENTER: (Length, Length) =
        (Length::Percent(0.5), Length::Percent(0.5));
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translate_x: Length::ZERO,
            translate_y: Length::ZERO,
            rotate_degrees: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            origin: Self::ORIGIN_CENTER,
        }
    }
}

/// A shadow cast by the node's box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    /// Whether the shadow falls inside the box rather than outside it.
    pub inset: bool,
    /// Horizontal offset in logical pixels.
    pub offset_x: f32,
    /// Vertical offset in logical pixels.
    pub offset_y: f32,
    /// Blur radius in logical pixels.
    pub blur: f32,
    /// How far the shadow's shape grows beyond the box before blurring.
    pub spread: f32,
    /// Shadow colour.
    pub color: Color,
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            inset: false,
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
            color: Color::BLACK,
        }
    }
}

/// A shadow cast by glyphs.
///
/// Separate from [`BoxShadow`] because CSS's `text-shadow` has no spread and no
/// inset, and offering fields the renderer must ignore is worse than not
/// offering them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextShadow {
    /// Horizontal offset in logical pixels.
    pub offset_x: f32,
    /// Vertical offset in logical pixels.
    pub offset_y: f32,
    /// Blur radius in logical pixels.
    pub blur: f32,
    /// Shadow colour.
    pub color: Color,
}

impl Default for TextShadow {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            color: Color::BLACK,
        }
    }
}

wire_enum! {
    /// A shape a mask can name without writing a path.
    ///
    /// Each is inscribed in the node's own box, so it needs no coordinates.
    pub enum MaskShape {
        /// The largest circle that fits.
        Circle = 0,
        /// The ellipse that fills the box.
        Ellipse = 1,
    }
}

wire_enum! {
    /// Which side of a path's winding counts as inside.
    pub enum FillRule {
        /// Inside where the winding number is not zero.
        NonZero = 0,
        /// Inside where the crossing count is odd.
        EvenOdd = 1,
    }
}

/// What restricts a node's drawing to part of its box.
#[derive(Debug, Clone, PartialEq)]
pub enum Mask {
    /// An image whose alpha channel is the mask.
    Image(crate::node::ImageSource),
    /// A shape inscribed in the node's box.
    Shape(MaskShape),
    /// SVG path data, in the node's own coordinate space.
    Path {
        /// The `d` attribute of an SVG path.
        data: String,
        /// Which side of the winding counts as inside.
        fill_rule: FillRule,
    },
    /// A gradient whose alpha channel is the mask, which is how a fade-out edge
    /// is written.
    Gradient(Gradient),
}

/// Everything applied to the node after it and its children are drawn.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Effects {
    /// Transform applied to the node and its subtree.
    pub transform: Option<Transform>,
    /// Shadows cast by the box, drawn in order.
    pub box_shadows: Vec<BoxShadow>,
    /// Shadows cast by glyphs, drawn in order.
    pub text_shadows: Vec<TextShadow>,
    /// What restricts the node's drawing.
    pub mask: Option<Mask>,
    /// A CSS filter chain applied to the node's own pixels.
    pub filter: Option<String>,
    /// A CSS filter chain applied to what is behind the node.
    pub backdrop_filter: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        BoxShadow, Effects, FillRule, MaskShape, TextShadow, Transform,
    };
    use crate::style::{Length, paint::Color};

    #[test]
    fn transform_defaults_to_the_identity_about_the_centre() {
        let transform = Transform::default();
        assert_eq!(transform.translate_x, Length::ZERO);
        assert!((transform.scale_x - 1.0).abs() < f32::EPSILON);
        assert!((transform.scale_y - 1.0).abs() < f32::EPSILON);
        assert!((transform.rotate_degrees - 0.0).abs() < f32::EPSILON);
        assert_eq!(transform.origin, Transform::ORIGIN_CENTER);
    }

    #[test]
    fn shadows_default_to_black_and_unoffset() {
        let box_shadow = BoxShadow::default();
        assert!(!box_shadow.inset);
        assert_eq!(box_shadow.color, Color::BLACK);
        assert!((box_shadow.spread - 0.0).abs() < f32::EPSILON);
        assert_eq!(TextShadow::default().color, Color::BLACK);
    }

    #[test]
    fn effects_default_to_nothing_applied() {
        let effects = Effects::default();
        assert!(effects.transform.is_none());
        assert!(effects.box_shadows.is_empty());
        assert!(effects.text_shadows.is_empty());
        assert!(effects.mask.is_none());
        assert!(effects.filter.is_none());
        assert!(effects.backdrop_filter.is_none());
    }

    #[test]
    fn every_effect_enum_lists_its_variants() {
        assert_eq!(MaskShape::ALL.len(), 2);
        assert_eq!(FillRule::ALL.len(), 2);
    }
}
