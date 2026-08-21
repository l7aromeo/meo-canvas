//! Runs taffy over a resolved scene and produces one absolute rectangle per
//! node.
//!
//! The taffy tree is built here, used here and dropped here. It never appears
//! in a public signature and never crosses a thread, because it cannot: every
//! length taffy stores is a tagged `*const ()`
//! (`taffy-0.13.0/src/style/compact_length.rs:62`), which makes `taffy::Style`,
//! and therefore `TaffyTree`, `!Send` and `!Sync` regardless of feature
//! selection -- a build with `calc` removed fails `assert_send` identically.
//! Confining the tree to one function is what keeps that fact from spreading
//! into the rest of the workspace.
//!
//! Output rectangles are absolute rather than parent-relative. taffy rounds on
//! cumulative viewport coordinates, rounding each edge and taking the
//! difference so adjacent boxes leave no seam; converting back to
//! parent-relative and re-adding during paint would reintroduce exactly the
//! seam that rounding avoids.
//!
//! One divergence from Yoga is unavoidable and visible: taffy rounds to whole
//! pixels with no configurable scale factor, where Yoga's
//! `YGConfigSetPointScaleFactor` can snap to halves or thirds. Layout here
//! always solves at scale 1 and the device scale is applied at paint time, so
//! the two agree; a caller that wants layout itself to snap at a device scale
//! does not get it.

use std::collections::HashMap;

use meo_canvas_scene::{Rect, node::NodeId};

use crate::{Error, resolve::Resolved};

/// Where every node ended up.
#[derive(Debug, Clone, Default)]
pub struct LayoutResult {
    /// Absolute rectangle per node, in logical pixels at scale 1.
    pub rects: HashMap<NodeId, Rect>,
}

impl LayoutResult {
    /// The rectangle computed for `node`, or `None` if it was not laid out.
    ///
    /// A node under a `Display::None` subtree has no rectangle, which is the
    /// difference between "not drawn" and "drawn at zero size".
    #[must_use]
    pub fn get(&self, node: NodeId) -> Option<Rect> {
        self.rects.get(&node).copied()
    }
}

/// Builds a taffy tree from the scene, solves it, and discards the tree.
///
/// # Errors
///
/// Returns [`Error::Layout`] when taffy rejects the tree, and propagates
/// whatever [`crate::measure`] reports for a leaf it cannot size.
pub fn solve(_resolved: &Resolved<'_>) -> Result<LayoutResult, Error> {
    unimplemented!()
}
