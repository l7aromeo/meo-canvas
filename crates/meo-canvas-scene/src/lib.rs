//! The scene description: what to draw, stated once, in types nothing else in
//! the workspace can disagree about.
//!
//! Three surfaces produce a scene -- the Node addon, the CLI, and Rust callers
//! going through `meo-canvas` -- and one consumes it. Putting the description
//! in its own crate is what stops that from being three descriptions: a field
//! added here appears on every surface at once, and a field the renderer cannot
//! honour fails to compile rather than failing to draw.
//!
//! ```
//! use meo_canvas_scene::{
//!     Scene, Size, codec,
//!     node::{Node, NodeId},
//! };
//!
//! let mut scene = Scene::new(Size::new(320.0, 240.0));
//! let label = scene.push(NodeId::ROOT, Node::text("hello"))?;
//! assert_eq!(scene.get(label).map(|node| node.children.len()), Some(0));
//!
//! let bytes = codec::encode(&scene);
//! assert_eq!(codec::decode(&bytes)?, scene);
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```
//!
//! # Two representations, one `Scene`
//!
//! A scene reaches this crate two ways, and they exist for different reasons.
//!
//! The **boundary** representation is an `f64` arena: JavaScript writes opcodes
//! and numeric properties into a `Float64Array`, with strings and buffers in a
//! side array holding indices into it. That decoder needs the side array, so it
//! lives in `meo-canvas-node` and not here.
//!
//! The **persistence** representation is [`codec`]: self-contained bytes with a
//! magic number, a version and no side channel, written to disk and read by the
//! CLI and the golden fixtures.
//!
//! Both produce the same [`Scene`], so a scene captured from JavaScript and
//! written to disk survives the trip. Every type in this crate round-trips
//! through [`codec`] without loss; a type that could not would say so in its
//! own documentation.
//!
//! # What this crate deliberately excludes
//!
//! It has no dependencies, and that is the point rather than an accident of
//! being small.
//!
//! No Skia. A node says `fill: Color` and `radius: f32`, not `SkPaint`. The
//! scene outlives any one backend, and a scene naming Skia types could not be
//! built by a caller who has not paid for a Skia build -- which is every test
//! in this crate.
//!
//! No taffy. [`style::layout::LayoutStyle`] is this crate's own vocabulary. Two
//! reasons, and the second is load-bearing: `taffy::Style` is `!Send` and
//! `!Sync` on every supported target, because `CompactLengthInner` stores every
//! length as a tagged `*const ()`
//! (`taffy-0.13.0/src/style/compact_length.rs:62`). A scene built from
//! `taffy::Style` could not cross a thread, which is exactly what a scene is
//! for. Translation happens once, inside `meo-canvas-core`, on the thread that
//! lays out.
//!
//! No neon. The scene crosses the JavaScript boundary as data, not as a tree of
//! `JsObject` handles walked field by field.
//!
//! No serde. [`codec`] is written by hand because it is a format rather than a
//! serialization: it is versioned, a JavaScript writer implements it too, and
//! its byte layout is a compatibility promise. A derive would make that promise
//! a side effect of field declaration order.

// **Nothing in this workspace writes `unsafe`, and this is what keeps it that
// way.** Measured before it was declared: zero occurrences of the token across
// every `crates/*/src`. A renderer reaching a C++ library through two binding
// layers is exactly the crate where an `unsafe` would look reasonable and go
// unquestioned, and the declaration turns adding one into a decision someone
// has to make deliberately rather than a line that passes review.
//
// The integration tests are separate crates and are not covered: the
// allocator that measures `codec::decode`'s reservation has to be an
// `unsafe impl GlobalAlloc`. That is the only `unsafe` in the repository and
// it exists to measure a defect.
#![forbid(unsafe_code)]
// `unreachable_pub` is a workspace lint, and `clippy::redundant_pub_crate` is
// its opposite: one asks for `pub(crate)` on an item a private module exports,
// the other calls that redundant. The workspace chose `unreachable_pub`, so the
// clippy half is off here rather than the visibility being written twice.
#![allow(
    clippy::redundant_pub_crate,
    reason = "contradicts the workspace's unreachable_pub"
)]

pub mod codec;
pub mod geometry;
pub mod node;
pub mod style;
pub mod surface;

mod wire;

pub use codec::CodecError;
pub use geometry::{Corners, Point, Rect, Sides, Size};
pub use node::{Node, NodeId, NodeKind};
pub use style::{Dimension, Length};
pub use surface::{ColorSpace, ColorType};

/// A complete drawing: the surface to draw onto, and the pages to draw on it.
///
/// Nodes live in one flat arena rather than as boxed children, so a [`NodeId`]
/// is a plain index that survives the [`codec`] round trip and can be handed
/// back to a caller as a stable handle. The arena's order is part of the
/// contract: [`codec`] preserves it exactly, because the indices mean nothing
/// otherwise.
///
/// # Pages
///
/// A scene carries one or more page trees, not one. One page is the common case
/// and encodes to a still image; several become frames in GIF and APNG, sheets
/// in PDF and TIFF, and sizes in ICO.
///
/// The pages share the arena rather than owning one each. Fonts and decoded
/// images are resolved once for the whole scene, so the thing they are resolved
/// against is one list of nodes; giving each page its own arena would make a
/// [`NodeId`] ambiguous without a page beside it, and every cache key in the
/// resolve stage a pair.
///
/// Sharing the arena is not sharing subtrees. The arena is a *forest*: every
/// node belongs to exactly one page, has at most one parent, and is reachable
/// from the root of its page. [`Scene::validate`] enforces all three, so two
/// pages naming the same subtree is an error rather than a drawing whose nodes
/// are laid out twice at two different sizes.
///
/// [`Scene::pages`] order is the page order, and the wire format preserves it:
/// frame one of an animation is `pages[0]`. A scene with no pages is an error
/// for the same reason a scene with no nodes is -- there is nothing to draw,
/// and the caller meant something they did not say.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    /// Surface dimensions in logical pixels, before `scale` is applied.
    ///
    /// When [`Scene::content_height`] is set, `size.height` is a **floor**
    /// rather than the height: the page is as tall as its content, and at
    /// least this tall. Zero is the ordinary value there, and asks for the
    /// content's own height with nothing added.
    pub size: Size,
    /// Whether the page height comes from what is in it.
    ///
    /// A caller who states a height gets that height. A caller who does not
    /// has no way to know one -- the content decides it -- and this is how the
    /// scene says so. Width is never derived this way: text cannot break into
    /// lines without knowing its room, so a width is a question the caller has
    /// to answer.
    pub content_height: bool,
    /// Device-pixel multiplier.
    ///
    /// Layout solves at scale 1 and the scale is applied at paint time, so a
    /// scene rendered at two scales lays out once and the two outputs differ
    /// only in resolution.
    pub scale: f32,
    /// Whether to rasterise on the GPU. `None` leaves it to the renderer.
    ///
    /// A request rather than an outcome: a build with no GPU backend compiled
    /// rasterises on the CPU whatever this says.
    pub gpu: Option<bool>,
    /// The pixel layout the surface composites in. `None` leaves it to the
    /// renderer.
    pub color_type: Option<ColorType>,
    /// The colour space the surface composites in. `None` leaves it to the
    /// renderer.
    pub color_space: Option<ColorSpace>,
    /// Every node of every page. Index by [`NodeId::get`].
    pub nodes: Vec<Node>,
    /// The root of each page, in the order the pages are drawn.
    pub pages: Vec<NodeId>,
}

/// Whether a canvas size is a size in pixels.
///
/// Zero is allowed: a zero-width canvas is empty rather than impossible, and
/// the encoders already answer for it. What is refused is a number that cannot
/// be a length -- negative, `NaN`, or infinite.
#[must_use]
pub fn size_is_pixels(size: Size) -> bool {
    [size.width, size.height]
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0)
}

impl Scene {
    /// The `scale` a scene has when nothing sets one.
    ///
    /// One device pixel per logical pixel. Not a judgement about output
    /// quality: a caller rendering for a display multiplies it, and a default
    /// above 1 would quadruple the memory of every scene that never asked.
    pub const DEFAULT_SCALE: f32 = 1.0;

    /// Creates a scene of one page, holding a single empty root container.
    #[must_use]
    pub fn new(size: Size) -> Self {
        Self {
            size,
            // A stated size is a stated size. `Scene::new` takes one, so the
            // caller has already answered the question this flag asks.
            content_height: false,
            scale: Self::DEFAULT_SCALE,
            // Absent rather than defaulted: a scene that says nothing about the
            // surface is a scene that lets the renderer decide, which is not
            // the same thing as one asking for the renderer's current default.
            gpu: None,
            color_type: None,
            color_space: None,
            nodes: vec![Node::container()],
            pages: vec![NodeId::ROOT],
        }
    }

    /// The root of the first page.
    ///
    /// The single-page case is the common one, and writing `scene.pages[0]`
    /// at every call site would put an index into code that never has a second
    /// page. Returns `None` only for a scene with no pages, which
    /// [`Scene::validate`] rejects.
    #[must_use]
    pub fn root(&self) -> Option<NodeId> {
        self.pages.first().copied()
    }

    /// Appends an empty page and returns the id of its root container.
    ///
    /// # Errors
    ///
    /// Returns [`SceneError::TooManyNodes`] if the arena already holds
    /// [`codec::MAX_NODES`].
    pub fn push_page(&mut self) -> Result<NodeId, SceneError> {
        let id = self.reserve()?;
        self.nodes.push(Node::container());
        self.pages.push(id);
        Ok(id)
    }

    /// The id the next appended node will have, checked against the limit.
    const fn reserve(&self) -> Result<NodeId, SceneError> {
        if self.nodes.len() >= codec::MAX_NODES as usize {
            return Err(SceneError::TooManyNodes);
        }
        // The cast is exact: the bound above is `MAX_NODES`, which is a `u32`.
        Ok(NodeId::new(self.nodes.len() as u32))
    }

    /// The number of nodes in the arena.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the arena holds no nodes at all.
    ///
    /// False for a scene from [`Scene::new`], which holds its root. True only
    /// for one decoded from a buffer that declared no nodes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns the node behind `id`, or `None` if the id does not index this
    /// scene.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.get() as usize)
    }

    /// Returns the node behind `id` for modification.
    #[must_use]
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.get() as usize)
    }

    /// Appends a node to the arena and attaches it under `parent`.
    ///
    /// # Errors
    ///
    /// Returns [`SceneError::UnknownNode`] if `parent` does not index this
    /// scene, and [`SceneError::TooManyNodes`] if the arena already holds
    /// [`codec::MAX_NODES`].
    pub fn push(
        &mut self,
        parent: NodeId,
        node: Node,
    ) -> Result<NodeId, SceneError> {
        if self.get(parent).is_none() {
            return Err(SceneError::UnknownNode(parent));
        }
        let id = self.reserve()?;
        self.nodes.push(node);
        match self.get_mut(parent) {
            Some(parent) => parent.children.push(id),
            // Unreachable: `parent` indexed the arena at the top of this
            // function and nothing here removes a node. Returning the error a
            // second time rather than asserting keeps the function total.
            None => return Err(SceneError::UnknownNode(parent)),
        }
        Ok(id)
    }

    /// Whether the arena is the forest the type claims it is.
    ///
    /// Four things, and the last two are what sharing one arena between pages
    /// costs. Every page root and every child id indexes a node; no node is
    /// named twice, whether by two parents or by a parent and a page; and every
    /// node is reachable from some page root.
    ///
    /// Together those make the arena exactly the pages and nothing else. In
    /// particular a cycle cannot pass: a node inside one either has two
    /// referents, which the in-degree check catches, or none from outside,
    /// which leaves it unreachable.
    ///
    /// A decoded scene is checked by [`codec::decode`], so this is for a scene
    /// a caller assembled by writing [`Scene::nodes`] directly.
    ///
    /// # Errors
    ///
    /// Returns [`SceneError::NoPages`] for a scene that draws nothing,
    /// [`SceneError::UnknownNode`] naming the first id that indexes nothing,
    /// [`SceneError::MultipleParents`] for a node two others claim, and
    /// [`SceneError::Unreachable`] for a node no page reaches.
    pub fn validate(&self) -> Result<(), SceneError> {
        // **Before the structure, because a size is not structural.** Every
        // check below is about which node holds which; this one is about
        // whether the canvas is a canvas. A scene sized `NaN` passes all of
        // them and lays out to nothing.
        if !size_is_pixels(self.size) {
            return Err(SceneError::CanvasSize {
                width: self.size.width,
                height: self.size.height,
            });
        }
        if self.pages.is_empty() {
            return Err(SceneError::NoPages);
        }

        let mut referents = vec![0_u32; self.nodes.len()];
        let mut note = |id: NodeId| -> Result<(), SceneError> {
            let slot = referents
                .get_mut(id.get() as usize)
                .ok_or(SceneError::UnknownNode(id))?;
            *slot += 1;
            if *slot > 1 {
                return Err(SceneError::MultipleParents(id));
            }
            Ok(())
        };

        for &page in &self.pages {
            note(page)?;
        }
        for node in &self.nodes {
            for &child in &node.children {
                note(child)?;
            }
        }

        self.check_reachable()
    }

    /// Walks every page and reports the first node no page reaches.
    ///
    /// Iterative with an explicit stack rather than recursive: a scene is
    /// caller data, and a chain of nodes deeper than the thread's stack would
    /// otherwise abort the process instead of returning an error.
    fn check_reachable(&self) -> Result<(), SceneError> {
        let mut seen = vec![false; self.nodes.len()];
        let mut stack = self.pages.clone();
        while let Some(id) = stack.pop() {
            let index = id.get() as usize;
            let Some(slot) = seen.get_mut(index) else {
                return Err(SceneError::UnknownNode(id));
            };
            // The in-degree pass already refused a second referent, so a node
            // reached twice here is impossible and this guard is the loop's
            // ordinary termination rather than a duplicate check.
            if *slot {
                continue;
            }
            *slot = true;
            if let Some(node) = self.nodes.get(index) {
                stack.extend_from_slice(&node.children);
            }
        }

        // The cast is exact: the index came from a `Vec` whose length is
        // bounded by `MAX_NODES`, which is a `u32`.
        seen.iter()
            .position(|reached| !reached)
            .map_or(Ok(()), |index| {
                Err(SceneError::Unreachable(NodeId::new(index as u32)))
            })
    }
}

/// What can be wrong with a scene whose bytes are well formed.
// `Eq` is not derived: `CanvasSize` carries the two floats the caller
// passed, and a size that is `NaN` is exactly the case worth reporting back.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SceneError {
    /// A [`NodeId`] that does not index this scene's arena.
    UnknownNode(NodeId),
    /// The arena already holds [`codec::MAX_NODES`].
    TooManyNodes,
    /// A scene with nothing to draw.
    NoPages,
    /// A node claimed by two parents, or by a parent and a page.
    MultipleParents(NodeId),
    /// A node in the arena that no page reaches.
    Unreachable(NodeId),
    /// A canvas size that is not a number of pixels.
    ///
    /// **Not merely negative.** `NaN` and the infinities reach here too, and
    /// they are the ones a caller produces by accident: a width divided by a
    /// count that turned out to be zero is `inf`, and one derived from an
    /// empty measurement is `NaN`. Both build a scene, pass every structural
    /// check, and lay out to nothing -- so the picture is the first thing that
    /// reports the arithmetic.
    #[non_exhaustive]
    CanvasSize {
        /// The width asked for.
        width: f32,
        /// The height asked for.
        height: f32,
    },
}

impl SceneError {
    /// A canvas size that is not a size, as the error for it.
    ///
    /// **A constructor because the variant is `#[non_exhaustive]`**, which
    /// stops a struct expression outside this crate and would otherwise stop
    /// `meo-canvas` reporting the thing it checks. The attribute is what keeps
    /// a field addable later without breaking every caller who destructured
    /// it; this is the door left open for the callers who build it.
    #[must_use]
    pub const fn canvas_size(width: f32, height: f32) -> Self {
        Self::CanvasSize { width, height }
    }
}

impl core::fmt::Display for SceneError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownNode(id) => {
                write!(f, "node {} is not in this scene", id.get())
            }
            Self::TooManyNodes => {
                write!(f, "a scene holds at most {} nodes", codec::MAX_NODES)
            }
            Self::NoPages => f.write_str("a scene has at least one page"),
            Self::MultipleParents(id) => {
                write!(f, "node {} is claimed more than once", id.get())
            }
            Self::Unreachable(id) => {
                write!(f, "node {} is in the arena but on no page", id.get())
            }
            Self::CanvasSize { width, height } => write!(
                f,
                "a canvas is {width} by {height}, which is not a size in pixels"
            ),
        }
    }
}

impl core::error::Error for SceneError {}

#[cfg(test)]
mod tests {
    use super::{Scene, SceneError, Size};
    use crate::node::{Node, NodeId};

    #[test]
    fn a_new_scene_holds_only_its_root() {
        let scene = Scene::new(Size::new(100.0, 50.0));
        assert_eq!(scene.len(), 1);
        assert!(!scene.is_empty());
        assert_eq!(scene.root(), Some(NodeId::ROOT));
        assert!(scene.get(NodeId::ROOT).is_some());
        assert!(scene.get(NodeId::new(1)).is_none());
        assert!((scene.scale - Scene::DEFAULT_SCALE).abs() < f32::EPSILON);
    }

    #[test]
    fn push_attaches_the_child_to_its_parent() -> Result<(), SceneError> {
        let mut scene = Scene::new(Size::new(10.0, 10.0));
        let child = scene.push(NodeId::ROOT, Node::container())?;
        let grandchild = scene.push(child, Node::text("x"))?;

        assert_eq!(scene.len(), 3);
        assert_eq!(scene.nodes[0].children, vec![child]);
        assert_eq!(scene.nodes[1].children, vec![grandchild]);
        scene.validate()
    }

    #[test]
    fn push_under_a_missing_parent_names_the_parent() {
        let mut scene = Scene::new(Size::ZERO);
        let missing = NodeId::new(9);
        assert_eq!(
            scene.push(missing, Node::container()),
            Err(SceneError::UnknownNode(missing))
        );
    }

    #[test]
    fn get_mut_reaches_the_same_node_get_does() {
        let mut scene = Scene::new(Size::ZERO);
        if let Some(root) = scene.get_mut(NodeId::ROOT) {
            root.name = Some("root".to_owned());
        }
        assert_eq!(
            scene.get(NodeId::ROOT).and_then(|n| n.name.as_deref()),
            Some("root")
        );
        assert!(scene.get_mut(NodeId::new(4)).is_none());
    }

    #[test]
    fn validate_rejects_a_dangling_child_and_a_missing_root() {
        let mut scene = Scene::new(Size::ZERO);
        let dangling = NodeId::new(7);
        scene.nodes[0].children.push(dangling);
        assert_eq!(scene.validate(), Err(SceneError::UnknownNode(dangling)));

        let rootless = Scene {
            nodes: Vec::new(),
            ..Scene::new(Size::ZERO)
        };
        assert!(rootless.is_empty());
        assert_eq!(
            rootless.validate(),
            Err(SceneError::UnknownNode(NodeId::ROOT))
        );
    }

    #[test]
    fn errors_name_what_is_wrong() {
        assert_eq!(
            SceneError::UnknownNode(NodeId::new(3)).to_string(),
            "node 3 is not in this scene"
        );
        assert!(
            SceneError::TooManyNodes
                .to_string()
                .starts_with("a scene holds at most ")
        );
    }
    #[test]
    fn a_scene_prints_and_clones() {
        let scene = Scene::new(Size::new(2.0, 3.0));
        assert!(!format!("{scene:?}").is_empty());
        assert_eq!(scene.clone(), scene);
        assert!(!format!("{:?}", SceneError::TooManyNodes).is_empty());
    }
    #[test]
    fn a_new_scene_has_exactly_one_page() {
        let scene = Scene::new(Size::new(4.0, 4.0));
        assert_eq!(scene.pages, vec![NodeId::ROOT]);
        assert_eq!(scene.root(), Some(NodeId::ROOT));
        assert!(scene.validate().is_ok());
    }

    #[test]
    fn push_page_adds_a_root_that_shares_the_arena() -> Result<(), SceneError> {
        let mut scene = Scene::new(Size::ZERO);
        let first = scene.push(NodeId::ROOT, Node::text("one"))?;
        let second_page = scene.push_page()?;
        let second = scene.push(second_page, Node::text("two"))?;

        assert_eq!(scene.pages, vec![NodeId::ROOT, second_page]);
        assert_eq!(scene.len(), 4);
        assert_ne!(first, second);
        // Page order is the draw order, so the second page is the second frame.
        assert_eq!(scene.pages[1], second_page);
        scene.validate()
    }

    #[test]
    fn a_scene_with_no_pages_draws_nothing_and_is_refused() {
        let scene = Scene {
            pages: Vec::new(),
            ..Scene::new(Size::ZERO)
        };
        assert_eq!(scene.validate(), Err(SceneError::NoPages));
    }

    #[test]
    fn two_pages_cannot_share_a_subtree() {
        let mut scene = Scene::new(Size::ZERO);
        scene.pages.push(NodeId::ROOT);
        assert_eq!(
            scene.validate(),
            Err(SceneError::MultipleParents(NodeId::ROOT))
        );
    }

    #[test]
    fn a_node_two_parents_claim_is_refused() -> Result<(), SceneError> {
        let mut scene = Scene::new(Size::ZERO);
        let first = scene.push(NodeId::ROOT, Node::container())?;
        let shared = scene.push(first, Node::text("x"))?;
        scene.nodes[0].children.push(shared);
        assert_eq!(scene.validate(), Err(SceneError::MultipleParents(shared)));
        Ok(())
    }

    #[test]
    fn a_node_no_page_reaches_is_refused() {
        let mut scene = Scene::new(Size::ZERO);
        scene.nodes.push(Node::text("orphan"));
        assert_eq!(
            scene.validate(),
            Err(SceneError::Unreachable(NodeId::new(1)))
        );
    }

    /// A cycle leaves its nodes either claimed twice or reached by nobody, so
    /// `validate` refuses one without a cycle check of its own.
    #[test]
    fn a_cycle_is_refused_by_the_rules_that_are_already_there() {
        let mut scene = Scene::new(Size::ZERO);
        scene.nodes.push(Node::container());
        scene.nodes.push(Node::container());
        scene.nodes[1].children.push(NodeId::new(2));
        scene.nodes[2].children.push(NodeId::new(1));
        assert_eq!(
            scene.validate(),
            Err(SceneError::Unreachable(NodeId::new(1)))
        );

        let mut reachable_cycle = Scene::new(Size::ZERO);
        reachable_cycle.nodes.push(Node::container());
        reachable_cycle.nodes[0].children.push(NodeId::new(1));
        reachable_cycle.nodes[1].children.push(NodeId::ROOT);
        assert_eq!(
            reachable_cycle.validate(),
            Err(SceneError::MultipleParents(NodeId::ROOT))
        );
    }

    #[test]
    fn every_scene_error_says_what_is_wrong() {
        for message in [
            SceneError::UnknownNode(NodeId::new(3)).to_string(),
            SceneError::TooManyNodes.to_string(),
            SceneError::NoPages.to_string(),
            SceneError::MultipleParents(NodeId::new(4)).to_string(),
            SceneError::Unreachable(NodeId::new(5)).to_string(),
        ] {
            assert!(!message.is_empty());
        }
        assert_eq!(
            SceneError::UnknownNode(NodeId::new(3)).to_string(),
            "node 3 is not in this scene"
        );
        assert!(
            SceneError::MultipleParents(NodeId::new(4))
                .to_string()
                .contains("node 4")
        );
        assert!(
            SceneError::Unreachable(NodeId::new(5))
                .to_string()
                .contains("node 5")
        );
    }
}

/// This crate's own README, compiled.
///
/// The fences in it are a public promise that a snippet works, and a fence
/// checked by nothing is the way that promise goes stale -- the reader finds
/// out, not the gate. Anchoring the file here puts its `rust` blocks in front
/// of rustdoc, so an example naming an item that moved is a failed build.
///
/// `../README.md`, one level up from `src/`: this is the crate's own front
/// page rather than the repository's, which `meo-canvas` anchors separately.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct CrateReadme;
