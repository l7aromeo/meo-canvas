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

pub mod encode;
pub mod layout;
pub mod measure;
pub mod paint;
pub mod resolve;

pub use encode::{EncodedImage, ImageFormat};
pub use layout::LayoutResult;
use meo_canvas_scene::{Scene, node::NodeId};
pub use resolve::Resolved;

/// Anything that can stop a render.
// `thiserror::Error` is named in full rather than imported: an imported `Error`
// makes every `[`Error`]` doc link in this file ambiguous between the derive
// macro and the enum below, which `rustdoc::all = "deny"` rejects.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A node names a source the renderer cannot obtain by itself.
    #[error("node {} names a source this crate does not fetch", .0.get())]
    UnresolvedSource(NodeId),

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
#[derive(Debug, Default)]
pub struct Renderer {
    _private: (),
}

impl Renderer {
    /// Creates a renderer with no fonts registered beyond the system's.
    #[must_use]
    pub const fn new() -> Self {
        Self { _private: () }
    }

    /// Runs every pass and returns the encoded image.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] from whichever pass fails first.
    pub fn render(
        &self,
        _scene: &Scene,
        _format: ImageFormat,
    ) -> Result<EncodedImage, Error> {
        unimplemented!()
    }
}
