//! The bridge between taffy's questions and Skia's answers.
//!
//! taffy calls a measure function for every leaf whose size it cannot derive
//! from style alone, handing over the space available and expecting an extent
//! back. For text that answer comes from shaping the run at the offered width,
//! which is Skia's paragraph layout; for images it is the decoded bitmap's
//! intrinsic size scaled to fit.
//!
//! The signature taffy imposes returns a size and nothing else, so a measured
//! leaf cannot report a baseline through it. Yoga's `YGNodeSetBaselineFunc` has
//! no counterpart at this level: `taffy::LayoutOutput` carries `baselines`, but
//! only the low-level `LayoutPartialTree` API can produce one. Baseline
//! alignment of measured text therefore needs the low-level tree, and until it
//! does, [`MeasuredLeaf`] carries the baseline this crate computed so the paint
//! pass can place glyphs correctly even when layout could not align on them.

use meo_canvas_scene::{Size, node::NodeId};

/// What measuring one leaf produced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredLeaf {
    /// The extent taffy is told about.
    pub size: Size,
    /// Distance from the top edge to the first baseline, when the leaf has
    /// one.
    ///
    /// taffy never sees this. It exists so the paint pass does not reshape the
    /// run a second time to find out where the glyphs sit.
    pub first_baseline: Option<f32>,
}

/// The space taffy offers a leaf on one axis.
///
/// Mirrors `taffy::AvailableSpace` in this crate's own vocabulary so callers of
/// [`measure_leaf`] need not name taffy. Note that this is not Yoga's
/// `YGMeasureMode`: Yoga distinguishes `Exactly`/`AtMost`/`Undefined`, taffy
/// distinguishes definite from the two intrinsic sizes, and `MinContent` has no
/// Yoga counterpart at all. A measure function ported from Yoga is rewritten
/// here, not translated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Available {
    /// A known extent.
    Definite(f32),
    /// The smallest extent the content fits in.
    MinContent,
    /// The extent the content takes when nothing constrains it.
    MaxContent,
}

/// Measures one leaf against the space offered on each axis.
///
/// # Errors
///
/// Returns [`crate::Error::UnknownFont`] when a text node names a family the
/// renderer's library does not hold.
pub fn measure_leaf(
    _node: NodeId,
    _available: (Available, Available),
) -> Result<MeasuredLeaf, crate::Error> {
    unimplemented!()
}
