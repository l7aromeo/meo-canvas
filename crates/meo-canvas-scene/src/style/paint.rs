//! What fills the box: colour, gradient, border, blending.
//!
//! Everything here is resolved rather than described. A colour is four bytes,
//! not `"rebeccapurple"`; a gradient is a stop list, not a `linear-gradient(…)`
//! string. Parsing belongs to the surface that accepted the string, which is
//! the only place that knows the caller's colour space and the only place that
//! can report a parse error against the property the caller wrote.

use crate::{
    geometry::{Corners, Sides},
    style::Length,
    wire::wire_enum,
};

/// A non-premultiplied sRGB colour with an alpha channel.
///
/// Eight bits per channel, matching what a CSS colour string parses to and what
/// the wire format carries in four bytes. Wider colour belongs to the surface,
/// not to a node: a scene names colours the way its author wrote them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel, where `255` is opaque.
    pub a: u8,
}

impl Color {
    /// Opaque black.
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    /// Fully transparent, and the value of an unset colour.
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// Creates an opaque colour.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Creates a colour with an explicit alpha channel.
    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Whether the colour contributes nothing when drawn.
    #[must_use]
    pub const fn is_invisible(self) -> bool {
        self.a == 0
    }
}

wire_enum! {
    /// How a border's line is drawn.
    ///
    /// Yoga lays out a border's width and has no notion of its look, so this
    /// has no Yoga counterpart and takes CSS's keywords directly.
    pub enum BorderStyle {
        /// One unbroken line.
        Solid = 0,
        /// A run of dashes.
        Dashed = 1,
        /// A run of dots, shorter and more numerous than the dashes.
        Dotted = 2,
    }
}

wire_enum! {
    /// How a node's pixels combine with what is behind them.
    ///
    /// The sixteen CSS `mix-blend-mode` keywords, which is the set
    /// `canvas.type.ts` names.
    pub enum BlendMode {
        /// Source over destination.
        Normal = 0,
        /// Product of the two, which darkens.
        Multiply = 1,
        /// Inverse of the product of the inverses, which lightens.
        Screen = 2,
        /// Multiply on dark backdrops, screen on light ones.
        Overlay = 3,
        /// The darker of the two.
        Darken = 4,
        /// The lighter of the two.
        Lighten = 5,
        /// Brightens the backdrop toward the source.
        ColorDodge = 6,
        /// Darkens the backdrop toward the source.
        ColorBurn = 7,
        /// Overlay with the operands exchanged.
        HardLight = 8,
        /// A gentler `HardLight`.
        SoftLight = 9,
        /// Absolute difference of the two.
        Difference = 10,
        /// As `Difference`, with lower contrast.
        Exclusion = 11,
        /// Source hue, backdrop saturation and luminosity.
        Hue = 12,
        /// Source saturation, backdrop hue and luminosity.
        Saturation = 13,
        /// Source hue and saturation, backdrop luminosity.
        Color = 14,
        /// Source luminosity, backdrop hue and saturation.
        Luminosity = 15,
    }
}

wire_enum! {
    /// The shape a gradient's stops are laid along.
    pub enum GradientKind {
        /// Along a line.
        Linear = 0,
        /// Outward from a point.
        Radial = 1,
        /// Around a point.
        Conic = 2,
    }
}

wire_enum! {
    /// How an image fills its box.
    pub enum ObjectFit {
        /// Stretched to the box, ignoring its own ratio.
        Fill = 0,
        /// Scaled to fit inside the box, preserving its ratio.
        Contain = 1,
        /// Scaled to cover the box, preserving its ratio and cropping.
        Cover = 2,
        /// Drawn at its intrinsic size.
        None = 3,
        /// The smaller of `None` and `Contain`.
        ScaleDown = 4,
    }
}

wire_enum! {
    /// How a background image tiles.
    pub enum BackgroundRepeat {
        /// Tiled on both axes.
        Repeat = 0,
        /// Tiled horizontally only.
        RepeatX = 1,
        /// Tiled vertically only.
        RepeatY = 2,
        /// Drawn once.
        NoRepeat = 3,
        /// Tiled whole, with the remainder spread between the tiles.
        Space = 4,
        /// Tiled with the tile scaled so a whole number fits.
        Round = 5,
    }
}

/// One stop of a gradient.
///
/// The offset is explicit rather than inferred from position in the list.
/// `canvas.type.ts` takes a bare colour array and spaces the stops evenly; that
/// spacing is computed at the surface, so a scene records where each stop
/// actually landed and two surfaces cannot space them differently.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    /// Position along the gradient, where `0.0` is the start and `1.0` the
    /// end.
    pub offset: f32,
    /// Colour at that position.
    pub color: Color,
}

/// A gradient fill.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    /// The shape the stops are laid along.
    pub kind: GradientKind,
    /// The stops, in increasing offset order.
    pub stops: Vec<GradientStop>,
    /// Angle in degrees, measured clockwise from twelve o'clock.
    ///
    /// Read by [`GradientKind::Linear`] as the direction of the line and by
    /// [`GradientKind::Conic`] as the angle the sweep begins at. Ignored by
    /// [`GradientKind::Radial`].
    pub angle_degrees: f32,
    /// Centre of a radial or conic gradient, as a fraction of the box.
    ///
    /// `(0.5, 0.5)` is the middle, which is what CSS defaults to.
    pub center: (Length, Length),
}

/// A background image and how it is placed.
#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundImage {
    /// Where the bytes come from.
    pub source: crate::node::ImageSource,
    /// How it tiles.
    pub repeat: BackgroundRepeat,
    /// Drawn size. `None` on an axis leaves that axis at its intrinsic size.
    pub size: (Option<Length>, Option<Length>),
    /// Offset of the first tile from the box's top-left corner.
    pub position: (Length, Length),
}

/// Everything that fills, outlines or composites the box.
#[derive(Debug, Clone, PartialEq)]
pub struct PaintStyle {
    /// Colour painted behind everything else in the node.
    pub background_color: Color,
    /// Gradient painted over the background colour.
    pub gradient: Option<Gradient>,
    /// Image painted over the gradient.
    pub background_image: Option<BackgroundImage>,
    /// Border colour per edge. An edge left `None` takes
    /// [`PaintStyle::border_color_all`].
    pub border_color: Sides<Option<Color>>,
    /// Colour for edges [`PaintStyle::border_color`] leaves unset.
    pub border_color_all: Color,
    /// How the border's line is drawn.
    pub border_style: BorderStyle,
    /// Corner rounding in logical pixels.
    pub border_radius: Corners<f32>,
    /// Whole-node opacity, from `0.0` to `1.0`.
    pub opacity: f32,
    /// How the node composites against what is behind it.
    pub blend_mode: BlendMode,
    /// Whether to dither the fill, which hides banding in a wide gradient at
    /// the cost of a noisier flat area.
    pub dither: bool,
    /// Paint order among siblings. Higher draws later.
    pub z_index: i32,
}

impl Default for PaintStyle {
    fn default() -> Self {
        Self {
            background_color: Color::TRANSPARENT,
            gradient: None,
            background_image: None,
            border_color: Sides::all(None),
            border_color_all: Color::BLACK,
            border_style: BorderStyle::Solid,
            border_radius: Corners::all(0.0),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            dither: false,
            z_index: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackgroundRepeat, BlendMode, BorderStyle, Color, GradientKind,
        ObjectFit, PaintStyle,
    };

    #[test]
    fn rgb_is_opaque_and_rgba_is_not_forced() {
        assert_eq!(Color::rgb(1, 2, 3), Color::rgba(1, 2, 3, 255));
        assert_eq!(Color::BLACK, Color::rgb(0, 0, 0));
        assert!(Color::TRANSPARENT.is_invisible());
        assert!(!Color::BLACK.is_invisible());
        assert_eq!(Color::default(), Color::TRANSPARENT);
    }

    #[test]
    fn paint_defaults_draw_nothing_and_composite_normally() {
        let style = PaintStyle::default();
        assert!(style.background_color.is_invisible());
        assert_eq!(style.blend_mode, BlendMode::Normal);
        assert_eq!(style.border_style, BorderStyle::Solid);
        assert_eq!(style.border_color_all, Color::BLACK);
        assert!((style.opacity - 1.0).abs() < f32::EPSILON);
        assert!(!style.dither);
        assert_eq!(style.z_index, 0);
        assert!(style.gradient.is_none());
        assert!(style.background_image.is_none());
    }

    #[test]
    fn every_paint_enum_lists_its_variants() {
        assert_eq!(BorderStyle::ALL.len(), 3);
        assert_eq!(BlendMode::ALL.len(), 16);
        assert_eq!(GradientKind::ALL.len(), 3);
        assert_eq!(ObjectFit::ALL.len(), 5);
        assert_eq!(BackgroundRepeat::ALL.len(), 6);
    }
}
