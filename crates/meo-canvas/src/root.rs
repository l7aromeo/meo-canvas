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
//! let mut canvas = Root::new(520.0)
//!     .height(180.0)
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
    EncodeOptions, Error, ImageFormat, ImageWarning, PreparedEncode,
    RenderedCanvas, Renderer,
};
use meo_canvas_scene::{OnImageError, Scene, SceneError, Size};

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
#[non_exhaustive]
pub enum BuildError {
    /// The pages contradict themselves, or there are none.
    Sequence(SequenceError),
    /// The pages do not form a scene the codec can address.
    Scene(SceneError),
    /// A pass of the render failed.
    Render(Error),
    /// The bytes could not be written to the path given.
    Write(std::io::Error),
    /// A filename's extension names no format this can write.
    Format(String),
}

impl fmt::Display for BuildError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sequence(error) => error.fmt(out),
            Self::Scene(error) => error.fmt(out),
            Self::Render(error) => error.fmt(out),
            Self::Write(error) => error.fmt(out),
            Self::Format(path) => write!(
                out,
                "cannot tell the format from {path:?}; name the file with an extension such as .png, or use `to_file_as`"
            ),
        }
    }
}

impl std::error::Error for BuildError {
    /// The error underneath, so a chain does not stop here.
    ///
    /// **Every variant wraps a real error and this returned `None` until 5
    /// September 2026.** `Display` forwarded, so a person reading the message
    /// saw the cause; `anyhow`'s `{:#}`, `eyre`'s chain and any caller walking
    /// `source()` saw one opaque error and lost the `io::ErrorKind` under
    /// `Write` -- which is the one thing a caller can act on programmatically.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sequence(error) => Some(error),
            Self::Scene(error) => Some(error),
            Self::Render(error) => Some(error),
            Self::Write(error) => Some(error),
            // The path is the whole of it; there is nothing underneath.
            Self::Format(_) => None,
        }
    }
}

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
#[non_exhaustive]
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
    /// Height in logical pixels, or the floor when [`Root::content_height`] is
    /// what set it.
    height: f32,
    /// What a render does when an image source cannot be resolved.
    on_image_error: OnImageError,
    /// Whether the height comes from the content rather than from the caller.
    content_height: bool,
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
    /// **The height is optional and comes from the content when not set**, the
    /// same as leaving `height` out on the JavaScript surface. Add one with
    /// [`Root::height`], or a lower bound with [`Root::min_height`].
    ///
    /// Rust has no optional argument, and a second constructor taking two
    /// numbers would make the caller pick a spelling before knowing there was
    /// a choice. `Root` configures everything else -- scale, name, gpu -- by
    /// chaining, so the height does too, and the two surfaces then say the
    /// same thing: a width is required and a height is not.
    ///
    /// There is no matching form for the width. Text breaks into lines against
    /// a width, so a width has to be known before anything can be measured,
    /// while a height is a result of that measuring.
    #[must_use]
    pub const fn new(width: f32) -> Self {
        Self {
            width,
            height: 0.0,
            content_height: true,
            scale: Self::DEFAULT_SCALE,
            on_image_error: OnImageError::Placeholder,
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

    /// Fixes the canvas height, instead of taking it from the content.
    #[must_use]
    pub const fn height(mut self, height: f32) -> Self {
        self.height = height;
        self.content_height = false;
        self
    }

    /// The least the canvas may be while its height comes from its content.
    ///
    /// Does nothing once [`Root::height`] has fixed a height: a floor under a
    /// number that is already stated has nothing to raise.
    #[must_use]
    pub const fn min_height(mut self, floor: f32) -> Self {
        if self.content_height {
            self.height = floor;
        }
        self
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

    /// What to do when an image source cannot be resolved.
    ///
    /// Defaults to [`OnImageError::Placeholder`]: a URL that cannot be fetched
    /// or decoded draws a neutral mark, the layout is unchanged, and the
    /// failure is recorded on [`Canvas::warnings`]. A
    /// [`Path`](meo_canvas_scene::node::ImageSource::Path) or
    /// [`Bytes`](meo_canvas_scene::node::ImageSource::Bytes) source fails the
    /// render whatever this says -- the caller is holding that input and can
    /// check it before rendering, where a fetch's outcome does not exist until
    /// the render runs.
    ///
    /// [`OnImageError::Throw`] is the behaviour of every version before this
    /// one, for a caller whose URLs come from a manifest they control.
    ///
    /// **Every setting records the warning.** This chooses what is drawn, not
    /// what is known.
    #[must_use]
    pub const fn on_image_error(mut self, policy: OnImageError) -> Self {
        self.on_image_error = policy;
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

        // The width came from `Root::new`, which is a `const fn` returning
        // `Self` and so has nowhere to report a number that is not a length.
        // This is where it reports: the first call that can fail.
        let size = Size::new(self.width, self.height);
        if !meo_canvas_scene::size_is_pixels(size) {
            return Err(BuildError::Scene(SceneError::canvas_size(
                self.width,
                self.height,
            )));
        }

        let mut scene = Scene::new(size);
        scene.content_height = self.content_height;
        scene.scale = self.scale;
        scene.gpu = self.gpu;
        scene.on_image_error = self.on_image_error;
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

    /// Takes the half of an encode that needs this canvas, and stops there.
    ///
    /// The returned [`PreparedEncode`] is [`Send`] where this canvas is not,
    /// so the expensive half of an export runs wherever the caller puts it
    /// while this canvas stays on the thread that owns it. At 4000x4000 that
    /// is 97 ms of the 100 a `to_buffer` costs.
    ///
    /// **Here because the JavaScript surface has it.** `toBuffer` there
    /// returns a promise that settles off the event loop, and a Rust caller
    /// serving requests from a thread pool wants the same thing for the same
    /// reason. A capability on one of these two surfaces and not the other is
    /// a defect rather than a difference.
    ///
    /// One handle is one format: encoding a second format is a second call,
    /// made here rather than on the worker.
    ///
    /// ```
    /// use meo_canvas::{EncodeOptions, Format, Renderer, Root};
    ///
    /// let renderer = Renderer::new();
    /// let mut canvas = Root::new(64.0).height(32.0).render(&renderer)?;
    ///
    /// let pending =
    ///     canvas.prepare_encode(Format::Png, &EncodeOptions::default())?;
    /// let png = std::thread::spawn(move || pending.encode())
    ///     .join()
    ///     .expect("the encoding thread")?;
    /// assert!(!png.bytes.is_empty());
    /// # Ok::<(), meo_canvas::BuildError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Render`] when the options do not fit the painted
    /// pages. Failures that belong to encoding itself are raised by
    /// [`PreparedEncode::encode`].
    pub fn prepare_encode(
        &mut self,
        format: ImageFormat,
        options: &EncodeOptions,
    ) -> Result<PreparedEncode, BuildError> {
        Ok(self.painted.prepare_encode(format, options)?)
    }

    /// Encodes the canvas and writes it to `path`, taking the format from the
    /// extension.
    ///
    /// `canvas.to_file("out.png")`, the same call a JavaScript caller writes,
    /// and it accepts the same extensions — `.raw` among them. An extension
    /// naming no format is an error rather than a default: writing a PNG
    /// because nothing said otherwise turns a typo into a file whose name lies
    /// about its contents.
    ///
    /// Resolved through [`ImageFormat::from_named`] rather than
    /// `from_extension`, which refuses `raw` — correctly, for a filename found
    /// on disk, and wrongly for one the caller has just typed. This once
    /// accepted a narrower set than `toFile` did on the JavaScript surface.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Format`] when the extension names no format, and
    /// [`BuildError::Render`] or [`BuildError::Write`] when the encode or the
    /// write fails.
    pub fn to_file(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<(), BuildError> {
        let path = path.as_ref();
        let format = path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(ImageFormat::from_named)
            .ok_or_else(|| BuildError::Format(path.display().to_string()))?;
        self.to_file_with(path, format, &EncodeOptions::default())
    }

    /// Encodes the canvas in the named format and writes it to `path`.
    ///
    /// # Errors
    ///
    /// As [`Canvas::to_file_with`], whose notes on the two error variants
    /// apply here too.
    pub fn to_file_as(
        &mut self,
        path: impl AsRef<Path>,
        format: ImageFormat,
    ) -> Result<(), BuildError> {
        self.to_file_with(path, format, &EncodeOptions::default())
    }

    /// Encodes the canvas with options and writes it to `path`.
    ///
    /// **The bytes are written where they are encoded.** This used to encode
    /// to a `Vec<u8>` and hand it to `std::fs::write`, so a page-spanning
    /// format existed whole in memory before any of it reached the disk. A
    /// format that gathers every page now streams into the file instead.
    ///
    /// The JavaScript surface's `toFile` was given this first, which left the
    /// two differing in a capability rather than in a spelling —
    /// `AGENTS.md`'s parity rule, and the interesting part is that **the
    /// parity gate could not see it**: `just example` compares the bytes the
    /// two surfaces write, and the bytes were never in question. What differed
    /// was how much memory it took to produce them.
    ///
    /// # Why the path is opened before the encode
    ///
    /// So a path that cannot be written is still [`BuildError::Write`] with
    /// its [`std::io::ErrorKind`] intact — a missing directory, a permission
    /// refusal, a read-only filesystem. The renderer folds a write failure
    /// into its own encode error, and that error carries a message rather than
    /// a kind, so delegating without this would have quietly undone the
    /// `source()` fix of 5 September 2026: the kind under `Write` is the one
    /// thing a caller can act on programmatically.
    ///
    /// It also fails sooner, which matters more the longer the export. What it
    /// does not cover is a failure *during* the write — a full disk — which
    /// arrives as [`BuildError::Render`] carrying the renderer's message.
    ///
    /// # Why the probe does not truncate, and does not leave a file behind
    ///
    /// **A failed encode must not destroy the file that was already there.**
    /// `File::create` truncates, so probing with it would empty the caller's
    /// previous render before attempting one that may be refused — and an
    /// encode is refused for ordinary reasons this method documents. Losing
    /// yesterday's output to a bad `EncodeOptions` is worse than losing the
    /// error kind this probe exists to keep.
    ///
    /// So an existing path is opened for writing without truncating, which
    /// asks the same question and answers it the same way. Truncation happens
    /// where it is correct: in the renderer, on the success path.
    ///
    /// A path that does *not* exist is created and removed again, because the
    /// only thing wanted from it was the filesystem's answer. Left in place it
    /// would put a zero-byte file where a failed encode used to leave nothing.
    /// That removal is the one error here that is swallowed, and it is safe to
    /// swallow: it can only fail by leaving behind the empty file the
    /// alternative design would have left anyway, and it can only ever touch a
    /// file this call has just created — `create_new` is what makes that
    /// atomic rather than a check and a hope.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Write`] when the path cannot be opened for
    /// writing, and [`BuildError::Render`] when the options do not suit the
    /// format, when the encoder refuses the surface, or when the write fails
    /// after it has begun.
    pub fn to_file_with(
        &mut self,
        path: impl AsRef<Path>,
        format: ImageFormat,
        options: &EncodeOptions,
    ) -> Result<(), BuildError> {
        let path = path.as_ref();
        // Opened and dropped, not written through: the renderer opens the path
        // itself. This is here to ask the filesystem the question while the
        // answer is still an `io::Error`. See the notes above for why it does
        // not truncate and why it cleans up after itself.
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(file) => {
                drop(file);
                drop(std::fs::remove_file(path));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                drop(
                    std::fs::OpenOptions::new()
                        .write(true)
                        .open(path)
                        .map_err(BuildError::Write)?,
                );
            }
            Err(error) => return Err(BuildError::Write(error)),
        }
        Ok(self.painted.prepare_encode(format, options)?.write(path)?)
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

    /// Every image source that could not be resolved, in node order.
    ///
    /// **Empty is the ordinary answer and always safe to ask for**, so
    /// `warnings().is_empty()` needs no guard. One entry per distinct source
    /// rather than per node.
    ///
    /// Populated whatever [`Root::on_image_error`] chose, including
    /// [`OnImageError::Ignore`]: a caller who finds the mark distracting keeps
    /// the diagnostic, because turning the drawing off must not turn the
    /// knowing off.
    #[must_use]
    pub fn warnings(&self) -> &[ImageWarning] {
        self.painted.warnings()
    }

    /// Whether the GPU was asked for.
    ///
    /// **Asking is not getting** -- compare [`Canvas::engine`]. This is what
    /// [`Root::gpu`] was told, and it is `true` when nothing was said, because
    /// that is the renderer's own default.
    #[must_use]
    pub const fn gpu(&self) -> bool {
        self.painted.gpu()
    }

    /// Which rasteriser drew the pages: `"gpu"` or `"cpu"`.
    ///
    /// The outcome rather than the request, and they disagree: a build with no
    /// GPU backend compiled, a machine with no device, a driver that declines,
    /// and a float `color_type` all rasterise on the CPU whatever `gpu` says.
    ///
    /// **Without it a caller who asks for the GPU and gets the CPU has no way
    /// to find out**, and neither has a test. This crate's own
    /// `the_two_rasterisers_do_not_draw_the_same_pixels` branched on whether a
    /// backend was *compiled in*, which is a different question: a headless
    /// Linux runner compiles Vulkan, finds no device, falls back correctly, and
    /// the test called the correct fallback a failure. It also meant the GPU
    /// path was asserted nowhere on that platform, because it had never run
    /// there.
    ///
    /// The JavaScript surface has carried this since it was written
    /// (`packages/meo-canvas/src/canvas.ts`); this is the Rust half of the same
    /// pair.
    #[must_use]
    pub fn engine(&self) -> &'static str {
        self.painted.engine()
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
        Box, ColorSpace, ColorType, Column, EncodeOptions, Format, Renderer,
        Row, Styled, Text, hex_rgb, px,
    };

    #[test]
    fn a_build_error_hands_back_what_went_wrong_underneath_it() {
        // `impl std::error::Error for BuildError {}` was empty until 5
        // September 2026, so `source()` was `None` while every variant wrapped
        // a real error. `Display` forwarded, so the message looked complete
        // and the chain was not: the `io::ErrorKind` under `Write` is the one
        // thing a caller can branch on, and it was unreachable.
        use std::error::Error as _;

        let refused = Root::new(f32::NAN)
            .children([Box::new()])
            .into_scene()
            .err()
            .unwrap_or_else(|| unreachable!("a NaN width is refused"));
        let source = refused
            .source()
            .unwrap_or_else(|| unreachable!("the scene error is underneath"));
        assert!(
            source
                .downcast_ref::<meo_canvas_scene::SceneError>()
                .is_some(),
            "the cause came back as something other than the scene error"
        );

        // The `io::Error` case, which is the one worth reaching: a caller
        // deciding whether to retry needs the kind, not the sentence.
        let io = BuildError::Write(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "no",
        ));
        let kind = io
            .source()
            .and_then(|error| error.downcast_ref::<std::io::Error>())
            .map(std::io::Error::kind);
        assert_eq!(kind, Some(std::io::ErrorKind::PermissionDenied));
    }

    #[test]
    fn a_written_file_is_the_buffer_and_a_bad_path_keeps_its_error_kind() {
        use std::error::Error as _;

        // Two claims about `to_file_with`, and the second is the reason it is
        // not simply `prepare_encode(..).write(..)`.
        //
        // The first: streaming the bytes into the file writes the same file
        // buffering them did. Byte for byte, because "it wrote a PNG" would
        // pass on either.
        let renderer = Renderer::new();
        let mut canvas = Root::new(16.0)
            .height(8.0)
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let expected = canvas
            .to_buffer(Format::Png)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let path = std::env::temp_dir().join("meo-canvas-root-to-file.png");
        canvas
            .to_file(&path)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let written = std::fs::read(&path)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(written, expected, "the file and the buffer differ");
        std::fs::remove_file(&path)
            .unwrap_or_else(|error| unreachable!("{error}"));

        // The second: a path that cannot be opened is still a `Write` with a
        // real `io::ErrorKind` under it. The renderer folds a write failure
        // into its own encode error, which carries a sentence rather than a
        // kind -- so without the file being created before the encode, this
        // would arrive as `Render` and the kind would be gone. That is the
        // property the `source()` fix of 5 September 2026 exists to give, and
        // this is what stops the streaming change from quietly taking it back.
        let missing = std::env::temp_dir()
            .join("meo-canvas-no-such-directory")
            .join("out.png");
        let refused = canvas.to_file(&missing);
        let Err(error) = refused else {
            unreachable!("writing into a missing directory succeeded");
        };
        assert!(
            matches!(error, BuildError::Write(_)),
            "a missing directory reported {error:?} rather than a write error"
        );
        let kind = error
            .source()
            .and_then(|cause| cause.downcast_ref::<std::io::Error>())
            .map(std::io::Error::kind);
        assert_eq!(kind, Some(std::io::ErrorKind::NotFound));
    }

    #[test]
    fn a_refused_encode_leaves_the_previous_file_alone() {
        // The failure this method's probe must not cause. `File::create`
        // truncates, so probing with it emptied whatever was at the path
        // before attempting an encode that may be refused -- and an encode is
        // refused for ordinary reasons: `fps` on a still format is one, and
        // that is a caller error, not a machine failure. Yesterday's render
        // would be a zero-byte file and the only thing returned would be a
        // message about frame timing.
        let renderer = Renderer::new();
        let mut canvas = Root::new(16.0)
            .height(8.0)
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let path = std::env::temp_dir().join("meo-canvas-not-clobbered.png");
        canvas
            .to_file(&path)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let before = std::fs::read(&path)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(!before.is_empty(), "the first write produced nothing");

        // A frame rate means nothing to a PNG, which `EncodeOptions::validate`
        // refuses before anything is drawn.
        let refused = canvas.to_file_with(
            &path,
            Format::Png,
            &EncodeOptions {
                fps: Some(30.0),
                ..EncodeOptions::default()
            },
        );
        assert!(refused.is_err(), "a frame rate on a PNG was accepted");

        let after = std::fs::read(&path)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(
            after, before,
            "a refused encode changed the file that was already there"
        );
        std::fs::remove_file(&path)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }

    #[test]
    fn a_refused_encode_leaves_no_file_where_there_was_none() {
        // The other half: the probe creates a path that does not exist, so it
        // has to remove it again. Left behind, a refused encode would put a
        // zero-byte file where it used to put nothing at all.
        let renderer = Renderer::new();
        let mut canvas = Root::new(16.0)
            .height(8.0)
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let path = std::env::temp_dir().join("meo-canvas-no-residue.png");
        drop(std::fs::remove_file(&path));

        let refused = canvas.to_file_with(
            &path,
            Format::Png,
            &EncodeOptions {
                fps: Some(30.0),
                ..EncodeOptions::default()
            },
        );
        assert!(refused.is_err(), "a frame rate on a PNG was accepted");
        assert!(
            !path.exists(),
            "a refused encode left a file at a path that had none"
        );
    }

    #[test]
    fn a_root_whose_width_is_not_a_length_is_refused() {
        // `Root::new` is a `const fn` returning `Self`, so it has nowhere to
        // report a width that is not a number of pixels. `into_scene` is the
        // first call that can, and until 5 September 2026 it did not: a root
        // of `NaN` built a scene sized `NaN by 0.0` and rendered nothing.
        for width in [f32::NAN, -100.0, f32::INFINITY] {
            let refused = Root::new(width).children([Box::new()]).into_scene();
            assert!(
                matches!(
                    refused,
                    Err(BuildError::Scene(
                        meo_canvas_scene::SceneError::CanvasSize { .. }
                    ))
                ),
                "a width of {width} was accepted"
            );
        }

        // A height set explicitly reaches the same check.
        let refused = Root::new(10.0)
            .height(-1.0)
            .children([Box::new()])
            .into_scene();
        assert!(matches!(
            refused,
            Err(BuildError::Scene(
                meo_canvas_scene::SceneError::CanvasSize { .. }
            ))
        ));

        // And zero stays a canvas, as it is on the element surface.
        assert!(Root::new(0.0).children([Box::new()]).into_scene().is_ok());
    }

    #[test]
    fn to_file_accepts_the_extensions_the_javascript_surface_does() {
        // `.raw` among them. `ImageFormat::from_extension` refuses it, and
        // rightly for a filename found on disk — upstream calls that container
        // `.bin` and a `.bin` of pixel bytes implies no format. A path the
        // caller has just typed is the other question, and this once answered
        // it more narrowly than `toFile` did.
        let dir = std::env::temp_dir()
            .join(format!("meo-to-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let mut canvas = Root::new(8.0)
            .height(4.0)
            .children(Box::new())
            .render(&Renderer::new())
            .unwrap_or_else(|error| unreachable!("{error}"));

        for extension in ["png", "raw", "RAW"] {
            let path = dir.join(format!("out.{extension}"));
            assert!(
                canvas.to_file(&path).is_ok(),
                ".{extension} should name a format"
            );
        }
        // Eight by four at four bytes a pixel, so the bytes are the pixels and
        // nothing else -- which is the whole of what `raw` promises.
        assert_eq!(
            std::fs::metadata(dir.join("out.raw"))
                .unwrap_or_else(|error| unreachable!("{error}"))
                .len(),
            8 * 4 * 4
        );

        // `.bin` is upstream's name for the container and not a format tag, so
        // neither surface takes it. Refusing it here is the parity, not a gap.
        for extension in ["bin", "nonsense"] {
            let path = dir.join(format!("out.{extension}"));
            assert!(
                matches!(canvas.to_file(&path), Err(BuildError::Format(_))),
                ".{extension} names no format"
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The scene `root` describes, or the failure it reports.
    fn scene_of(root: Root) -> meo_canvas_scene::Scene {
        root.into_scene()
            .unwrap_or_else(|error| unreachable!("{error}"))
    }

    #[test]
    fn a_root_takes_its_size_and_defaults_the_scale() {
        let scene = scene_of(Root::new(120.0).height(60.0));

        assert_eq!(scene.size.width.to_bits(), 120.0_f32.to_bits());
        assert_eq!(scene.size.height.to_bits(), 60.0_f32.to_bits());
        assert_eq!(scene.scale.to_bits(), Root::DEFAULT_SCALE.to_bits());
    }

    #[test]
    fn the_scale_reaches_the_scene_without_moving_the_layout() {
        // Layout always solves at one, so this changes resolution and nothing
        // about where things sit.
        let scene = scene_of(Root::new(10.0).height(10.0).scale(3.0));

        assert_eq!(scene.scale.to_bits(), 3.0_f32.to_bits());
    }

    #[test]
    fn the_root_is_the_page_rather_than_a_box_inside_it() {
        // `background_color` on `Root` paints the canvas. A wrapper node would
        // paint a box the size of the content instead, which is the bug this
        // shape exists to make impossible.
        let scene = scene_of(
            Root::new(10.0)
                .height(10.0)
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
        let one = scene_of(Root::new(10.0).height(10.0).children(Row::new()));
        let many = scene_of(
            Root::new(10.0)
                .height(10.0)
                .children([Row::new(), Column::new()]),
        );
        let conditional = scene_of(Root::new(10.0).height(10.0).children([
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
        let scene =
            scene_of(Root::new(10.0).height(10.0).children(
                Row::new().gap(px(4.0)).padding(crate::all(px(8.0))),
            ));
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
            Root::new(10.0)
                .height(10.0)
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
        let scene = scene_of(Root::new(10.0).height(10.0).children(Row::new()));
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
        let silent = scene_of(Root::new(10.0).height(10.0));
        let stated = scene_of(
            Root::new(10.0)
                .height(10.0)
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
        // The check a fake cannot satisfy. An assertion that a flag was copied
        // from one object to another stays true when nothing on the far side
        // reads it; two real renders that must differ do not. If this build has
        // no GPU compiled in, both are the CPU and the test says so rather than
        // passing vacuously.
        //
        // The content is load-bearing: the two rasterisers differ on
        // anti-aliased edges and agree exactly on a picture without any, so
        // text is what makes them disagree. A plain filled box here would fail
        // this rather than pass it quietly, which is the right way round, but
        // worth knowing before changing the scene.
        let renderer = Renderer::new();
        let rounded = || {
            Box::new()
                .size(px(120.0), px(60.0))
                .border_radius(24.0)
                .background_color(hex_rgb(0xff_ff_ff))
        };

        let mut on = Root::new(200.0)
            .height(80.0)
            .gpu(true)
            .children(rounded())
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut off = Root::new(200.0)
            .height(80.0)
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

        // **What actually happened, not what was compiled in.**
        //
        // This branched on `cfg!(any(feature = "metal", feature = "vulkan"))`,
        // and that is a different question. A feature says a backend was built;
        // it says nothing about a *device*. A headless Linux runner compiles
        // Vulkan, finds no device, correctly falls back to the CPU -- and the
        // two renders then agree to the byte, which the old assertion called a
        // failure. It passed on macOS only because a Metal device is always
        // there.
        //
        // The other direction is worse and quieter: **on Linux this test had
        // never once verified that the GPU path differs**, because the GPU path
        // had never run there. It is still not verified there, and now it says
        // so rather than pretending otherwise -- the `else` arm below is
        // reached on any machine without a device, and asserts the fallback was
        // clean rather than asserting anything about the GPU.
        // The one thing a feature flag *can* say, kept as an assertion rather
        // than a comment: with no backend compiled there is nothing to fall
        // back from, so the outcome is not in doubt. It is the direction the
        // old test had right, and it costs nothing to keep.
        if !cfg!(any(feature = "metal", feature = "vulkan")) {
            assert_eq!(
                on.engine(),
                "cpu",
                "no GPU backend is compiled in, so no render can have used one"
            );
        }
        if on.engine() == "gpu" {
            assert_ne!(
                with, without,
                "a GPU device drew one of these, so the two rasterisers should not agree to the byte"
            );
        } else {
            assert_eq!(
                with, without,
                "no GPU device was used, so both renders are the CPU's and must match"
            );
        }
    }

    #[test]
    fn a_page_builder_runs_once_per_page() {
        let scene =
            scene_of(Root::new(10.0).height(10.0).pages(3).page_builder(
                |page| Text::new(format!("page {}", page.index)),
            ));

        assert_eq!(scene.pages.len(), 3);
    }

    #[test]
    fn a_duration_becomes_a_page_count_at_the_rate_given() {
        // `ceil(duration * fps)`, as v1 derives it: a fraction of a page is
        // still a page that has to be drawn.
        let scene = scene_of(
            Root::new(10.0)
                .height(10.0)
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
        let both = Root::new(10.0)
            .height(10.0)
            .pages(2)
            .duration(1.0)
            .page_builder(|_| Row::new())
            .into_scene();
        let builder_alone = Root::new(10.0)
            .height(10.0)
            .page_builder(|_| Row::new())
            .into_scene();
        let length_alone = Root::new(10.0)
            .height(10.0)
            .pages(2)
            .children(Row::new())
            .into_scene();
        let none = Root::new(10.0)
            .height(10.0)
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
        let mut canvas = Root::new(8.0)
            .height(4.0)
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
    fn a_filename_names_the_format_it_is_written_in() {
        // The same call a JavaScript caller writes,
        // `canvas.to_file("out.png")`. An extension naming no format is
        // refused rather than defaulted: a typo would otherwise produce
        // a file whose name lies about its contents.
        let renderer = Renderer::new();
        let mut canvas = Root::new(4.0)
            .height(4.0)
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let directory = std::env::temp_dir().join("meo-canvas-to-file");
        std::fs::create_dir_all(&directory)
            .unwrap_or_else(|error| unreachable!("{error}"));

        canvas
            .to_file(directory.join("out.png"))
            .unwrap_or_else(|error| unreachable!("{error}"));
        let written = std::fs::read(directory.join("out.png"))
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(&written[..8], b"\x89PNG\r\n\x1a\n");
        assert!(matches!(
            canvas.to_file(directory.join("out.nonsense")),
            Err(BuildError::Format(_))
        ));
        // `raw` is not inferable -- a `.bin` of pixel bytes is a file nothing
        // reads back -- so it is named rather than guessed.
        assert!(matches!(
            canvas.to_file(directory.join("out.bin")),
            Err(BuildError::Format(_))
        ));
        canvas
            .to_file_as(directory.join("out.bin"), Format::Raw)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }

    #[test]
    fn a_data_url_carries_the_format_and_the_bytes() {
        let renderer = Renderer::new();
        let mut canvas = Root::new(4.0)
            .height(4.0)
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
            scene_of(Root::new(10.0).height(10.0).children(
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
