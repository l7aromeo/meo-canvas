//! What fills the box: colour, gradient, border, blending.
//!
//! Everything here is resolved rather than described. A colour is four bytes,
//! not `"rebeccapurple"`; a gradient is a stop list, not a `linear-gradient(…)`
//! string. Parsing belongs to the surface that accepted the string, which is
//! the only place that knows the caller's colour space and the only place that
//! can report a parse error against the property the caller wrote.

use crate::{
    geometry::{Corners, Sides},
    style::{Dimension, Length},
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
        /// No line, and no width either.
        ///
        /// CSS's initial value, and CSS's meaning: the style forces the used
        /// border width to zero, so a node with a declared width and this
        /// style neither paints a border nor reserves room for one. It is not
        /// a paint-time skip -- the content box is not inset, which is the
        /// half of the rule a screenshot cannot show.
        None = 3,
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

/// Which way a linear gradient runs.
///
/// A union on one field rather than two kinds of gradient, which is where v1
/// has it (`canvas.type.ts:239`: a `GradientDirection` is four numbers *or* one
/// of eight keywords) and where CSS has it — `linear-gradient()` takes an angle
/// or a `to <side>` and never changes function name for it. A caller porting a
/// linear gradient should not have to work out which kind their direction
/// implies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LinearDirection {
    /// Degrees clockwise from twelve o'clock, as CSS measures them.
    ///
    /// The eight keyword directions are this: `to right` is `90.0`, `to
    /// bottom-right` is `135.0`. Resolving a keyword to an angle is arithmetic
    /// and belongs to whichever surface offered the keyword.
    Angle(f32),
    /// Two explicit endpoints, as fractions of the box.
    ///
    /// What an angle cannot say: where the ramp begins and where it ends, as
    /// against merely which way it runs. v1's four-number direction.
    Between {
        /// Where the first stop sits.
        start: (Length, Length),
        /// Where the last stop sits.
        end: (Length, Length),
    },
}

impl Default for LinearDirection {
    fn default() -> Self {
        // Top to bottom, which is what CSS's `linear-gradient()` does when
        // given no direction at all.
        Self::Angle(180.0)
    }
}

/// A gradient's shape, and the geometry that shape reads.
///
/// One variant per [`GradientKind`], each carrying exactly the fields its kind
/// uses. The fields were flat until the shape changed, and the type's own
/// documentation admitted the problem: `angle_degrees` was ignored by `Radial`
/// and `center` was ignored by `Linear`, so three kinds shared four fields and
/// each read two. A radial gradient can no longer carry an angle nobody reads.
///
/// The tag stays a separate fieldless [`GradientKind`], as [`NodeTag`] is to
/// [`NodeKind`]: `wire_enum!` writes `from_wire`, which turns a byte back into
/// a value and cannot invent a payload to go with it. Keeping the tag apart is
/// what lets both wire formats and the TypeScript keyword table stay exactly as
/// they are.
///
/// [`NodeTag`]: crate::node::NodeTag
/// [`NodeKind`]: crate::node::NodeKind
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientGeometry {
    /// Along a line.
    Linear {
        /// Which way the line runs.
        direction: LinearDirection,
    },
    /// Outward from a point.
    Radial {
        /// The point it radiates from, as a fraction of the box.
        ///
        /// Named `at` rather than `center` because that is what a caller
        /// writes: v1 spells the conic one `at` (`canvas.type.ts:280`) and
        /// both v2 surfaces follow it, under the settled rule that the
        /// JavaScript name is authoritative and Rust snake-cases it.
        at: (Length, Length),
    },
    /// Around a point.
    Conic {
        /// The point it sweeps around, as a fraction of the box.
        at: (Length, Length),
        /// The angle the sweep begins at, in degrees clockwise from twelve
        /// o'clock.
        ///
        /// Named for CSS's own `from <angle>` keyword, which v1 also matched
        /// (`canvas.type.ts:285`). The unit lives in this sentence rather than
        /// in the name: a suffix is what a type reaches for when it cannot say
        /// its unit any other way, and a doc comment can.
        from: f32,
    },
}

impl GradientGeometry {
    /// The point CSS uses when a gradient names none: the middle of the box.
    pub const CENTER: (Length, Length) =
        (Length::Percent(0.5), Length::Percent(0.5));

    /// Which kind this geometry describes.
    #[must_use]
    pub const fn kind(&self) -> GradientKind {
        match self {
            Self::Linear { .. } => GradientKind::Linear,
            Self::Radial { .. } => GradientKind::Radial,
            Self::Conic { .. } => GradientKind::Conic,
        }
    }
}

impl Default for GradientGeometry {
    fn default() -> Self {
        Self::Linear {
            direction: LinearDirection::default(),
        }
    }
}

/// A gradient fill.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    /// The shape, and the geometry that shape reads.
    pub geometry: GradientGeometry,
    /// The stops, in increasing offset order.
    pub stops: Vec<GradientStop>,
}

/// How large a background image is drawn.
///
/// CSS's `background-size`, which is `auto | <length-percentage>{1,2} | cover |
/// contain` and nothing else. Deliberately **not** [`ObjectFit`], which is the
/// tempting reuse: that carries `Fill`, `None` and `ScaleDown`, none of which
/// `background-size` has, and three keywords the renderer must ignore is worse
/// than one more enum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackgroundSize {
    /// A size per axis, where [`Dimension::Auto`] on an axis is that axis's
    /// intrinsic size.
    ///
    /// [`Dimension`] rather than `Option<Length>` because the per-axis auto
    /// then has a name: `background-size: auto 50%` is valid CSS, so the pair
    /// has to stay, and an `Option` would give the all-auto case two spellings
    /// — this variant with two `None`s, and a separate `Auto` variant beside
    /// it. One state, one spelling.
    PerAxis(Dimension, Dimension),
    /// Scaled to cover the box, cropping whichever axis overflows.
    Cover,
    /// Scaled to fit inside the box, leaving whichever axis falls short.
    Contain,
}

impl BackgroundSize {
    /// Both axes at their intrinsic size, which is CSS's initial value.
    pub const AUTO: Self = Self::PerAxis(Dimension::Auto, Dimension::Auto);
}

impl Default for BackgroundSize {
    fn default() -> Self {
        Self::AUTO
    }
}

/// A background image and how it is placed.
#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundImage {
    /// Where the bytes come from.
    pub source: crate::node::ImageSource,
    /// How it tiles.
    pub repeat: BackgroundRepeat,
    /// Drawn size.
    pub size: BackgroundSize,
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
    /// Paint order among siblings, and whether this node isolates them.
    ///
    /// `None` is CSS's `auto` and the initial value. **It is not the same as
    /// `Some(0)`**, and the difference is not the paint order — the two sort
    /// identically — but whether the node establishes a stacking context. A
    /// positioned node at `z-index: 0` establishes one and confines its
    /// negative-`z_index` descendants; the same node at `auto` does not, and
    /// those descendants belong to an ancestor's context instead, where they
    /// paint before this node's own background.
    ///
    /// Measured rather than read off the specification: a positioned parent
    /// with a `z-index: -1` child hides that child at `z-index: auto` and
    /// shows it at `z-index: 0`.
    ///
    /// The distinction has to live in the data, because an `i32` defaulting to
    /// `0` says every positioned node establishes a context and no amount of
    /// care in the painter can recover a difference the scene never recorded.
    /// It is the same rule as [`crate::Scene::gpu`] and an unpinned `inset`
    /// edge: a default that means something other than the same value stated
    /// explicitly cannot be a default, it has to be absent.
    pub z_index: Option<i32>,
}

/// The defaults follow what a browser computes, not what CSS names as initial.
///
/// `border_style` is [`BorderStyle::None`], which is both -- CSS's initial
/// value and what Chrome gives an element that names no style. Where the two
/// part company the browser wins, and `display` is the case that shows why:
/// CSS's initial `display` is `inline`, and a scene is a tree of boxes with no
/// node for inline layout, so the initial value is not representable here at
/// all. What is representable is what Chrome gives a `div`.
///
/// **A default that cannot be reached by writing the value explicitly is not a
/// default**, which is the rule `z_index` above is written to. This is the
/// same rule one level up: a default nobody can name in CSS terms is one a
/// caller cannot reason about from what they know.
impl Default for PaintStyle {
    fn default() -> Self {
        Self {
            background_color: Color::TRANSPARENT,
            gradient: None,
            background_image: None,
            border_color: Sides::all(None),
            border_color_all: Color::BLACK,
            border_style: BorderStyle::None,
            border_radius: Corners::all(0.0),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            dither: false,
            z_index: None,
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
        assert_eq!(style.border_style, BorderStyle::None);
        assert_eq!(style.border_color_all, Color::BLACK);
        assert!((style.opacity - 1.0).abs() < f32::EPSILON);
        assert!(!style.dither);
        assert_eq!(style.z_index, None);
        assert!(style.gradient.is_none());
        assert!(style.background_image.is_none());
    }

    #[test]
    fn every_paint_enum_lists_its_variants() {
        assert_eq!(BorderStyle::ALL.len(), 4);
        assert_eq!(BlendMode::ALL.len(), 16);
        assert_eq!(GradientKind::ALL.len(), 3);
        assert_eq!(ObjectFit::ALL.len(), 5);
        assert_eq!(BackgroundRepeat::ALL.len(), 6);
    }
}
