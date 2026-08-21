//! Walks the laid-out scene and issues draws against a Skia surface.
//!
//! Paint reads the layout result rather than recomputing anything: by this
//! point every node has an absolute rectangle, so the pass is a flat traversal
//! in child order with no measurement and no arithmetic beyond the device
//! scale.
//!
//! The device scale is applied here, once, as a transform on the surface --
//! never in layout. Layout solving at scale 1 means a scene rendered at 1x and
//! at 3x lays out once and produces the same boxes, which is what makes the two
//! outputs comparable pixel for pixel after downsampling.
//!
//! Nothing in this module's signatures names a Skia type. The surface is owned
//! behind [`Surface`], so a caller can drive the pass without linking against a
//! Skia build's public API.

use meo_canvas_scene::Size;

use crate::{Error, layout::LayoutResult, resolve::Resolved};

/// A drawable surface of a fixed pixel size.
#[derive(Debug)]
pub struct Surface {
    _private: (),
}

impl Surface {
    /// Allocates a surface `size * scale` pixels across.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Paint`] when the backend cannot allocate the surface,
    /// which for large scales is a memory limit rather than a bug.
    pub fn new(_size: Size, _scale: f32) -> Result<Self, Error> {
        unimplemented!()
    }
}

/// Draws every node of the scene onto the surface.
///
/// # Errors
///
/// Returns [`Error::Paint`] when a draw fails, and [`Error::UnknownFont`] when
/// a text node names a family that measurement resolved but paint cannot.
pub fn draw(
    _surface: &mut Surface,
    _resolved: &Resolved<'_>,
    _layout: &LayoutResult,
) -> Result<(), Error> {
    unimplemented!()
}
