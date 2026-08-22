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

pub mod element;
pub mod style;
pub mod unit;

pub use element::{Box, Column, Element, Grid, Image, Path, Row, Text};
pub use meo_canvas_core::{
    EncodeOptions, Error, ImageFormat as Format, RenderedCanvas, Renderer,
};
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

use meo_canvas_scene::{Scene, SceneError, Size};

use crate::element::write_page;

/// The surface a tree is drawn onto, and the way in.
///
/// A canvas carries its size, its scale, and the pages to draw. Handing it a
/// [`Renderer`] paints them and gives back a [`RenderedCanvas`], which is what
/// encodes:
///
/// ```
/// use meo_canvas::{Canvas, EncodeOptions, Format, Renderer, Row};
///
/// let renderer = Renderer::new();
/// let mut canvas =
///     Canvas::new(64.0, 32.0).page(Row::new()).render(&renderer)?;
///
/// let png = canvas.to_buffer(Format::Png, &EncodeOptions::default())?;
/// let jpg = canvas.to_buffer(Format::Jpeg, &EncodeOptions::default())?;
/// # Ok::<(), meo_canvas_core::Error>(())
/// ```
///
/// Two formats, one paint. That split is why [`Canvas::render`] and
/// `to_buffer` are separate calls: resolving, measuring, laying out and
/// painting happen once, and each encode is only an encode.
///
/// The binding is `mut` because encoding takes `&mut self` — every encode
/// entry point in the renderer beneath does, since writing a format prepares
/// the page sequence first. A signature hiding that behind interior mutability
/// would let two encodes read as independent when they are not.
#[derive(Debug, Clone, PartialEq)]
pub struct Canvas {
    /// Width in logical pixels.
    width: f32,
    /// Height in logical pixels.
    height: f32,
    /// Device-pixel multiplier applied at paint time.
    scale: f32,
    /// One page per element, drawn in order.
    pages: Vec<Element>,
}

impl Canvas {
    /// The scale a canvas has when nothing sets one.
    ///
    /// One device pixel per logical pixel. Not a judgement about quality: a
    /// caller rendering for a display multiplies it, and a default above one
    /// would quadruple the memory of every render that never asked.
    pub const DEFAULT_SCALE: f32 = 1.0;

    /// A canvas of the given size in logical pixels.
    ///
    /// Bare pixels rather than a [`Length`](scene::Length), and deliberately: a
    /// canvas size is device-independent pixels, and a percentage of nothing
    /// has no meaning. The same reason [`Element::into_scene`] takes them.
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            scale: Self::DEFAULT_SCALE,
            pages: Vec::new(),
        }
    }

    /// The device-pixel multiplier.
    ///
    /// Layout always solves at scale one, so this changes resolution and
    /// nothing else about where things sit.
    #[must_use]
    pub const fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// The single page to draw, **replacing** any pages already set.
    ///
    /// Replaces rather than appends, matching [`Element::children`]. A chained
    /// singular that appended beside a chained plural that replaced would be
    /// two rules where a reader expects one: `.page(a).page(b)` yielding two
    /// pages while `.children([a]).children([b])` yields one is a trap nobody
    /// could infer. Use [`pages`](Self::pages) for more than one.
    #[must_use]
    pub fn page(mut self, page: Element) -> Self {
        self.pages = vec![page];
        self
    }

    /// Every page to draw, in order, **replacing** any already set.
    ///
    /// What a page means is the format's answer: a frame for GIF and APNG, a
    /// sheet for PDF and TIFF, one size of the same icon for ICO. Every other
    /// format writes one page and `EncodeOptions::page` chooses which.
    ///
    /// ```
    /// use meo_canvas::{Canvas, Column, Row};
    ///
    /// let frames = Canvas::new(64.0, 64.0).pages([Row::new(), Column::new()]);
    /// ```
    #[must_use]
    pub fn pages(mut self, pages: impl IntoIterator<Item = Element>) -> Self {
        self.pages = pages.into_iter().collect();
        self
    }

    /// Flattens the pages into a scene.
    ///
    /// What [`render`](Self::render) hands to the renderer, and public for a
    /// caller who wants the scene itself — to write to disk, to send over the
    /// wire, or to render more than once. [`Element::into_scene`] is the
    /// shortcut for the single-page case that needs no canvas.
    ///
    /// # Errors
    ///
    /// Returns [`SceneError::NoPages`] when no page was set, and
    /// [`SceneError::TooManyNodes`] when the pages together hold more nodes
    /// than the codec can address.
    pub fn into_scene(self) -> Result<Scene, SceneError> {
        let mut written = self.pages.into_iter();
        let first = written.next().ok_or(SceneError::NoPages)?;

        let mut scene = Scene::new(Size::new(self.width, self.height));
        scene.scale = self.scale;

        // `Scene::new` already made one page, so the first element styles it
        // and every later one adds its own root.
        let root = scene.root().ok_or(SceneError::NoPages)?;
        write_page(&mut scene, root, first)?;

        for page in written {
            let root = scene.push_page()?;
            write_page(&mut scene, root, page)?;
        }
        Ok(scene)
    }

    /// Paints every page and returns the canvas to encode from.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Scene`] when the pages do not form a scene, and
    /// whatever pass fails first otherwise — a font the renderer does not hold,
    /// an image it cannot read, a URL it does not fetch.
    pub fn render(self, renderer: &Renderer) -> Result<RenderedCanvas, Error> {
        let scene = self.into_scene().map_err(Error::Scene)?;
        renderer.render(&scene)
    }
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::SceneError;

    use super::{
        Canvas, Column, EncodeOptions, Error, Format, Renderer, Row, Style,
        hex_rgb,
    };

    #[test]
    fn a_new_canvas_takes_its_size_and_defaults_the_scale() {
        let scene = Canvas::new(120.0, 60.0)
            .page(Row::new())
            .into_scene()
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(scene.size.width.to_bits(), 120.0_f32.to_bits());
        assert_eq!(scene.size.height.to_bits(), 60.0_f32.to_bits());
        assert_eq!(scene.scale.to_bits(), Canvas::DEFAULT_SCALE.to_bits());
    }

    #[test]
    fn the_scale_reaches_the_scene_without_moving_the_layout() {
        // Layout always solves at one, so this changes resolution and nothing
        // about where things sit.
        let scene = Canvas::new(10.0, 10.0)
            .scale(3.0)
            .page(Row::new())
            .into_scene()
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(scene.scale.to_bits(), 3.0_f32.to_bits());
    }

    #[test]
    fn page_and_pages_both_replace() {
        // One rule, matching `Element::children`. A chained singular that
        // appended beside a chained plural that replaced would be a trap
        // nobody could infer.
        let one = Canvas::new(10.0, 10.0)
            .page(Row::new())
            .page(Column::new())
            .into_scene()
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(one.pages.len(), 1);

        let two = Canvas::new(10.0, 10.0)
            .pages([Row::new(), Column::new()])
            .pages([Row::new()])
            .into_scene()
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(two.pages.len(), 1);
    }

    #[test]
    fn every_page_becomes_a_root_of_its_own() {
        let scene = Canvas::new(10.0, 10.0)
            .pages([
                Row::new().style(Style::new().background(hex_rgb(0x11_11_11))),
                Row::new().style(Style::new().background(hex_rgb(0x22_22_22))),
                Row::new().style(Style::new().background(hex_rgb(0x33_33_33))),
            ])
            .into_scene()
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(scene.pages.len(), 3);
        assert!(scene.validate().is_ok(), "the pages form a valid scene");

        // The first page styles the root `Scene::new` already made; the rest
        // add their own. Each must carry its own style rather than the first.
        for (index, expected) in
            [0x11_11_11, 0x22_22_22, 0x33_33_33].into_iter().enumerate()
        {
            let root = scene
                .get(scene.pages[index])
                .unwrap_or_else(|| unreachable!("page {index} has a root"));
            assert_eq!(root.paint.background_color, hex_rgb(expected));
        }
    }

    #[test]
    fn a_canvas_with_no_page_draws_nothing_and_is_refused() {
        // A scene with no pages is an error for the same reason a scene with no
        // nodes is: the caller meant something they did not say.
        let empty = Canvas::new(10.0, 10.0).into_scene();
        assert!(matches!(empty, Err(SceneError::NoPages)));

        let emptied = Canvas::new(10.0, 10.0)
            .page(Row::new())
            .pages([])
            .into_scene();
        assert!(matches!(emptied, Err(SceneError::NoPages)));
    }

    #[test]
    fn a_canvas_renders_and_then_encodes_twice_from_one_paint() {
        // The whole point of `render` and `to_buffer` being separate calls:
        // resolving, measuring, laying out and painting happen once, and each
        // encode is only an encode.
        let renderer = Renderer::new();
        let mut canvas = Canvas::new(8.0, 4.0)
            .page(
                Row::new().style(Style::new().background(hex_rgb(0x10_10_14))),
            )
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(canvas.page_count(), 1);

        let png = canvas
            .to_buffer(Format::Png, &EncodeOptions::default())
            .unwrap_or_else(|error| unreachable!("{error}"));
        let jpg = canvas
            .to_buffer(Format::Jpeg, &EncodeOptions::default())
            .unwrap_or_else(|error| unreachable!("{error}"));

        // Container magic rather than a length: a JPEG written under a PNG's
        // name would pass a length check.
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&jpg[..2], b"\xff\xd8");
    }

    #[test]
    fn a_multi_page_canvas_paints_every_page() {
        let renderer = Renderer::new();
        let canvas = Canvas::new(4.0, 4.0)
            .pages([Row::new(), Row::new(), Row::new()])
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(canvas.page_count(), 3);
    }

    #[test]
    fn rendering_a_canvas_with_no_page_reports_the_scene_error() {
        let renderer = Renderer::new();
        let refused = Canvas::new(4.0, 4.0).render(&renderer);

        assert!(matches!(refused, Err(Error::Scene(SceneError::NoPages))));
    }

    #[test]
    fn a_pages_children_are_flattened_under_its_own_root() {
        let scene = Canvas::new(10.0, 10.0)
            .pages([Row::new().children([Column::new(), Column::new()])])
            .into_scene()
            .unwrap_or_else(|error| unreachable!("{error}"));

        // One root plus its two children.
        assert_eq!(scene.nodes.len(), 3);
        assert_eq!(scene.pages.len(), 1);
    }
}
