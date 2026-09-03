//! One flat style, as CSS writes one.
//!
//! The scene keeps layout, paint, text and effects in four structs because the
//! codec needs them separated. Authoring does not: a caller writing `gap`
//! beside `background_color` should never have to know which group either lives
//! in, and v1's `BoxProps` already mixed all four. [`Style::into_parts`] does
//! the splitting at the moment the tree becomes a scene.
//!
//! ```
//! use meo_canvas::{Style, all, hex_rgb, px};
//!
//! const CARD: Style = Style::new()
//!     .padding(all(px(24.0)))
//!     .gap(px(16.0))
//!     .border_radius(12.0);
//!
//! let dark = CARD.background_color(hex_rgb(0x10_10_14));
//! let light = CARD.background_color(hex_rgb(0xf4_f4_f6));
//! ```
//!
//! Setters take `self` and are `const` wherever the property allows, which is
//! what makes that `const CARD` work: a `const` is substituted at each use, so
//! every mention is a fresh value the setter may consume. No clone and no
//! lifetime.
//!
//! A property whose value is a `String` or a `Vec` cannot have a `const`
//! setter, because assigning one drops the value it replaces and a `const fn`
//! cannot drop. [`Style::font_family`] and [`Style::box_shadow`] are the
//! notable ones. Where a base style needs those, a function returning a `Style`
//! serves the purpose a `const` serves elsewhere:
//!
//! ```
//! use meo_canvas::{Style, px};
//!
//! fn heading() -> Style {
//!     Style::new().font_size(24.0).bold().font_family("Inter")
//! }
//!
//! let title = heading().letter_spacing(px(-0.5));
//! ```

use meo_canvas_scene::{
    Corners, Sides,
    style::{
        Dimension, Length, PaintOrder,
        effect::{BoxShadow, Effects, Mask, TextShadow, Transform},
        layout::{
            Align, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
            GridAutoFlow, GridPlacement, Justify, LayoutStyle, Overflow,
            PositionType, TrackSize,
        },
        paint::{
            BackgroundImage, BlendMode, BorderStyle, Color, Gradient,
            ObjectFit, PaintStyle,
        },
        text::{
            FontStyle, FontVariant, FontWeight, LineHeight, Spacing, TextAlign,
            TextDecoration, TextStroke, TextStyle, VerticalAlign,
        },
    },
};

/// Everything a node can be styled with, in one type.
///
/// Every field is public, so a property with no setter is still reachable by
/// literal:
///
/// ```
/// use meo_canvas::{Style, px};
///
/// let golden = Style {
///     aspect_ratio: Some(1.618),
///     ..Style::new().gap(px(8.0))
/// };
/// ```
// Not `#[non_exhaustive]`: the documented way to reach a property with no
// setter is a literal closed with `..Style::new()`, and that attribute forbids
// the literal outright. The rest pattern is what keeps such a literal compiling
// when a field is added, which is the protection `non_exhaustive` would have
// bought.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Style {
    // -- Layout ---------------------------------------------------------
    /// How this node's children are arranged.
    pub display: Option<Display>,
    /// Whether the node is placed by the flow or by its own offsets.
    pub position_type: Option<PositionType>,
    /// Offsets from the container's edges.
    pub position: Option<Sides<Option<Length>>>,
    /// Requested width.
    pub width: Option<Dimension>,
    /// Requested height.
    pub height: Option<Dimension>,
    /// Lower bound on the width.
    pub min_width: Option<Dimension>,
    /// Lower bound on the height.
    pub min_height: Option<Dimension>,
    /// Upper bound on the width.
    pub max_width: Option<Dimension>,
    /// Upper bound on the height.
    pub max_height: Option<Dimension>,
    /// Width divided by height, honoured when one axis is automatic.
    pub aspect_ratio: Option<f32>,
    /// Space outside the border.
    pub margin: Option<Sides<Dimension>>,
    /// Space inside the border.
    pub padding: Option<Sides<Length>>,
    /// Border thickness, which occupies space whether or not it is painted.
    pub border: Option<Sides<f32>>,
    /// The axis children run along.
    pub flex_direction: Option<FlexDirection>,
    /// Whether children overflow onto further lines.
    pub flex_wrap: Option<FlexWrap>,
    /// Share of free space this node absorbs.
    pub flex_grow: Option<f32>,
    /// Share of overflow this node gives up.
    pub flex_shrink: Option<f32>,
    /// Size along the main axis before growing or shrinking.
    pub flex_basis: Option<Dimension>,
    /// Main-axis distribution.
    pub justify_content: Option<Justify>,
    /// Cross-axis placement of children.
    pub align_items: Option<Align>,
    /// This node's own cross-axis placement.
    pub align_self: Option<Align>,
    /// Cross-axis distribution of wrapped lines.
    pub align_content: Option<Align>,
    /// Space between children, as `(row, column)` after CSS's shorthand.
    pub gap: Option<(Length, Length)>,
    /// Clipping behaviour, per axis as `(x, y)`.
    pub overflow: Option<(Overflow, Overflow)>,
    /// Whether `width` and `height` include padding and border.
    pub box_sizing: Option<BoxSizing>,
    /// Inline direction, which decides which edge is the start.
    pub direction: Option<Direction>,
    /// Column tracks of a grid.
    pub grid_template_columns: Option<Vec<TrackSize>>,
    /// Row tracks of a grid.
    pub grid_template_rows: Option<Vec<TrackSize>>,
    /// Size given to rows the template does not name.
    pub grid_auto_rows: Option<TrackSize>,
    /// Size given to columns the template does not name.
    pub grid_auto_columns: Option<TrackSize>,
    /// The order auto-placement fills tracks in.
    pub grid_auto_flow: Option<GridAutoFlow>,
    /// Where a grid item sits on the column axis.
    pub grid_column: Option<GridPlacement>,
    /// Where a grid item sits on the row axis.
    pub grid_row: Option<GridPlacement>,

    // -- Paint ----------------------------------------------------------
    /// The box's fill. CSS's `background-color`.
    pub background_color: Option<Color>,
    /// A gradient painted over the fill.
    pub gradient: Option<Gradient>,
    /// An image painted over the gradient.
    pub background_image: Option<BackgroundImage>,
    /// Border colour, per edge.
    pub border_color: Option<Sides<Option<Color>>>,
    /// Border colour for every edge that names none of its own.
    pub border_color_all: Option<Color>,
    /// Whether the border is solid, dashed or dotted.
    pub border_style: Option<BorderStyle>,
    /// Corner radii.
    pub border_radius: Option<Corners<f32>>,
    /// Opacity of this node and its subtree, from `0.0` to `1.0`.
    pub opacity: Option<f32>,
    /// How this node composites onto what is beneath it.
    pub mix_blend_mode: Option<BlendMode>,
    /// Whether gradients are dithered.
    pub dither: Option<bool>,
    /// Paint order among positioned siblings.
    pub z_index: Option<i32>,
    /// How an image fills its box. Read only by an image node.
    pub object_fit: Option<ObjectFit>,
    /// Where an image sits in its box when it does not fill it, as a fraction
    /// of the leftover space on each axis. Read only by an image node.
    pub object_position: Option<(Length, Length)>,
    /// Which frame of an animated source to draw. Read only by an image node.
    pub frame: Option<u32>,

    // -- Text -----------------------------------------------------------
    /// The family name text is drawn in. Inherits.
    pub font_family: Option<String>,
    /// Em size in logical pixels. Inherits.
    pub font_size: Option<f32>,
    /// Weight from 1 to 1000. Inherits.
    pub font_weight: Option<FontWeight>,
    /// Upright or italic. Inherits.
    pub font_style: Option<FontStyle>,
    /// The colour glyphs are drawn in, CSS's `color`. Inherits.
    pub color: Option<Color>,
    /// Horizontal alignment within the box. Inherits.
    pub text_align: Option<TextAlign>,
    /// A line through, over or under. Inherits.
    pub text_decoration: Option<TextDecoration>,
    /// Where a line sits within its box. Inherits.
    pub vertical_align: Option<VerticalAlign>,
    /// Which of a glyph's fill and stroke is on top. Inherits.
    pub paint_order: Option<PaintOrder>,
    /// How tall a line box is. `None` is CSS's `normal`. Inherits.
    pub line_height: Option<LineHeight>,
    /// Extra space between lines, in pixels. Inherits.
    pub line_gap: Option<f32>,
    /// OpenType features applied to the run. Inherits.
    pub font_variant: Option<Vec<FontVariant>>,
    /// Space added between characters. Inherits.
    pub letter_spacing: Option<Spacing>,
    /// Space added between words. Inherits.
    pub word_spacing: Option<Spacing>,
    /// An outline drawn on the glyphs. Inherits.
    pub text_stroke: Option<TextStroke>,

    // -- Effects --------------------------------------------------------
    /// A transform applied to this node and its subtree.
    pub transform: Option<Transform>,
    /// Shadows cast by the box.
    pub box_shadows: Option<Vec<BoxShadow>>,
    /// Shadows cast by the glyphs.
    pub text_shadows: Option<Vec<TextShadow>>,
    /// A shape or gradient the subtree is clipped to.
    pub mask: Option<Mask>,
    /// A CSS filter applied to this node's own drawing.
    pub filter: Option<String>,
    /// A CSS filter applied to what shows through this node.
    pub backdrop_filter: Option<String>,
}

/// The property table: every flat setter, written once.
///
/// One list produces two things — the setters on [`Style`], and the same
/// setters on [`Styled`], which every node implements. Writing them twice is
/// what the sixty-five-methods-per-node objection was actually about, and a
/// second list is a second place for a property to be forgotten: a node would
/// simply lack a setter, with nothing failing to compile.
///
/// `plain` is a `const fn`, `owned` is not. The line between them is whether
/// the field needs dropping — assigning over an owning field in a `const fn` is
/// E0493, which `gradient` and `mask` hit despite carrying no `String` of their
/// own. An `owned` setter takes `impl Into<_>` as well, since a heap value is
/// the kind a caller usually has in another form.
macro_rules! properties {
    (
        fields {
            $(
                $(#[$doc:meta])*
                $kind:ident $name:ident: $field:ident, $type:ty;
            )+
        }
        via {
            $( $via:ident($($arg:ident: $arg_type:ty),*); )*
        }
    ) => {
        /// How many tracks lie between two grid lines, at least one.
///
/// An end at or before its start is an empty area rather than a placement, and
/// a `const fn` on the authoring surface has nowhere to report that to — so it
/// becomes the smallest placement that means anything. The JavaScript surface
/// refuses the same input, where a throw is what a caller expects.
#[expect(
    clippy::cast_sign_loss,
    reason = "the subtraction is guarded to a positive difference"
)]
const fn span_between(start: i16, end: i16) -> u16 {
    if end > start {
        end.saturating_sub(start) as u16
    } else {
        1
    }
}

impl Style {
            $( properties!(@style $kind $(#[$doc])* $name: $field, $type); )+
        }

        /// The setters every node carries, written flat.
        ///
        /// A node is styled by naming properties on it directly —
        /// `Row::new().gap(px(16.0))` — rather than by handing it a style
        /// object. The methods are here, once, and a node type implements this
        /// by pointing at the [`Style`] it holds: one line per node type
        /// against sixty-nine methods.
        ///
        /// ```
        /// use meo_canvas::{Row, Styled, hex, px};
        ///
        /// let card = Row::new().gap(px(16.0)).background_color(hex("#101014"));
        /// ```
        ///
        /// [`Style`] keeps the same setters as `const fn`s, because a reusable
        /// `const CARD: Style` is worth having and a trait method cannot be
        /// one. The two lists cannot drift: they are one list.
        pub trait Styled: Sized {
            /// The style this node carries.
            ///
            /// The one method a node type writes. Everything else here is
            /// provided.
            fn style_mut(&mut self) -> &mut Style;

            $( properties!(@node $kind $name, $type); )+

            $(
                #[doc = concat!(
                    "Sets [`", stringify!($via), "`](Style::", stringify!($via), ") on this node."
                )]
                #[must_use]
                fn $via(mut self, $($arg: $arg_type),*) -> Self {
                    let style = ::core::mem::replace(
                        <Self as Styled>::style_mut(&mut self),
                        Style::new(),
                    );
                    *<Self as Styled>::style_mut(&mut self) = style.$via($($arg),*);
                    self
                }
            )*
        }
    };

    (@style plain $(#[$doc:meta])* $name:ident: $field:ident, $type:ty) => {
        $(#[$doc])*
        #[must_use]
        pub const fn $name(mut self, value: $type) -> Self {
            self.$field = Some(value);
            self
        }
    };

    (@style sides $(#[$doc:meta])* $name:ident: $field:ident, $type:ty) => {
        $(#[$doc])*
        #[must_use]
        pub const fn $name(mut self, value: Sides<$type>) -> Self {
            self.$field = Some(value);
            self
        }
    };

    (@style corners $(#[$doc:meta])* $name:ident: $field:ident, $type:ty) => {
        $(#[$doc])*
        #[must_use]
        pub const fn $name(mut self, value: Corners<$type>) -> Self {
            self.$field = Some(value);
            self
        }
    };

    (@node sides $name:ident, $type:ty) => {
        #[doc = concat!(
            "Sets [`", stringify!($name), "`](Style::", stringify!($name), ") on this node."
        )]
        #[doc = ""]
        #[doc = "One value for every edge, or the four named."]
        #[must_use]
        fn $name(mut self, value: impl crate::unit::IntoSides<$type>) -> Self {
            let style = ::core::mem::replace(
                <Self as Styled>::style_mut(&mut self),
                Style::new(),
            );
            *<Self as Styled>::style_mut(&mut self) = style.$name(value.into_sides());
            self
        }
    };

    (@node corners $name:ident, $type:ty) => {
        #[doc = concat!(
            "Sets [`", stringify!($name), "`](Style::", stringify!($name), ") on this node."
        )]
        #[doc = ""]
        #[doc = "One radius for every corner, or the four named."]
        #[must_use]
        fn $name(mut self, value: impl crate::unit::IntoCorners<$type>) -> Self {
            let style = ::core::mem::replace(
                <Self as Styled>::style_mut(&mut self),
                Style::new(),
            );
            *<Self as Styled>::style_mut(&mut self) = style.$name(value.into_corners());
            self
        }
    };

    (@style owned $(#[$doc:meta])* $name:ident: $field:ident, $type:ty) => {
        $(#[$doc])*
        #[must_use]
        pub fn $name(mut self, value: impl Into<$type>) -> Self {
            self.$field = Some(value.into());
            self
        }
    };

    (@node plain $name:ident, $type:ty) => {
        #[doc = concat!(
            "Sets [`", stringify!($name), "`](Style::", stringify!($name), ") on this node."
        )]
        #[must_use]
        fn $name(mut self, value: $type) -> Self {
            let style = ::core::mem::replace(
                <Self as Styled>::style_mut(&mut self),
                Style::new(),
            );
            *<Self as Styled>::style_mut(&mut self) = style.$name(value);
            self
        }
    };

    (@node owned $name:ident, $type:ty) => {
        #[doc = concat!(
            "Sets [`", stringify!($name), "`](Style::", stringify!($name), ") on this node."
        )]
        #[must_use]
        fn $name(mut self, value: impl Into<$type>) -> Self {
            let style = ::core::mem::replace(self.style_mut(), Style::new());
            *self.style_mut() = style.$name(value);
            self
        }
    };
}

properties! {
    fields {
        /// How this node's children are arranged.
        ///
        /// ```
        /// use meo_canvas::{Style, scene::Display};
        ///
        /// const SHEET: Style = Style::new().display(Display::Grid);
        /// ```
    plain display: display, Display;

        /// Whether the node is placed by the flow or by its own `position`.
    plain position_type: position_type, PositionType;

        /// Offsets from the container's edges.
    sides position: position, Option<Length>;

        /// Width divided by height, honoured when one axis is automatic.
    plain aspect_ratio: aspect_ratio, f32;

        /// Space outside the border.
    sides margin: margin, Dimension;

        /// Space inside the border.
        ///
        /// ```
        /// use meo_canvas::{Style, all, px, xy};
        ///
        /// const EVEN: Style = Style::new().padding(all(px(24.0)));
        /// const TIGHT: Style = Style::new().padding(xy(px(8.0), px(16.0)));
        /// ```
    sides padding: padding, Length;

        /// Border thickness, which occupies space whether or not it is painted.
    sides border: border, f32;

        /// The axis children run along.
    plain flex_direction: flex_direction, FlexDirection;

        /// Whether children overflow onto further lines.
    plain flex_wrap: flex_wrap, FlexWrap;

        /// Share of free space this node absorbs.
    plain flex_grow: flex_grow, f32;

        /// Share of overflow this node gives up.
    plain flex_shrink: flex_shrink, f32;

        /// Size along the main axis before growing or shrinking.
    plain flex_basis: flex_basis, Dimension;

        /// Main-axis distribution of children.
        ///
        /// ```
        /// use meo_canvas::{Style, scene::Justify};
        ///
        /// const SPREAD: Style = Style::new().justify_content(Justify::SpaceBetween);
        /// ```
    plain justify_content: justify_content, Justify;

        /// Cross-axis placement of children.
    plain align_items: align_items, Align;

        /// This node's own cross-axis placement, overriding its parent's.
    plain align_self: align_self, Align;

        /// Cross-axis distribution of wrapped lines.
    plain align_content: align_content, Align;

        /// Whether `width` and `height` include padding and border.
    plain box_sizing: box_sizing, BoxSizing;

        /// Inline direction, which decides which edge is the start.
    plain direction: direction, Direction;

        /// The grid's column tracks.
        ///
        /// ```
        /// use meo_canvas::{Style, fr, px, track, scene::Display};
        ///
        /// let sidebar = Style::new()
        ///     .display(Display::Grid)
        ///     .grid_template_columns([track(px(240.0)), fr(1.0)]);
        /// ```
    owned grid_template_columns: grid_template_columns, Vec<TrackSize>;

        /// The grid's row tracks.
    owned grid_template_rows: grid_template_rows, Vec<TrackSize>;

        /// Size given to rows the template does not name.
    plain grid_auto_rows: grid_auto_rows, TrackSize;

        /// Size given to columns the template does not name.
    plain grid_auto_columns: grid_auto_columns, TrackSize;

        /// The order auto-placement fills tracks in.
    plain grid_auto_flow: grid_auto_flow, GridAutoFlow;

        /// Where this item sits on the column axis.
    plain grid_column: grid_column, GridPlacement;

        /// Where this item sits on the row axis.
    plain grid_row: grid_row, GridPlacement;

        /// The box's fill.
        ///
        /// CSS's `background-color`, and distinct from [`color`](Self::color),
        /// which is the text colour. The two sit adjacent and mean different
        /// things; that is CSS's trap and keeping its names is what lets a
        /// design be ported without translation.
        ///
        /// ```
        /// use meo_canvas::{Style, hex_rgb};
        ///
        /// const PANEL: Style = Style::new().background_color(hex_rgb(0x10_10_14));
        /// ```
    plain background_color: background_color, Color;

        /// A gradient painted over the fill.
        ///
        /// Not `const`: a gradient owns its stops, and assigning over one drops
        /// the vector it replaces.
    owned gradient: gradient, Gradient;

        /// An image painted over the gradient.
        ///
        /// The tiling, the drawn size and the offset of the first tile travel
        /// with the source rather than as three properties beside it, which is
        /// where CSS's `background-repeat`, `background-size` and
        /// `background-position` each sit on their own -- one value here cannot
        /// describe a repeat for an image that is not there.
        ///
        /// ```
        /// use meo_canvas::{Style, px, scene::{BackgroundImage, BackgroundRepeat, BackgroundSize, ImageSource}};
        ///
        /// let tiled = Style::new().background_image(BackgroundImage {
        ///     source: ImageSource::Path("grain.png".into()),
        ///     repeat: BackgroundRepeat::Repeat,
        ///     size: BackgroundSize::PerAxis(px(32.0).into(), px(32.0).into()),
        ///     position: (px(0.0), px(0.0)),
        /// });
        /// ```
    owned background_image: background_image, BackgroundImage;

        /// One border colour on every edge.
    plain border_color: border_color_all, Color;

        /// Border colour per edge, for a box whose edges differ.
    sides border_color_sides: border_color, Option<Color>;

        /// Whether the border is solid, dashed or dotted.
    plain border_style: border_style, BorderStyle;

        /// Corner radii, each named.
    corners border_radius_corners: border_radius, f32;

        /// Opacity of this node and its subtree, from `0.0` to `1.0`.
    plain opacity: opacity, f32;

        /// How this node composites onto what is beneath it.
    plain mix_blend_mode: mix_blend_mode, BlendMode;

        /// Whether gradients are dithered.
    plain dither: dither, bool;

        /// Paint order among positioned siblings.
    plain z_index: z_index, i32;

        /// How an image fills its box.
        ///
        /// ```
        /// use meo_canvas::{Image, Styled, px, scene::ObjectFit};
        ///
        /// let avatar = Image::path("a.png")
        ///     .size(px(64.0), px(64.0))
        ///     .object_fit(ObjectFit::Cover);
        /// ```
    plain object_fit: object_fit, ObjectFit;

        /// Where an image sits in its box when it does not fill it.
    plain object_position: object_position, (Length, Length);

        /// Which frame of an animated source to draw.
    plain frame: frame, u32;

        /// The colour glyphs are drawn in.
        ///
        /// CSS's `color`: it inherits, so setting it on a container reaches
        /// every descendant. See [`background_color`](Self::background_color) for the fill.
    plain color: color, Color;

        /// The family name text is drawn in.
        ///
        /// Not `const`, because the name is a `String`. A base style needing one
        /// is a function returning a [`Style`] rather than a `const`.
    owned font_family: font_family, String;

        /// Em size in logical pixels.
    plain font_size: font_size, f32;

        /// Weight from 1 to 1000.
    plain font_weight: font_weight, FontWeight;

        /// Upright or italic, named.
    plain font_style: font_style, FontStyle;

        /// Horizontal alignment within the box.
    plain text_align: text_align, TextAlign;

        /// A line through, over or under the text.
    plain text_decoration: text_decoration, TextDecoration;

        /// Where a line sits within its box.
    plain vertical_align: vertical_align, VerticalAlign;

        /// Which of a glyph's fill and stroke is painted on top.
    plain paint_order: paint_order, PaintOrder;

        /// OpenType features applied to the run.
        ///
        /// A list because CSS's `font-variant` is a space-separated shorthand
        /// and a caller routinely wants two at once -- `small-caps
        /// tabular-nums` is one setting rather than a choice between them.
        ///
        /// Reaches the **measurer** as well as the painter:
        /// `DiagonalFractions` moves a nineteen-character sample from 220.61
        /// to 211.04, so a feature that only reached the drawing would lay
        /// text out at one width and paint it at another.
        ///
        /// **A feature does nothing unless the face carries it.** Seventeen
        /// tags swept against this repository's Oswald move exactly one,
        /// `frac`: that face has no small-caps glyphs and nothing synthesises
        /// them, so `SmallCaps` draws what `Normal` draws and is not a defect.
        /// A test reaching for "a representative feature" and picking the
        /// wrong one reports a working property as dead.
        ///
        /// ```
        /// use meo_canvas::{Style, scene::FontVariant};
        ///
        /// let recipe =
        ///     Style::new().font_variant([FontVariant::DiagonalFractions]);
        /// ```
    owned font_variant: font_variant, Vec<FontVariant>;

        /// How tall a line box is, in any of CSS's three stated spellings.
        ///
        /// ```
        /// use meo_canvas::{LineHeight, Style};
        ///
        /// // A multiple of the font size, recomputed by whoever inherits it.
        /// const LOOSE: Style = Style::new().line_height(LineHeight::Number(1.4));
        /// // An absolute height, which descends unchanged.
        /// const FIXED: Style = Style::new().line_height(LineHeight::Length(24.0));
        /// // A share of THIS element's size, resolved here and inherited as
        /// // the length it comes to.
        /// const HALF: Style = Style::new().line_height(LineHeight::Percent(1.5));
        /// ```
        ///
        /// **Saying nothing is CSS's `normal`** and is not the same as
        /// `Number(1.0)`, which is a line box exactly one em tall.
    plain line_height: line_height, LineHeight;

        /// Extra space between lines, in pixels.
    plain line_gap: line_gap, f32;

        /// An outline drawn on the glyphs.
    plain text_stroke: text_stroke, TextStroke;

        /// A transform applied to this node and its subtree.
    plain transform: transform, Transform;

        /// Shadows cast by the box, painted in the order given.
    owned box_shadow: box_shadows, Vec<BoxShadow>;

        /// Shadows cast by the glyphs.
    owned text_shadow: text_shadows, Vec<TextShadow>;

        /// A shape or gradient the subtree is clipped to.
        ///
        /// Not `const`, for the same reason [`gradient`](Self::gradient) is not:
        /// a path mask owns its data and a gradient mask owns its stops.
    owned mask: mask, Mask;

        /// A CSS filter applied to this node's own drawing.
    owned filter: filter, String;

        /// A CSS filter applied to what shows through this node.
    owned backdrop_filter: backdrop_filter, String;
    }

    via {
        width(width: Length);
        height(height: Length);
        size(width: Length, height: Length);
        min_width(width: Length);
        min_height(height: Length);
        max_width(width: Length);
        max_height(height: Length);
        gap(gap: Length);
        gap_xy(row: Length, column: Length);
        overflow(overflow: Overflow);
        border_radius(border_radius: f32);
        bold();
        italic();
        letter_spacing(spacing: Length);
        word_spacing(spacing: Length);
    }
}

impl Style {
    // -- Layout ---------------------------------------------------------

    // -- Paint ----------------------------------------------------------

    // -- Image ----------------------------------------------------------
    //
    // Three properties that belong to an image rather than to a box. They live
    // on `Style` because the surface is one flat style and a caller writing
    // `.style(Style::new().size(..).object_fit(Cover))` should not have to know
    // that two of those three words are read by different halves of the
    // scene. A node that is not an image ignores them, which is what CSS
    // does with a property a element does not define.

    // -- Text -----------------------------------------------------------

    // -- Effects --------------------------------------------------------

    /// Layers another style over this one, property by property.
    ///
    /// A property the argument names wins; a property it leaves absent leaves
    /// this style's value alone. `None` is not a value to be copied over — it
    /// is the absence of one, which is what makes a partial style partial.
    ///
    /// ```
    /// use meo_canvas::{Style, all, px};
    ///
    /// const CARD: Style = Style::new().padding(all(px(24.0))).gap(px(16.0));
    ///
    /// // The gap is overridden; the padding, which `tight` never mentions,
    /// // survives.
    /// let tight = CARD.merge(Style::new().gap(px(8.0)));
    /// assert_eq!(tight.gap, Some((px(8.0), px(8.0))));
    /// assert!(tight.padding.is_some());
    /// ```
    ///
    /// This is what [`Element::with_style`](crate::Element::with_style) does,
    /// and what the JavaScript surface's props spread has always done — a
    /// container factory there is `{ display: 'grid', ...props }`, so a caller
    /// who does not name `display` keeps the one the factory set. A replace
    /// would make the two surfaces disagree about the same call.
    ///
    /// Not a `const fn`: assigning over a field that owns a heap value in a
    /// `const fn` is E0493, the same rule that keeps the `owned` setters out of
    /// `const`.
    ///
    /// # A field added to `Style` will not compile until it is merged here
    ///
    /// The argument is destructured without a rest pattern, deliberately. A
    /// sixty-ninth property that this method forgot would otherwise be a
    /// property that silently does not carry — visible only as a picture that
    /// came out wrong, with nothing to point at. Here it is a build error
    /// naming the field.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "one line per property, which is the exhaustiveness that makes a forgotten field a build error"
    )]
    pub fn merge(mut self, other: Self) -> Self {
        let Self {
            display,
            position_type,
            position,
            width,
            height,
            min_width,
            min_height,
            max_width,
            max_height,
            aspect_ratio,
            margin,
            padding,
            border,
            flex_direction,
            flex_wrap,
            flex_grow,
            flex_shrink,
            flex_basis,
            justify_content,
            align_items,
            align_self,
            align_content,
            gap,
            overflow,
            box_sizing,
            direction,
            grid_template_columns,
            grid_template_rows,
            grid_auto_rows,
            grid_auto_columns,
            grid_auto_flow,
            grid_column,
            grid_row,
            background_color,
            gradient,
            background_image,
            border_color,
            border_color_all,
            border_style,
            border_radius,
            opacity,
            mix_blend_mode,
            dither,
            z_index,
            object_fit,
            object_position,
            frame,
            font_family,
            font_size,
            font_weight,
            font_style,
            color,
            text_align,
            text_decoration,
            vertical_align,
            paint_order,
            line_height,
            line_gap,
            font_variant,
            letter_spacing,
            word_spacing,
            text_stroke,
            transform,
            box_shadows,
            text_shadows,
            mask,
            filter,
            backdrop_filter,
        } = other;

        self.display = display.or(self.display);
        self.position_type = position_type.or(self.position_type);
        self.position = position.or(self.position);
        self.width = width.or(self.width);
        self.height = height.or(self.height);
        self.min_width = min_width.or(self.min_width);
        self.min_height = min_height.or(self.min_height);
        self.max_width = max_width.or(self.max_width);
        self.max_height = max_height.or(self.max_height);
        self.aspect_ratio = aspect_ratio.or(self.aspect_ratio);
        self.margin = margin.or(self.margin);
        self.padding = padding.or(self.padding);
        self.border = border.or(self.border);
        self.flex_direction = flex_direction.or(self.flex_direction);
        self.flex_wrap = flex_wrap.or(self.flex_wrap);
        self.flex_grow = flex_grow.or(self.flex_grow);
        self.flex_shrink = flex_shrink.or(self.flex_shrink);
        self.flex_basis = flex_basis.or(self.flex_basis);
        self.justify_content = justify_content.or(self.justify_content);
        self.align_items = align_items.or(self.align_items);
        self.align_self = align_self.or(self.align_self);
        self.align_content = align_content.or(self.align_content);
        self.gap = gap.or(self.gap);
        self.overflow = overflow.or(self.overflow);
        self.box_sizing = box_sizing.or(self.box_sizing);
        self.direction = direction.or(self.direction);
        self.grid_template_columns =
            grid_template_columns.or(self.grid_template_columns);
        self.grid_template_rows =
            grid_template_rows.or(self.grid_template_rows);
        self.grid_auto_rows = grid_auto_rows.or(self.grid_auto_rows);
        self.grid_auto_columns = grid_auto_columns.or(self.grid_auto_columns);
        self.grid_auto_flow = grid_auto_flow.or(self.grid_auto_flow);
        self.grid_column = grid_column.or(self.grid_column);
        self.grid_row = grid_row.or(self.grid_row);
        self.background_color = background_color.or(self.background_color);
        self.gradient = gradient.or(self.gradient);
        self.background_image = background_image.or(self.background_image);
        self.border_color = border_color.or(self.border_color);
        self.border_color_all = border_color_all.or(self.border_color_all);
        self.border_style = border_style.or(self.border_style);
        self.border_radius = border_radius.or(self.border_radius);
        self.opacity = opacity.or(self.opacity);
        self.mix_blend_mode = mix_blend_mode.or(self.mix_blend_mode);
        self.dither = dither.or(self.dither);
        self.z_index = z_index.or(self.z_index);
        self.object_fit = object_fit.or(self.object_fit);
        self.object_position = object_position.or(self.object_position);
        self.frame = frame.or(self.frame);
        self.font_family = font_family.or(self.font_family);
        self.font_size = font_size.or(self.font_size);
        self.font_weight = font_weight.or(self.font_weight);
        self.font_style = font_style.or(self.font_style);
        self.color = color.or(self.color);
        self.text_align = text_align.or(self.text_align);
        self.text_decoration = text_decoration.or(self.text_decoration);
        self.vertical_align = vertical_align.or(self.vertical_align);
        self.paint_order = paint_order.or(self.paint_order);
        self.line_height = line_height.or(self.line_height);
        self.line_gap = line_gap.or(self.line_gap);
        self.font_variant = font_variant.or(self.font_variant);
        self.letter_spacing = letter_spacing.or(self.letter_spacing);
        self.word_spacing = word_spacing.or(self.word_spacing);
        self.text_stroke = text_stroke.or(self.text_stroke);
        self.transform = transform.or(self.transform);
        self.box_shadows = box_shadows.or(self.box_shadows);
        self.text_shadows = text_shadows.or(self.text_shadows);
        self.mask = mask.or(self.mask);
        self.filter = filter.or(self.filter);
        self.backdrop_filter = backdrop_filter.or(self.backdrop_filter);

        self
    }

    /// A style that sets nothing.
    ///
    /// Every property is absent rather than defaulted, so a style layered over
    /// another only overrides what it names.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            display: None,
            position_type: None,
            position: None,
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            aspect_ratio: None,
            margin: None,
            padding: None,
            border: None,
            flex_direction: None,
            flex_wrap: None,
            flex_grow: None,
            flex_shrink: None,
            flex_basis: None,
            justify_content: None,
            align_items: None,
            align_self: None,
            align_content: None,
            gap: None,
            overflow: None,
            box_sizing: None,
            direction: None,
            grid_template_columns: None,
            grid_template_rows: None,
            grid_auto_rows: None,
            grid_auto_columns: None,
            grid_auto_flow: None,
            grid_column: None,
            grid_row: None,
            background_color: None,
            gradient: None,
            background_image: None,
            border_color: None,
            border_color_all: None,
            border_style: None,
            border_radius: None,
            opacity: None,
            mix_blend_mode: None,
            dither: None,
            z_index: None,
            object_fit: None,
            object_position: None,
            frame: None,
            font_family: None,
            font_size: None,
            font_weight: None,
            font_style: None,
            color: None,
            text_align: None,
            text_decoration: None,
            vertical_align: None,
            paint_order: None,
            line_height: None,
            line_gap: None,
            font_variant: None,
            letter_spacing: None,
            word_spacing: None,
            text_stroke: None,
            transform: None,
            box_shadows: None,
            text_shadows: None,
            mask: None,
            filter: None,
            backdrop_filter: None,
        }
    }

    /// The width, in pixels or as a percentage.
    ///
    /// ```
    /// use meo_canvas::{Style, pct, px};
    ///
    /// const FULL: Style = Style::new().width(pct(100.0));
    /// const FIXED: Style = Style::new().width(px(320.0));
    /// ```
    #[must_use]
    pub const fn width(mut self, width: Length) -> Self {
        self.width = Some(widen(width));
        self
    }

    /// The height, in pixels or as a percentage.
    #[must_use]
    pub const fn height(mut self, height: Length) -> Self {
        self.height = Some(widen(height));
        self
    }

    /// Both axes at once.
    ///
    /// ```
    /// use meo_canvas::{Style, px};
    ///
    /// const AVATAR: Style = Style::new().size(px(64.0), px(64.0));
    /// ```
    #[must_use]
    pub const fn size(self, width: Length, height: Length) -> Self {
        self.width(width).height(height)
    }

    /// A lower bound on the width.
    #[must_use]
    pub const fn min_width(mut self, width: Length) -> Self {
        self.min_width = Some(widen(width));
        self
    }

    /// A lower bound on the height.
    #[must_use]
    pub const fn min_height(mut self, height: Length) -> Self {
        self.min_height = Some(widen(height));
        self
    }

    /// An upper bound on the width.
    #[must_use]
    pub const fn max_width(mut self, width: Length) -> Self {
        self.max_width = Some(widen(width));
        self
    }

    /// An upper bound on the height.
    #[must_use]
    pub const fn max_height(mut self, height: Length) -> Self {
        self.max_height = Some(widen(height));
        self
    }

    /// One gap on both axes.
    ///
    /// ```
    /// use meo_canvas::{Style, px};
    ///
    /// const LIST: Style = Style::new().gap(px(16.0));
    /// ```
    #[must_use]
    pub const fn gap(mut self, gap: Length) -> Self {
        self.gap = Some((gap, gap));
        self
    }

    /// Row and column gaps separately, in CSS's shorthand order.
    #[must_use]
    pub const fn gap_xy(mut self, row: Length, column: Length) -> Self {
        self.gap = Some((row, column));
        self
    }

    /// One clipping behaviour on both axes.
    #[must_use]
    pub const fn overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = Some((overflow, overflow));
        self
    }

    /// One `border_radius` on every corner.
    ///
    /// ```
    /// use meo_canvas::Style;
    ///
    /// const ROUNDED: Style = Style::new().border_radius(12.0);
    /// ```
    #[must_use]
    pub const fn border_radius(mut self, border_radius: f32) -> Self {
        self.border_radius = Some(Corners {
            top_left: border_radius,
            top_right: border_radius,
            bottom_right: border_radius,
            bottom_left: border_radius,
        });
        self
    }

    /// Weight 700.
    ///
    /// ```
    /// use meo_canvas::Style;
    ///
    /// const TITLE: Style = Style::new().font_size(24.0).bold();
    /// ```
    #[must_use]
    pub const fn bold(mut self) -> Self {
        self.font_weight = Some(FontWeight::BOLD);
        self
    }

    /// Slanted glyphs.
    #[must_use]
    pub const fn italic(mut self) -> Self {
        self.font_style = Some(FontStyle::Italic);
        self
    }

    /// A shorthand for that many equal columns.
    ///
    /// v1's `columns`, and pure sugar: this **is**
    /// [`grid_template_columns`](Self::grid_template_columns) with that many
    /// `1fr` tracks, so nothing new reaches the wire. A shorthand that needed
    /// a field of its own would mean the long form could not express it, which
    /// would be a finding rather than a convenience.
    ///
    /// Assigning over a template already set replaces it, as every setter
    /// here does; the JavaScript surface refuses the pair instead, because
    /// there both spellings can appear in one object literal and neither is
    /// obviously later.
    ///
    /// ```
    /// use meo_canvas::{Style, scene::Display};
    ///
    /// let thirds = Style::new().display(Display::Grid).columns(3);
    /// ```
    #[must_use]
    pub fn columns(mut self, columns: u16) -> Self {
        self.grid_template_columns =
            Some(vec![TrackSize::Fraction(1.0); usize::from(columns)]);
        self
    }

    /// Both grid axes at once, as CSS's `grid-area` orders them.
    ///
    /// `row_start`, `column_start`, `row_end`, `column_end`, lines counting
    /// from one and the two ends **exclusive** — `(1, 1, 3, 2)` is the item
    /// covering rows 1 and 2 of column 1, which is CSS's reading of
    /// `grid-area: 1 / 1 / 3 / 2`. Sugar over
    /// [`grid_row`](Self::grid_row) and [`grid_column`](Self::grid_column).
    ///
    /// An end at or before its start is an empty area rather than a
    /// placement, so it is clamped to a span of one: this is a `const fn` on
    /// the authoring surface and has nowhere to report an error to. The
    /// JavaScript surface refuses it, where a throw is what a caller expects.
    ///
    /// ```
    /// use meo_canvas::{Style, scene::Display};
    ///
    /// let corner = Style::new().display(Display::Grid).grid_area(1, 1, 3, 2);
    /// ```
    #[must_use]
    pub const fn grid_area(
        mut self,
        row_start: i16,
        column_start: i16,
        row_end: i16,
        column_end: i16,
    ) -> Self {
        self.grid_row = Some(GridPlacement {
            start: Some(row_start),
            span: Some(span_between(row_start, row_end)),
        });
        self.grid_column = Some(GridPlacement {
            start: Some(column_start),
            span: Some(span_between(column_start, column_end)),
        });
        self
    }

    /// Space added between characters.
    ///
    /// ```
    /// use meo_canvas::{Style, px};
    ///
    /// const TIGHT: Style = Style::new().letter_spacing(px(-0.5));
    /// ```
    #[must_use]
    pub const fn letter_spacing(mut self, spacing: Length) -> Self {
        self.letter_spacing = Some(to_spacing(spacing));
        self
    }

    /// Space added between words.
    #[must_use]
    pub const fn word_spacing(mut self, spacing: Length) -> Self {
        self.word_spacing = Some(to_spacing(spacing));
        self
    }

    /// Splits this style into the four the scene keeps.
    ///
    /// The only place the flat authoring shape and the grouped wire shape meet.
    /// A property left unset takes the scene's own default rather than one
    /// restated here, so the two cannot drift.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "one line per property; splitting it would scatter the mapping"
    )]
    pub fn into_parts(self) -> (LayoutStyle, PaintStyle, TextStyle, Effects) {
        let mut layout = LayoutStyle::default();
        let mut paint = PaintStyle::default();
        let mut text = TextStyle::default();
        let mut effects = Effects::default();

        if let Some(value) = self.display {
            layout.display = value;
        }
        if let Some(value) = self.position_type {
            layout.position_type = value;
        }
        if let Some(value) = self.position {
            layout.inset = value;
        }
        if let Some(value) = self.width {
            layout.size.0 = value;
        }
        if let Some(value) = self.height {
            layout.size.1 = value;
        }
        if let Some(value) = self.min_width {
            layout.min_size.0 = value;
        }
        if let Some(value) = self.min_height {
            layout.min_size.1 = value;
        }
        if let Some(value) = self.max_width {
            layout.max_size.0 = value;
        }
        if let Some(value) = self.max_height {
            layout.max_size.1 = value;
        }
        layout.aspect_ratio = self.aspect_ratio;
        if let Some(value) = self.margin {
            layout.margin = value;
        }
        if let Some(value) = self.padding {
            layout.padding = value;
        }
        if let Some(value) = self.border {
            layout.border = value;
        }
        if let Some(value) = self.flex_direction {
            layout.flex_direction = value;
        }
        if let Some(value) = self.flex_wrap {
            layout.flex_wrap = value;
        }
        if let Some(value) = self.flex_grow {
            layout.flex_grow = value;
        }
        if let Some(value) = self.flex_shrink {
            layout.flex_shrink = value;
        }
        if let Some(value) = self.flex_basis {
            layout.flex_basis = value;
        }
        layout.justify_content = self.justify_content;
        layout.align_items = self.align_items;
        layout.align_self = self.align_self;
        layout.align_content = self.align_content;
        if let Some(value) = self.gap {
            layout.gap = value;
        }
        if let Some(value) = self.overflow {
            layout.overflow = value;
        }
        if let Some(value) = self.box_sizing {
            layout.box_sizing = value;
        }
        if let Some(value) = self.direction {
            layout.direction = value;
        }
        if let Some(value) = self.grid_template_columns {
            layout.grid_template_columns = value;
        }
        if let Some(value) = self.grid_template_rows {
            layout.grid_template_rows = value;
        }
        layout.grid_auto_rows = self.grid_auto_rows;
        layout.grid_auto_columns = self.grid_auto_columns;
        if let Some(value) = self.grid_auto_flow {
            layout.grid_auto_flow = value;
        }
        if let Some(value) = self.grid_column {
            layout.grid_column = value;
        }
        if let Some(value) = self.grid_row {
            layout.grid_row = value;
        }

        if let Some(value) = self.background_color {
            paint.background_color = value;
        }
        paint.gradient = self.gradient;
        paint.background_image = self.background_image;
        if let Some(value) = self.border_color {
            paint.border_color = value;
        }
        if let Some(value) = self.border_color_all {
            paint.border_color_all = value;
        }
        if let Some(value) = self.border_style {
            paint.border_style = value;
        }
        if let Some(value) = self.border_radius {
            paint.border_radius = value;
        }
        if let Some(value) = self.opacity {
            paint.opacity = value;
        }
        if let Some(value) = self.mix_blend_mode {
            paint.blend_mode = value;
        }
        if let Some(value) = self.dither {
            paint.dither = value;
        }
        // A caller who states a `z_index` means it, so the scene's `None` --
        // CSS's `auto` -- is what an unset style leaves rather than something
        // this surface can spell. Saying `auto` explicitly is saying nothing.
        if let Some(value) = self.z_index {
            paint.z_index = Some(value);
        }

        text.font_family = self.font_family;
        text.font_size = self.font_size;
        text.font_weight = self.font_weight;
        text.font_style = self.font_style;
        text.color = self.color;
        text.text_align = self.text_align;
        text.text_decoration = self.text_decoration;
        text.vertical_align = self.vertical_align;
        text.paint_order = self.paint_order;
        text.line_height = self.line_height;
        text.line_gap = self.line_gap;
        text.font_variant = self.font_variant;
        text.letter_spacing = self.letter_spacing;
        text.word_spacing = self.word_spacing;
        text.text_stroke = self.text_stroke;

        effects.transform = self.transform;
        if let Some(value) = self.box_shadows {
            effects.box_shadows = value;
        }
        if let Some(value) = self.text_shadows {
            effects.text_shadows = value;
        }
        effects.mask = self.mask;
        effects.filter = self.filter;
        effects.backdrop_filter = self.backdrop_filter;

        (layout, paint, text, effects)
    }
}

/// A [`Length`] as the [`Dimension`] a sizing property takes.
///
/// Sizes admit `auto` and lengths do not, so every length is a dimension and
/// the widening is total. A caller wanting `auto` reaches for the field.
const fn widen(length: Length) -> Dimension {
    match length {
        Length::Points(points) => Dimension::Points(points),
        Length::Percent(fraction) => Dimension::Percent(fraction),
    }
}

/// A [`Length`] as the [`Spacing`] letter and word spacing take.
const fn to_spacing(length: Length) -> Spacing {
    match length {
        Length::Points(points) => Spacing::Points(points),
        // A percentage of the font size is what CSS's `em` means here.
        Length::Percent(fraction) => Spacing::Em(fraction),
    }
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::style::{
        Dimension, Length,
        layout::{
            Align, Display, FlexDirection, GridPlacement, Justify, LayoutStyle,
            Overflow,
        },
        paint::PaintStyle,
        text::{FontStyle, FontWeight, LineHeight, Spacing, TextStyle},
    };

    use super::Style;
    use crate::{all, hex_rgb, pct, px, top, track, xy};

    #[test]
    fn a_new_style_sets_nothing() {
        // Absent rather than defaulted, so a style layered over another
        // overrides only what it names.
        let style = Style::new();
        assert!(style.width.is_none());
        assert!(style.background_color.is_none());
        assert!(style.font_size.is_none());
        assert_eq!(style, Style::default());
    }

    #[test]
    fn a_shorthand_is_the_long_form_and_nothing_else() {
        // The test of whether a shorthand is a shorthand: it has to produce
        // exactly what a caller could have written out, so nothing new reaches
        // the wire. A shorthand needing a field of its own would mean the long
        // form could not express it.
        assert_eq!(
            Style::new().columns(3).grid_template_columns,
            Style::new()
                .grid_template_columns([
                    crate::fr(1.0),
                    crate::fr(1.0),
                    crate::fr(1.0)
                ])
                .grid_template_columns
        );

        let area = Style::new().grid_area(1, 1, 3, 2);
        let long = Style::new()
            .grid_row(GridPlacement::spanning(1, 2))
            .grid_column(GridPlacement::spanning(1, 1));
        assert_eq!(area.grid_row, long.grid_row);
        assert_eq!(area.grid_column, long.grid_column);
    }

    #[test]
    fn an_empty_grid_area_becomes_the_smallest_one_that_means_anything() {
        // `const fn` has nowhere to report an error to, so an end at or before
        // its start is clamped rather than refused. The JavaScript surface
        // throws on the same input, where a caller expects it to.
        let backwards = Style::new().grid_area(3, 1, 1, 2);
        assert_eq!(
            backwards.grid_row,
            Some(GridPlacement {
                start: Some(3),
                span: Some(1)
            })
        );
    }

    #[test]
    fn a_const_style_is_reusable_without_a_clone() {
        // The whole reuse story: a `const` is substituted at each use, so every
        // mention is a fresh value a `self`-taking setter may consume.
        const CARD: Style = Style::new().padding(all(px(24.0))).gap(px(16.0));

        let dark = CARD.background_color(hex_rgb(0x10_10_14));
        let light = CARD.background_color(hex_rgb(0x1c_1c_22));

        assert_eq!(dark.padding, light.padding);
        assert_ne!(dark.background_color, light.background_color);
    }

    #[test]
    fn an_unset_property_takes_the_scenes_default_rather_than_one_restated() {
        let (layout, paint, text, effects) = Style::new().into_parts();

        assert_eq!(layout, LayoutStyle::default());
        assert_eq!(paint, PaintStyle::default());
        assert_eq!(text, TextStyle::default());
        assert_eq!(effects.box_shadows.len(), 0);
    }

    #[test]
    fn layout_properties_reach_the_layout_half() {
        let (layout, ..) = Style::new()
            .display(Display::Grid)
            .flex_direction(FlexDirection::Column)
            .justify_content(Justify::SpaceBetween)
            .align_items(Align::Center)
            .width(px(320.0))
            .height(pct(50.0))
            .min_width(px(10.0))
            .max_height(px(400.0))
            .padding(xy(px(8.0), px(16.0)))
            .margin(all(Dimension::Points(4.0)))
            .border(top(2.0))
            .flex_grow(1.0)
            .flex_shrink(0.0)
            .aspect_ratio(1.618)
            .grid_template_columns([track(px(240.0))])
            .into_parts();

        assert_eq!(layout.display, Display::Grid);
        assert_eq!(layout.flex_direction, FlexDirection::Column);
        assert_eq!(layout.justify_content, Some(Justify::SpaceBetween));
        assert_eq!(layout.align_items, Some(Align::Center));
        assert_eq!(layout.size.0, Dimension::Points(320.0));
        assert_eq!(layout.size.1, Dimension::Percent(0.5));
        assert_eq!(layout.min_size.0, Dimension::Points(10.0));
        assert_eq!(layout.max_size.1, Dimension::Points(400.0));
        assert_eq!(layout.padding.top, Length::Points(8.0));
        assert_eq!(layout.padding.left, Length::Points(16.0));
        assert_eq!(layout.border.top.to_bits(), 2.0_f32.to_bits());
        assert_eq!(layout.border.left.to_bits(), 0.0_f32.to_bits());
        assert_eq!(layout.aspect_ratio, Some(1.618));
        assert_eq!(layout.grid_template_columns.len(), 1);
    }

    #[test]
    fn one_gap_sets_both_axes_and_the_pair_form_keeps_csss_order() {
        let (one, ..) = Style::new().gap(px(12.0)).into_parts();
        assert_eq!(one.gap, (Length::Points(12.0), Length::Points(12.0)));

        // CSS's shorthand is row then column, which is the order the scene
        // stores and the reason this setter does not take `(x, y)`.
        let (pair, ..) = Style::new().gap_xy(px(4.0), px(8.0)).into_parts();
        assert_eq!(pair.gap, (Length::Points(4.0), Length::Points(8.0)));
    }

    #[test]
    fn paint_properties_reach_the_paint_half() {
        let (_, paint, ..) = Style::new()
            .background_color(hex_rgb(0x10_10_14))
            .border_radius(12.0)
            .opacity(0.5)
            .z_index(3)
            .dither(true)
            .into_parts();

        assert_eq!(paint.background_color, hex_rgb(0x10_10_14));
        assert_eq!(paint.border_radius.top_left.to_bits(), 12.0_f32.to_bits());
        assert_eq!(
            paint.border_radius.bottom_right.to_bits(),
            12.0_f32.to_bits()
        );
        assert_eq!(paint.opacity.to_bits(), 0.5_f32.to_bits());
        assert_eq!(paint.z_index, Some(3));
        assert!(paint.dither);
    }

    #[test]
    fn color_is_the_text_colour_and_background_is_the_fill() {
        // CSS's names, and CSS's trap. Keeping it is what lets a design be
        // ported without translation.
        let (_, paint, text, _) = Style::new()
            .color(hex_rgb(0xff_ff_ff))
            .background_color(hex_rgb(0x00_00_00))
            .into_parts();

        assert_eq!(text.color, Some(hex_rgb(0xff_ff_ff)));
        assert_eq!(paint.background_color, hex_rgb(0x00_00_00));
    }

    #[test]
    fn text_properties_reach_the_text_half() {
        let (.., text, _) = Style::new()
            .font_family("Inter")
            .font_size(24.0)
            .bold()
            .italic()
            .line_height(LineHeight::Number(1.4))
            .letter_spacing(px(-0.5))
            .word_spacing(pct(10.0))
            .into_parts();

        assert_eq!(text.font_family.as_deref(), Some("Inter"));
        assert_eq!(text.font_size, Some(24.0));
        assert_eq!(text.font_weight, Some(FontWeight::BOLD));
        assert_eq!(text.font_style, Some(FontStyle::Italic));
        assert_eq!(text.line_height, Some(LineHeight::Number(1.4)));
        assert_eq!(text.letter_spacing, Some(Spacing::Points(-0.5)));
        // A percentage of the font size is what CSS's `em` means here.
        assert_eq!(text.word_spacing, Some(Spacing::Em(0.1)));
    }

    #[test]
    fn the_bounds_and_overflow_setters_reach_their_fields() {
        let (layout, ..) = Style::new()
            .min_height(px(4.0))
            .max_width(px(8.0))
            .overflow(Overflow::Hidden)
            .into_parts();

        assert_eq!(layout.min_size.1, Dimension::Points(4.0));
        assert_eq!(layout.max_size.0, Dimension::Points(8.0));
        // One value sets both axes, as CSS's one-value `overflow` does.
        assert_eq!(layout.overflow, (Overflow::Hidden, Overflow::Hidden));
    }

    #[test]
    fn a_later_setter_replaces_an_earlier_one() {
        let (layout, ..) =
            Style::new().width(px(10.0)).width(px(20.0)).into_parts();
        assert_eq!(layout.size.0, Dimension::Points(20.0));
    }

    #[test]
    fn a_literal_reaches_a_property_with_no_setter() {
        // The documented escape hatch, and the reason the fields are public and
        // the struct is not `non_exhaustive`.
        let style = Style {
            aspect_ratio: Some(1.618),
            ..Style::new().gap(px(8.0))
        };
        let (layout, ..) = style.into_parts();

        assert_eq!(layout.aspect_ratio, Some(1.618));
        assert_eq!(layout.gap.0, Length::Points(8.0));
    }
}
