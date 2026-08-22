//! One flat style, as CSS writes one.
//!
//! The scene keeps layout, paint, text and effects in four structs because the
//! codec needs them separated. Authoring does not: a caller writing `gap`
//! beside `background` should never have to know which group either lives in,
//! and v1's `BoxProps` already mixed all four. [`Style::into_parts`] does the
//! splitting at the moment the tree becomes a scene.
//!
//! ```
//! use meo_canvas::{Style, all, hex_rgb, px};
//!
//! const CARD: Style = Style::new()
//!     .padding(all(px(24.0)))
//!     .gap(px(16.0))
//!     .radius(12.0);
//!
//! let dark = CARD.background(hex_rgb(0x10_10_14));
//! let light = CARD.background(hex_rgb(0xf4_f4_f6));
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
            BlendMode, BorderStyle, Color, Gradient, ObjectFit, PaintStyle,
        },
        text::{
            FontStyle, FontWeight, Spacing, TextAlign, TextDecoration,
            TextStroke, TextStyle, VerticalAlign,
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
    pub position: Option<PositionType>,
    /// Offsets from the container's edges.
    pub inset: Option<Sides<Option<Length>>>,
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
    pub border_width: Option<Sides<f32>>,
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
    pub grid_columns: Option<Vec<TrackSize>>,
    /// Row tracks of a grid.
    pub grid_rows: Option<Vec<TrackSize>>,
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
    pub background: Option<Color>,
    /// A gradient painted over the fill.
    pub gradient: Option<Gradient>,
    /// Border colour, per edge.
    pub border_color: Option<Sides<Option<Color>>>,
    /// Border colour for every edge that names none of its own.
    pub border_color_all: Option<Color>,
    /// Whether the border is solid, dashed or dotted.
    pub border_style: Option<BorderStyle>,
    /// Corner radii.
    pub radius: Option<Corners<f32>>,
    /// Opacity of this node and its subtree, from `0.0` to `1.0`.
    pub opacity: Option<f32>,
    /// How this node composites onto what is beneath it.
    pub blend_mode: Option<BlendMode>,
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
    /// Line box height as a multiple of the em size. Inherits.
    pub line_height: Option<f32>,
    /// Extra space between lines, in pixels. Inherits.
    pub line_gap: Option<f32>,
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

/// Writes `$field` and returns the style, as a `const fn`.
///
/// The setters are near-identical and the docs are not, so the macro takes the
/// documentation as an argument rather than generating it. Written out by hand
/// each time, sixty setters is sixty chances to assign the wrong field.
macro_rules! setter {
    ($(#[$doc:meta])* $name:ident: $field:ident, $type:ty) => {
        $(#[$doc])*
        #[must_use]
        pub const fn $name(mut self, value: $type) -> Self {
            self.$field = Some(value);
            self
        }
    };
}

/// A setter for a property whose value owns a heap allocation.
///
/// Not `const`: assigning over an `Option<String>` or `Option<Vec<_>>` drops
/// what was there, and a `const fn` cannot drop.
macro_rules! owned_setter {
    ($(#[$doc:meta])* $name:ident: $field:ident, $type:ty) => {
        $(#[$doc])*
        #[must_use]
        pub fn $name(mut self, value: impl Into<$type>) -> Self {
            self.$field = Some(value.into());
            self
        }
    };
}

impl Style {
    // -- Layout ---------------------------------------------------------

    setter! {
        /// How this node's children are arranged.
        ///
        /// ```
        /// use meo_canvas::{Style, scene::Display};
        ///
        /// const SHEET: Style = Style::new().display(Display::Grid);
        /// ```
        display: display, Display
    }

    setter! {
        /// Whether the node is placed by the flow or by its own `inset`.
        position: position, PositionType
    }

    setter! {
        /// Offsets from the container's edges.
        inset: inset, Sides<Option<Length>>
    }

    setter! {
        /// Width divided by height, honoured when one axis is automatic.
        aspect_ratio: aspect_ratio, f32
    }

    setter! {
        /// Space outside the border.
        margin: margin, Sides<Dimension>
    }

    setter! {
        /// Space inside the border.
        ///
        /// ```
        /// use meo_canvas::{Style, all, px, xy};
        ///
        /// const EVEN: Style = Style::new().padding(all(px(24.0)));
        /// const TIGHT: Style = Style::new().padding(xy(px(8.0), px(16.0)));
        /// ```
        padding: padding, Sides<Length>
    }

    setter! {
        /// Border thickness, which occupies space whether or not it is painted.
        border_width: border_width, Sides<f32>
    }

    setter! {
        /// The axis children run along.
        flex_direction: flex_direction, FlexDirection
    }

    setter! {
        /// Whether children overflow onto further lines.
        flex_wrap: flex_wrap, FlexWrap
    }

    setter! {
        /// Share of free space this node absorbs.
        flex_grow: flex_grow, f32
    }

    setter! {
        /// Share of overflow this node gives up.
        flex_shrink: flex_shrink, f32
    }

    setter! {
        /// Size along the main axis before growing or shrinking.
        flex_basis: flex_basis, Dimension
    }

    setter! {
        /// Main-axis distribution of children.
        ///
        /// ```
        /// use meo_canvas::{Style, scene::Justify};
        ///
        /// const SPREAD: Style = Style::new().justify_content(Justify::SpaceBetween);
        /// ```
        justify_content: justify_content, Justify
    }

    setter! {
        /// Cross-axis placement of children.
        align_items: align_items, Align
    }

    setter! {
        /// This node's own cross-axis placement, overriding its parent's.
        align_self: align_self, Align
    }

    setter! {
        /// Cross-axis distribution of wrapped lines.
        align_content: align_content, Align
    }

    setter! {
        /// Whether `width` and `height` include padding and border.
        box_sizing: box_sizing, BoxSizing
    }

    setter! {
        /// Inline direction, which decides which edge is the start.
        direction: direction, Direction
    }

    owned_setter! {
        /// The grid's column tracks.
        ///
        /// ```
        /// use meo_canvas::{Style, fr, px, track, scene::Display};
        ///
        /// let sidebar = Style::new()
        ///     .display(Display::Grid)
        ///     .grid_columns([track(px(240.0)), fr(1.0)]);
        /// ```
        grid_columns: grid_columns, Vec<TrackSize>
    }

    owned_setter! {
        /// The grid's row tracks.
        grid_rows: grid_rows, Vec<TrackSize>
    }

    setter! {
        /// Size given to rows the template does not name.
        grid_auto_rows: grid_auto_rows, TrackSize
    }

    setter! {
        /// Size given to columns the template does not name.
        grid_auto_columns: grid_auto_columns, TrackSize
    }

    setter! {
        /// The order auto-placement fills tracks in.
        grid_auto_flow: grid_auto_flow, GridAutoFlow
    }

    setter! {
        /// Where this item sits on the column axis.
        grid_column: grid_column, GridPlacement
    }

    setter! {
        /// Where this item sits on the row axis.
        grid_row: grid_row, GridPlacement
    }

    // -- Paint ----------------------------------------------------------

    setter! {
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
        /// const PANEL: Style = Style::new().background(hex_rgb(0x10_10_14));
        /// ```
        background: background, Color
    }

    owned_setter! {
        /// A gradient painted over the fill.
        ///
        /// Not `const`: a gradient owns its stops, and assigning over one drops
        /// the vector it replaces.
        gradient: gradient, Gradient
    }

    setter! {
        /// One border colour on every edge.
        border_color: border_color_all, Color
    }

    setter! {
        /// Border colour per edge, for a box whose edges differ.
        border_color_sides: border_color, Sides<Option<Color>>
    }

    setter! {
        /// Whether the border is solid, dashed or dotted.
        border_style: border_style, BorderStyle
    }

    setter! {
        /// Corner radii, each named.
        radius_corners: radius, Corners<f32>
    }

    setter! {
        /// Opacity of this node and its subtree, from `0.0` to `1.0`.
        opacity: opacity, f32
    }

    setter! {
        /// How this node composites onto what is beneath it.
        blend_mode: blend_mode, BlendMode
    }

    setter! {
        /// Whether gradients are dithered.
        dither: dither, bool
    }

    setter! {
        /// Paint order among positioned siblings.
        z_index: z_index, i32
    }

    // -- Image ----------------------------------------------------------
    //
    // Three properties that belong to an image rather than to a box. They live
    // on `Style` because the surface is one flat style and a caller writing
    // `.style(Style::new().size(..).fit(Cover))` should not have to know that
    // two of those three words are read by different halves of the scene. A
    // node that is not an image ignores them, which is what CSS does with a
    // property a element does not define.

    setter! {
        /// How an image fills its box.
        ///
        /// ```
        /// use meo_canvas::{Image, Style, px, scene::ObjectFit};
        ///
        /// let avatar = Image::path("a.png")
        ///     .style(Style::new().size(px(64.0), px(64.0)).fit(ObjectFit::Cover));
        /// ```
        fit: object_fit, ObjectFit
    }

    setter! {
        /// Where an image sits in its box when it does not fill it.
        object_position: object_position, (Length, Length)
    }

    setter! {
        /// Which frame of an animated source to draw.
        frame: frame, u32
    }

    // -- Text -----------------------------------------------------------

    setter! {
        /// The colour glyphs are drawn in.
        ///
        /// CSS's `color`: it inherits, so setting it on a container reaches
        /// every descendant. See [`background`](Self::background) for the fill.
        color: color, Color
    }

    owned_setter! {
        /// The family name text is drawn in.
        ///
        /// Not `const`, because the name is a `String`. A base style needing one
        /// is a function returning a [`Style`] rather than a `const`.
        font_family: font_family, String
    }

    setter! {
        /// Em size in logical pixels.
        font_size: font_size, f32
    }

    setter! {
        /// Weight from 1 to 1000.
        font_weight: font_weight, FontWeight
    }

    setter! {
        /// Upright or italic, named.
        font_style: font_style, FontStyle
    }

    setter! {
        /// Horizontal alignment within the box.
        text_align: text_align, TextAlign
    }

    setter! {
        /// A line through, over or under the text.
        text_decoration: text_decoration, TextDecoration
    }

    setter! {
        /// Where a line sits within its box.
        vertical_align: vertical_align, VerticalAlign
    }

    setter! {
        /// Which of a glyph's fill and stroke is painted on top.
        paint_order: paint_order, PaintOrder
    }

    setter! {
        /// Line box height as a multiple of the em size.
        line_height: line_height, f32
    }

    setter! {
        /// Extra space between lines, in pixels.
        line_gap: line_gap, f32
    }

    setter! {
        /// An outline drawn on the glyphs.
        text_stroke: text_stroke, TextStroke
    }

    // -- Effects --------------------------------------------------------

    setter! {
        /// A transform applied to this node and its subtree.
        transform: transform, Transform
    }

    owned_setter! {
        /// Shadows cast by the box, painted in the order given.
        box_shadow: box_shadows, Vec<BoxShadow>
    }

    owned_setter! {
        /// Shadows cast by the glyphs.
        text_shadow: text_shadows, Vec<TextShadow>
    }

    owned_setter! {
        /// A shape or gradient the subtree is clipped to.
        ///
        /// Not `const`, for the same reason [`gradient`](Self::gradient) is not:
        /// a path mask owns its data and a gradient mask owns its stops.
        mask: mask, Mask
    }

    owned_setter! {
        /// A CSS filter applied to this node's own drawing.
        filter: filter, String
    }

    owned_setter! {
        /// A CSS filter applied to what shows through this node.
        backdrop_filter: backdrop_filter, String
    }

    /// A style that sets nothing.
    ///
    /// Every property is absent rather than defaulted, so a style layered over
    /// another only overrides what it names.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            display: None,
            position: None,
            inset: None,
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            aspect_ratio: None,
            margin: None,
            padding: None,
            border_width: None,
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
            grid_columns: None,
            grid_rows: None,
            grid_auto_rows: None,
            grid_auto_columns: None,
            grid_auto_flow: None,
            grid_column: None,
            grid_row: None,
            background: None,
            gradient: None,
            border_color: None,
            border_color_all: None,
            border_style: None,
            radius: None,
            opacity: None,
            blend_mode: None,
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

    /// One corner radius on every corner.
    ///
    /// ```
    /// use meo_canvas::Style;
    ///
    /// const ROUNDED: Style = Style::new().radius(12.0);
    /// ```
    #[must_use]
    pub const fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(Corners {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
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
        if let Some(value) = self.position {
            layout.position_type = value;
        }
        if let Some(value) = self.inset {
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
        if let Some(value) = self.border_width {
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
        if let Some(value) = self.grid_columns {
            layout.grid_template_columns = value;
        }
        if let Some(value) = self.grid_rows {
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

        if let Some(value) = self.background {
            paint.background_color = value;
        }
        paint.gradient = self.gradient;
        if let Some(value) = self.border_color {
            paint.border_color = value;
        }
        if let Some(value) = self.border_color_all {
            paint.border_color_all = value;
        }
        if let Some(value) = self.border_style {
            paint.border_style = value;
        }
        if let Some(value) = self.radius {
            paint.border_radius = value;
        }
        if let Some(value) = self.opacity {
            paint.opacity = value;
        }
        if let Some(value) = self.blend_mode {
            paint.blend_mode = value;
        }
        if let Some(value) = self.dither {
            paint.dither = value;
        }
        if let Some(value) = self.z_index {
            paint.z_index = value;
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
        layout::{Align, Display, FlexDirection, Justify, LayoutStyle},
        paint::PaintStyle,
        text::{FontStyle, FontWeight, Spacing, TextStyle},
    };

    use super::Style;
    use crate::{all, hex_rgb, pct, px, top, track, xy};

    #[test]
    fn a_new_style_sets_nothing() {
        // Absent rather than defaulted, so a style layered over another
        // overrides only what it names.
        let style = Style::new();
        assert!(style.width.is_none());
        assert!(style.background.is_none());
        assert!(style.font_size.is_none());
        assert_eq!(style, Style::default());
    }

    #[test]
    fn a_const_style_is_reusable_without_a_clone() {
        // The whole reuse story: a `const` is substituted at each use, so every
        // mention is a fresh value a `self`-taking setter may consume.
        const CARD: Style = Style::new().padding(all(px(24.0))).gap(px(16.0));

        let dark = CARD.background(hex_rgb(0x10_10_14));
        let light = CARD.background(hex_rgb(0x1c_1c_22));

        assert_eq!(dark.padding, light.padding);
        assert_ne!(dark.background, light.background);
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
            .border_width(top(2.0))
            .flex_grow(1.0)
            .flex_shrink(0.0)
            .aspect_ratio(1.618)
            .grid_columns([track(px(240.0))])
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
            .background(hex_rgb(0x10_10_14))
            .radius(12.0)
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
        assert_eq!(paint.z_index, 3);
        assert!(paint.dither);
    }

    #[test]
    fn color_is_the_text_colour_and_background_is_the_fill() {
        // CSS's names, and CSS's trap. Keeping it is what lets a design be
        // ported without translation.
        let (_, paint, text, _) = Style::new()
            .color(hex_rgb(0xff_ff_ff))
            .background(hex_rgb(0x00_00_00))
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
            .line_height(1.4)
            .letter_spacing(px(-0.5))
            .word_spacing(pct(10.0))
            .into_parts();

        assert_eq!(text.font_family.as_deref(), Some("Inter"));
        assert_eq!(text.font_size, Some(24.0));
        assert_eq!(text.font_weight, Some(FontWeight::BOLD));
        assert_eq!(text.font_style, Some(FontStyle::Italic));
        assert_eq!(text.line_height, Some(1.4));
        assert_eq!(text.letter_spacing, Some(Spacing::Points(-0.5)));
        // A percentage of the font size is what CSS's `em` means here.
        assert_eq!(text.word_spacing, Some(Spacing::Em(0.1)));
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
