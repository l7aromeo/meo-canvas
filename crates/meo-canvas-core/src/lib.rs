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
//! **With `net` on, this crate does fetch, and the policy is fixed.** Five
//! seconds to connect, sixty seconds for the whole fetch, and thirty-two
//! mebibytes per image -- numbers rather than adjectives, because someone
//! planning a deployment needs them and a version bump is where they would
//! change. The size limit is functional: an image past it does not render and
//! the caller gets [`FetchFailure::TooLarge`]. A caller wanting their own
//! policy fetches the bytes themselves and passes `ImageSource::Bytes`, which
//! is the same escape the TypeScript surface has. `resolve`'s `fetch` carries
//! the derivation, including why the timeout and the size cap are one decision
//! rather than two.
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

// Nothing in this workspace writes `unsafe`, and this keeps it so: in a crate
// reaching C++ through two binding layers one would look reasonable.
// Integration tests are separate crates; the only `unsafe` in the repository is
// the allocator in `meo-canvas-scene/tests/codec_reservation.rs`.
#![forbid(unsafe_code)]
// `unreachable_pub` is a workspace lint, and `clippy::redundant_pub_crate` is
// its opposite: one asks for `pub(crate)` on an item a private module exports,
// the other calls that redundant. The workspace chose `unreachable_pub`, so the
// clippy half is off here rather than the visibility being written twice.
#![allow(
    clippy::redundant_pub_crate,
    reason = "contradicts the workspace's unreachable_pub"
)]

pub mod animate;
pub mod color;
pub mod diagnostic;
pub mod encode;
pub mod layout;
pub mod lines;
pub mod markup;
pub mod measure;
pub mod paint;
pub mod resolve;

/// The largest magnitude a clamped length or flex factor may reach, measured in
/// this engine: a `width` sweep collapses between `1e9` and `1e10`, and this
/// sits three orders below. Chrome's `2^25` layout-unit ceiling agreeing is
/// chance, so re-run the sweep rather than align to it.
pub(crate) const FINITE_CEILING: f32 = 3.355_443_2e7;

// `f32::MAX` would put back the collapse these clamps remove, since one sum
// past it is infinity. 2048 stands for a line, or a row of siblings, longer
// than any this engine lays out. Asserted, so a wrong value does not compile.
const _: () = assert!(FINITE_CEILING.is_finite());
const _: () = assert!(FINITE_CEILING * 2048.0 < f32::MAX);

pub use color::parse_color;
pub use encode::{EncodeOptions, EncodedImage, ImageFormat, PreparedEncode};
pub use layout::LayoutResult;
pub use measure::{Available, Measure, MeasuredLeaf};
use meo_canvas_scene::{Scene, Size, node::NodeId, style::Dimension};
pub use paint::{Surface, SurfaceOptions};
pub use resolve::{Fonts, Resolved};

use crate::measure::SceneMeasurer;

/// One image source that could not be resolved, and what a render did about it.
///
/// **A render that finishes with warnings is not a render that succeeded
/// quietly.** Every entry here is a picture the caller asked for and did not
/// get; the render completed because the alternative -- failing the whole page
/// for one decorative icon -- is worse for a server drawing cards from
/// somebody else's payload, not because the failure stopped mattering.
///
/// Recorded whatever [`OnImageError`](meo_canvas_scene::OnImageError) says,
/// including [`Ignore`](meo_canvas_scene::OnImageError::Ignore). Turning the
/// drawing off never turns the knowing off.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageWarning {
    /// The URL that could not be resolved.
    pub url: String,
    /// The first node that asked for it, in node order.
    ///
    /// One warning per distinct source rather than per node: sixty nodes
    /// drawing one dead URL is one thing that went wrong, and a caller
    /// deduplicating a list we could have deduplicated is a worse surface.
    pub node: NodeId,
    /// What went wrong, for a program to branch on.
    ///
    /// The same classification a hard failure carries in
    /// [`Error::SourceFetch`], so the two paths describe one event and a
    /// caller moving between `throw` and `placeholder` reads the same reasons.
    pub failure: FetchFailure,
    /// What the HTTP client reported, for a person to read.
    pub detail: String,
    /// How many nodes in this scene named this source.
    ///
    /// **Deduplicated by URL, with the count beside it**, because one dead URL
    /// asked for three times is one thing that went wrong and three identical
    /// entries is noise. The URL is what locates a defect -- it is the only
    /// thing that separates "the art is not uploaded yet" from "this path has
    /// never been right" -- and the count says how much of the page it cost.
    pub nodes: usize,
}

impl From<meo_canvas_scene::ImageFetchFailure> for FetchFailure {
    /// **Matched exhaustively on purpose.** A variant added to either list
    /// fails this build rather than a test, which is the same guarantee
    /// `ColorType`'s two-sided mapping gives.
    ///
    /// Total, and it has no reason not to be: both sides carry the HTTP status
    /// inside `Status`, so every variant maps to exactly one variant and
    /// nothing is fabricated.
    fn from(failure: meo_canvas_scene::ImageFetchFailure) -> Self {
        use meo_canvas_scene::ImageFetchFailure as Wire;
        match failure {
            // The status travels inside the variant on both sides, so this is
            // a rename rather than a reconstruction -- there is no absent-code
            // case to invent a number for.
            Wire::Status(code) => Self::Status(code),
            Wire::HostNotFound => Self::HostNotFound,
            Wire::BadUrl => Self::BadUrl,
            Wire::Transport => Self::Transport,
            Wire::TooLarge => Self::TooLarge,
            Wire::Other => Self::Other,
        }
    }
}

/// Why a fetch failed, in the terms a caller can act on.
///
/// **The classification and not the sentence.** [`Error::SourceFetch`] carries
/// both: `detail` is what the HTTP client said, which crosses the addon
/// boundary into a JavaScript exception message and is what a person reads;
/// this is what a program branches on. A caller deciding whether to try again
/// should not have to parse prose to do it.
///
/// Every variant says what to do with it, because a classification whose
/// meaning is not written down only moves the guesswork.
///
/// **There is no `Timeout` variant.** This crate's sixty-second global timeout
/// does fire -- `fetch_policy.rs` records it at 60.1 seconds -- and arrives as
/// `Transport`, whose advice, retry, is the right advice for one.
/// `#[non_exhaustive]` is what lets a `Timeout` variant arrive without a
/// breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FetchFailure {
    /// The server answered, with a status that is not a success.
    ///
    /// **Retry a 5xx, do not retry a 4xx**: the first says the server could
    /// not answer now, the second says it will not answer this request.
    Status(u16),
    /// The host's name does not resolve: the lookup answered that there is no
    /// such name, or no address for it.
    ///
    /// **Do not retry; fix the URL, or the network it is being resolved on.**
    /// A lookup that could not finish -- the resolver unreachable, or saying
    /// to try again -- is [`FetchFailure::Transport`] instead, because that
    /// one may well answer on a retry.
    HostNotFound,
    /// The URL is not one this client can use -- no scheme, or no host.
    ///
    /// **Do not retry; fix the URL.** Nothing about the network will change
    /// this.
    BadUrl,
    /// The connection failed, failed partway through, or the host's name
    /// could not be looked up at all.
    ///
    /// **Retry.** This is the class where trying again is the right first
    /// move: a refused connection, a socket that dropped, a read that stopped,
    /// a resolver that was unreachable or said to try again, and this crate's
    /// own timeout.
    Transport,
    /// The image is larger than this renderer fetches.
    ///
    /// **Do not retry; the asset has to change, or be fetched by the caller.**
    /// Distinct from [`FetchFailure::Transport`] because the two want opposite
    /// responses: "too big" and "too slow" must not read the same to a caller
    /// deciding whether to try again.
    ///
    /// The limit is this crate's rather than the client's, which is what makes
    /// the case knowable at all -- see `resolve`'s `MAX_IMAGE_BYTES`.
    TooLarge,
    /// Something else went wrong.
    ///
    /// **Do not retry blindly.** TLS, a proxy configuration, a malformed
    /// response, too many redirects: none of them are helped by repeating the
    /// request, and lumping them under a hopeful name would invite exactly
    /// that. `Other` says the classification does not know, which is the
    /// honest thing to say.
    Other,
}

/// Anything that can stop a render.
// `thiserror::Error` is named in full rather than imported: an imported `Error`
// makes every `[`Error`]` doc link in this file ambiguous between the derive
// macro and the enum below, which `rustdoc::all = "deny"` rejects.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A node names a URL and this build cannot fetch.
    ///
    /// **The message names the feature**, because a flag whose failure does not
    /// name the flag teaches nobody: a caller who meets this has already shown
    /// they did not read the documentation that would have told them, and the
    /// error is the one thing they are certainly looking at.
    #[error(
        "node {} names a URL, and this build cannot fetch: enable the `net` \
         feature on meo-canvas or meo-canvas-core, or resolve the bytes \
         yourself and pass ImageSource::Bytes",
        .0.get()
    )]
    UnresolvedSource(NodeId),

    /// A URL source the fetcher could not obtain.
    ///
    /// Distinct from [`Error::UnresolvedSource`], which is the same node with
    /// the `net` feature **off**: that one says this build does not fetch, and
    /// this one says the fetch was attempted and failed. A caller can act on
    /// the difference -- the first is a build flag, the second is the network.
    #[error("cannot fetch {url}: {detail}")]
    #[non_exhaustive]
    SourceFetch {
        /// The URL the node named.
        url: String,
        /// What the HTTP client reported, for a person to read.
        detail: String,
        /// What went wrong, for a program to branch on.
        failure: FetchFailure,
    },

    /// A [`animate::easing::steps`] count with no width to hold a value for.
    #[error("steps({0}) needs at least one step")]
    Steps(u32),

    /// A spring whose parameters have no equation: the three regimes all
    /// divide by something derived from the stiffness and the mass.
    #[error("a spring's {0}")]
    Spring(&'static str),

    /// A keyframe track whose stops and values do not describe a track.
    #[error("a keyframe track {0}")]
    Keyframes(&'static str),

    /// A track whose timing does not describe a motion.
    #[error("a track's {0}")]
    Track(&'static str),

    /// A chart given data it cannot draw.
    ///
    /// **A refusal rather than a guess.** Negative values and data that is
    /// all zero have no drawing this chart defines, so it refuses instead of
    /// picking one.
    #[error("{0}")]
    Chart(&'static str),

    /// The scene is not the forest of pages the contract requires.
    ///
    /// [`meo_canvas_scene::codec::decode`] checks this, so a scene read from
    /// bytes cannot fail here; a scene a Rust caller assembled by writing
    /// `Scene::nodes` directly can.
    /// The cause is **not** interpolated here, and that is the convention
    /// rather than an omission. Every `#[source]` is rendered once, by the
    /// walk at the Neon boundary; a variant that also wrote `{0}` would print
    /// its cause twice for a JavaScript caller and once for a Rust one.
    #[error("the scene cannot be drawn")]
    Scene(#[source] meo_canvas_scene::SceneError),

    /// A font file that cannot be read or parsed.
    #[error("cannot register font family {family:?}: {detail}")]
    #[non_exhaustive]
    FontRegister {
        /// The family name the caller asked for.
        family: String,
        /// What the font backend reported.
        detail: String,
    },

    /// A local image path that cannot be read.
    #[error("cannot read image at {path}{}", url_hint(path))]
    #[non_exhaustive]
    ImageRead {
        /// The path as the scene spelled it.
        path: String,
        /// The underlying filesystem failure.
        #[source]
        source: std::io::Error,
    },

    /// A `data:` image source that is not a well-formed data URI.
    ///
    /// **Separate from [`Error::ImageRead`].** A `data:` URI carries its own
    /// bytes and names no file, so a filesystem error would quote something
    /// that was never a filename.
    #[error("cannot read a data: image source: {detail}")]
    #[non_exhaustive]
    DataUri {
        /// What is wrong with it, and what the form takes.
        detail: String,
    },

    /// Bytes that no decoder recognises.
    #[error("image bytes for node {} are in no format this decodes", .0.get())]
    UndecodableImage(NodeId),

    /// A colour asked for on a source that is not a vector document.
    ///
    /// **Refused rather than ignored.** A colour recolours an SVG's
    /// `currentColor`; a bitmap has no such thing, so a caller who set one on
    /// a photograph has asked for something that cannot happen, and drawing
    /// the photograph unchanged is the silent wrong picture this refusal
    /// exists to prevent.
    ///
    /// **Raised at render rather than when the node is built**, because
    /// neither surface knows what a source is until it is read: a path and a
    /// URL are opaque, and only bytes could be sniffed.
    #[error(
        "node {} sets a colour on an image source that is not an SVG \
         document; a colour recolours an SVG's `currentColor` and has no \
         reading on a bitmap",
        .0.get()
    )]
    TintOnRaster(NodeId),

    /// An SVG document that would not parse.
    ///
    /// **Separate from [`Error::UndecodableImage`] so the sniff earns its
    /// place.** Bytes that reach the SVG parser have already been refused by
    /// every raster decoder *and* looked like a document, so telling a caller
    /// only that nothing reads them describes the raster decoders and not the
    /// thing that actually failed. A caller whose icon has a stray tag wants
    /// to hear about the tag.
    #[error("the SVG document for node {} did not parse", .0.get())]
    UnparsableSvg(NodeId),

    /// A decoder panicked while reading a source.
    ///
    /// **Distinct from [`Error::UndecodableImage`] so that it can never be
    /// softened.** Bytes a decoder refuses are a fact about the bytes, and for
    /// a URL that is the broken-image case a placeholder exists for. A decoder
    /// that *panicked* is a fact about us: the input may have been perfectly
    /// good, and drawing a tasteful grey rectangle over our own crash is how a
    /// real defect ships quietly for a year.
    #[error("a decoder panicked reading the image for node {}", .0.get())]
    DecoderPanicked(NodeId),

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
    #[non_exhaustive]
    Encode {
        /// The format that was asked for.
        format: ImageFormat,
        /// What the encoder reported.
        detail: String,
    },
}

/// Schemes an image source is fetched over rather than opened: the two the
/// JavaScript surface pre-fetches and `net` speaks. Anything else, `file:`, a
/// drive letter or a relative path, is a filename.
const FETCHED_SCHEMES: [&str; 2] = ["http://", "https://"];

/// The sentence appended to [`Error::ImageRead`] when the path is a URL: a bare
/// string source is a path, so `{ src: 'https://...' }` fails as a file, and
/// this points the reader at the spelling rather than at the network.
fn url_hint(path: &str) -> &'static str {
    if FETCHED_SCHEMES
        .iter()
        .any(|scheme| path.starts_with(scheme))
    {
        " -- a URL here is read as a filename; name it as a URL source \
         (`{ url: ... }` in JavaScript, `ImageSource::Url` in Rust) to have it \
         fetched"
    } else {
        ""
    }
}

/// An error and every cause beneath it, as one sentence.
///
/// `thiserror`'s `Display` renders only the `#[error(...)]` string, so a cause
/// held as `#[source]` reaches a caller only if something walks the chain. A
/// Rust caller can walk it; a JavaScript caller receives one string and the
/// command line prints one line, so both need this.
///
/// **It lives here rather than at either boundary because a variant must not
/// interpolate its own cause**: one that did would print it twice wherever the
/// chain is walked. One walk, one definition, and every `#[source]` rendered
/// exactly once wherever an error is shown.
#[must_use]
pub fn chained(error: &dyn std::error::Error) -> String {
    core::iter::successors(Some(error), |current| current.source())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}

impl Error {
    /// A path the renderer could not read, as the error for it.
    ///
    /// **A constructor because the variant is `#[non_exhaustive]`.** That
    /// attribute keeps a field addable later without breaking every caller who
    /// destructured the variant, and it also stops a struct expression outside
    /// this crate -- including `meo-canvas-cli`, which builds one to check the
    /// exit code it maps to. This is the door left open for that.
    #[must_use]
    pub const fn image_read(path: String, source: std::io::Error) -> Self {
        Self::ImageRead { path, source }
    }
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

/// The size to begin a page at: a page root's own definite size, which lets ICO
/// pages differ, and otherwise `scene.size`. Not solved, since solving needs
/// the page; the height is the exception, the solved one under
/// [`Scene::content_height`], with `scene.size.height` its floor.
fn page_size(scene: &Scene, page: NodeId, solved: &LayoutResult) -> Size {
    let stated = |dimension, fallback| match dimension {
        Dimension::Points(value) => value,
        Dimension::Auto | Dimension::Percent(_) => fallback,
    };
    scene.get(page).map_or(scene.size, |root| Size {
        width: stated(root.layout.size.0, scene.size.width),
        height: if scene.content_height && root.layout.size.1 == Dimension::Auto
        {
            solved.get(page).map_or(scene.size.height, |rect| {
                rect.size.height.max(scene.size.height)
            })
        } else {
            stated(root.layout.size.1, scene.size.height)
        },
    })
}

impl Renderer {
    /// Whether a renderer asks for the GPU when nothing says otherwise.
    ///
    /// True, matching v9 -- its `RootProps` carries `gpu` and
    /// `meo-skia-canvas` defaults it on, so a scene ported from v9 behaves the
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

    /// The surface a scene asks for, falling back to this renderer: absent
    /// means the renderer decides, so a scene cannot override a renderer set to
    /// the CPU. `color_type` and `color_space` fall back to `CanvasOptions`'
    /// own defaults.
    fn surface_for(&self, scene: &Scene) -> SurfaceOptions {
        SurfaceOptions {
            gpu: scene.gpu.unwrap_or(self.gpu),
            color_type: scene.color_type.unwrap_or_default(),
            color_space: scene.color_space.unwrap_or_default(),
        }
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

        // Solve, then allocate: a page whose height comes from its content has
        // none until layout runs, so the first page creates the surface and
        // every later page begins on it.
        let mut surface: Option<Surface> = None;

        for &page in &scene.pages {
            let solved = layout::solve(scene, page, &mut measurer)?;
            let size = page_size(scene, page, &solved);
            let surface = match surface {
                Some(ref mut existing) => {
                    existing.begin_page(size)?;
                    existing
                }
                None => surface.insert(Surface::new(
                    size,
                    scene.scale,
                    self.surface_for(scene),
                )?),
            };
            paint::draw(surface, &resolved, &solved, &mut measurer)?;
        }

        // `validate` rejects a scene with no pages, so the loop ran at least
        // once and the surface exists. Unwrapping on that is a claim about a
        // check twenty lines up; asking again costs nothing and cannot rot.
        let surface = surface.ok_or_else(|| {
            Error::Layout("a validated scene produced no page".to_owned())
        })?;
        Ok(RenderedCanvas {
            surface,
            // Moved out of the resolve tables rather than copied: the vector
            // is empty on every render where nothing failed, and an empty
            // `Vec` owns no allocation to move.
            warnings: resolved.into_warnings(),
        })
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
    warnings: Vec<ImageWarning>,
}

impl RenderedCanvas {
    /// Every image source that could not be resolved, in node order.
    ///
    /// **Empty is the ordinary answer and is always valid to ask for.** A
    /// render where every source resolved returns an empty slice rather than
    /// nothing, so `warnings().is_empty()` is a check a caller can write once
    /// and never guard.
    ///
    /// One entry per distinct source rather than per node. Populated whatever
    /// [`OnImageError`](meo_canvas_scene::OnImageError) the scene chose,
    /// including [`Ignore`](meo_canvas_scene::OnImageError::Ignore): the
    /// option decides what is drawn, never what is recorded.
    #[must_use]
    pub fn warnings(&self) -> &[ImageWarning] {
        &self.warnings
    }

    /// Encodes the painted pages in one format.
    ///
    /// Takes `&mut self` because encoding mutates: every encode entry point
    /// upstream is `&mut self` since `Canvas::to_buffer` prepares the surface
    /// before reading it. That is
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

    /// Takes the half of an encode that needs this canvas, and stops there.
    ///
    /// The returned [`PreparedEncode`] is [`Send`] where this canvas is not,
    /// so the expensive half runs wherever the caller puts it -- a worker
    /// thread, a pool -- while this canvas stays on the thread that owns it.
    /// [`RenderedCanvas::to_buffer`] is this followed immediately by
    /// [`PreparedEncode::encode`], so the two produce identical bytes by
    /// construction rather than by agreement.
    ///
    /// **Almost all of the cost is on the far side.** At 4000x4000 the record
    /// costs 2.74 ms and the encode 97.23 ms, and the handle itself is
    /// negligible beside either.
    ///
    /// One handle is one format: see [`PreparedEncode`] for why, and take a
    /// second handle rather than reusing this one.
    ///
    /// ```
    /// use meo_canvas_core::{EncodeOptions, ImageFormat, Renderer};
    /// use meo_canvas_scene::{Scene, Size};
    ///
    /// let renderer = Renderer::new();
    /// let scene = Scene::new(Size::new(64.0, 32.0));
    /// let mut canvas = renderer.render(&scene)?;
    ///
    /// let pending =
    ///     canvas.prepare_encode(ImageFormat::Png, &EncodeOptions::default())?;
    /// let png = std::thread::spawn(move || pending.encode())
    ///     .join()
    ///     .expect("the encoding thread")?;
    /// assert!(!png.bytes.is_empty());
    /// # Ok::<(), meo_canvas_core::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Encode`] when the options do not suit the format, and
    /// when the surface cannot resolve the pages they name. Failures that
    /// belong to encoding itself are raised by [`PreparedEncode::encode`].
    pub fn prepare_encode(
        &mut self,
        format: ImageFormat,
        options: &EncodeOptions,
    ) -> Result<PreparedEncode, Error> {
        encode::prepare(&mut self.surface, format, options)
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

    /// Whether this canvas asked for the GPU.
    ///
    /// The request, not the outcome — see [`RenderedCanvas::engine`], which is
    /// the other one. Both are reported because they can disagree and a caller
    /// otherwise has no way to find out: a build with no GPU backend compiled,
    /// a driver that declines and a float `color_type` all rasterise on the CPU
    /// whatever was asked for.
    #[must_use]
    pub const fn gpu(&self) -> bool {
        self.surface.gpu()
    }

    /// Which rasteriser this canvas actually got: `"gpu"` or `"cpu"`.
    ///
    /// The outcome, not the request. See [`RenderedCanvas::gpu`].
    #[must_use]
    pub fn engine(&self) -> &'static str {
        self.surface.engine()
    }
}

#[cfg(test)]
mod tests {

    /// A cause held as `#[source]` reaches the message, once: putting `{0}`
    /// back on `Error::Scene` fails the first assertion, and removing the walk
    /// the second.
    #[test]
    fn a_source_reaches_the_message_exactly_once() {
        let error = Error::Scene(meo_canvas_scene::SceneError::NoPages);
        let cause = meo_canvas_scene::SceneError::NoPages.to_string();

        assert_eq!(error.to_string(), "the scene cannot be drawn");
        assert_eq!(
            chained(&error),
            format!("the scene cannot be drawn: {cause}")
        );
        assert_eq!(chained(&error).matches(&cause).count(), 1);
    }

    /// The io cause of an unreadable image is what separates three failures.
    ///
    /// Without the walk these three differ only by the path, which is the one
    /// thing the caller already knew.
    #[test]
    fn an_image_read_names_which_filesystem_failure_it_was() {
        let missing = Error::ImageRead {
            path: "/nope.png".to_owned(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        };
        let denied = Error::ImageRead {
            path: "/nope.png".to_owned(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        };

        assert!(
            chained(&missing).starts_with("cannot read image at /nope.png: ")
        );
        assert_ne!(chained(&missing), chained(&denied));
    }

    /// A URL read as a filename says so, and a filename does not: the URL case
    /// alone would pass if the sentence were appended to every path.
    #[test]
    fn a_url_opened_as_a_path_says_which_mistake_it_was() {
        let as_path = |path: &str| Error::ImageRead {
            path: path.to_owned(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        };

        let url = as_path("https://example.invalid/x.png").to_string();
        assert!(url.contains("read as a filename"), "{url}");
        assert!(url.contains("ImageSource::Url"), "{url}");

        // A real path, a scheme this crate does not fetch, and a name that
        // merely begins with the letters. `data:` is quiet for two reasons,
        // matching no `FETCHED_SCHEMES` prefix and having its own variant, so
        // removing either passes here.
        for quiet in [
            "/nope.png",
            "file:///nope.png",
            "https-notes.png",
            "data:image/png;base64,AAA",
        ] {
            let message = as_path(quiet).to_string();
            assert!(
                !message.contains("read as a filename"),
                "{quiet} was called a URL: {message}"
            );
        }
    }
    use meo_canvas_scene::{
        ColorSpace, ColorType, Scene, Size,
        node::{Node, NodeId},
    };

    use super::{EncodeOptions, Error, ImageFormat, Renderer, chained};
    use crate::resolve::tests::{TEST_FAMILY, TEST_FONT};

    /// A scene of `pages` pages, each carrying one text node, so a font
    /// resolved per page rather than per scene still succeeds.
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
    fn a_float_layout_reports_the_cpu_however_the_gpu_was_asked_for() {
        // The one oracle the `ColorType` aliases have: v9 documents a float
        // `colorType` falling back to the CPU (`canvas.type.ts:1211`), so a
        // float alias reports `"cpu"` with the GPU requested. `F16` and `F32`
        // both do, so it pins no more.
        let renderer = Renderer::new();
        let mut scene = Scene::new(Size::new(4.0, 2.0));
        scene.gpu = Some(true);

        let engine_for = |color_type| {
            let mut scene = scene.clone();
            scene.color_type = Some(color_type);
            renderer
                .render(&scene)
                .unwrap_or_else(|error| unreachable!("{error}"))
                .engine()
        };

        // Vacuous unless this build can reach a GPU at all: with no backend
        // compiled every layout reports `"cpu"` and the assertion below would
        // hold for a reason that has nothing to do with the colour type.
        if engine_for(ColorType::Uint8) != "gpu" {
            return;
        }

        for float in [ColorType::F16, ColorType::F32, ColorType::F16Norm] {
            assert_eq!(
                engine_for(float),
                "cpu",
                "{float:?} is a float layout and must fall back"
            );
        }
    }

    #[test]
    fn a_canvas_reports_the_request_and_the_outcome_separately() {
        // The pair exists because they disagree, and a caller who asks for the
        // GPU and silently gets the CPU otherwise has no way to find out.
        let mut renderer = Renderer::new();
        renderer.set_gpu(false);
        let scene = Scene::new(Size::new(4.0, 2.0));

        let canvas = renderer
            .render(&scene)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(!canvas.gpu(), "the request is what the renderer was set to");
        assert_eq!(canvas.engine(), "cpu");
        assert_eq!(canvas.page_count(), 1);
        assert!((canvas.scale() - Scene::DEFAULT_SCALE).abs() < f32::EPSILON);
    }

    #[test]
    fn a_scene_that_states_a_surface_overrides_the_renderer() {
        // And one that states nothing does not. The `Option` is the whole
        // point: absent means the renderer decides, which is a different thing
        // from asking for what the renderer happens to default to.
        let mut renderer = Renderer::new();
        renderer.set_gpu(false);

        let mut scene = Scene::new(Size::new(4.0, 2.0));
        assert!(
            !renderer.surface_for(&scene).gpu,
            "an absent gpu takes the renderer's"
        );

        scene.gpu = Some(true);
        assert!(
            renderer.surface_for(&scene).gpu,
            "a stated gpu overrides the renderer's"
        );

        scene.color_type = Some(ColorType::F16);
        scene.color_space = Some(ColorSpace::DisplayP3);
        let surface = renderer.surface_for(&scene);
        assert_eq!(surface.color_type, ColorType::F16);
        assert_eq!(surface.color_space, ColorSpace::DisplayP3);
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

    /// Every page reaches the encoder exactly once: a GIF's frame count is its
    /// page count, so two frames means a page skipped `begin_page` and four
    /// means the first page ran it twice.
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
    /// drawn, which says resolving happens once for the whole scene.
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

    /// The GPU is the renderer's decision, defaulting to v9's.
    #[test]
    fn the_gpu_is_a_renderer_property_with_v9_s_default() {
        let mut cpu_renderer = renderer();
        cpu_renderer.set_gpu(true);
        assert!(cpu_renderer.gpu(), "v9 defaults the GPU on");
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
    /// Two formats of one picture cost one render, asserted through the output:
    /// the second encode is a real image, and the PNG is byte-identical to a
    /// fresh single-format render.
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

#[cfg(test)]
mod ico_promise {
    use meo_canvas_scene::{Scene, Size, style::Dimension};

    use super::{EncodeOptions, ImageFormat, Renderer};

    /// The sizes an icon conventionally carries, and the ones
    /// `ImageFormat::Ico` names.
    const SIZES: [f32; 4] = [16.0, 32.0, 48.0, 256.0];

    /// A caller can reach `ImageFormat::Ico`'s promise: four page roots state
    /// four sizes, go through the public `Renderer`, and the directory is read
    /// back from the bytes. Driving `begin_page` directly would pass either
    /// way.
    #[test]
    fn a_scene_whose_page_roots_state_four_sizes_writes_four_ico_entries() {
        // The scene size is none of the four, so a page that fell back to it
        // shows in the directory rather than agreeing with an answer.
        let mut scene = Scene::new(Size::new(100.0, 100.0));
        for (index, side) in SIZES.into_iter().enumerate() {
            // `Scene::new` already carries a page, so only the rest are pushed.
            // Pushing one per size leaves a fifth page at `scene.size` ahead of
            // them all, which the directory reports and nothing else would.
            let page = if index == 0 {
                scene.pages[0]
            } else {
                scene.push_page().unwrap_or_else(|error| {
                    unreachable!("the page pushes: {error}")
                })
            };
            let root = scene
                .get_mut(page)
                .unwrap_or_else(|| unreachable!("the page root exists"));
            root.layout.size =
                (Dimension::Points(side), Dimension::Points(side));
        }

        let written = Renderer::new()
            .render_to_buffer(
                &scene,
                ImageFormat::Ico,
                &EncodeOptions::default(),
            )
            .unwrap_or_else(|error| {
                unreachable!("the ICO did not encode: {error}")
            });

        // A zero width or height means 256: one byte cannot hold it, which is
        // why an icon's largest conventional size reads as nothing.
        let bytes = &written;
        let count = usize::from(u16::from_le_bytes([bytes[4], bytes[5]]));
        let entries: Vec<(u32, u32)> = (0..count)
            .map(|index| {
                let at = 6 + index * 16;
                let side =
                    |byte: u8| if byte == 0 { 256 } else { u32::from(byte) };
                (side(bytes[at]), side(bytes[at + 1]))
            })
            .collect();

        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "every side here is a small whole number"
        )]
        let want: Vec<(u32, u32)> = SIZES
            .iter()
            .map(|&side| (side as u32, side as u32))
            .collect();
        assert_eq!(
            entries, want,
            "four page roots stated four sizes and the file should carry them"
        );
    }
}

/// This crate's own README, compiled, so a `rust` fence naming an item that
/// moved fails the build. `../README.md` is this crate's front page, not the
/// repository's.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct CrateReadme;
