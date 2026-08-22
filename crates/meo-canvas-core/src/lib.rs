//! The pipeline: a [`Scene`] in, encoded bytes out.
//!
//! Five passes, each its own module, each with an input and an output another
//! pass could substitute for:
//!
//! 1. [`resolve`] turns external references into bytes the renderer holds.
//! 2. [`measure`] answers taffy's questions about how big text and images are.
//! 3. [`layout`] runs taffy and produces an absolute rectangle per node.
//! 4. [`paint`] walks that result and issues draws.
//! 5. [`encode`] turns the finished surface into a file format.
//!
//! Separate passes rather than one recursive draw because measurement has to
//! finish before placement can start, and placement has to finish before
//! painting can start. Interleaving them is what forces the two-phase layout
//! hacks that a single-pass renderer accumulates.
//!
//! # What this crate deliberately excludes
//!
//! No network and no async runtime. [`resolve`] takes bytes the caller already
//! has and reads local paths; a [`meo_canvas_scene::node::ImageSource::Url`]
//! reaching this crate is a [`Error::UnresolvedSource`], not a fetch. A runtime
//! here is a runtime in every consumer, including the ones already running
//! inside one, and neither the CLI nor a Rust caller with no executor can be
//! asked to host it. The CLI fetches behind its own `net` feature.
//!
//! No JavaScript. Nothing here names a neon type. The addon crate converts at
//! its own boundary, which is what lets this crate be tested without building a
//! `.node` binary.
//!
//! No public Skia types. `meo-skia-canvas` is an implementation detail of
//! [`paint`] and [`encode`]; a signature here that returned one would put a
//! Skia build in the path of anyone who only wanted to lay out.
//!
//! # Threading
//!
//! [`layout`] owns its taffy tree and never lets it escape, because
//! `taffy::TaffyTree` is `!Send` and `!Sync` on every supported target --
//! `CompactLengthInner` stores each length as a tagged `*const ()`
//! (`taffy-0.13.0/src/style/compact_length.rs:62`), which poisons the auto
//! traits for `Style` and everything holding one. A [`Scene`] is `Send`, so
//! parallelism happens between renders: each render builds, uses and drops its
//! own tree on one thread.

// `unreachable_pub` is a workspace lint, and `clippy::redundant_pub_crate` is
// its opposite: one asks for `pub(crate)` on an item a private module exports,
// the other calls that redundant. The workspace chose `unreachable_pub`, so the
// clippy half is off here rather than the visibility being written twice.
#![allow(
    clippy::redundant_pub_crate,
    reason = "contradicts the workspace's unreachable_pub"
)]

pub mod encode;
pub mod layout;
pub mod measure;
pub mod paint;
pub mod resolve;

pub use encode::{EncodeOptions, EncodedImage, ImageFormat};
pub use layout::LayoutResult;
pub use measure::{Available, Measure, MeasuredLeaf};
use meo_canvas_scene::{Scene, node::NodeId};
pub use paint::Surface;
pub use resolve::{Fonts, Resolved};

use crate::measure::SceneMeasurer;

/// Anything that can stop a render.
// `thiserror::Error` is named in full rather than imported: an imported `Error`
// makes every `[`Error`]` doc link in this file ambiguous between the derive
// macro and the enum below, which `rustdoc::all = "deny"` rejects.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A node names a source the renderer cannot obtain by itself.
    #[error("node {} names a source this crate does not fetch", .0.get())]
    UnresolvedSource(NodeId),

    /// The scene is not the forest of pages the contract requires.
    ///
    /// [`meo_canvas_scene::codec::decode`] checks this, so a scene read from
    /// bytes cannot fail here; a scene a Rust caller assembled by writing
    /// `Scene::nodes` directly can.
    #[error("the scene cannot be drawn: {0}")]
    Scene(#[source] meo_canvas_scene::SceneError),

    /// A font file that cannot be read or parsed.
    #[error("cannot register font family {family:?}: {detail}")]
    FontRegister {
        /// The family name the caller asked for.
        family: String,
        /// What the font backend reported.
        detail: String,
    },

    /// A local image path that cannot be read.
    #[error("cannot read image at {path}")]
    ImageRead {
        /// The path as the scene spelled it.
        path: String,
        /// The underlying filesystem failure.
        #[source]
        source: std::io::Error,
    },

    /// Bytes that no decoder recognises.
    #[error("image bytes for node {} are in no format this decodes", .0.get())]
    UndecodableImage(NodeId),

    /// A font family the renderer's library does not hold.
    #[error("font family {0:?} is not registered")]
    UnknownFont(String),

    /// taffy refused the tree it was handed.
    #[error("layout failed: {0}")]
    Layout(String),

    /// The surface could not be created or drawn onto.
    #[error("paint failed: {0}")]
    Paint(String),

    /// The finished surface could not be written in the requested format.
    #[error("encoding to {format} failed: {detail}")]
    Encode {
        /// The format that was asked for.
        format: ImageFormat,
        /// What the encoder reported.
        detail: String,
    },
}

/// Everything a render needs that is not the scene itself.
///
/// Separate from [`Scene`] because these outlive any one scene: a server
/// rendering a thousand pictures registers its fonts once.
#[derive(Debug)]
pub struct Renderer {
    fonts: Fonts,
    gpu: bool,
}

impl Default for Renderer {
    fn default() -> Self {
        Self {
            fonts: Fonts::new(),
            gpu: Self::DEFAULT_GPU,
        }
    }
}

impl Renderer {
    /// Whether a renderer asks for the GPU when nothing says otherwise.
    ///
    /// True, matching v1 -- its `RootProps` carries `gpu` and
    /// `meo-skia-canvas` defaults it on, so a scene ported from v1 behaves the
    /// same without the caller restating it.
    ///
    /// It is a request rather than an outcome. `Canvas::gpu`'s own
    /// documentation calls it "the request, not the outcome": a build with no
    /// GPU backend compiled rasterises on the CPU whatever this says. That is
    /// why it is worth stating -- a project that never sets it is relying on
    /// which features happened to be compiled, which is not a decision anyone
    /// wrote down.
    pub const DEFAULT_GPU: bool = true;

    /// Creates a renderer with no fonts registered beyond the platform's.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether renders from here ask for the GPU.
    #[must_use]
    pub const fn gpu(&self) -> bool {
        self.gpu
    }

    /// Chooses whether renders from here ask for the GPU.
    ///
    /// A property of the renderer rather than of the scene or the encode: two
    /// renders of one scene, one on the GPU and one on the CPU, are meant to
    /// produce the same picture, so this describes the environment a render
    /// happens in and not the picture it draws. A server picks once.
    ///
    /// The golden-fixture harness turns it off, which is what makes the gate
    /// rest on a decision rather than on which backend a build happened to
    /// compile.
    pub const fn set_gpu(&mut self, gpu: bool) {
        self.gpu = gpu;
    }

    /// Registers a font file under a family name of the caller's choosing.
    ///
    /// Takes `&mut self` although the registry beneath it does not need it.
    /// Registration changes what every later render draws, and a `&self` that
    /// mutates behind a lock is how two threads begin contending on a type
    /// whose whole purpose is that they do not.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FontRegister`] if the file cannot be read or its bytes
    /// are not a font this build can parse.
    pub fn register_font(
        &mut self,
        family: &str,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(), Error> {
        self.fonts.register_path(family, path)
    }

    /// The fonts this renderer draws with.
    #[must_use]
    pub const fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    /// Runs every pass over every page and hands back the painted surface.
    ///
    /// Stops short of encoding, because encoding is not part of drawing. Two
    /// formats of one picture are two encodes of one surface, not two renders:
    /// `render` then two [`RenderedCanvas::to_buffer`] calls costs one resolve,
    /// one shaping pass, one layout per page and one paint. Folding the encode
    /// in would make the second format cost all of that again -- which is the
    /// shape the JavaScript surface already avoids with `toBuffer`, and the two
    /// surfaces are siblings rather than one wrapping the other.
    ///
    /// Resolving and shaping happen once for the whole scene; layout and paint
    /// run per page. That split is why the arena is one list: the caches are
    /// keyed by `NodeId` alone, with no page beside it.
    ///
    /// Takes `&self`: the measurer and its paragraph cache are built per render
    /// and dropped with it, so nothing here outlives the call.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Scene`] if the scene is not a well-formed forest of
    /// pages, and otherwise [`Error`] from whichever pass fails first.
    pub fn render(&self, scene: &Scene) -> Result<RenderedCanvas, Error> {
        // Checked before anything is allocated. Without it a scene with no
        // pages renders the blank sheet `Surface::new` created and reports
        // success, which is a picture the caller never described.
        scene.validate().map_err(Error::Scene)?;

        let resolved = Resolved::new(scene, &self.fonts)?;
        let mut measurer = SceneMeasurer::prepare(&resolved, &self.fonts)?;
        let mut surface = Surface::new(scene.size, scene.scale, self.gpu)?;

        for (index, &page) in scene.pages.iter().enumerate() {
            // The first page is the one `Surface::new` created; beginning a
            // page for it would leave a blank sheet ahead of the drawing.
            if index > 0 {
                surface.begin_page(scene.size)?;
            }
            let solved = layout::solve(scene, page, &mut measurer)?;
            paint::draw(&mut surface, &resolved, &solved, &mut measurer)?;
        }

        Ok(RenderedCanvas { surface })
    }

    /// Renders a scene and encodes it once.
    ///
    /// The one-format case, which is most of them: the CLI writes one file and
    /// a fixture compares one image. It earns its place because the split
    /// otherwise costs every such caller a `let mut` and a second line to say
    /// something they never wanted to say twice — and because a caller who
    /// only ever wants one format should not have to hold a canvas to get it.
    ///
    /// # Errors
    ///
    /// As [`Renderer::render`], plus whatever the encode reports.
    pub fn render_to_buffer(
        &self,
        scene: &Scene,
        format: ImageFormat,
        options: &EncodeOptions,
    ) -> Result<Vec<u8>, Error> {
        self.render(scene)?.to_buffer(format, options)
    }
}

/// A scene that has been drawn and not yet encoded.
///
/// Holds every page of the painted surface, so encoding it again in another
/// format re-reads pixels rather than redrawing them.
#[derive(Debug)]
pub struct RenderedCanvas {
    surface: Surface,
}

impl RenderedCanvas {
    /// Encodes the painted pages in one format.
    ///
    /// Takes `&mut self` because encoding mutates: every encode entry point
    /// upstream is `&mut self` since `Canvas::to_buffer` prepares the surface
    /// before reading it (`meo-skia-canvas-0.11.0/src/canvas.rs:551`). That is
    /// not a detail to hide behind interior mutability — a `RefCell` here would
    /// let two encodes of one canvas read as independent when they are not.
    /// `&mut` says encoding consumes preparation, which is true.
    ///
    /// Calling it twice is the point: two formats of one picture cost one
    /// render.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Encode`] when the encoder refuses the surface, and
    /// whatever [`EncodeOptions`] validation reports — the page count it
    /// validates against is a property of the painted surface rather than of
    /// the scene, which is why the check lives here and not in
    /// [`Renderer::render`].
    pub fn to_buffer(
        &mut self,
        format: ImageFormat,
        options: &EncodeOptions,
    ) -> Result<Vec<u8>, Error> {
        encode::encode(&mut self.surface, format, options)
            .map(|image| image.bytes)
    }

    /// How many pages were painted.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.surface.page_count()
    }

    /// The device-pixel multiplier the pages were drawn at.
    #[must_use]
    pub const fn scale(&self) -> f32 {
        self.surface.scale()
    }
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::{
        Scene, Size,
        node::{Node, NodeId},
    };

    use super::{EncodeOptions, Error, ImageFormat, Renderer};
    use crate::resolve::tests::{TEST_FAMILY, TEST_FONT};

    /// A scene of `pages` pages, each carrying one text node.
    ///
    /// Text on every page rather than only the first, so a font resolved per
    /// page rather than per scene would still succeed and the tests that care
    /// about that distinction have to say so another way.
    fn paged_scene(pages: usize, size: Size) -> Scene {
        let mut scene = Scene::new(size);
        for index in 0..pages {
            let root = if index == 0 {
                NodeId::ROOT
            } else {
                scene
                    .push_page()
                    .unwrap_or_else(|error| unreachable!("{error}"))
            };
            let leaf = scene
                .push(root, Node::text(format!("page {index}")))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(leaf) {
                node.text.font_family = Some(TEST_FAMILY.to_owned());
                node.text.font_size = Some(12.0);
            }
        }
        scene
    }

    fn renderer() -> Renderer {
        let mut renderer = Renderer::new();
        // Off, matching the fixture harness: a gate that rests on which
        // backend a build happened to compile rests on nothing written down.
        renderer.set_gpu(false);
        renderer
            .register_font(TEST_FAMILY, TEST_FONT)
            .unwrap_or_else(|error| unreachable!("{error}"));
        renderer
    }

    #[test]
    fn a_scene_renders_to_a_decodable_image_at_its_scale() {
        let mut scene = paged_scene(1, Size::new(40.0, 20.0));
        scene.scale = 2.0;

        let image = renderer()
            .render_to_buffer(
                &scene,
                ImageFormat::Png,
                &EncodeOptions::default(),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(&image[..4], b"\x89PNG");

        // Decoded rather than trusted: the scale is applied at paint time, so
        // a surface built at the logical size would still produce valid PNG
        // bytes and only the pixel count would betray it.
        let decoded = meo_skia_canvas::Image::from_encoded(&image)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!((decoded.width(), decoded.height()), (80, 40));
    }

    /// Every page reaches the encoder, and exactly once.
    ///
    /// This is the ordering proof for the render loop. A GIF is
    /// `PageUse::All`, so the frame count is the page count: two frames would
    /// mean `begin_page` was skipped for a page, four would mean it ran for
    /// the first page as well as the later ones, and any count at all proves
    /// the surface reached `encode` carrying every page rather than one.
    #[test]
    fn every_page_reaches_the_encoder_exactly_once() {
        const PAGES: usize = 3;

        let scene = paged_scene(PAGES, Size::new(24.0, 16.0));
        assert_eq!(scene.pages.len(), PAGES);

        let image = renderer()
            .render_to_buffer(
                &scene,
                ImageFormat::Gif,
                &EncodeOptions::default(),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        let decoded = meo_skia_canvas::Image::from_encoded(&image)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(decoded.frame_count(), PAGES);
    }

    /// A still format takes one page from a multi-page scene rather than
    /// refusing it.
    #[test]
    fn a_still_format_writes_one_page_of_a_multi_page_scene() {
        let scene = paged_scene(3, Size::new(24.0, 16.0));
        let image = renderer()
            .render_to_buffer(
                &scene,
                ImageFormat::Png,
                &EncodeOptions::default(),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        let decoded = meo_skia_canvas::Image::from_encoded(&image)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(decoded.frame_count(), 1);
    }

    /// A font missing on a *later* page fails the render before anything is
    /// drawn.
    ///
    /// This is what says resolving happens once for the whole scene rather
    /// than per page: were it per page, page one would draw and the failure
    /// would arrive with a partly-painted surface.
    #[test]
    fn a_font_missing_on_a_later_page_fails_the_whole_render() {
        let mut scene = paged_scene(2, Size::new(20.0, 20.0));
        let second_page = scene.pages[1];
        let leaf = scene
            .get(second_page)
            .and_then(|page| page.children.first().copied())
            .unwrap_or_else(|| unreachable!("each page carries a text node"));
        if let Some(node) = scene.get_mut(leaf) {
            node.text.font_family = Some("NoSuchFamilyAnywhere".to_owned());
        }

        let Err(error) = renderer().render_to_buffer(
            &scene,
            ImageFormat::Png,
            &EncodeOptions::default(),
        ) else {
            unreachable!("an unregistered family fails the render")
        };
        assert!(
            matches!(&error, Error::UnknownFont(family)
                if family == "NoSuchFamilyAnywhere"),
            "expected the missing family to be named, found {error}"
        );
    }

    /// The options are read per call, not held on the renderer.
    #[test]
    fn encode_options_reach_the_encoder() {
        let scene = paged_scene(1, Size::new(64.0, 64.0));
        let renderer = renderer();

        let coarse = renderer
            .render_to_buffer(
                &scene,
                ImageFormat::Jpeg,
                &EncodeOptions {
                    quality: Some(0.1),
                    ..EncodeOptions::default()
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        let fine = renderer
            .render_to_buffer(
                &scene,
                ImageFormat::Jpeg,
                &EncodeOptions {
                    quality: Some(1.0),
                    ..EncodeOptions::default()
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        // One renderer, two calls, two different results: the quality is a
        // property of the encode rather than of the renderer.
        assert!(
            fine.len() > coarse.len(),
            "quality 1.0 produced {} bytes against {} at 0.1",
            fine.len(),
            coarse.len()
        );
    }

    /// The GPU is the renderer's decision, defaulting to v1's.
    #[test]
    fn the_gpu_is_a_renderer_property_with_v1_s_default() {
        let mut cpu_renderer = renderer();
        cpu_renderer.set_gpu(true);
        assert!(cpu_renderer.gpu(), "v1 defaults the GPU on");
        assert_eq!(Renderer::new().gpu(), Renderer::DEFAULT_GPU);

        cpu_renderer.set_gpu(false);
        assert!(!cpu_renderer.gpu());

        // Both settings render the same scene; the choice describes the
        // environment, not the picture. Bit-exact rather than merely both
        // succeeding, which is the property the fixture gate depends on.
        let scene = paged_scene(1, Size::new(24.0, 16.0));
        let cpu = cpu_renderer
            .render_to_buffer(
                &scene,
                ImageFormat::Png,
                &EncodeOptions::default(),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        let mut asking = renderer();
        asking.set_gpu(true);
        let requested = asking
            .render_to_buffer(
                &scene,
                ImageFormat::Png,
                &EncodeOptions::default(),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(cpu, requested, "asking for the GPU changed the picture");
    }

    #[test]
    fn a_renderer_reports_the_fonts_it_holds() {
        let renderer = renderer();
        assert!(renderer.fonts().has(TEST_FAMILY));
        assert_eq!(renderer.fonts().registered(), vec![TEST_FAMILY.to_owned()]);
        assert!(!format!("{renderer:?}").is_empty());

        let mut empty = Renderer::new();
        assert!(empty.fonts().registered().is_empty());
        assert!(empty.register_font("Broken", "/no/such/font.ttf").is_err());
    }

    #[test]
    fn a_scene_with_no_pages_is_refused_before_a_surface_is_made() {
        let scene = Scene {
            pages: Vec::new(),
            ..Scene::new(Size::new(10.0, 10.0))
        };
        let Err(error) = renderer().render_to_buffer(
            &scene,
            ImageFormat::Png,
            &EncodeOptions::default(),
        ) else {
            unreachable!("a scene with no pages draws nothing")
        };
        assert!(
            matches!(
                error,
                Error::Scene(meo_canvas_scene::SceneError::NoPages)
            ),
            "expected the scene to be refused as unbuildable"
        );
    }

    /// A dangling child is refused for the same reason, through the same
    /// check.
    #[test]
    fn a_scene_that_is_not_a_forest_is_refused() {
        let mut scene = paged_scene(1, Size::new(10.0, 10.0));
        let dangling = NodeId::new(99);
        scene.nodes[0].children.push(dangling);

        let Err(error) = renderer().render_to_buffer(
            &scene,
            ImageFormat::Png,
            &EncodeOptions::default(),
        ) else {
            unreachable!("a dangling child is not a drawable tree")
        };
        assert!(
            matches!(
                error,
                Error::Scene(meo_canvas_scene::SceneError::UnknownNode(id))
                    if id == dangling
            ),
            "expected the dangling node to be named, found {error}"
        );
    }
    /// Two formats of one picture cost one render.
    ///
    /// The property the split exists for. Asserted through the output rather
    /// than by counting passes: the second encode must produce a real image of
    /// the same surface, and the PNG must be byte-identical to what a fresh
    /// single-format render produces — so re-encoding is not a different
    /// drawing that happens to look similar.
    #[test]
    fn one_render_encodes_to_several_formats() {
        let scene = paged_scene(1, Size::new(32.0, 24.0));
        let renderer = renderer();

        let mut canvas = renderer
            .render(&scene)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(canvas.page_count(), 1);
        assert!((canvas.scale() - scene.scale).abs() < f32::EPSILON);

        let png = canvas
            .to_buffer(ImageFormat::Png, &EncodeOptions::default())
            .unwrap_or_else(|error| unreachable!("{error}"));
        let jpeg = canvas
            .to_buffer(ImageFormat::Jpeg, &EncodeOptions::default())
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(&png[..4], b"\x89PNG");
        assert_eq!(&jpeg[..2], b"\xFF\xD8");

        // The same surface, so the same picture as rendering once for PNG.
        let once = renderer
            .render_to_buffer(
                &scene,
                ImageFormat::Png,
                &EncodeOptions::default(),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(png, once, "re-encoding drew a different picture");

        // And encoding the same format twice is idempotent, which says the
        // first encode did not consume the pixels.
        let again = canvas
            .to_buffer(ImageFormat::Png, &EncodeOptions::default())
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(png, again);
    }

    /// Option validation happens where the page count lives.
    ///
    /// A page index past the end is a property of the painted surface, not of
    /// the scene, so `render` cannot catch it and `to_buffer` must.
    #[test]
    fn encode_options_are_validated_against_the_painted_pages() {
        let scene = paged_scene(2, Size::new(16.0, 16.0));
        let mut canvas = renderer()
            .render(&scene)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(canvas.page_count(), 2);

        let past_the_end = EncodeOptions {
            page: Some(9),
            ..EncodeOptions::default()
        };
        assert!(
            canvas.to_buffer(ImageFormat::Png, &past_the_end).is_err(),
            "a page index past the end should be refused"
        );

        // And the canvas is still usable afterwards: a refused encode is not a
        // consumed one.
        assert!(
            canvas
                .to_buffer(ImageFormat::Png, &EncodeOptions::default())
                .is_ok()
        );
        assert!(!format!("{canvas:?}").is_empty());
    }
}
