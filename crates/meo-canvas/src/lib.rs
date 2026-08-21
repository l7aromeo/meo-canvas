//! Draw a picture by describing it, not by issuing commands.
//!
//! A caller builds a tree of nodes with sizes expressed the way CSS expresses
//! them, hands it over, and gets image bytes back. Placement is solved by a
//! real flexbox, grid and block implementation rather than by the caller doing
//! arithmetic on measured extents.
//!
//! ```no_run
//! use meo_canvas::{
//!     Canvas, CanvasOptions, Format,
//!     scene::{Color, Node, NodeId, PaintStyle, Size},
//! };
//!
//! let canvas = Canvas::new(CanvasOptions {
//!     size: Size::new(800.0, 600.0),
//!     scale: 2.0,
//!     ..CanvasOptions::default()
//! });
//!
//! let mut scene = canvas.scene();
//! scene.push(
//!     NodeId::ROOT,
//!     Node::container().with_paint(PaintStyle {
//!         background_color: Color::rgb(0x11, 0x22, 0x33),
//!         ..PaintStyle::default()
//!     }),
//! )?;
//!
//! let _bytes = canvas.render(&scene, Format::Png)?;
//! # Ok::<(), Box<dyn core::error::Error>>(())
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

pub use meo_canvas_core::{Error, ImageFormat as Format};

/// The scene vocabulary, re-exported so callers need one dependency rather than
/// two.
pub mod scene {
    pub use meo_canvas_scene::{
        Corners, Dimension, Length, Node, NodeId, NodeKind, Point, Rect, Scene,
        SceneError, Sides, Size,
        style::{
            layout::{Display, LayoutStyle},
            paint::{Color, PaintStyle},
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
