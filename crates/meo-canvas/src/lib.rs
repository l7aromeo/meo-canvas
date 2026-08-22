//! Draw a picture by describing it, not by issuing commands.
//!
//! ```
//! use meo_canvas::{
//!     Column, Image, Row, Style, Text, all, hex_rgb, px, scene::ObjectFit,
//! };
//!
//! let card = Row::new()
//!     .style(
//!         Style::new()
//!             .gap(px(16.0))
//!             .padding(all(px(24.0)))
//!             .background(hex_rgb(0x10_10_14)),
//!     )
//!     .children([
//!         Image::path("avatar.png").style(
//!             Style::new().size(px(64.0), px(64.0)).fit(ObjectFit::Cover),
//!         ),
//!         Column::new().children([
//!             Text::new("Ukasyah").style(Style::new().font_size(24.0).bold()),
//!             Text::new("Bandung")
//!                 .style(Style::new().color(hex_rgb(0x88_88_90))),
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
//! reader never has to know which group `gap` lives in versus `background`. The
//! scene keeps them grouped because the codec needs them grouped, and
//! [`Style::into_parts`] splits at the moment the tree becomes a scene.
//!
//! The names and the behaviour are CSS's. [`Style::color`] is the inherited
//! text colour and [`Style::background`] is the fill; the two sit adjacent and
//! mean different things, which is CSS's trap and not one invented here.
//! Keeping it is what lets a design be ported without translation.
//!
//! Setters are `const` wherever the property allows, so a reusable base is a
//! `const`:
//!
//! ```
//! use meo_canvas::{Row, Style, all, hex_rgb, px};
//!
//! const CARD: Style = Style::new().padding(all(px(24.0))).gap(px(16.0));
//!
//! let dark = Row::new().style(CARD.background(hex_rgb(0x10_10_14)));
//! let light = Row::new().style(CARD.background(hex_rgb(0x1c_1c_22)));
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
//! # The shape of the API
//!
//! Options are struct literals closed with `..Default::default()`, not builder
//! chains. A literal shows every value the caller chose in one place, and the
//! rest pattern is what keeps that literal compiling when a field is added --
//! the same reason the workspace allows `clippy::needless_update`'s equivalent
//! elsewhere. Builders buy nothing here: there is no ordering constraint
//! between options and no invalid intermediate state to prevent.
//!
//! # What this crate deliberately excludes
//!
//! No Skia and no taffy in any signature, and neither as a direct dependency.
//! Both live behind `meo-canvas-core`. A caller who upgrades either one does so
//! on that crate's schedule, and a caller who reads this crate's documentation
//! never has to learn two other vocabularies to place a rectangle.
//!
//! No fetching. [`Canvas::render`] resolves local paths and inline bytes; a URL
//! is an error. This crate imposes no async runtime on anyone, which it could
//! not do while owning an HTTP client.
//!
//! No mutable drawing context. There is no `move_to`/`line_to` state machine
//! here -- that API already exists, in `meo-skia-canvas`, and reproducing it
//! would give the workspace two answers to the same question.

pub mod element;
pub mod style;
pub mod unit;

pub use element::{Box, Column, Element, Grid, Image, Path, Row, Text};
pub use meo_canvas_core::{Error, ImageFormat as Format};
pub use style::Style;
pub use unit::{
    DefaultZero, all, auto, bottom, corners, corners_all, fr, hex, hex_rgb,
    hex_rgba, left, pct, px, rgb, rgba, right, sides, size_auto, top, track,
    xy,
};

/// The scene vocabulary, re-exported so callers need one dependency rather than
/// two.
pub mod scene {
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
                BackgroundRepeat, BlendMode, BorderStyle, Color, GradientKind,
                ObjectFit, PaintStyle,
            },
            text::{
                FontStyle, FontVariant, FontWeight, ParagraphStyle, Spacing,
                TextAlign, TextDecoration, TextStroke, TextStyle,
                VerticalAlign,
            },
        },
    };
}

use meo_canvas_scene::{Scene, Size};

/// Everything a canvas is configured with.
///
/// Every field has a defensible default, so a caller states only what differs.
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasOptions {
    /// Surface dimensions in logical pixels.
    pub size: Size,
    /// Device-pixel multiplier applied at paint time.
    ///
    /// Layout always solves at scale 1, so changing this changes resolution
    /// and nothing else about where things sit.
    pub scale: f32,
    /// Colour painted before anything else.
    ///
    /// Transparent by default, which is the only choice that does not silently
    /// flatten an alpha channel the caller may have wanted.
    pub background: scene::Color,
}

impl Default for CanvasOptions {
    fn default() -> Self {
        Self {
            size: Size::new(300.0, 150.0),
            scale: 1.0,
            background: scene::Color::TRANSPARENT,
        }
    }
}

/// A configured surface that renders scenes.
///
/// Reusable across renders. Font registration and backend setup happen once, on
/// construction, so a server rendering many pictures pays for them once.
#[derive(Debug)]
pub struct Canvas {
    options: CanvasOptions,
}

impl Canvas {
    /// Creates a canvas from the given options.
    #[must_use]
    pub const fn new(options: CanvasOptions) -> Self {
        Self { options }
    }

    /// The options this canvas was built with.
    #[must_use]
    pub const fn options(&self) -> &CanvasOptions {
        &self.options
    }

    /// An empty one-page scene, sized and scaled from these options.
    ///
    /// The starting point for building a drawing: the caller fills it through
    /// [`scene::Scene::push`] and hands it back to [`Canvas::render`]. It takes
    /// its size and scale from the canvas rather than from the caller, so the
    /// two cannot disagree about how large the surface is.
    ///
    /// [`CanvasOptions::background`] is not applied here. It is painted beneath
    /// the page at render time rather than written onto the root node, so a
    /// caller who inspects the scene sees the tree they built and nothing the
    /// canvas added to it.
    #[must_use]
    pub fn scene(&self) -> Scene {
        let mut scene = Scene::new(self.options.size);
        scene.scale = self.options.scale;
        scene
    }

    /// Lays out and draws every page of `scene`, returning the encoded image.
    ///
    /// A `Scene` rather than a root node, because a node's children are arena
    /// indices: a lone [`scene::Node`] cannot carry a subtree, and a scene is
    /// what holds the arena those indices point into.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] from whichever pass fails: an unreadable image path, a
    /// URL this crate does not fetch, an unregistered font family, a tree taffy
    /// rejects, or an encoder that refuses the surface.
    pub fn render(
        &self,
        _scene: &Scene,
        _format: Format,
    ) -> Result<Vec<u8>, Error> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::{Canvas, CanvasOptions, Format, scene};

    #[test]
    fn the_defaults_are_the_html_canvas_ones() {
        let options = CanvasOptions::default();
        assert_eq!(options.size, scene::Size::new(300.0, 150.0));
        assert!((options.scale - 1.0).abs() < f32::EPSILON);
        assert_eq!(options.background, scene::Color::TRANSPARENT);
    }

    #[test]
    fn a_canvas_keeps_the_options_it_was_given() {
        let options = CanvasOptions {
            size: scene::Size::new(800.0, 600.0),
            scale: 3.0,
            background: scene::Color::rgb(1, 2, 3),
        };
        let canvas = Canvas::new(options.clone());
        assert_eq!(canvas.options(), &options);
        assert_eq!(*canvas.options(), options);
    }

    #[test]
    fn the_scene_takes_its_geometry_from_the_canvas() {
        let canvas = Canvas::new(CanvasOptions {
            size: scene::Size::new(120.0, 45.0),
            scale: 2.5,
            ..CanvasOptions::default()
        });
        let built = canvas.scene();

        assert_eq!(built.size, scene::Size::new(120.0, 45.0));
        assert!((built.scale - 2.5).abs() < f32::EPSILON);
        assert_eq!(built.pages, vec![scene::NodeId::ROOT]);
        assert_eq!(built.len(), 1);
        assert!(built.validate().is_ok());
    }

    #[test]
    fn the_background_is_not_written_onto_the_root() {
        let canvas = Canvas::new(CanvasOptions {
            background: scene::Color::rgb(9, 9, 9),
            ..CanvasOptions::default()
        });
        let built = canvas.scene();
        let root = built
            .get(scene::NodeId::ROOT)
            .unwrap_or_else(|| unreachable!("a new scene has a root"));
        assert_eq!(root.paint.background_color, scene::Color::TRANSPARENT);
    }

    #[test]
    fn a_scene_from_the_canvas_accepts_nodes() -> Result<(), scene::SceneError>
    {
        let canvas = Canvas::new(CanvasOptions::default());
        let mut built = canvas.scene();
        let child =
            built.push(scene::NodeId::ROOT, scene::Node::text("hello"))?;
        assert_eq!(built.len(), 2);
        assert_eq!(built.get(child).map(|node| node.children.len()), Some(0));
        built.validate()
    }

    #[test]
    fn the_re_exported_vocabulary_is_the_scene_crate_s() {
        // A compile-time assertion that the facade re-exports rather than
        // redefines: a redefinition would make these two types distinct.
        let colour: scene::Color = meo_canvas_scene::style::paint::Color::BLACK;
        assert_eq!(colour, scene::Color::BLACK);

        let format = Format::Png;
        assert_eq!(format, meo_canvas_core::ImageFormat::Png);
        assert!(!format!("{format:?}").is_empty());
        assert!(
            !format!("{:?}", Canvas::new(CanvasOptions::default())).is_empty()
        );
        assert!(!format!("{:?}", CanvasOptions::default()).is_empty());
    }
}
