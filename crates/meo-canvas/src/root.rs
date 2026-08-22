//! The entry point, and the canvas a render gives back.
//!
//! `Root` is the whole surface's front door: it carries the canvas size, the
//! renderer's options and the page root's own style, and it is the page root —
//! its children are the tree. That is the same shape the JavaScript surface
//! has, deliberately. A person moving between the two should be translating
//! syntax rather than a design.
//!
//! ```
//! use meo_canvas::{Format, Renderer, Root, Row, Styled, Text, hex, px};
//!
//! let renderer = Renderer::new();
//! let mut canvas = Root::new(520.0, 180.0)
//!     .background_color(hex("#101014"))
//!     .children(Row::new().padding(px(24.0)).children(Text::new("Ukasyah")))
//!     .render(&renderer)?;
//!
//! let png = canvas.to_buffer(Format::Png)?;
//! # Ok::<(), meo_canvas::BuildError>(())
//! ```
//!
//! **Resolve, measure, layout and paint happen once.** Every method on the
//! [`Canvas`] that comes back is an encode of work already done, which is why
//! two formats cost one paint.

use std::{fmt, path::Path};

use meo_canvas_core::{
    EncodeOptions, Error, ImageFormat, RenderedCanvas, Renderer,
};
use meo_canvas_scene::{Scene, SceneError, Size};

use crate::{
    ColorSpace, ColorType, Element, IntoElements, Style, Styled,
    element::write_page,
};

/// Collects anything acceptable as children.
fn collect(children: impl IntoElements) -> Vec<Element> {
    let mut out = Vec::new();
    children.write_elements(&mut out);
    out
}

/// Where one page sits in a sequence.
///
/// The four derived numbers rather than the index alone, because each is right
/// for a different job and deriving the wrong one is a bug that looks like a
/// design choice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageInfo {
    /// Zero-based position in the sequence.
    pub index: u32,
    /// Total pages in this render.
    pub count: u32,
    /// Position along the sequence, `0` on the first page and `1` on the last.
    ///
    /// `index / (count - 1)`, which spans the sequence inclusively: a one-shot
    /// animation should finish at its end value on the frame the viewer stops
    /// on. The wrong curve for anything that repeats — see
    /// [`PageInfo::cycle`]. A single-page render reports `0`.
    pub progress: f32,
    /// Position around a loop, `0` on the first page and approaching `1`
    /// without reaching it.
    ///
    /// `index / count`, and the one to feed anything periodic. `1` and `0` are
    /// the same point on a circle, so driving a rotation from
    /// [`PageInfo::progress`] makes the last page a copy of the first and the
    /// animation stutters for one frame on every repeat.
    pub cycle: f32,
    /// Seconds elapsed at this page, `index / fps`.
    pub time: f32,
}

impl PageInfo {
    /// Where page `index` of `count` sits, at `fps`.
    fn new(index: u32, count: u32, fps: f32) -> Self {
        // A single page is the start of its own sequence and the whole of it at
        // once, so both curves report zero rather than dividing by no interval.
        let spread = if count > 1 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a page count large enough to lose precision in an f32 is one no renderer would finish"
            )]
            {
                (index as f32, (count - 1) as f32, count as f32)
            }
        } else {
            (0.0, 1.0, 1.0)
        };
        Self {
            index,
            count,
            progress: spread.0 / spread.1,
            cycle: spread.0 / spread.2,
            time: spread.0 / fps,
        }
    }
}

/// What went wrong between describing a canvas and painting it.
#[derive(Debug)]
pub enum BuildError {
    /// The pages contradict themselves, or there are none.
    Sequence(SequenceError),
    /// The pages do not form a scene the codec can address.
    Scene(SceneError),
    /// A pass of the render failed.
    Render(Error),
    /// The bytes could not be written to the path given.
    Write(std::io::Error),
}

impl fmt::Display for BuildError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sequence(error) => error.fmt(out),
            Self::Scene(error) => error.fmt(out),
            Self::Render(error) => error.fmt(out),
            Self::Write(error) => error.fmt(out),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<Error> for BuildError {
    fn from(error: Error) -> Self {
        Self::Render(error)
    }
}

/// A page count that cannot mean what it says.
///
/// Refused rather than resolved by precedence. A caller who named both a page
/// count and a duration, or a count with nothing to vary per page, asked for
/// something that would not happen — and v1 ignoring it quietly is the reason
/// it is worth refusing here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceError {
    /// Both a page count and a duration were named.
    CountAndDuration,
    /// A page builder was set with no count or duration to run it over.
    BuilderWithoutLength,
    /// A count or duration was named with no page builder to vary.
    LengthWithoutBuilder,
    /// A render has at least one page.
    NoPages,
}

impl fmt::Display for SequenceError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let said = match self {
            Self::CountAndDuration => {
                "name a page count or a duration, not both; they are two spellings of one number"
            }
            Self::BuilderWithoutLength => {
                "a page builder needs `pages` or `duration`; without one there is no sequence to build"
            }
            Self::LengthWithoutBuilder => {
                "`pages` and `duration` describe a sequence, so there has to be a page builder to vary"
            }
            Self::NoPages => "a render has at least one page",
        };
        out.write_str(said)
    }
}

impl std::error::Error for SequenceError {}

/// How the pages of a render are described.
enum Content {
    /// One page, holding these children.
    Fixed(Vec<Element>),
    /// A page per call, over a sequence whose length is set separately.
    Built(Box<dyn Fn(PageInfo) -> Vec<Element>>),
}

impl fmt::Debug for Content {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fixed(children) => {
                out.debug_tuple("Fixed").field(children).finish()
            }
            Self::Built(_) => out.write_str("Built(<closure>)"),
        }
    }
}

/// The canvas to draw, and the tree to draw on it.
///
/// `Root` is the page root as well as the entry point: its style is the page's
/// style, and its children are the tree. That is why `background_color` here
/// paints the canvas rather than a box inside it.
#[derive(Debug)]
pub struct Root {
    /// Width in logical pixels.
    width: f32,
    /// Height in logical pixels.
    height: f32,
    /// Device-pixel multiplier applied at paint time.
    scale: f32,
    /// The page root's own style.
    style: Style,
    /// A name carried through for diagnostics.
    name: Option<String>,
    /// Whether to rasterise on the GPU, where the caller said.
    gpu: Option<bool>,
    /// The pixel layout to composite in, where the caller said.
    color_type: Option<ColorType>,
    /// The colour space to composite in, where the caller said.
    color_space: Option<ColorSpace>,
    /// What to draw.
    content: Content,
    /// How many pages, when a builder describes them.
    pages: Option<u32>,
    /// How long the sequence runs, in seconds.
    duration: Option<f32>,
    /// The rate a duration and a page's time are derived at.
    fps: f32,
}

impl Root {
    /// The rate a sequence is timed at when nothing says otherwise.
    pub const DEFAULT_FPS: f32 = 30.0;
    /// The scale a canvas has when nothing sets one.
    ///
    /// One device pixel per logical pixel. Not a judgement about quality: a
    /// caller rendering for a display multiplies it, and a default above one
    /// would quadruple the memory of every render that never asked.
    pub const DEFAULT_SCALE: f32 = 1.0;

    /// A canvas of the given size in logical pixels.
    ///
    /// Bare pixels rather than a [`Length`](crate::scene::Length), and
    /// deliberately: a canvas size is device-independent pixels, and a
    /// percentage of nothing has no meaning.
    ///
    /// The height is required, where v1 derives it from the content when it is
    /// left out. The renderer gives a page root the scene's extent on any axis
    /// left automatic, so there is no content-sizing pass for a page — a height
    /// derived from content is something this surface cannot honour yet rather
    /// than something it declines to.
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            scale: Self::DEFAULT_SCALE,
            style: Style::new(),
            name: None,
            gpu: None,
            color_type: None,
            color_space: None,
            content: Content::Fixed(Vec::new()),
            pages: None,
            duration: None,
            fps: Self::DEFAULT_FPS,
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

    /// A name carried through for diagnostics.
    ///
    /// Every page gets it, because every page is this root. The renderer never
    /// reads it.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Rasterise on the GPU when there is one. `false` forces the CPU.
    ///
    /// A property of the scene rather than of the [`Renderer`], because the
    /// canvas is the thing being rasterised — `scale` lives there for the same
    /// reason. Saying nothing leaves it to the renderer, which is a different
    /// thing from saying `true`: a renderer someone set to CPU stays there.
    ///
    /// Asking is not getting. A build without GPU support, or a driver that
    /// declines, falls back — and a float [`ColorType`] forces the CPU whatever
    /// this says, since no GPU composites float. Set it `false` for output that
    /// must be identical between machines: the two rasterisers resolve
    /// anti-aliased edges a level or two apart, which a pixel comparison sees.
    #[must_use]
    pub const fn gpu(mut self, gpu: bool) -> Self {
        self.gpu = Some(gpu);
        self
    }

    /// The pixel layout to composite in.
    ///
    /// Governs the precision everything is drawn at and the depth the encoded
    /// formats that carry one write. Saying nothing leaves it to the renderer.
    #[must_use]
    pub const fn color_type(mut self, color_type: ColorType) -> Self {
        self.color_type = Some(color_type);
        self
    }

    /// The colour space to composite in.
    ///
    /// Fixed for the whole render rather than chosen per export: colours are
    /// interpreted in it, and one outside its gamut is clipped as it is drawn.
    /// Saying nothing leaves it to the renderer.
    #[must_use]
    pub const fn color_space(mut self, color_space: ColorSpace) -> Self {
        self.color_space = Some(color_space);
        self
    }

    /// The tree to draw, **replacing** anything already set.
    #[must_use]
    pub fn children(mut self, children: impl IntoElements) -> Self {
        self.content = Content::Fixed(collect(children));
        self
    }

    /// A tree per page, for a sequence.
    ///
    /// Needs [`pages`](Self::pages) or [`duration`](Self::duration) to say how
    /// long the sequence is. This is where the surfaces differ in syntax and
    /// not in shape: JavaScript passes a function as `children` because it can,
    /// and Rust names the two forms apart because it cannot.
    #[must_use]
    pub fn page_builder<C: IntoElements>(
        mut self,
        builder: impl Fn(PageInfo) -> C + 'static,
    ) -> Self {
        self.content =
            Content::Built(Box::new(move |page| collect(builder(page))));
        self
    }

    /// How many pages to render.
    #[must_use]
    pub const fn pages(mut self, pages: u32) -> Self {
        self.pages = Some(pages);
        self
    }

    /// How long the sequence runs, in seconds.
    ///
    /// The page count becomes `ceil(duration * fps)`.
    #[must_use]
    pub const fn duration(mut self, seconds: f32) -> Self {
        self.duration = Some(seconds);
        self
    }

    /// The rate a duration and a page's time are derived at.
    ///
    /// Describes the render, not the encode. An animation encoded to play at
    /// this rate needs it named in [`EncodeOptions`] as well.
    #[must_use]
    pub const fn fps(mut self, fps: f32) -> Self {
        self.fps = fps;
        self
    }

    /// How many pages this describes, and at what rate.
    fn sequence(&self) -> Result<(u32, f32), SequenceError> {
        let built = matches!(self.content, Content::Built(_));

        if self.pages.is_some() && self.duration.is_some() {
            return Err(SequenceError::CountAndDuration);
        }

        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a duration long enough to overflow a u32 of pages is one no renderer would finish, and a negative one is refused as a count below one"
        )]
        let asked = self
            .pages
            .or_else(|| self.duration.map(|s| (s * self.fps).ceil() as u32));

        let Some(count) = asked else {
            if built {
                return Err(SequenceError::BuilderWithoutLength);
            }
            return Ok((1, self.fps));
        };
        if !built {
            return Err(SequenceError::LengthWithoutBuilder);
        }
        if count == 0 {
            return Err(SequenceError::NoPages);
        }
        Ok((count, self.fps))
    }

    /// Flattens this into a scene.
    ///
    /// What [`render`](Self::render) hands to the renderer, and public for a
    /// caller who wants the scene itself — to write to disk, to send over the
    /// wire, or to render more than once.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Sequence`] when the page count contradicts itself,
    /// and [`BuildError::Scene`] when the pages together hold more nodes than
    /// the codec can address.
    pub fn into_scene(self) -> Result<Scene, BuildError> {
        let (count, fps) = self.sequence().map_err(BuildError::Sequence)?;

        let mut scene = Scene::new(Size::new(self.width, self.height));
        scene.scale = self.scale;
        scene.gpu = self.gpu;
        scene.color_type = self.color_type;
        scene.color_space = self.color_space;

        let page_root = |children: Vec<Element>| {
            let root = Element::new(meo_canvas_scene::node::NodeKind::Box)
                .with_style(self.style.clone())
                .children(children);
            match &self.name {
                Some(name) => root.name(name.clone()),
                None => root,
            }
        };

        // `Scene::new` already made one page, so the first tree styles it and
        // every later one adds its own root.
        let write = |scene: &mut Scene,
                     first: bool,
                     children: Vec<Element>|
         -> Result<(), BuildError> {
            let root = if first {
                scene
                    .root()
                    .ok_or(BuildError::Sequence(SequenceError::NoPages))?
            } else {
                scene.push_page().map_err(BuildError::Scene)?
            };
            write_page(scene, root, page_root(children))
                .map_err(BuildError::Scene)
        };

        match &self.content {
            Content::Fixed(children) => {
                write(&mut scene, true, children.clone())?;
            }
            Content::Built(builder) => {
                for index in 0..count {
                    let children = builder(PageInfo::new(index, count, fps));
                    write(&mut scene, index == 0, children)?;
                }
            }
        }
        Ok(scene)
    }

    /// Paints every page and returns the canvas to encode from.
    ///
    /// The GPU request and the registered families live on the [`Renderer`]
    /// rather than here, which is the one place these two surfaces are shaped
    /// differently. JavaScript puts them on `Root` because it exposes no
    /// renderer to put them on; Rust does, and a renderer outlives any one
    /// scene — a server registering its fonts once is the reason that type
    /// exists. Carrying them in both places would be two settings that can
    /// disagree, so `Renderer::set_gpu` and `Renderer::register_font` are where
    /// they are said. Nothing a JavaScript caller can express is missing.
    ///
    /// # Errors
    ///
    /// Returns whatever [`into_scene`](Self::into_scene) reports, and
    /// [`BuildError::Render`] when a pass of the render fails — a font the
    /// renderer does not hold, an image it cannot read, a URL it does not
    /// fetch.
    pub fn render(self, renderer: &Renderer) -> Result<Canvas, BuildError> {
        let scene = self.into_scene()?;
        Ok(Canvas {
            painted: renderer.render(&scene)?,
        })
    }
}

impl Styled for Root {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// A painted canvas, and the ways to read it back.
///
/// Returned by [`Root::render`]; a caller never builds one. It holds the
/// painted surface and nothing else, so every method here is an encode of work
/// already done.
///
/// The binding is `mut` because encoding takes `&mut self` — every encode entry
/// point in the renderer beneath does, since writing a format prepares the page
/// sequence first. A signature hiding that behind interior mutability would let
/// two encodes read as independent when they are not.
#[derive(Debug)]
pub struct Canvas {
    /// The painted pages.
    painted: RenderedCanvas,
}

impl Canvas {
    /// Encodes the canvas and returns the bytes.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Render`] when the encoder refuses the surface.
    pub fn to_buffer(
        &mut self,
        format: ImageFormat,
    ) -> Result<Vec<u8>, BuildError> {
        self.to_buffer_with(format, &EncodeOptions::default())
    }

    /// Encodes the canvas with quality and container settings.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Render`] when the encoder refuses the surface or
    /// the options do not fit the painted pages.
    pub fn to_buffer_with(
        &mut self,
        format: ImageFormat,
        options: &EncodeOptions,
    ) -> Result<Vec<u8>, BuildError> {
        Ok(self.painted.to_buffer(format, options)?)
    }

    /// Encodes the canvas and writes it to `path`.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Render`] when the encode fails or the file cannot
    /// be written.
    pub fn to_file(
        &mut self,
        path: impl AsRef<Path>,
        format: ImageFormat,
    ) -> Result<(), BuildError> {
        self.to_file_with(path, format, &EncodeOptions::default())
    }

    /// Encodes the canvas with options and writes it to `path`.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Render`] when the encode fails or the file cannot
    /// be written.
    pub fn to_file_with(
        &mut self,
        path: impl AsRef<Path>,
        format: ImageFormat,
        options: &EncodeOptions,
    ) -> Result<(), BuildError> {
        let bytes = self.to_buffer_with(format, options)?;
        std::fs::write(path, bytes).map_err(BuildError::Write)
    }

    /// Encodes the canvas and returns a `data:` URL.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Render`] when the encoder refuses the surface.
    pub fn to_url(
        &mut self,
        format: ImageFormat,
    ) -> Result<String, BuildError> {
        let bytes = self.to_buffer(format)?;
        Ok(format!(
            "data:{};base64,{}",
            format.media_type(),
            base64(&bytes)
        ))
    }

    /// The `HTMLCanvasElement` spelling of [`to_url`](Self::to_url).
    ///
    /// Taking a quality rather than an options object, because the DOM method
    /// it is named after does. v1 has it for the same reason.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Render`] when the encoder refuses the surface.
    pub fn to_data_url(
        &mut self,
        format: ImageFormat,
        quality: Option<f32>,
    ) -> Result<String, BuildError> {
        let options = EncodeOptions {
            quality,
            ..EncodeOptions::default()
        };
        let bytes = self.to_buffer_with(format, &options)?;
        Ok(format!(
            "data:{};base64,{}",
            format.media_type(),
            base64(&bytes)
        ))
    }

    /// How many pages were painted.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.painted.page_count()
    }

    /// The device-pixel multiplier the pages were drawn at.
    #[must_use]
    pub const fn scale(&self) -> f32 {
        self.painted.scale()
    }
}

/// Base64, for a `data:` URL.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let (a, b, c) = (
            u32::from(chunk[0]),
            chunk.get(1).map_or(0, |byte| u32::from(*byte)),
            chunk.get(2).map_or(0, |byte| u32::from(*byte)),
        );
        let triple = (a << 16) | (b << 8) | c;

        out.push(ALPHABET[((triple >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(triple & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::node::NodeKind;

    use super::{BuildError, PageInfo, Root, SequenceError};
    use crate::{
        Box as BoxNode, ColorSpace, ColorType, Column, Format, Renderer, Row,
        Styled, Text, hex_rgb, px,
    };

    /// The scene `root` describes, or the failure it reports.
    fn scene_of(root: Root) -> meo_canvas_scene::Scene {
        root.into_scene()
            .unwrap_or_else(|error| unreachable!("{error}"))
    }

    #[test]
    fn a_root_takes_its_size_and_defaults_the_scale() {
        let scene = scene_of(Root::new(120.0, 60.0));

        assert_eq!(scene.size.width.to_bits(), 120.0_f32.to_bits());
        assert_eq!(scene.size.height.to_bits(), 60.0_f32.to_bits());
        assert_eq!(scene.scale.to_bits(), Root::DEFAULT_SCALE.to_bits());
    }

    #[test]
    fn the_scale_reaches_the_scene_without_moving_the_layout() {
        // Layout always solves at one, so this changes resolution and nothing
        // about where things sit.
        let scene = scene_of(Root::new(10.0, 10.0).scale(3.0));

        assert_eq!(scene.scale.to_bits(), 3.0_f32.to_bits());
    }

    #[test]
    fn the_root_is_the_page_rather_than_a_box_inside_it() {
        // `background_color` on `Root` paints the canvas. A wrapper node would
        // paint a box the size of the content instead, which is the bug this
        // shape exists to make impossible.
        let scene = scene_of(
            Root::new(10.0, 10.0)
                .background_color(hex_rgb(0x10_10_14))
                .children(Row::new()),
        );
        let root = scene
            .get(scene.pages[0])
            .unwrap_or_else(|| unreachable!("the scene has no root"));

        assert_eq!(root.paint.background_color, hex_rgb(0x10_10_14));
        assert_eq!(root.children.len(), 1);
    }

    #[test]
    fn children_take_one_or_many_and_skip_what_did_not_render() {
        let one = scene_of(Root::new(10.0, 10.0).children(Row::new()));
        let many = scene_of(
            Root::new(10.0, 10.0).children([Row::new(), Column::new()]),
        );
        let conditional = scene_of(Root::new(10.0, 10.0).children([
            Some(Row::new()),
            None,
            Some(Column::new()),
        ]));

        let count = |scene: &meo_canvas_scene::Scene| {
            scene
                .get(scene.pages[0])
                .map_or(0, |root| root.children.len())
        };
        assert_eq!(count(&one), 1);
        assert_eq!(count(&many), 2);
        // The `None` contributes nothing rather than an empty node, which is
        // what `cond && Text(…)` means on the other surface.
        assert_eq!(count(&conditional), 2);
    }

    #[test]
    fn a_flat_setter_writes_the_same_field_the_style_does() {
        // The trait's whole claim: one list of properties, two places they can
        // be written, and no chance of the two drifting apart.
        let scene = scene_of(
            Root::new(10.0, 10.0)
                .children(Row::new().gap(px(4.0)).padding(crate::all(px(8.0)))),
        );
        let root = scene
            .get(scene.pages[0])
            .unwrap_or_else(|| unreachable!("the scene has no root"));
        let child = scene
            .get(root.children[0])
            .unwrap_or_else(|| unreachable!("the root has no child"));

        assert_eq!(child.layout.gap.0, meo_canvas_scene::Length::Points(4.0));
        assert_eq!(
            child.layout.padding.top,
            meo_canvas_scene::Length::Points(8.0)
        );
    }

    #[test]
    fn a_name_reaches_the_node_it_was_written_on() {
        // Not a style property: the scene keeps it on the node beside the kind
        // and nothing inherits it, so it does not go through `properties!`.
        let scene = scene_of(
            Root::new(10.0, 10.0)
                .name("page")
                .children(Row::new().name("card")),
        );
        let root = scene
            .get(scene.pages[0])
            .unwrap_or_else(|| unreachable!("the scene has no root"));
        let child = scene
            .get(root.children[0])
            .unwrap_or_else(|| unreachable!("the root has no child"));

        assert_eq!(root.name.as_deref(), Some("page"));
        assert_eq!(child.name.as_deref(), Some("card"));
    }

    #[test]
    fn a_name_is_absent_when_none_was_given() {
        let scene = scene_of(Root::new(10.0, 10.0).children(Row::new()));
        let root = scene
            .get(scene.pages[0])
            .unwrap_or_else(|| unreachable!("the scene has no root"));

        assert_eq!(root.name, None);
    }

    #[test]
    fn the_surface_options_reach_the_scene_and_stay_absent_otherwise() {
        // Absent is not `false`: the renderer decides when the caller said
        // nothing, and a default written here would take that decision away
        // from a renderer someone deliberately set to CPU.
        let silent = scene_of(Root::new(10.0, 10.0));
        let stated = scene_of(
            Root::new(10.0, 10.0)
                .gpu(false)
                .color_type(ColorType::F32)
                .color_space(ColorSpace::DisplayP3),
        );

        assert_eq!(silent.gpu, None);
        assert_eq!(silent.color_type, None);
        assert_eq!(silent.color_space, None);
        assert_eq!(stated.gpu, Some(false));
        assert_eq!(stated.color_type, Some(ColorType::F32));
        assert_eq!(stated.color_space, Some(ColorSpace::DisplayP3));
    }

    #[test]
    fn the_two_rasterisers_do_not_draw_the_same_pixels() {
        // The check a fake cannot satisfy. `gpu` reached the addon through a
        // paint-options object once, the addon stopped reading it there, and
        // every test asserting the flag had been copied stayed green while it
        // reached nothing. Two real renders that must differ cannot pass that
        // way -- and if this build has no GPU compiled in, both are the CPU and
        // the test says so rather than passing vacuously.
        //
        // The content is load-bearing: the two rasterisers differ on
        // anti-aliased edges and agree exactly on a picture without any, so
        // text is what makes them disagree. A plain filled box here would fail
        // this rather than pass it quietly, which is the right way round, but
        // worth knowing before changing the scene.
        let renderer = Renderer::new();
        let rounded = || {
            BoxNode::new()
                .size(px(120.0), px(60.0))
                .border_radius(24.0)
                .background_color(hex_rgb(0xff_ff_ff))
        };

        let mut on = Root::new(200.0, 80.0)
            .gpu(true)
            .children(rounded())
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut off = Root::new(200.0, 80.0)
            .gpu(false)
            .children(rounded())
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let with = on
            .to_buffer(Format::Png)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let without = off
            .to_buffer(Format::Png)
            .unwrap_or_else(|error| unreachable!("{error}"));

        if cfg!(any(feature = "metal", feature = "vulkan")) {
            assert_ne!(
                with, without,
                "a GPU backend is compiled in, so the two rasterisers should not agree to the byte"
            );
        } else {
            assert_eq!(
                with, without,
                "no GPU backend is compiled in, so both renders are the CPU's"
            );
        }
    }

    #[test]
    fn a_page_builder_runs_once_per_page() {
        let scene =
            scene_of(Root::new(10.0, 10.0).pages(3).page_builder(|page| {
                Text::new(format!("page {}", page.index))
            }));

        assert_eq!(scene.pages.len(), 3);
    }

    #[test]
    fn a_duration_becomes_a_page_count_at_the_rate_given() {
        // `ceil(duration * fps)`, as v1 derives it: a fraction of a page is
        // still a page that has to be drawn.
        let scene = scene_of(
            Root::new(10.0, 10.0)
                .duration(0.1)
                .fps(24.0)
                .page_builder(|_| Row::new()),
        );

        assert_eq!(scene.pages.len(), 3);
    }

    #[test]
    fn a_page_knows_where_it_sits() {
        let first = PageInfo::new(0, 4, 10.0);
        let last = PageInfo::new(3, 4, 10.0);

        // `progress` reaches one on the last page; `cycle` never does, because
        // the page after the last is the next loop's first.
        assert_eq!(first.progress.to_bits(), 0.0_f32.to_bits());
        assert_eq!(last.progress.to_bits(), 1.0_f32.to_bits());
        assert_eq!(last.cycle.to_bits(), 0.75_f32.to_bits());
        assert_eq!(last.time.to_bits(), 0.3_f32.to_bits());
    }

    #[test]
    fn a_single_page_reports_both_curves_as_zero() {
        let only = PageInfo::new(0, 1, 30.0);

        assert_eq!(only.progress.to_bits(), 0.0_f32.to_bits());
        assert_eq!(only.cycle.to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn a_sequence_that_contradicts_itself_is_refused() {
        let both = Root::new(10.0, 10.0)
            .pages(2)
            .duration(1.0)
            .page_builder(|_| Row::new())
            .into_scene();
        let builder_alone = Root::new(10.0, 10.0)
            .page_builder(|_| Row::new())
            .into_scene();
        let length_alone = Root::new(10.0, 10.0)
            .pages(2)
            .children(Row::new())
            .into_scene();
        let none = Root::new(10.0, 10.0)
            .pages(0)
            .page_builder(|_| Row::new())
            .into_scene();

        assert!(matches!(
            both,
            Err(BuildError::Sequence(SequenceError::CountAndDuration))
        ));
        assert!(matches!(
            builder_alone,
            Err(BuildError::Sequence(SequenceError::BuilderWithoutLength))
        ));
        assert!(matches!(
            length_alone,
            Err(BuildError::Sequence(SequenceError::LengthWithoutBuilder))
        ));
        assert!(matches!(
            none,
            Err(BuildError::Sequence(SequenceError::NoPages))
        ));
    }

    #[test]
    fn a_canvas_encodes_twice_from_one_paint() {
        let renderer = Renderer::new();
        let mut canvas = Root::new(8.0, 4.0)
            .background_color(hex_rgb(0x10_10_14))
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let png = canvas
            .to_buffer(Format::Png)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let jpg = canvas
            .to_buffer(Format::Jpeg)
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&jpg[..2], b"\xff\xd8");
        assert_eq!(canvas.page_count(), 1);
    }

    #[test]
    fn a_data_url_carries_the_format_and_the_bytes() {
        let renderer = Renderer::new();
        let mut canvas = Root::new(4.0, 4.0)
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let url = canvas
            .to_url(Format::Png)
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert!(url.starts_with("data:image/png;base64,"), "{url}");
        // The PNG signature, base64-encoded, is what every PNG data URL opens
        // with -- so this says the bytes are the image rather than anything.
        assert!(url.contains("iVBORw0KGgo"), "{url}");
    }

    #[test]
    fn a_page_root_holds_the_tree_rather_than_wrapping_it() {
        let scene =
            scene_of(Root::new(10.0, 10.0).children(
                Row::new().children([Text::new("a"), Text::new("b")]),
            ));
        let root = scene
            .get(scene.pages[0])
            .unwrap_or_else(|| unreachable!("no root"));
        let row = scene
            .get(root.children[0])
            .unwrap_or_else(|| unreachable!("no row"));

        assert!(matches!(root.kind, NodeKind::Box));
        assert_eq!(row.children.len(), 2);
    }
}
