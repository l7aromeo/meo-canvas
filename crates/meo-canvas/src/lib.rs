//! Draw a picture by describing it, not by issuing commands.
//!
//! ```
//! use meo_canvas::{
//!     Column, Image, Row, Style, Text, all, hex_rgb, px, scene::ObjectFit,
//! };
//!
//! let card = Row::new()
//!     .with_style(
//!         Style::new()
//!             .gap(px(16.0))
//!             .padding(all(px(24.0)))
//!             .background_color(hex_rgb(0x10_10_14)),
//!     )
//!     .children([
//!         Image::path("avatar.png").with_style(
//!             Style::new()
//!                 .size(px(64.0), px(64.0))
//!                 .object_fit(ObjectFit::Cover),
//!         ),
//!         Column::new().children([
//!             Text::new("Ukasyah")
//!                 .with_style(Style::new().font_size(24.0).bold()),
//!             Text::new("Bandung")
//!                 .with_style(Style::new().color(hex_rgb(0x88_88_90))),
//!         ]),
//!     ]);
//!
//! let scene = card.into_scene(360.0, 112.0)?;
//! # Ok::<(), meo_canvas_scene::SceneError>(())
//! ```
//!
//! A node carries a constructor, a [`Style`], and its children. Three methods,
//! and the set never grows: a new property is a new method on `Style`, not on
//! nine node types. Each node's essential argument is a constructor parameter
//! -- [`Text::new`] takes the content, [`Image::path`] the source, [`Path::d`]
//! the data -- so it cannot be forgotten.
//!
//! # One flat style
//!
//! [`Style`] is one type, not four. Authoring is flat because CSS is flat: a
//! reader never has to know which group `gap` lives in versus
//! `background_color`. The scene keeps them grouped because the codec needs
//! them grouped, and [`Style::into_parts`] splits at the moment the tree
//! becomes a scene.
//!
//! The names and the behaviour are CSS's. [`Style::color`] is the inherited
//! text colour and [`Style::background_color`] is the fill; the two sit
//! adjacent and mean different things, which is CSS's trap and not one invented
//! here. Keeping it is what lets a design be ported without translation.
//!
//! Setters are `const` wherever the property allows, so a reusable base is a
//! `const`:
//!
//! ```
//! use meo_canvas::{Row, Style, all, hex_rgb, px};
//!
//! const CARD: Style = Style::new().padding(all(px(24.0))).gap(px(16.0));
//!
//! let dark =
//!     Row::new().with_style(CARD.background_color(hex_rgb(0x10_10_14)));
//! let light =
//!     Row::new().with_style(CARD.background_color(hex_rgb(0x1c_1c_22)));
//! ```
//!
//! A `const` is substituted at each use, so every `CARD` is a fresh value a
//! `self`-taking setter may consume. No clone and no lifetime. A property
//! holding a `String` or a `Vec` cannot be `const`, because assigning one drops
//! what it replaces; a function returning a `Style` serves the same purpose
//! there.
//!
//! Every field is public, so a property with no setter is still reachable:
//!
//! ```
//! use meo_canvas::{Style, px};
//!
//! let golden = Style {
//!     aspect_ratio: Some(1.618),
//!     ..Style::new().gap(px(8.0))
//! };
//! ```
//!
//! # Chains and literals
//!
//! A tree is chained and a bag of options is a literal, and the difference is
//! not a matter of taste.
//!
//! A node's children nest, and each carries a style of its own, so the shape of
//! the call has to be the shape of the picture -- `Row::new().children([..])`
//! reads as the row it describes where the same tree written as a literal
//! reads as bookkeeping. [`Style`] chains for a second reason: the properties
//! are optional and independent, so a chain names the handful a caller set
//! rather than defaulting the sixty they did not.
//!
//! Options are struct literals closed with `..Default::default()`. A literal
//! shows every value the caller chose in one place, and there is no ordering
//! between options and no invalid intermediate state for a builder to prevent.
//!
//! `Style` takes both, which is why its fields are public and why it is not
//! `#[non_exhaustive]`: the chain covers the properties with setters, and the
//! literal reaches the ones without. The rest pattern is what keeps such a
//! literal compiling when a field is added.
//!
//! # What this crate deliberately excludes
//!
//! No Skia and no taffy in any signature, and neither as a direct dependency.
//! Both live behind `meo-canvas-core`. A caller who upgrades either one does so
//! on that crate's schedule, and a caller who reads this crate's documentation
//! never has to learn two other vocabularies to place a rectangle.
//!
//! No fetching. The renderer beneath resolves local paths and inline bytes; an
//! [`ImageSource::Url`](scene::ImageSource) is an error there rather than a
//! request. This crate imposes no async runtime on anyone, which it could not
//! do while owning an HTTP client.
//!
//! No mutable drawing context. There is no `move_to`/`line_to` state machine
//! here -- that API already exists, in `meo-skia-canvas`, and reproducing it
//! would give the workspace two answers to the same question.

pub mod chart;
pub mod element;
pub mod root;
pub mod style;
pub mod unit;

pub use element::{
    Box, Column, Each, Element, Grid, Image, IntoElements, Path, Row, Text,
    each,
};
/// The animation helpers, as a module rather than a curated list.
///
/// **A list would be a second surface that can drift from the first.**
/// These are five submodules and a dozen or so items -- `Easing`,
/// `cubic_bezier`, `steps`, `Spring`, `Shape`, `Rgba`, `mix`, `keyframes`,
/// `Track`, `Motion`, `Sequence`, `page_time` -- and naming them here
/// means a caller's reach is whatever someone remembered to add. **The
/// failure mode of that is silence**: a helper exists in the core, is
/// absent from the facade, and nothing says so. Re-exporting the module
/// cannot omit.
///
/// The cost is the other direction: **the module publishes whatever it
/// later grows.** That is acceptable here because `animate` is already a
/// curated module rather than a dumping ground -- anything added to it is
/// added for callers, since nothing inside the renderer uses it. If that
/// stops being true, this should become a list and the reason will have
/// changed.
///
/// # Examples
///
/// ```
/// use meo_canvas::animate::{easing::Easing, spring::Spring};
///
/// assert!((Easing::OutCubic.at(0.5) - 0.875).abs() < f64::EPSILON);
/// assert!(Spring::default().at(0.2).unwrap_or(0.0) > 0.0);
/// ```
///
/// That example is the point rather than decoration: **it compiles from
/// outside this crate**, which is the thing a `pub use` either achieves or
/// does not, and which building this crate cannot tell you.
pub use meo_canvas_core::animate;
pub use meo_canvas_core::{
    EncodeOptions, Error, ImageFormat as Format, RenderedCanvas, Renderer,
};
pub use meo_canvas_scene::style::{
    PaintOrder,
    effect::FillRule,
    layout::{
        Align, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
        GridAutoFlow, GridPlacement, Justify, Overflow, PositionType,
        TrackSize,
    },
    paint::{
        BackgroundRepeat, BlendMode, BorderStyle, Color, GradientKind,
        ObjectFit,
    },
    text::{
        FontStyle, FontVariant, FontWeight, LineHeight, TextAlign,
        TextDecoration, VerticalAlign,
    },
};
/// The style keywords, at the crate root.
///
/// `Justify::Center` and its siblings are authoring vocabulary — a caller
/// names one every time they write a container — so they sit beside the
/// node constructors rather than under [`scene`]. The scene module keeps
/// them too, along with everything a caller assembling a `Scene` by hand
/// would need.
pub use meo_canvas_scene::surface::{ColorSpace, ColorType};
pub use root::{BuildError, Canvas, PageInfo, Root, SequenceError};
pub use style::{Style, Styled};
pub use unit::{
    DefaultZero, IntoCorners, IntoSides, all, auto, bottom, corners,
    corners_all, fr, fraction, hex, hex_rgb, hex_rgba, left, pct, px, rgb,
    rgba, right, sides, size_auto, top, track, xy,
};

/// The scene vocabulary, re-exported so callers need one dependency rather than
/// two.
pub mod scene {
    /// Reading and writing a scene as bytes.
    ///
    /// [`Root::into_scene`](crate::Root::into_scene) is documented as
    /// being for a caller who wants the scene itself — to write to
    /// disk, to send over the wire, or to render more than once — and
    /// this is what turns one into bytes and back. Without it that
    /// sentence names something a caller cannot do without a second
    /// dependency.
    pub use meo_canvas_scene::codec;
    pub use meo_canvas_scene::{
        Corners, Dimension, Length, Node, NodeId, NodeKind, Point, Rect, Scene,
        SceneError, Sides, Size,
        node::{ImageSource, LineCap, LineJoin, PathPaint},
        style::{
            PaintOrder,
            effect::{
                BoxShadow, Effects, FillRule, Mask, MaskShape, TextShadow,
                Transform,
            },
            layout::{
                Align, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
                GridAutoFlow, GridPlacement, Justify, LayoutStyle, Overflow,
                PositionType, TrackSize,
            },
            paint::{
                BackgroundImage, BackgroundRepeat, BackgroundSize, BlendMode,
                BorderStyle, Color, Gradient, GradientGeometry, GradientKind,
                GradientStop, LinearDirection, ObjectFit, PaintStyle,
            },
            text::{
                FontStyle, FontVariant, FontWeight, LineHeight, ParagraphStyle,
                Spacing, TextAlign, TextDecoration, TextStroke, TextStyle,
                VerticalAlign,
            },
        },
    };
}
