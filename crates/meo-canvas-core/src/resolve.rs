//! Turns a scene's external references into bytes the later passes can use.
//!
//! Runs before measurement because an image's intrinsic size is a layout input:
//! a node sized `Auto` on both axes takes its extent from the decoded bitmap,
//! so the bitmap has to exist before taffy is asked anything.
//!
//! This pass reads local files and accepts bytes the caller already holds. It
//! does not fetch. A [`ImageSource::Url`] arriving here is
//! [`crate::Error::UnresolvedSource`] -- resolving it needs an HTTP client, an
//! HTTP client needs a policy about runtimes, and that policy belongs to the
//! surface talking to the user rather than to a library every surface links.

use meo_canvas_scene::{Scene, node::ImageSource};

use crate::Error;

/// A scene whose image sources have all become bytes.
///
/// Holds the decoded images alongside the scene rather than inside it, so the
/// scene stays the cheap, `Send`, serialisable thing it is defined to be.
/// `#[non_exhaustive]` because the decoded bitmaps live here too, and adding
/// them is not a breaking change to a caller that only reads
/// [`Resolved::scene`].
#[derive(Debug)]
#[non_exhaustive]
pub struct Resolved<'scene> {
    /// The scene these bytes belong to.
    pub scene: &'scene Scene,
}

impl<'scene> Resolved<'scene> {
    /// Reads every local source in the scene and validates every inline one.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnresolvedSource`] for any [`ImageSource::Url`],
    /// [`Error::ImageRead`] for a path that cannot be read, and
    /// [`Error::UndecodableImage`] for bytes no decoder recognises.
    pub fn from_scene(_scene: &'scene Scene) -> Result<Self, Error> {
        unimplemented!()
    }
}

/// Whether this pass can obtain the given source without a network.
#[must_use]
pub const fn is_local(source: &ImageSource) -> bool {
    match source {
        ImageSource::Path(_) | ImageSource::Bytes(_) => true,
        ImageSource::Url(_) => false,
    }
}
