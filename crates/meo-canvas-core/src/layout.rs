//! Runs taffy over a resolved scene and produces one absolute rectangle per
//! node.
//!
//! The taffy tree is built here, used here and dropped here. It never appears
//! in a public signature and never crosses a thread, because it cannot: every
//! length taffy stores is a tagged `*const ()`
//! (`taffy-0.14.0/src/style/compact_length.rs:64`), which makes `taffy::Style`,
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
//!
//! # The style mapping
//!
//! [`to_taffy_style`] sets every field of `taffy::Style` that the scene has an
//! opinion about, and never falls through to `Style::default()` for one of
//! them. A field-for-field port that leaned on taffy's defaults would agree
//! with the scene only for as long as the two happened to choose the same
//! value, and would change behaviour on a taffy upgrade with nothing in this
//! repository to show for it. The fields taffy has that the scene does not --
//! `float`, `clear`, `justify_items`, `text_align`, named grid lines -- come
//! from `Style::default()` through the struct-update syntax, because the scene
//! cannot express them and there is nothing to translate.

use std::collections::HashMap;

use meo_canvas_scene::{
    Rect, Scene, Sides, Size,
    node::NodeId,
    style::{
        Dimension, Length,
        layout::{
            Align, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
            GridAutoFlow, GridPlacement, Justify, LayoutStyle, Overflow,
            PositionType, TrackSize,
        },
    },
};
// The one trait imported from taffy rather than named through it:
// resolving a `LengthPercentage` against its containing block is a method,
// and a method needs its trait in scope.
use taffy::ResolveOrZero as _;

use crate::{
    Error,
    measure::{Available, Measure},
};

/// The scale layout solves at.
///
/// One, always. taffy rounds to whole pixels on the coordinates it is given, so
/// solving at a device scale would round to whole *device* pixels and put a
/// box's logical position on a fraction the paint pass then rounds a second
/// time. Paint applies [`Scene::scale`] to the context instead, which scales
/// the whole drawing including its rounding.
const LAYOUT_SCALE: f32 = 1.0;

/// Where every node ended up.
///
/// **`#[non_exhaustive]` because a field was just added to it.** `insets`
/// arrived after `rects` and `baselines`, which is this struct demonstrating
/// that it grows -- and the window to say so without breaking anyone is now:
/// `meo-canvas-core` is public API of a crate `just release-crate` publishes,
/// and nothing of it is on crates.io yet. The day after the first publish this
/// costs a major version.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct LayoutResult {
    /// Absolute rectangle per node, in logical pixels at scale 1.
    pub rects: HashMap<NodeId, Rect>,
    /// How far a node's own content sits inside that rectangle: its border
    /// plus its padding, per edge.
    ///
    /// **Kept rather than re-derived, and that is the whole reason it is
    /// here.** Percentage padding resolves against the containing block's
    /// width, so working it out at paint time reimplements a rule layout has
    /// already applied -- and two implementations of one rule are two answers
    /// waiting to disagree. taffy computes both and hands them back with the
    /// rectangle; this stops throwing them away.
    pub insets: HashMap<NodeId, Sides<f32>>,
    /// Distance from a measured leaf's top edge to its first baseline.
    ///
    /// Only text has one, and only because [`crate::measure`] computed it on
    /// the way past: taffy's measure closure returns a size and discards
    /// everything else, so a baseline that is not caught here is a second
    /// shaping pass at paint time.
    pub baselines: HashMap<NodeId, f32>,
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

    /// The rectangle a node's own content sits in: its box, less its border
    /// and padding.
    ///
    /// **This is where replaced content goes**, which is what CSS says and
    /// what Chrome does — an `<img>` with an 8px border or 8px of padding puts
    /// its picture 8px in on every edge, and with both, 16. Text and child
    /// boxes already land here; the image path was the one drawing into the
    /// box itself.
    ///
    /// Never larger than the box and never inverted: an inset wider than the
    /// box it insets leaves an empty rectangle at the box's centre rather than
    /// a negative one.
    #[must_use]
    pub fn content(&self, node: NodeId) -> Option<Rect> {
        let rect = self.get(node)?;
        let inset = self.insets.get(&node).copied().unwrap_or(Sides {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        });
        let width = (rect.size.width - inset.left - inset.right).max(0.0);
        let height = (rect.size.height - inset.top - inset.bottom).max(0.0);
        Some(Rect {
            origin: meo_canvas_scene::Point {
                x: rect.origin.x + inset.left.min(rect.size.width),
                y: rect.origin.y + inset.top.min(rect.size.height),
            },
            size: Size::new(width, height),
        })
    }

    /// The first baseline computed for `node`, or `None` if it has none.
    #[must_use]
    pub fn baseline(&self, node: NodeId) -> Option<f32> {
        self.baselines.get(&node).copied()
    }
}

/// Builds a taffy tree for one page, solves it, and discards the tree.
///
/// `page` is a [`Scene::pages`] entry. A scene carrying several pages is solved
/// one page per call, so the tree for a page is built, read and freed before
/// the next page allocates one -- which is what keeps a sixty-frame sequence
/// holding one page's layout rather than sixty.
///
/// Generic over the measurer rather than taking `&mut dyn Measure`, with
/// `?Sized` so that a caller who already holds a trait object can still pass
/// it. `measure` is called several times per leaf per pass, and the generic
/// form lets those calls devirtualise for a caller who knows the type; the
/// `?Sized` bound means that choice costs the trait-object caller nothing.
///
/// # Errors
///
/// Returns [`Error::Layout`] when `page` is not a node of `scene`, when the
/// scene's tree is malformed, or when taffy refuses the tree it is handed.
/// Measuring cannot fail -- see [`Measure`] for why that is taffy's constraint
/// rather than a simplification.
pub fn solve<M>(
    scene: &Scene,
    page: NodeId,
    measure: &mut M,
) -> Result<LayoutResult, Error>
where
    M: Measure + ?Sized,
{
    let mut tree: taffy::TaffyTree<NodeId> =
        taffy::TaffyTree::with_capacity(scene.nodes.len());

    // Only taffy-to-scene is kept. The reverse direction exists while a parent
    // is built, as the `Vec` its children were just created into, and nothing
    // after the build asks which taffy node a scene node became.
    let mut to_scene: HashMap<taffy::NodeId, NodeId> = HashMap::new();

    // Out-of-flow nodes are attached to the box that contains them rather than
    // to the parent they sit under in the scene. See `build`. Anything still
    // unclaimed at the top belongs to the page, which is the initial containing
    // block and the last chance to be one.
    let mut orphans = Vec::new();
    // Fixed boxes no transform captured. They belong to the page, which is the
    // viewport they resolve against.
    let mut viewport = Vec::new();
    let root = build(
        scene,
        page,
        &mut tree,
        &mut to_scene,
        &mut orphans,
        &mut viewport,
        // **The page's own height is definite unless the caller asked for a
        // content height**, in which case the page is as tall as what is in it
        // and a percentage against it has nothing to resolve against -- the
        // same condition as any other content-sized box, one level up.
        //
        // The page has no parent, so both answers are the same one: nothing
        // above it is content-sized.
        Definite {
            parent: !scene.content_height,
            own: !scene.content_height,
        },
    )?;
    for node in viewport.into_iter().chain(orphans) {
        tree.add_child(root, node)
            .map_err(|error| Error::Layout(error.to_string()))?;
    }
    pin_page_root(scene, page, root, &mut tree)?;

    // A content height is solved rather than stated, so the height axis is
    // offered `MaxContent` instead of a number. The circularity that stops a
    // *width* being derived this way does not reach the height: text breaks
    // into lines against the width, so the width has to be known before
    // anything can be measured, while the height is only ever a consequence of
    // that measuring.
    let available = taffy::Size {
        width: taffy::AvailableSpace::Definite(scene.size.width * LAYOUT_SCALE),
        height: if scene.content_height {
            taffy::AvailableSpace::MaxContent
        } else {
            taffy::AvailableSpace::Definite(scene.size.height * LAYOUT_SCALE)
        },
    };

    // Baselines are collected during the solve because this closure is the only
    // place a measurer runs. They go two ways from here: into taffy's
    // `LayoutOutput`, which is what `align-items: baseline` reads, and into the
    // result, which is what places glyphs at paint time.
    let mut baselines: HashMap<NodeId, f32> = HashMap::new();

    tree.compute_layout_with_measure(
        root,
        available,
        |inputs, _taffy_node, context, style| {
            // A node with no context is a container taffy sizes from its
            // children; only the leaves built with `new_leaf_with_context`
            // reach the measurer.
            let node = context.map(|context| *context);

            // `compute_leaf_layout` turns a measured extent into the leaf's
            // full result: it applies the node's own padding, border, box
            // sizing, aspect ratio and min-max clamps, and reports the
            // scrollable overflow. taffy hands the whole `LayoutOutput` to
            // this closure and builds none of it, so calling the helper is
            // what keeps a measured leaf sized the way every other leaf in
            // the tree is.
            //
            // The calc resolver answers zero because `calc` is off: with no
            // way to build a `calc` length, nothing can reach the resolver
            // and any value it returned would be unobservable.
            let mut first_baseline = None;
            let mut output = taffy::compute_leaf_layout(
                inputs,
                style,
                |_, _| 0.0,
                |known, space| {
                    let Some(node) = node else {
                        return taffy::Size::ZERO;
                    };

                    let measured = measure.measure(
                        node,
                        (known.width, known.height),
                        (to_available(space.width), to_available(space.height)),
                    );

                    if let Some(baseline) = measured.first_baseline {
                        baselines.insert(node, baseline);
                        first_baseline = Some(baseline);
                    }

                    // Measured text is a used length like any other, so it
                    // enters the grid at the same boundary the styled lengths
                    // do.
                    taffy::Size {
                        width: contains(measured.size.width),
                        height: contains(measured.size.height),
                    }
                },
            );

            // A measurer works in the content box, and CSS measures a flex
            // item's baseline from its **border box**, so the leaf's own top
            // padding and border are part of the answer. Text with padding
            // aligns a hair low without this, which is the kind of wrong that
            // reads as a font metric.
            //
            // Percentages resolve against the containing block's inline size
            // in both edges, which is CSS's rule rather than a simplification.
            output.baselines =
                taffy::Baselines::from_first(first_baseline.map(|baseline| {
                    let top = style
                        .padding
                        .top
                        .resolve_or_zero(inputs.parent_size.width, |_, _| 0.0)
                        + style.border.top.resolve_or_zero(
                            inputs.parent_size.width,
                            |_, _| 0.0,
                        );
                    baseline + top
                }));
            output
        },
    )
    .map_err(|error| Error::Layout(error.to_string()))?;

    let mut rects = HashMap::with_capacity(to_scene.len());
    let mut insets = HashMap::with_capacity(to_scene.len());
    collect(&tree, root, &to_scene, 0.0, 0.0, &mut rects, &mut insets)?;
    bottom_align_reversed_wraps(scene, page, &mut rects);

    Ok(LayoutResult {
        rects,
        insets,
        baselines,
    })
}

/// Moves an overflowing `wrap-reverse` line stack to the bottom of its box.
///
/// # The one row of the wrap table we answer differently
///
/// `flex-wrap: wrap-reverse` reverses the cross axis, so its lines are packed
/// from the **bottom**. taffy reverses the line order and then packs the stack
/// from the top whenever it overflows, which is visible only when it does:
/// six 28x44 children in an 88x56 box give lines at `y = 0` and `44` here and
/// `y = -32` and `12` in Chrome. Both reverse the stack; only Chrome puts the
/// last line's bottom on the box's bottom edge and lets the first hang off the
/// top.
///
/// **It is a defensible reading of the specification rather than a taffy
/// bug.** css-align-3 says a distributed alignment that overflows falls back
/// to a positional one with *safe* semantics, and safe alignment falls back to
/// `start` — and taffy takes `start` as the physical start, so the reversal
/// stops applying at exactly the moment it would push content out of the box.
/// Chrome keeps the reversal. The browser is the baseline for behaviour, so
/// Chrome wins and this shifts the stack after the solve.
///
/// # Why after, and why only in-flow children
///
/// taffy has no style that asks for this: `FlexStart` would lose the stretch
/// when the lines *do* fit, and the safe fallback is applied inside the
/// algorithm rather than chosen by a keyword. So the correction is a shift of
/// the solved rectangles.
///
/// Out-of-flow children are left where they are. An absolute box resolves
/// against its containing block's padding box, which this does not move; only
/// the flow the wrap arranged is out of place.
fn bottom_align_reversed_wraps(
    scene: &Scene,
    page: NodeId,
    rects: &mut HashMap<NodeId, Rect>,
) {
    let mut pending = vec![page];
    while let Some(id) = pending.pop() {
        let Some(node) = scene.get(id) else { continue };
        pending.extend(node.children.iter().copied());

        if !matches!(node.layout.flex_wrap, FlexWrap::WrapReverse)
            || !matches!(node.layout.display, Display::Flex)
        {
            continue;
        }
        let Some(rect) = rects.get(&id).copied() else {
            continue;
        };

        let flow: Vec<NodeId> = node
            .children
            .iter()
            .copied()
            .filter(|child| {
                scene.get(*child).is_some_and(|child| {
                    matches!(
                        child.layout.position_type,
                        PositionType::Static
                            | PositionType::Relative
                            | PositionType::Sticky
                    )
                })
            })
            .collect();
        let bottom = flow
            .iter()
            .filter_map(|child| rects.get(child))
            .map(Rect::bottom)
            .fold(f32::NEG_INFINITY, f32::max);

        // The content box's own bottom: the border box less the edges taffy
        // took off before it placed anything.
        // A percentage padding resolves against the containing block's
        // width, which for this node's own padding is its own width.
        let padding = match node.layout.padding.bottom {
            Length::Points(points) => points,
            Length::Percent(fraction) => fraction * rect.size.width,
        };
        // The used width, as everywhere else: this inset has to agree with
        // the room taffy reserved, or the stack lands on the wrong edge.
        let inset = used_border_width(node.layout.border.bottom) + padding;
        let content_bottom = rect.bottom() - inset;
        let shift = content_bottom - bottom;
        if shift >= 0.0 {
            continue;
        }

        // Every in-flow child, and everything inside it, since the rectangles
        // are absolute.
        let mut subtree: Vec<NodeId> = flow;
        while let Some(moving) = subtree.pop() {
            if let Some(rect) = rects.get_mut(&moving) {
                rect.origin.y += shift;
            }
            if let Some(node) = scene.get(moving) {
                subtree.extend(node.children.iter().copied());
            }
        }
    }
}

/// Gives the page root the scene's extent on any axis it leaves to content.
///
/// A page is the canvas, so a root that sized to its content would put the
/// layout in a box smaller than the surface it is drawn on: a percentage width
/// beneath it would resolve against the content rather than against the canvas,
/// and a `justify-content: center` would centre within the content it is
/// centring. Every other node keeps `Auto` as written, because for them content
/// sizing is the CSS behaviour a caller asked for.
///
/// An explicit size on the page root is honoured, so a caller who wants a page
/// smaller than the surface can still say so.
/// **Two answers, one level apart.**
///
/// `parent` is what this node's own percentages resolve against; `own` is what
/// its children's do. Threading a single value for both was wrong by exactly
/// one level -- a percentage on a child of a content-sized box survived,
/// because the *child* had a declared height, which is not the question being
/// asked.
#[derive(Debug, Clone, Copy)]
struct Definite {
    /// Whether the containing block's height is definite.
    parent: bool,
    /// Whether this node's height is, for the children inside it.
    own: bool,
}

/// Whether a child's height is one a percentage inside it can resolve against.
///
/// Definite means the number is known before the child's own contents are laid
/// out. A declared length is definite; a percentage is definite exactly when
/// the box it resolves against is, which is why this is threaded down the tree
/// rather than read off a single node.
///
/// **`auto` counts as definite in the two cases flex layout settles it.** An
/// item stretched across a row takes the line's cross size, and Chrome
/// resolves a percentage against that. An item that `flex-grow`s in a column
/// takes the line's remaining space, and **Chrome does not**: measured, a
/// `min-height: 200%` child of a `flex-grow: 1` box inside a 120-tall column
/// is 20 in Chrome and 240 here.
///
/// That second case is a deliberate divergence rather than an oversight.
/// `chart.ts` sizes every bar as a percentage of a `flexGrow: 1` plot area, so
/// the browser's rule would leave every bar at nothing -- which is exactly
/// what an earlier version of this check did, and what the chart render tests
/// caught. Matching Chrome there means changing how charts are built, which is
/// a larger change than this one and belongs on its own.
///
/// A `min-height` is deliberately not enough on its own: it bounds the height
/// from below and leaves it content-sized above the bound, so the number is
/// still not known until the children are laid out.
fn child_height_is_definite(
    parent: &meo_canvas_scene::node::Node,
    child: &meo_canvas_scene::node::Node,
    parent_is_definite: bool,
) -> bool {
    match child.layout.size.1 {
        Dimension::Points(_) => true,
        Dimension::Percent(_) => parent_is_definite,
        Dimension::Auto => parent_is_definite && flex_settles_it(parent, child),
    }
}

/// Whether flex layout gives this child a height its own contents did not.
fn flex_settles_it(
    parent: &meo_canvas_scene::node::Node,
    child: &meo_canvas_scene::node::Node,
) -> bool {
    // **An out-of-flow box is not a flex item.** Neither `align-items` nor a
    // grow factor reaches it, so an `auto` height is its content's height and
    // a percentage inside it has nothing to resolve against. Measured: a
    // `min-height: 200%` child of an absolutely positioned, content-sized box
    // is 20 in Chrome, not 40.
    if matches!(
        child.layout.position_type,
        PositionType::Absolute | PositionType::Fixed
    ) {
        return false;
    }
    match parent.layout.flex_direction {
        FlexDirection::Column | FlexDirection::ColumnReverse => {
            child.layout.flex_grow > 0.0
        }
        FlexDirection::Row | FlexDirection::RowReverse => {
            // The cross axis, where the default is to stretch: a child with no
            // height of its own takes the line's.
            matches!(
                child.layout.align_self.or(parent.layout.align_items),
                Some(Align::Stretch) | None
            )
        }
    }
}

fn pin_page_root(
    scene: &Scene,
    page: NodeId,
    root: taffy::NodeId,
    tree: &mut taffy::TaffyTree<NodeId>,
) -> Result<(), Error> {
    let source = scene.get(page).ok_or_else(|| {
        Error::Layout(format!("node {} is not in the scene", page.get()))
    })?;

    let mut style = to_taffy_style(&source.layout);
    if style.size.width.is_auto() {
        style.size.width =
            taffy::Dimension::length(scene.size.width * LAYOUT_SCALE);
    }
    if style.size.height.is_auto() {
        if scene.content_height {
            // Left automatic on purpose: pinning it here is exactly what makes
            // a page the height it was told rather than the height of what is
            // in it. `size.height` becomes the floor instead, which is what a
            // caller asking for "at least this tall" means.
            style.min_size.height = taffy::LengthPercentageAuto::length(
                scene.size.height * LAYOUT_SCALE,
            );
        } else {
            style.size.height =
                taffy::Dimension::length(scene.size.height * LAYOUT_SCALE);
        }
    }

    tree.set_style(root, style)
        .map_err(|error| Error::Layout(error.to_string()))
}

/// Creates the taffy node for `node` and, depth-first, for its children.
///
/// A `Display::None` subtree is not built at all rather than built and hidden.
/// The scene defines the node and its descendants as neither laid out nor
/// drawn, and a node taffy never sees cannot contribute a rectangle that paint
/// would then have to know to skip.
/// Builds the taffy tree for a subtree, hoisting its `Fixed` nodes.
///
/// # Why a `Fixed` node is not a child of its parent here
///
/// CSS resolves `fixed` against the viewport, and a still render's viewport is
/// its page — there is nothing to scroll relative to, so the page is the whole
/// of it. taffy resolves an absolute child against **its parent** and has no
/// notion of a nearest positioned ancestor, so the containing block is decided
/// by where a node is attached rather than by anything in its style.
///
/// So a `Fixed` node is built, left out of its parent's children, and handed up
/// to be attached to the page root. It keeps its place in the *scene* tree,
/// which is what the painter walks: out of flow for layout, in place for paint
/// order and for style inheritance.
///
/// # And why an `Absolute` node is not one either
///
/// The same mechanism, stopping one rung lower. CSS resolves an absolute node
/// against its **nearest positioned ancestor**, skipping every static box in
/// between; taffy resolves it against its parent whatever that parent is. So an
/// absolute node is handed up too, and claimed by the first ancestor that is
/// positioned — the page root claiming whatever reaches the top, as the initial
/// containing block.
///
/// Measured before the change: a relative grandparent at `(20, 20)`, a static
/// parent at `(50, 50)`, and an absolute grandchild at inset zero landed at
/// `(50, 50)` where CSS puts it at `(20, 20)`.
///
/// v1 fixed the same defect in `d6bfe23` by giving a node that names no
/// position type Yoga's real `Static`, which stops it being a containing block.
/// taffy has no such value — its `Relative` is the in-flow default and every
/// node is a containing block for its absolute children — so the distinction
/// has to be made by where a node is attached rather than by what its style
/// says.
///
/// No sibling moves as a result. An out-of-flow child contributes nothing to
/// its parent's flow, so removing it from that parent's children changes
/// nothing the solver would have done with it.
/// Whether a node is a containing block for the **absolute** boxes beneath it.
///
/// Crate-internal rather than private because the painter needs the same
/// answer: a box is clipped by its containing block's `overflow`, so capture
/// and clip have to be one rule. Measured in Chrome as ten rows where they were
/// two -- a transformed clipper placed an out-of-flow child exactly right and
/// then drew it whole, because layout knew the transform captured it and paint
/// did not.
///
/// Positioned, or transformed. CSS Transforms 1 §3 makes any element with a
/// transform the containing block for its absolute and fixed descendants,
/// positioned or not -- measured in Chrome, where a static clipper carrying
/// `translateZ(0)` captures an absolute child at 50,20 that the same clipper
/// without it lets through to the outer box at 30,20.
///
/// **A fixed box is not decided by this**, and that is deliberate: it passes
/// every positioned ancestor and stops only at a transformed one, which is
/// what makes it fixed rather than absolute. The caller tests the transform on
/// its own for that list.
pub(crate) const fn is_containing_block(
    node: &meo_canvas_scene::node::Node,
) -> bool {
    !matches!(node.layout.position_type, PositionType::Static)
        || node.effects.transform.is_some()
}

fn build(
    scene: &Scene,
    node: NodeId,
    tree: &mut taffy::TaffyTree<NodeId>,
    to_scene: &mut HashMap<taffy::NodeId, NodeId>,
    orphans: &mut Vec<taffy::NodeId>,
    captive: &mut Vec<taffy::NodeId>,
    heights: Definite,
) -> Result<taffy::NodeId, Error> {
    let source = scene.get(node).ok_or_else(|| {
        Error::Layout(format!("node {} is not in the scene", node.get()))
    })?;

    let mut style = to_taffy_style(&source.layout);
    // **A percentage against an indefinite containing block resolves to
    // `auto`**, which for a size is no size and for a minimum or maximum is no
    // constraint. taffy resolves it against the parent's height whether or not
    // layout had definitely established one, so a child of a content-sized box
    // got a number where the browser gets nothing.
    //
    // Only the block axis: a shrink-to-fit box still has a definite inline
    // size to resolve against, and the six inline-axis percentages measured
    // against the same container agree with Chrome already.
    //
    // Asked of the scene's own values rather than of the converted ones,
    // because taffy's types do not answer "were you a percentage" and a
    // round-trip through them would be a second place to keep in step.
    if !heights.parent {
        if matches!(source.layout.size.1, Dimension::Percent(_)) {
            style.size.height = taffy::Dimension::auto();
        }
        if matches!(source.layout.min_size.1, Dimension::Percent(_)) {
            style.min_size.height = taffy::LengthPercentageAuto::auto();
        }
        if matches!(source.layout.max_size.1, Dimension::Percent(_)) {
            style.max_size.height = taffy::LengthPercentageAuto::auto();
        }
    }

    let mut children: Vec<taffy::NodeId> = Vec::new();
    // Absolute descendants from anywhere beneath here that have not yet met a
    // containing block. They become this node's children if it is one, and are
    // handed further up if it is not.
    let mut unclaimed: Vec<taffy::NodeId> = Vec::new();
    // Fixed descendants from beneath here, which only a transform captures.
    // They pass every positioned ancestor untouched -- a fixed box resolves
    // against the viewport, not against the nearest positioned box -- and stop
    // at the first transformed one.
    let mut captured: Vec<taffy::NodeId> = Vec::new();

    for child in &source.children {
        let Some(child_source) = scene.get(*child) else {
            // A dangling id is caught by `Scene::validate`; building it as a
            // leaf here would report the wrong error from the wrong pass.
            children.push(build(
                scene,
                *child,
                tree,
                to_scene,
                &mut unclaimed,
                &mut captured,
                // A dangling id has no style to read, so it inherits this
                // node's answers rather than being given ones of its own.
                Definite {
                    parent: heights.own,
                    own: heights.own,
                },
            )?);
            continue;
        };
        if child_source.layout.display == Display::None {
            continue;
        }

        let built = build(
            scene,
            *child,
            tree,
            to_scene,
            &mut unclaimed,
            &mut captured,
            Definite {
                parent: heights.own,
                own: child_height_is_definite(
                    source,
                    child_source,
                    heights.own,
                ),
            },
        )?;
        match child_source.layout.position_type {
            PositionType::Fixed => captured.push(built),
            PositionType::Absolute => unclaimed.push(built),
            PositionType::Static
            | PositionType::Relative
            | PositionType::Sticky => children.push(built),
        }
    }

    // A positioned box is a containing block, and **so is a transformed one**:
    // CSS Transforms 1 §3 makes any element with a transform the containing
    // block for its absolute *and* fixed descendants, positioned or not.
    // Measured in Chrome, where a static clipper carrying `translateZ(0)`
    // captures an absolute child that the same clipper without it lets through
    // to the outer box -- 50,20 against 30,20.
    //
    // Anything this node does not claim goes to whichever ancestor does.
    if is_containing_block(source) {
        children.append(&mut unclaimed);
    } else {
        orphans.append(&mut unclaimed);
    }
    // A fixed box passes a merely positioned ancestor untouched -- that is
    // what makes it fixed rather than absolute -- and stops only at a
    // transformed one.
    if source.effects.transform.is_some() {
        children.append(&mut captured);
    } else {
        captive.append(&mut captured);
    }

    // A childless node is given the measurer's context whatever it draws.
    // Layout does not know which kinds have an intrinsic size -- that is what
    // `measure` is for -- and the trait's own contract says a node the measurer
    // was never prepared for answers `MeasuredLeaf::EMPTY`. Deciding here would
    // put a second, disagreeing copy of that knowledge in the module that
    // deliberately holds none of it.
    let created = if children.is_empty() {
        tree.new_leaf_with_context(style, node)
    } else {
        tree.new_with_children(style, &children)
    }
    .map_err(|error| Error::Layout(error.to_string()))?;

    to_scene.insert(created, node);
    Ok(created)
}

/// Walks the solved tree, converting taffy's parent-relative locations into
/// absolute rectangles.
///
/// `parent_x` and `parent_y` are the accumulated origin. taffy reports a
/// location relative to the parent's content box, so the sum down the path is
/// the absolute position -- which is the form paint wants and the form taffy's
/// rounding was computed against.
fn collect(
    tree: &taffy::TaffyTree<NodeId>,
    node: taffy::NodeId,
    to_scene: &HashMap<taffy::NodeId, NodeId>,
    parent_x: f32,
    parent_y: f32,
    rects: &mut HashMap<NodeId, Rect>,
    insets: &mut HashMap<NodeId, Sides<f32>>,
) -> Result<(), Error> {
    let layout = tree
        .layout(node)
        .map_err(|error| Error::Layout(error.to_string()))?;

    let x = parent_x + layout.location.x;
    let y = parent_y + layout.location.y;

    if let Some(&scene_node) = to_scene.get(&node) {
        rects.insert(
            scene_node,
            Rect {
                origin: meo_canvas_scene::Point { x, y },
                size: Size::new(layout.size.width, layout.size.height),
            },
        );
        // Taken from the same `Layout` the rectangle came from, already
        // resolved. A percentage padding has a containing block behind it and
        // taffy has just used it; asking again here would be a second
        // implementation of that rule.
        insets.insert(
            scene_node,
            Sides {
                left: layout.border.left + layout.padding.left,
                right: layout.border.right + layout.padding.right,
                top: layout.border.top + layout.padding.top,
                bottom: layout.border.bottom + layout.padding.bottom,
            },
        );
    }

    let children = tree
        .children(node)
        .map_err(|error| Error::Layout(error.to_string()))?;
    for child in children {
        collect(tree, child, to_scene, x, y, rects, insets)?;
    }

    Ok(())
}

/// Translates one axis of taffy's offered space into this crate's vocabulary.
const fn to_available(space: taffy::AvailableSpace) -> Available {
    match space {
        taffy::AvailableSpace::Definite(extent) => Available::Definite(extent),
        taffy::AvailableSpace::MinContent => Available::MinContent,
        taffy::AvailableSpace::MaxContent => Available::MaxContent,
    }
}

/// Maps a scene node's layout style onto taffy's.
///
/// Every field the scene carries is written, none is left to taffy's default.
/// See the module documentation for why.
#[must_use]
pub fn to_taffy_style(layout: &LayoutStyle) -> taffy::Style {
    taffy::Style {
        display: to_display(layout.display),
        box_sizing: to_box_sizing(layout.box_sizing),
        direction: to_direction(layout.direction),
        overflow: taffy::Point {
            x: to_overflow(layout.overflow.0),
            y: to_overflow(layout.overflow.1),
        },
        position: to_position(layout.position_type),

        inset: to_taffy_inset(layout),
        size: taffy::Size {
            width: to_dimension(sized(layout.size.0)),
            height: to_dimension(sized(layout.size.1)),
        },
        min_size: taffy::Size {
            width: to_auto_length(sized(layout.min_size.0)),
            height: to_auto_length(sized(layout.min_size.1)),
        },
        max_size: taffy::Size {
            width: to_auto_length(sized(layout.max_size.0)),
            height: to_auto_length(sized(layout.max_size.1)),
        },
        aspect_ratio: layout.aspect_ratio.filter(|ratio| {
            // A ratio is a positive finite number or it is not a ratio.
            // Chrome drops `0`, `-2`, `NaN` and `Infinity` alike and keeps the
            // declared size; applying any of them abandons that size and
            // shrinks the box to its content.
            ratio.is_finite() && *ratio > 0.0
        }),

        margin: taffy::Rect {
            left: to_auto_length(margin(layout.margin.left)),
            right: to_auto_length(margin(layout.margin.right)),
            top: to_auto_length(margin(layout.margin.top)),
            bottom: to_auto_length(margin(layout.margin.bottom)),
        },
        padding: taffy::Rect {
            left: to_length(spacing(layout.padding.left)),
            right: to_length(spacing(layout.padding.right)),
            top: to_length(spacing(layout.padding.top)),
            bottom: to_length(spacing(layout.padding.bottom)),
        },
        // `snapped` is gone rather than composed: a used border width is a
        // whole number and a whole number is already on the grid.
        border: {
            let used = used_border(layout.border);
            taffy::Rect {
                left: taffy::LengthPercentage::length(used.left),
                right: taffy::LengthPercentage::length(used.right),
                top: taffy::LengthPercentage::length(used.top),
                bottom: taffy::LengthPercentage::length(used.bottom),
            }
        },

        align_items: layout.align_items.map(to_align_items),
        align_self: layout.align_self.map(to_align_items),
        align_content: layout.align_content.map(to_align_content),
        justify_content: layout.justify_content.map(to_justify),

        // The scene spells this `(row, column)`, following CSS's `gap`
        // shorthand; taffy spells it `(width, height)`, which is `(column,
        // row)`. The pair is swapped here rather than at either end, because
        // the two orders are each right in their own vocabulary and only the
        // crossing has to know.
        gap: taffy::Size {
            width: to_length(spacing(layout.gap.1)),
            height: to_length(spacing(layout.gap.0)),
        },

        flex_direction: to_flex_direction(layout.flex_direction),
        flex_wrap: to_flex_wrap(layout.flex_wrap),
        // A negative or non-finite factor is not a factor. Dropped, each to
        // its own initial value rather than to a shared one -- CSS's initial
        // `flex-grow` is 0 and its initial `flex-shrink` is 1, and a factor
        // that fell back to the wrong one would be a second defect wearing the
        // first one's repair.
        flex_grow: factor(layout.flex_grow, 0.0),
        flex_shrink: factor(layout.flex_shrink, 1.0),
        flex_basis: to_dimension(sized(layout.flex_basis)),

        grid_template_columns: layout
            .grid_template_columns
            .iter()
            .copied()
            .map(|size| {
                taffy::GridTemplateComponent::Single(to_track_sizing(size))
            })
            .collect(),
        grid_template_rows: layout
            .grid_template_rows
            .iter()
            .copied()
            .map(|size| {
                taffy::GridTemplateComponent::Single(to_track_sizing(size))
            })
            .collect(),
        grid_auto_columns: layout
            .grid_auto_columns
            .iter()
            .copied()
            .map(to_track_sizing)
            .collect(),
        grid_auto_rows: layout
            .grid_auto_rows
            .iter()
            .copied()
            .map(to_track_sizing)
            .collect(),
        grid_auto_flow: to_grid_auto_flow(layout.grid_auto_flow),
        grid_column: to_placement(layout.grid_column),
        grid_row: to_placement(layout.grid_row),

        // taffy carries these and the scene does not, so there is nothing to
        // translate: `float`, `clear`, `justify_items`, `justify_self`,
        // `text_align`, `scrollbar_width`, the named-line grid vectors, and the
        // table markers.
        ..taffy::Style::default()
    }
}

const fn to_display(display: Display) -> taffy::Display {
    match display {
        Display::Flex => taffy::Display::Flex,
        Display::Grid => taffy::Display::Grid,
        Display::Block => taffy::Display::Block,
        Display::None => taffy::Display::None,
    }
}

const fn to_box_sizing(sizing: BoxSizing) -> taffy::BoxSizing {
    match sizing {
        BoxSizing::BorderBox => taffy::BoxSizing::BorderBox,
        BoxSizing::ContentBox => taffy::BoxSizing::ContentBox,
    }
}

const fn to_direction(direction: Direction) -> taffy::Direction {
    match direction {
        Direction::Ltr => taffy::Direction::Ltr,
        Direction::Rtl => taffy::Direction::Rtl,
    }
}

const fn to_overflow(overflow: Overflow) -> taffy::Overflow {
    match overflow {
        Overflow::Visible => taffy::Overflow::Visible,
        Overflow::Hidden => taffy::Overflow::Hidden,
        Overflow::Scroll => taffy::Overflow::Scroll,
    }
}

/// The `inset` taffy is given, which is not always the one the node carries.
///
/// CSS's offset properties do not apply to a `position: static` element, and
/// taffy has no `Static`: its `Relative` honours an inset. So a static node's
/// inset is dropped here, and dropping it here rather than at the surface is
/// what makes it dropped for every surface at once.
///
/// Measured in Chrome rather than read off the specification, because the
/// specification's wording is about used values and the question is what a
/// browser draws: a static child given `top: 30px; left: 30px` sits at its flow
/// position in a block, a flex and a grid container alike -- the container's
/// layout mode does not enter into it, which is why this reads only the child.
fn to_taffy_inset(
    layout: &LayoutStyle,
) -> taffy::Rect<taffy::LengthPercentageAuto> {
    if matches!(layout.position_type, PositionType::Static) {
        return taffy::Rect::auto();
    }
    taffy::Rect {
        left: to_inset(layout.inset.left),
        right: to_inset(layout.inset.right),
        top: to_inset(layout.inset.top),
        bottom: to_inset(layout.inset.bottom),
    }
}

/// taffy has two positions where CSS has three.
///
/// `Static` and `Relative` both become taffy's `Relative`: both are placed by
/// the flow, and the two ways they differ -- whether `inset` moves the node and
/// whether `z_index` places it in its parent's stack -- are both settled on
/// this side. See [`to_taffy_inset`] and `stacks_by_z_index` in
/// [`crate::paint`].
const fn to_position(position: PositionType) -> taffy::Position {
    match position {
        // `Sticky` is `Relative` here and not by approximation: CSS defines it
        // against a scroll position and a still page has none, so Chrome itself
        // draws the two identically at the only offset this renderer has.
        PositionType::Static
        | PositionType::Relative
        | PositionType::Sticky => taffy::Position::Relative,
        // `Fixed` is out of flow like `Absolute`; **which box it resolves
        // against is not settled here** but by where the node sits in the tree
        // handed to taffy. See `fixed_containing_block`.
        PositionType::Absolute | PositionType::Fixed => {
            taffy::Position::Absolute
        }
    }
}

const fn to_flex_direction(direction: FlexDirection) -> taffy::FlexDirection {
    match direction {
        FlexDirection::Row => taffy::FlexDirection::Row,
        FlexDirection::RowReverse => taffy::FlexDirection::RowReverse,
        FlexDirection::Column => taffy::FlexDirection::Column,
        FlexDirection::ColumnReverse => taffy::FlexDirection::ColumnReverse,
    }
}

const fn to_flex_wrap(wrap: FlexWrap) -> taffy::FlexWrap {
    match wrap {
        FlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
        FlexWrap::Wrap => taffy::FlexWrap::Wrap,
        FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
    }
}

const fn to_grid_auto_flow(flow: GridAutoFlow) -> taffy::GridAutoFlow {
    match flow {
        GridAutoFlow::Row => taffy::GridAutoFlow::Row,
        GridAutoFlow::Column => taffy::GridAutoFlow::Column,
        GridAutoFlow::RowDense => taffy::GridAutoFlow::RowDense,
        GridAutoFlow::ColumnDense => taffy::GridAutoFlow::ColumnDense,
    }
}

/// Cross-axis placement of an item.
///
/// `SpaceBetween` and `SpaceAround` reach here because the scene carries one
/// [`Align`] for `align-items`, `align-self` and `align-content` together, and
/// those two belong only to the last of the three. CSS discards a value a
/// property does not define and the property keeps its initial value, which for
/// `align-items` is `stretch`; that is what taffy's own default resolves to, so
/// discarding here and discarding in a browser produce the same layout.
const fn to_align_items(align: Align) -> taffy::AlignItems {
    match align {
        Align::FlexStart => taffy::AlignItems::FLEX_START,
        Align::FlexEnd => taffy::AlignItems::FLEX_END,
        Align::Center => taffy::AlignItems::CENTER,
        Align::Stretch
        | Align::SpaceBetween
        | Align::SpaceAround
        | Align::SpaceEvenly => taffy::AlignItems::STRETCH,
        Align::Baseline => taffy::AlignItems::BASELINE,
    }
}

/// Cross-axis distribution of wrapped lines.
///
/// `Baseline` reaches here for the mirror-image reason [`to_align_items`]
/// receives the `Space*` pair: one enum serves three properties and
/// `align-content: baseline` is not one of them. It is discarded to `stretch`,
/// the property's initial value.
const fn to_align_content(align: Align) -> taffy::AlignContent {
    match align {
        Align::FlexStart => taffy::AlignContent::FLEX_START,
        Align::FlexEnd => taffy::AlignContent::FLEX_END,
        Align::Center => taffy::AlignContent::CENTER,
        Align::Stretch | Align::Baseline => taffy::AlignContent::STRETCH,
        Align::SpaceBetween => taffy::AlignContent::SPACE_BETWEEN,
        Align::SpaceAround => taffy::AlignContent::SPACE_AROUND,
        Align::SpaceEvenly => taffy::AlignContent::SPACE_EVENLY,
    }
}

const fn to_justify(justify: Justify) -> taffy::JustifyContent {
    match justify {
        Justify::FlexStart => taffy::JustifyContent::FLEX_START,
        Justify::FlexEnd => taffy::JustifyContent::FLEX_END,
        Justify::Center => taffy::JustifyContent::CENTER,
        Justify::SpaceBetween => taffy::JustifyContent::SPACE_BETWEEN,
        Justify::SpaceAround => taffy::JustifyContent::SPACE_AROUND,
        Justify::SpaceEvenly => taffy::JustifyContent::SPACE_EVENLY,
    }
}

/// Chrome's layout grid: a length is held in sixty-fourths of a pixel.
///
/// # Why the snap happens here rather than at the end
///
/// **Chrome snaps a used length into this grid once and then accumulates the
/// snapped values exactly.** We accumulated the exact values and rounded the
/// total, and the two part company only when an accumulated coordinate lands
/// on a half: five boxes of `10.3` sum to exactly `51.5` and a tie rounds up,
/// where Chrome snaps to `10.296875` first, reaches `51.484375`, and rounds
/// down. **Chrome never sees the tie, because the snap has already nudged the
/// value below it.**
///
/// So the fix is not to round differently at the end -- that only moves which
/// inputs are wrong -- but to **snap before accumulating, so the tie never
/// forms.**
///
/// # Floor rather than round
///
/// `10.3 x 64` is `659.2`, and Chrome's `10.296875` is `659 / 64`. Floor and
/// round agree there and part on a value whose sixty-fourths land at or past a
/// half; the browser's own conversion is `LayoutUnit`, which truncates toward
/// negative infinity. **Every case measured against Chrome so far agrees with
/// the floor**, and a value that would tell them apart is named in
/// `rounding_drift.rs` as the thing to measure if this is ever in doubt.
const LAYOUT_GRID: f32 = 64.0;

/// One length, snapped into [`LAYOUT_GRID`].
///
/// Percentages are not snapped: they resolve against a containing block this
/// stage has not computed yet, and Chrome snaps the **resolved** value.
/// Snapping the fraction would quantise a ratio rather than a length.
/// A measured content size, snapped **outward** onto [`LAYOUT_GRID`].
///
/// **A styled length is a request and a measured size is a claim.** Flooring a
/// request is right -- Chrome's `LayoutUnit` truncates and every case measured
/// against it agrees. Flooring a *measurement* says the content fits in a box
/// it does not fit in, by up to a sixty-fourth of a pixel, and the next pass
/// re-measures at that width and wraps the last word out of the line.
fn contains(points: f32) -> f32 {
    if points.is_finite() {
        (points * LAYOUT_GRID).ceil() / LAYOUT_GRID
    } else {
        points
    }
}

fn snapped(points: f32) -> f32 {
    if points.is_finite() {
        (points * LAYOUT_GRID).floor() / LAYOUT_GRID
    } else {
        points
    }
}

fn to_length(length: Length) -> taffy::LengthPercentage {
    match length {
        Length::Points(points) => {
            taffy::LengthPercentage::length(snapped(points))
        }
        Length::Percent(fraction) => taffy::LengthPercentage::percent(fraction),
    }
}

fn to_dimension(dimension: Dimension) -> taffy::Dimension {
    match dimension {
        Dimension::Auto => taffy::Dimension::auto(),
        Dimension::Points(points) => taffy::Dimension::length(snapped(points)),
        Dimension::Percent(fraction) => taffy::Dimension::percent(fraction),
    }
}

/// A border width as CSS *uses* it, which is not what the author wrote.
///
/// **Chrome resolves a border width to an integer at used-value time**, and
/// layout sees the resolved value rather than the declared one:
/// `getComputedStyle` reports the integer and the border box grows by the
/// integer, so `border: 3.5px` and `border: 3px` render identically. Measured
/// across `0.1`, `0.4`, `0.5`, `0.9`, `1.4`, `1.5`, `1.6`, `2.5`, `3.4`,
/// `3.5`, `3.6` and `3.9`, at **both** device scales with the same answers --
/// so it is a CSS-pixel rule and not a device-pixel one.
///
/// **It floors rather than rounds.** `1.6` gives `1` and `3.9` gives `3`,
/// which no rounding mode produces; `2.5` gives `2` and `3.5` gives `3`, where
/// half-to-even would give `2` and `4`.
///
/// **The minimum is the part that would bite.** Chrome draws a `0.1px` border
/// as `1px`, so a bare `floor` makes every hairline vanish -- a visible
/// regression rather than a subtle one, which is why the `0.1` row is pinned
/// beside the `3.5` one.
///
/// **Derived rather than stored.** The scene keeps what the author wrote, as
/// it does for a percentage line height. This is the one place the used value
/// is computed, and both readers -- layout here and the painter -- go through
/// it, so the two cannot drift apart.
pub(crate) fn used_border(border: Sides<f32>) -> Sides<f32> {
    Sides {
        left: used_border_width(border.left),
        right: used_border_width(border.right),
        top: used_border_width(border.top),
        bottom: used_border_width(border.bottom),
    }
}

/// One edge of [`used_border`].
fn used_border_width(width: f32) -> f32 {
    // `NaN <= 0.0` is false, so a non-finite width would otherwise reach
    // `floor().max(1.0)` and stay non-finite all the way into the border box.
    if !width.is_finite() {
        return 0.0;
    }
    if width <= 0.0 {
        0.0
    } else {
        width.floor().max(1.0)
    }
}

/// A length for the three fields taffy spells `LengthPercentageAuto`, where
/// `auto` means something different in each: a margin that absorbs free space,
/// the automatic minimum size a flex item takes from its content, and the
/// absence of a maximum. All three are the scene's `Dimension::Auto`, and the
/// field decides which one it is.
fn to_auto_length(dimension: Dimension) -> taffy::LengthPercentageAuto {
    match dimension {
        Dimension::Auto => taffy::LengthPercentageAuto::auto(),
        Dimension::Points(points) => {
            taffy::LengthPercentageAuto::length(snapped(points))
        }
        Dimension::Percent(fraction) => {
            taffy::LengthPercentageAuto::percent(fraction)
        }
    }
}

/// A size, min-size, max-size or flex basis with an unusable value dropped.
///
/// # Why the layout pass rather than the boundary a value arrives at
///
/// **The two public surfaces reach here by different doors.** A JavaScript
/// caller's number crosses the wire and is decoded by
/// `meo_canvas_scene::codec`; a Rust caller writes `Length::Points(f32)` and
/// never touches the codec at all. A check placed at either door repairs one
/// surface and leaves the other exactly as it was -- measured, not assumed:
/// the same 64 bad-value cells fail identically through both.
///
/// So the check is here, where the two doors meet, which is also where the
/// browser puts it: an invalid declaration is dropped when the property is
/// used, not when the stylesheet is parsed.
///
/// # What "unusable" means, per property
///
/// Chrome's rule is one rule -- an invalid value is dropped and the property
/// takes its unset value -- and the work is in knowing what is invalid where.
/// **A negative `margin` and a negative inset are valid CSS and are kept.**
/// Everything else measured against Chrome 151 refuses a negative, and every
/// property refuses a non-finite number.
const fn sized(dimension: Dimension) -> Dimension {
    match dimension {
        Dimension::Points(points) if !points.is_finite() || points < 0.0 => {
            Dimension::Auto
        }
        Dimension::Percent(fraction)
            if !fraction.is_finite() || fraction < 0.0 =>
        {
            Dimension::Auto
        }
        kept => kept,
    }
}

/// A margin edge with an unusable value dropped.
///
/// **A negative margin is valid CSS and survives**: it pulls the box outside
/// its parent, which is what it is for. Only a non-finite value is dropped,
/// and it falls back to zero rather than to `auto` -- `auto` on a margin
/// absorbs free space, so dropping to it would centre a box that asked for
/// nothing of the kind.
const fn margin(dimension: Dimension) -> Dimension {
    match dimension {
        Dimension::Points(points) if !points.is_finite() => {
            Dimension::Points(0.0)
        }
        Dimension::Percent(fraction) if !fraction.is_finite() => {
            Dimension::Points(0.0)
        }
        kept => kept,
    }
}

/// A padding or gap length with an unusable value dropped.
///
/// Both refuse negatives in CSS and both have an initial value of zero, so
/// there is one fallback rather than a choice.
const fn spacing(length: Length) -> Length {
    match length {
        Length::Points(points) if !points.is_finite() || points < 0.0 => {
            Length::ZERO
        }
        Length::Percent(fraction)
            if !fraction.is_finite() || fraction < 0.0 =>
        {
            Length::ZERO
        }
        kept => kept,
    }
}

/// A flex factor with an unusable value dropped to the initial value given.
///
/// The caller passes the initial value because CSS's differ: `flex-grow`
/// starts at 0 and `flex-shrink` at 1.
fn factor(value: f32, initial: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        initial
    }
}

/// An inset edge with an unusable value dropped.
///
/// **A negative inset is valid CSS**, the same as a negative margin: it moves
/// the box the other way. A non-finite one becomes absence, which for an inset
/// is `auto` -- the edge taffy places rather than an edge pinned anywhere.
const fn inset(edge: Option<Length>) -> Option<Length> {
    match edge {
        Some(Length::Points(points)) if !points.is_finite() => None,
        Some(Length::Percent(fraction)) if !fraction.is_finite() => None,
        kept => kept,
    }
}

/// One `inset` edge, where absence is `auto` -- the edge taffy is free to place
/// rather than an edge pinned at zero.
fn to_inset(edge: Option<Length>) -> taffy::LengthPercentageAuto {
    match inset(edge) {
        None => taffy::LengthPercentageAuto::auto(),
        Some(Length::Points(points)) => {
            taffy::LengthPercentageAuto::length(snapped(points))
        }
        Some(Length::Percent(fraction)) => {
            taffy::LengthPercentageAuto::percent(fraction)
        }
    }
}

/// A track's sizing function, as the `minmax()` pair CSS defines it to be.
///
/// The template entries wrap this in `GridTemplateComponent::Single` at the
/// call site rather than through a helper of their own, because that type is
/// generic over taffy's name type and the alias supplying its default is
/// `pub(crate)` -- a helper would have to name a type this crate cannot see, so
/// inference names it instead. The scene has no `repeat()` either way: a
/// template is a list of tracks, and a surface offering the shorthand expands
/// it before encoding.
///
/// `auto` is `minmax(auto, auto)`, a fixed length or percentage is that value
/// on both bounds, and `<n>fr` is `minmax(auto, <n>fr)` -- a flexible track has
/// no fixed minimum, which is what lets it shrink below its share when the
/// fixed tracks take the room.
#[expect(
    clippy::match_same_arms,
    reason = "the `auto` arm and the `#[non_exhaustive]` arm agree today and \
              mean different things: one is the track CSS names, the other is \
              a track this build has never heard of. Merging them would hide \
              which is which the first time they stop agreeing."
)]
const fn to_track_sizing(size: TrackSize) -> taffy::TrackSizingFunction {
    match size {
        TrackSize::Auto => taffy::TrackSizingFunction {
            min: taffy::MinTrackSizingFunction::auto(),
            max: taffy::MaxTrackSizingFunction::auto(),
        },
        TrackSize::Points(points) => taffy::TrackSizingFunction {
            min: taffy::MinTrackSizingFunction::length(points),
            max: taffy::MaxTrackSizingFunction::length(points),
        },
        TrackSize::Percent(fraction) => taffy::TrackSizingFunction {
            min: taffy::MinTrackSizingFunction::percent(fraction),
            max: taffy::MaxTrackSizingFunction::percent(fraction),
        },
        TrackSize::Fraction(share) => taffy::TrackSizingFunction {
            min: taffy::MinTrackSizingFunction::auto(),
            max: taffy::MaxTrackSizingFunction::fr(share),
        },
        // `TrackSize` is `#[non_exhaustive]`, so this arm is what a track this
        // build does not know becomes. `auto` is the neutral one: a track that
        // takes what it is given rather than one that claims a size.
        _ => taffy::TrackSizingFunction {
            min: taffy::MinTrackSizingFunction::auto(),
            max: taffy::MaxTrackSizingFunction::auto(),
        },
    }
}

/// A grid item's placement on one axis.
///
/// The scene carries a start line and a span, both optional, which is the
/// `<line> / span <n>` form. An absent start is auto-placement, and an absent
/// span is CSS's default of one track.
fn to_placement(placement: GridPlacement) -> taffy::Line<taffy::GridPlacement> {
    let start = placement.start.map_or(taffy::GridPlacement::Auto, |line| {
        taffy::GridPlacement::Line(line.into())
    });
    let end = placement
        .span
        .map_or(taffy::GridPlacement::Auto, taffy::GridPlacement::Span);

    taffy::Line { start, end }
}

#[cfg(test)]
mod tests {
    use super::snapped;

    /// Chrome's grid: sixty-fourths of a CSS pixel.
    const GRID: f32 = 64.0;

    #[test]
    fn a_length_is_truncated_into_sixty_fourths_rather_than_rounded() {
        // **Here rather than in `tests/rounding_drift.rs` because the snap is
        // the only place the fractional grid is observable, and it is not
        // public.** taffy rounds the solved tree to whole pixels, so nothing
        // outside this crate can read a sixty-fourth back: a box of
        // `10.0234375` is `10` in a `LayoutResult` and `10.015625` in
        // `getBoundingClientRect`, and **those are not the same measurement**
        // -- a painted edge against a layout rect. Asserting one against the
        // other reads exactly like a defect and is a category error.
        //
        // Chrome measured by MC Main through Playwright,
        // `getBoundingClientRect().height` on a single box, `box-sizing:
        // border-box`, margins and padding zeroed.
        //
        // **`10.0234375` is the row that settles the rule**: exactly `641.5`
        // sixty-fourths, an exact tie, and Chrome takes `641`. Not half-up,
        // not half-even -- **truncation, which no rounding mode reproduces.**
        // One row excluding every mode at once.
        //
        // `7.999` floors to `7.984375` where any rounding gives a clean `8`,
        // so a reader who suspects the snap is cosmetic can see that it is
        // not. The last three agree under either rule and are here as the
        // control: they are what the accumulation tests in `rounding_drift`
        // rest on, and **they cannot tell floor from round**, which is why
        // the three above them exist.
        for (length, chrome) in [
            (10.008_f32, 10.0_f32),
            (10.023_437_5, 10.015_625),
            (7.999, 7.984_375),
            (10.02, 10.015_625),
            (3.3, 3.296_875),
            (10.3, 10.296_875),
        ] {
            let ours = snapped(length);
            assert!(
                (ours - chrome).abs() < f32::EPSILON,
                "{length} snaps to {ours} where Chrome makes it {chrome}"
            );
            assert!(
                (ours * GRID).fract().abs() < f32::EPSILON,
                "{ours} is not a whole number of sixty-fourths"
            );
        }
    }

    use meo_canvas_scene::{
        Point, Scene, Sides, Size,
        node::{Node, NodeId},
        style::{
            Dimension, Length,
            layout::{
                Align, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
                GridAutoFlow, GridPlacement, Justify, LayoutStyle, Overflow,
                PositionType, TrackSize,
            },
        },
    };

    use super::{LayoutResult, solve};
    use crate::measure::{Available, Measure, MeasuredLeaf};

    /// A measurer that answers one size for every leaf, and no baseline.
    ///
    /// Shared by the layout tests and by any other unit test in this crate that
    /// needs a solve without fonts. `pub(crate)` and `#[cfg(test)]` rather than
    /// part of `measure`'s public surface: a helper exported from the library
    /// is API to keep working, and every test that wants this one lives in this
    /// crate.
    ///
    /// Honours [`Measure`]'s contract the cheap way -- the answer depends on no
    /// argument at all, so it is trivially a function of them.
    #[derive(Debug, Clone, Copy)]
    pub(crate) struct Fixed {
        /// What every leaf measures.
        size: Size,
    }

    impl Fixed {
        /// A measurer answering `width` by `height` for every leaf.
        pub(crate) const fn new(width: f32, height: f32) -> Self {
            Self {
                size: Size::new(width, height),
            }
        }
    }

    impl Measure for Fixed {
        fn measure(
            &mut self,
            _node: NodeId,
            _known: (Option<f32>, Option<f32>),
            _available: (Available, Available),
        ) -> MeasuredLeaf {
            MeasuredLeaf::sized(self.size)
        }
    }

    /// A measurer shaped like a text run: it has a natural width and never
    /// exceeds what it is offered.
    ///
    /// Answers the two intrinsic questions the way a paragraph does --
    /// `MaxContent` is the run on one line, `MinContent` its longest
    /// unbreakable piece -- so a test exercises the same path a real measurer
    /// takes. Deterministic per [`Measure`]'s contract: the answer is a pure
    /// function of `available` and nothing is carried between calls.
    ///
    /// # Why `min` is a field rather than a zero
    ///
    /// It returned `0.0` for `MinContent` while this doc comment said
    /// "longest unbreakable piece", and the two disagreed for as long as
    /// nothing asked. That is worse than a wrong line: flexbox floors an item
    /// at exactly this answer (CSS Flexbox 1 §4.5), so a mock that reports
    /// zero says an item may always be squeezed to nothing -- and a reader
    /// who came here to learn the rule would have found the wrong one written
    /// down, and read a real paragraph collapsing to its ellipsis as
    /// correct-by-design. `crates/meo-canvas-core/tests/chrome_min_content.rs`
    /// is the measured version of the rule.
    #[derive(Debug, Clone, Copy)]
    struct Wrapping {
        /// The width the content takes with nothing constraining it.
        natural: f32,
        /// The narrowest it goes without overflowing: its longest unbreakable
        /// piece. Never zero, and never more than `natural`.
        min: f32,
        /// The height it reports at any width, which keeps the arithmetic in
        /// the assertions about width alone.
        height: f32,
    }

    impl Measure for Wrapping {
        fn measure(
            &mut self,
            _node: NodeId,
            _known: (Option<f32>, Option<f32>),
            available: (Available, Available),
        ) -> MeasuredLeaf {
            let width = match available.0 {
                // Offered less than the longest piece, a real run does not
                // shed it -- the word overflows and the reported width stays.
                Available::Definite(extent) => {
                    self.natural.min(extent).max(self.min)
                }
                Available::MinContent => self.min,
                Available::MaxContent => self.natural,
            };
            MeasuredLeaf::sized(Size::new(width, self.height))
        }
    }

    /// A scene with one page whose root is a plain box.
    fn scene_with_page(width: f32, height: f32) -> (Scene, NodeId) {
        let mut scene = Scene::new(Size::new(width, height));
        let page = scene
            .push_page()
            .unwrap_or_else(|error| unreachable!("{error}"));
        (scene, page)
    }

    fn solved(scene: &Scene, page: NodeId) -> LayoutResult {
        solve(scene, page, &mut Fixed::new(0.0, 0.0))
            .unwrap_or_else(|error| unreachable!("{error}"))
    }

    #[test]
    fn page_root_fills_the_scene() {
        let (scene, page) = scene_with_page(200.0, 100.0);

        let result = solved(&scene, page);
        let root = result
            .get(page)
            .unwrap_or_else(|| unreachable!("the page root is laid out"));

        assert_eq!(root.origin, Point { x: 0.0, y: 0.0 });
    }

    /// Whether a page root's own definite size already survives layout.
    ///
    /// **The compatibility question under the ICO change**, and it has to be
    /// answered before a page is begun at the root's size rather than the
    /// scene's: if a definite root size were currently discarded, giving it
    /// meaning would move existing scenes rather than leave them alone.
    /// `pin_page_root` substitutes `scene.size` only where the root's own
    /// dimension is `auto`, so a stated one is already honoured -- and this
    /// says so in a test rather than in a reading of that function.
    #[test]
    fn a_definite_root_size_is_kept_where_the_scene_says_otherwise() {
        let (mut scene, page) = scene_with_page(100.0, 60.0);
        let root = scene
            .get_mut(page)
            .unwrap_or_else(|| unreachable!("the page root was just created"));
        root.layout.size = (Dimension::Points(40.0), Dimension::Points(20.0));

        let result = solved(&scene, page);
        let solved_root = result
            .get(page)
            .unwrap_or_else(|| unreachable!("the page root is laid out"));

        assert_eq!(
            solved_root.size,
            Size::new(40.0, 20.0),
            "a stated root size is the root's size, not the scene's"
        );
    }

    #[test]
    fn a_used_border_width_floors_but_never_to_nothing() {
        // Chrome, measured directly at dpr 1 and dpr 2 with identical answers.
        // The hairline row comes FIRST because it is the one a bare `floor`
        // breaks: `0.1px` is a visible border in a browser and nothing here.
        for (declared, used) in [
            (0.1_f32, 1.0_f32),
            (0.4, 1.0),
            (0.5, 1.0),
            (0.9, 1.0),
            (1.4, 1.0),
            (1.5, 1.0),
            (1.6, 1.0),
            (2.5, 2.0),
            (3.4, 3.0),
            (3.5, 3.0),
            (3.6, 3.0),
            (3.9, 3.0),
            // Not a browser reading: nothing declared is nothing drawn, and a
            // minimum that applied here would put a border on every box.
            (0.0, 0.0),
        ] {
            assert!(
                (super::used_border_width(declared) - used).abs()
                    < f32::EPSILON,
                "{declared} is used as {}, and Chrome uses it as {used}",
                super::used_border_width(declared)
            );
        }
    }

    #[test]
    fn a_fractional_border_is_used_as_the_integer_chrome_uses() {
        // Chrome resolves a border width to an integer at used-value time and
        // LAYOUT sees it: `getComputedStyle` reports the integer and the
        // border box grows by the integer, so `3.5px` and `3px` render
        // identically. Measured at dpr 1 and dpr 2 with the same answer, so it
        // is a CSS-pixel rule rather than a device-pixel one.
        //
        // A 20-tall content box inside a 3.5 border is 27 in Chrome, not 27.5.
        let (mut scene, page) = scene_with_page(100.0, 60.0);
        let root = scene
            .get_mut(page)
            .unwrap_or_else(|| unreachable!("the page root was just created"));
        root.layout.size = (Dimension::Points(40.0), Dimension::Points(20.0));
        root.layout.box_sizing = BoxSizing::ContentBox;
        root.layout.border = Sides::all(3.5);

        let result = solved(&scene, page);
        let solved_root = result
            .get(page)
            .unwrap_or_else(|| unreachable!("the page root is laid out"));

        assert_eq!(
            solved_root.size,
            Size::new(46.0, 26.0),
            "a 3.5 border is used as 3, so the box grows by 6 and not by 7"
        );
    }

    #[test]
    fn a_missing_page_is_a_layout_error() {
        let (scene, _page) = scene_with_page(10.0, 10.0);
        let absent = NodeId::new(u32::MAX);

        let solved = solve(&scene, absent, &mut Fixed::new(0.0, 0.0));

        assert!(matches!(solved, Err(crate::Error::Layout(_))));
    }

    #[test]
    fn a_leaf_takes_the_size_the_measurer_reports() {
        let (mut scene, page) = scene_with_page(200.0, 100.0);
        let leaf = scene
            .push(page, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));

        // The cross axis is pinned to the start because `align-items` is
        // `stretch` by default, and a stretched child takes its parent's height
        // rather than the one it measured. Without this the test would assert
        // the page's height and prove nothing about the measurer.
        scene
            .get_mut(page)
            .unwrap_or_else(|| unreachable!("the page root was just created"))
            .layout
            .align_items = Some(Align::FlexStart);

        let result = solve(&scene, page, &mut Fixed::new(40.0, 20.0))
            .unwrap_or_else(|error| unreachable!("{error}"));
        let rect = result
            .get(leaf)
            .unwrap_or_else(|| unreachable!("the leaf is laid out"));

        assert_eq!(rect.size, Size::new(40.0, 20.0));
    }

    #[test]
    fn a_page_root_fills_the_scene_even_with_a_small_child() {
        // The root's own size is `Auto`, and layout resolves that to the
        // scene's extent rather than to its content -- a page is the canvas.
        let (mut scene, page) = scene_with_page(200.0, 100.0);
        scene
            .push(page, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));

        let result = solve(&scene, page, &mut Fixed::new(10.0, 10.0))
            .unwrap_or_else(|error| unreachable!("{error}"));
        let root = result
            .get(page)
            .unwrap_or_else(|| unreachable!("the page root is laid out"));

        assert_eq!(root.size, Size::new(200.0, 100.0));
    }

    #[test]
    fn a_leaf_with_room_takes_its_natural_width() {
        let (mut scene, page) = scene_with_page(200.0, 60.0);
        let leaf = scene
            .push(page, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));

        // Cross-axis stretch is off so the measured height survives and the
        // assertion is about width alone.
        scene
            .get_mut(page)
            .unwrap_or_else(|| unreachable!("the page root was just created"))
            .layout
            .align_items = Some(Align::FlexStart);

        let result = solve(
            &scene,
            page,
            &mut Wrapping {
                natural: 50.0,
                min: 12.0,
                height: 10.0,
            },
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
        let rect = result
            .get(leaf)
            .unwrap_or_else(|| unreachable!("the leaf is laid out"));

        assert_eq!(rect.size, Size::new(50.0, 10.0));
    }

    #[test]
    fn a_leaf_wider_than_its_container_shrinks_to_it() {
        // `flex_shrink` defaults to CSS's 1.0 rather than Yoga's 0, so an
        // over-wide item gives space up instead of overflowing. This is the
        // test that fails if that default is ever taken from taffy rather than
        // written down.
        let (mut scene, page) = scene_with_page(30.0, 60.0);
        let leaf = scene
            .push(page, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));

        // Cross-axis stretch is off so the measured height survives and the
        // assertion is about width alone.
        scene
            .get_mut(page)
            .unwrap_or_else(|| unreachable!("the page root was just created"))
            .layout
            .align_items = Some(Align::FlexStart);

        let result = solve(
            &scene,
            page,
            &mut Wrapping {
                natural: 50.0,
                min: 12.0,
                height: 10.0,
            },
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
        let rect = result
            .get(leaf)
            .unwrap_or_else(|| unreachable!("the leaf is laid out"));

        assert_eq!(rect.size, Size::new(30.0, 10.0));
    }

    #[test]
    fn a_leaf_does_not_shrink_below_its_min_content_width() {
        // The half `a_leaf_wider_than_its_container_shrinks_to_it` cannot
        // see. That test shrinks 50 into 30 and passes just as well if the
        // floor is zero, because 30 is above it either way. Here the
        // container is narrower than the leaf's longest unbreakable piece, so
        // the automatic minimum size (CSS Flexbox 1 §4.5) is the only thing
        // deciding the answer: the item keeps its 12 and overflows the 8 it
        // was offered.
        //
        // This is the arrangement that made a real paragraph collapse to its
        // ellipsis -- the item shrank correctly to a minimum that was itself
        // wrong. A mock reporting zero here would call that behaviour right.
        let (mut scene, page) = scene_with_page(8.0, 60.0);
        let leaf = scene
            .push(page, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));

        scene
            .get_mut(page)
            .unwrap_or_else(|| unreachable!("the page root was just created"))
            .layout
            .align_items = Some(Align::FlexStart);

        let result = solve(
            &scene,
            page,
            &mut Wrapping {
                natural: 50.0,
                min: 12.0,
                height: 10.0,
            },
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
        let rect = result
            .get(leaf)
            .unwrap_or_else(|| unreachable!("the leaf is laid out"));

        assert_eq!(rect.size, Size::new(12.0, 10.0));
    }

    #[test]
    fn a_display_none_subtree_has_no_rectangles() {
        let (mut scene, page) = scene_with_page(100.0, 100.0);
        let hidden = scene
            .push(page, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));
        let under = scene
            .push(hidden, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));

        scene
            .get_mut(hidden)
            .unwrap_or_else(|| unreachable!("the node was just created"))
            .layout
            .display = Display::None;

        let result = solved(&scene, page);

        assert!(result.get(page).is_some());
        assert!(result.get(hidden).is_none());
        assert!(result.get(under).is_none());
    }

    /// Maps every variant of an enum and asserts no two collapse onto one
    /// taffy value.
    ///
    /// A mapping that is total and injective is one that lost nothing. Written
    /// once and applied to each enum the scene and taffy agree on, because the
    /// failure it catches -- a new variant falling into an existing arm, or two
    /// arms naming the same taffy value -- is the same failure every time.
    fn maps_injectively<S: Copy, T: PartialEq + core::fmt::Debug>(
        all: &[S],
        map: impl Fn(S) -> T,
    ) {
        let mapped: Vec<T> = all.iter().copied().map(map).collect();
        for (index, value) in mapped.iter().enumerate() {
            assert!(
                !mapped[..index].contains(value),
                "two variants map onto {value:?}"
            );
        }
        assert_eq!(mapped.len(), all.len());
    }

    #[test]
    fn every_layout_enum_maps_one_to_one() {
        maps_injectively(Display::ALL, super::to_display);
        maps_injectively(FlexDirection::ALL, super::to_flex_direction);
        maps_injectively(FlexWrap::ALL, super::to_flex_wrap);
        maps_injectively(Overflow::ALL, super::to_overflow);
        maps_injectively(BoxSizing::ALL, super::to_box_sizing);
        maps_injectively(Direction::ALL, super::to_direction);
        maps_injectively(GridAutoFlow::ALL, super::to_grid_auto_flow);
        maps_injectively(Justify::ALL, super::to_justify);
    }

    #[test]
    fn static_and_relative_are_one_position_to_taffy() {
        // Not injective, and deliberately: taffy has two positions where CSS
        // has three, and both in-flow ones are in flow. What separates them --
        // whether `inset` moves the node, and whether `z_index` stacks it --
        // is settled on our side, so collapsing them here loses nothing.
        assert_eq!(
            super::to_position(PositionType::Static),
            taffy::Position::Relative
        );
        assert_eq!(
            super::to_position(PositionType::Relative),
            taffy::Position::Relative
        );
        assert_eq!(
            super::to_position(PositionType::Absolute),
            taffy::Position::Absolute
        );
    }

    #[test]
    fn a_static_node_does_not_reach_taffy_with_an_inset() {
        // CSS's offset properties do not apply to a static element, and taffy
        // would honour them, so they are dropped on the way in.
        let mut style = LayoutStyle {
            inset: Sides::all(Some(Length::Points(30.0))),
            position_type: PositionType::Static,
            ..LayoutStyle::default()
        };

        assert_eq!(
            super::to_taffy_style(&style).inset,
            taffy::Rect::auto(),
            "a static inset is dropped"
        );

        style.position_type = PositionType::Relative;
        assert_eq!(
            super::to_taffy_style(&style).inset.top,
            taffy::LengthPercentageAuto::length(30.0),
            "a relative inset is not"
        );
    }

    #[test]
    fn a_reversed_wrap_that_overflows_hangs_off_the_top() {
        use meo_canvas_scene::style::layout::FlexWrap;

        // Six 28x44 children in an 88x56 box: three fit across, so two lines
        // of 44 in a box of 56. Chrome puts the last line's bottom on the
        // box's bottom edge and lets the first hang off the top -- y = 12 and
        // -32 -- where taffy packs the pair from y = 0.
        let placed = |wrap: FlexWrap, height: f32| {
            // A page far larger than the box, so a line placed above it is
            // still measured rather than cropped: the page a thing is
            // measured on is part of the measurement.
            let mut scene = Scene::new(Size::new(300.0, 300.0));
            let mut outer = Node::container();
            outer.layout.size =
                (Dimension::Points(88.0), Dimension::Points(height));
            outer.layout.flex_wrap = wrap;
            let outer = scene
                .push(NodeId::ROOT, outer)
                .unwrap_or_else(|error| unreachable!("{error}"));
            let mut ids = Vec::new();
            for _ in 0..6 {
                let mut child = Node::container();
                child.layout.size =
                    (Dimension::Points(28.0), Dimension::Points(44.0));
                ids.push(
                    scene
                        .push(outer, child)
                        .unwrap_or_else(|error| unreachable!("{error}")),
                );
            }
            let result = solved(&scene, NodeId::ROOT);
            let origin = result
                .get(outer)
                .unwrap_or_else(|| unreachable!("the box is laid out"))
                .origin;
            ids.into_iter()
                .filter_map(|id| result.get(id))
                .map(|rect| (rect.origin.y - origin.y) as i32)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            placed(FlexWrap::WrapReverse, 56.0),
            vec![12, 12, 12, -32, -32, -32],
            "an overflowing reversed stack sits on the bottom edge"
        );
        // Unreversed, and reversed where the lines fit, are taffy's and stay
        // taffy's: the correction applies only where the safe fallback threw
        // the reversal away.
        assert_eq!(placed(FlexWrap::Wrap, 56.0), vec![0, 0, 0, 44, 44, 44]);
        assert_eq!(
            placed(FlexWrap::WrapReverse, 140.0),
            vec![96, 96, 96, 26, 26, 26]
        );
    }

    #[test]
    fn a_fixed_node_resolves_against_the_page_and_an_absolute_one_against_its_parent()
     {
        // The whole difference between the two in a still render. CSS resolves
        // `fixed` against the viewport, which here is the page; `absolute`
        // resolves against its containing block, which taffy takes to be the
        // parent. A padded, offset parent is what separates them: an absolute
        // child lands inside it and a fixed one ignores it.
        let placed = |position| {
            let (mut scene, page) = scene_with_page(200.0, 200.0);
            let mut parent = Node::container();
            // Positioned, so it is a containing block. A static parent is not
            // one, which is what the test below covers.
            parent.layout.position_type = PositionType::Relative;
            parent.layout.margin = Sides::all(Dimension::Points(40.0));
            parent.layout.padding = Sides::all(Length::Points(10.0));
            parent.layout.size =
                (Dimension::Points(100.0), Dimension::Points(100.0));
            let parent = scene
                .push(page, parent)
                .unwrap_or_else(|error| unreachable!("{error}"));

            let mut child = Node::container();
            child.layout.position_type = position;
            child.layout.inset = Sides {
                top: Some(Length::Points(5.0)),
                left: Some(Length::Points(5.0)),
                right: None,
                bottom: None,
            };
            child.layout.size =
                (Dimension::Points(10.0), Dimension::Points(10.0));
            let child = scene
                .push(parent, child)
                .unwrap_or_else(|error| unreachable!("{error}"));

            solved(&scene, page)
                .get(child)
                .unwrap_or_else(|| unreachable!("the child is laid out"))
                .origin
        };

        // Inside the parent: its margin of 40, then the inset of 5. The
        // padding is not added — CSS measures an absolute inset from the
        // padding *box*, whose origin is inside the border and outside the
        // padding, so a padded parent does not push its absolute child in.
        assert_eq!(placed(PositionType::Absolute), Point { x: 45.0, y: 45.0 });
        // Against the page, which is what makes it fixed rather than absolute.
        assert_eq!(placed(PositionType::Fixed), Point { x: 5.0, y: 5.0 });
    }

    #[test]
    fn an_absolute_node_skips_a_static_parent_for_the_nearest_positioned_one() {
        // CSS resolves an absolute node against its nearest *positioned*
        // ancestor, skipping every static box between. taffy resolves it
        // against its parent whatever that is, so this is settled by where the
        // node is attached rather than by its style.
        //
        // Measured before the fix: the grandchild landed at (50, 50), the
        // static parent's own origin.
        let (mut scene, page) = scene_with_page(200.0, 200.0);

        let mut grandparent = Node::container();
        grandparent.layout.position_type = PositionType::Relative;
        grandparent.layout.margin = Sides::all(Dimension::Points(20.0));
        grandparent.layout.size =
            (Dimension::Points(150.0), Dimension::Points(150.0));
        let grandparent = scene
            .push(page, grandparent)
            .unwrap_or_else(|error| unreachable!("{error}"));

        // Static, and offset, so resolving against it is visibly different
        // from resolving against the grandparent.
        let mut parent = Node::container();
        parent.layout.margin = Sides::all(Dimension::Points(30.0));
        parent.layout.size = (Dimension::Points(80.0), Dimension::Points(80.0));
        let parent = scene
            .push(grandparent, parent)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let mut child = Node::container();
        child.layout.position_type = PositionType::Absolute;
        child.layout.inset = Sides {
            top: Some(Length::ZERO),
            left: Some(Length::ZERO),
            right: None,
            bottom: None,
        };
        child.layout.size = (Dimension::Points(10.0), Dimension::Points(10.0));
        let child = scene
            .push(parent, child)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let solved = solved(&scene, page);
        assert_eq!(
            solved
                .get(parent)
                .unwrap_or_else(|| unreachable!("the parent is laid out"))
                .origin,
            Point { x: 50.0, y: 50.0 },
            "the static parent is where it always was"
        );
        assert_eq!(
            solved
                .get(child)
                .unwrap_or_else(|| unreachable!("the child is laid out"))
                .origin,
            Point { x: 20.0, y: 20.0 },
            "the child resolves against the grandparent, not the parent"
        );
    }

    #[test]
    fn hoisting_a_fixed_node_moves_none_of_its_siblings() {
        // An out-of-flow child contributes nothing to its parent's flow, so
        // taking it out of that parent's children changes nothing the solver
        // would have done. Asserted rather than argued, because the hoist is
        // the one part of this that reaches into the tree shape.
        let sibling_origin = |position| {
            let (mut scene, page) = scene_with_page(200.0, 200.0);
            let mut out_of_flow = Node::container();
            out_of_flow.layout.position_type = position;
            out_of_flow.layout.size =
                (Dimension::Points(10.0), Dimension::Points(10.0));
            scene
                .push(page, out_of_flow)
                .unwrap_or_else(|error| unreachable!("{error}"));

            let mut sibling = Node::container();
            sibling.layout.size =
                (Dimension::Points(20.0), Dimension::Points(20.0));
            let sibling = scene
                .push(page, sibling)
                .unwrap_or_else(|error| unreachable!("{error}"));

            solved(&scene, page)
                .get(sibling)
                .unwrap_or_else(|| unreachable!("the sibling is laid out"))
                .origin
        };

        assert_eq!(
            sibling_origin(PositionType::Fixed),
            sibling_origin(PositionType::Absolute),
            "hoisting the fixed node moved its sibling"
        );
    }

    #[test]
    fn an_inset_moves_a_relative_child_and_not_a_static_one() {
        // The measurement this reproduces: in Chrome a static child given
        // `top: 30px; left: 30px` sits at its flow position, and the same child
        // made relative moves 30 on both axes while its sibling does not move
        // at all.
        let placed = |position| {
            let (mut scene, page) = scene_with_page(200.0, 200.0);
            if let Some(root) = scene.get_mut(page) {
                root.layout.display = Display::Block;
            }
            let mut child = Node::container();
            child.layout.position_type = position;
            child.layout.inset = Sides::all(Some(Length::Points(30.0)));
            child.layout.size =
                (Dimension::Points(50.0), Dimension::Points(20.0));
            let child = scene
                .push(page, child)
                .unwrap_or_else(|error| unreachable!("{error}"));

            let sibling = scene
                .push(page, Node::container())
                .unwrap_or_else(|error| unreachable!("{error}"));

            let result = solved(&scene, page);
            (
                result
                    .get(child)
                    .unwrap_or_else(|| unreachable!("the child is laid out"))
                    .origin,
                result
                    .get(sibling)
                    .unwrap_or_else(|| unreachable!("the sibling is laid out"))
                    .origin,
            )
        };

        let (static_child, static_sibling) = placed(PositionType::Static);
        assert_eq!(static_child, Point { x: 0.0, y: 0.0 });

        let (relative_child, relative_sibling) = placed(PositionType::Relative);
        assert_eq!(relative_child, Point { x: 30.0, y: 30.0 });

        // And the shift is visual: the sibling lands in the same place either
        // way, which is what makes `relative` a shift rather than a placement.
        assert_eq!(static_sibling, relative_sibling);
    }

    #[test]
    fn align_content_keeps_every_variant_the_property_defines() {
        // Not injective, and deliberately: `baseline` is not an
        // `align-content` value, so it is discarded to the property's initial
        // `stretch` exactly as a browser discards it.
        assert_eq!(
            super::to_align_content(Align::Baseline),
            taffy::AlignContent::STRETCH
        );
        assert_eq!(
            super::to_align_content(Align::Stretch),
            taffy::AlignContent::STRETCH
        );

        maps_injectively(
            &[
                Align::FlexStart,
                Align::FlexEnd,
                Align::Center,
                Align::Stretch,
                Align::SpaceBetween,
                Align::SpaceAround,
                Align::SpaceEvenly,
            ],
            super::to_align_content,
        );
    }

    #[test]
    fn align_items_discards_the_values_the_property_does_not_define() {
        // The three `space-*` values belong to `align-content` alone. CSS drops
        // a value a property does not define and the property keeps its initial
        // one, which for `align-items` is `stretch`.
        for align in
            [Align::SpaceBetween, Align::SpaceAround, Align::SpaceEvenly]
        {
            assert_eq!(
                super::to_align_items(align),
                taffy::AlignItems::STRETCH
            );
        }

        maps_injectively(
            &[
                Align::FlexStart,
                Align::FlexEnd,
                Align::Center,
                Align::Stretch,
                Align::Baseline,
            ],
            super::to_align_items,
        );
    }

    #[test]
    fn lengths_and_dimensions_carry_their_unit() {
        assert_eq!(
            super::to_length(Length::Points(4.0)),
            taffy::LengthPercentage::length(4.0)
        );
        assert_eq!(
            super::to_length(Length::Percent(0.25)),
            taffy::LengthPercentage::percent(0.25)
        );

        assert_eq!(
            super::to_dimension(Dimension::Auto),
            taffy::Dimension::auto()
        );
        assert_eq!(
            super::to_dimension(Dimension::Points(8.0)),
            taffy::Dimension::length(8.0)
        );
        assert_eq!(
            super::to_dimension(Dimension::Percent(0.5)),
            taffy::Dimension::percent(0.5)
        );
    }

    #[test]
    fn an_auto_margin_is_cshs_free_space_margin_and_an_absent_inset_is_auto() {
        assert_eq!(
            super::to_auto_length(Dimension::Auto),
            taffy::LengthPercentageAuto::auto()
        );
        assert_eq!(
            super::to_auto_length(Dimension::Points(3.0)),
            taffy::LengthPercentageAuto::length(3.0)
        );
        assert_eq!(
            super::to_auto_length(Dimension::Percent(0.1)),
            taffy::LengthPercentageAuto::percent(0.1)
        );

        // An edge nobody named is one taffy places, not one pinned at zero.
        assert_eq!(super::to_inset(None), taffy::LengthPercentageAuto::auto());
        assert_eq!(
            super::to_inset(Some(Length::Points(2.0))),
            taffy::LengthPercentageAuto::length(2.0)
        );
        assert_eq!(
            super::to_inset(Some(Length::Percent(0.75))),
            taffy::LengthPercentageAuto::percent(0.75)
        );
    }

    #[test]
    fn a_flexible_track_has_no_fixed_minimum() {
        // `1fr` is `minmax(auto, 1fr)`: a flexible track shrinks below its
        // share when the fixed tracks take the room, which a fixed minimum
        // would prevent.
        let flexible = super::to_track_sizing(TrackSize::Fraction(1.0));
        assert_eq!(flexible.min, taffy::MinTrackSizingFunction::auto());
        assert_eq!(flexible.max, taffy::MaxTrackSizingFunction::fr(1.0));

        let fixed = super::to_track_sizing(TrackSize::Points(40.0));
        assert_eq!(fixed.min, taffy::MinTrackSizingFunction::length(40.0));
        assert_eq!(fixed.max, taffy::MaxTrackSizingFunction::length(40.0));

        let proportional = super::to_track_sizing(TrackSize::Percent(0.5));
        assert_eq!(
            proportional.min,
            taffy::MinTrackSizingFunction::percent(0.5)
        );
        assert_eq!(
            proportional.max,
            taffy::MaxTrackSizingFunction::percent(0.5)
        );

        let automatic = super::to_track_sizing(TrackSize::Auto);
        assert_eq!(automatic.min, taffy::MinTrackSizingFunction::auto());
        assert_eq!(automatic.max, taffy::MaxTrackSizingFunction::auto());
    }

    #[test]
    fn a_placement_without_a_start_is_auto_placed() {
        let auto = super::to_placement(GridPlacement::default());
        assert_eq!(auto.start, taffy::GridPlacement::Auto);
        assert_eq!(auto.end, taffy::GridPlacement::Auto);

        let pinned = super::to_placement(GridPlacement {
            start: Some(2),
            span: Some(3),
        });
        assert_eq!(pinned.start, taffy::GridPlacement::Line(2.into()));
        assert_eq!(pinned.end, taffy::GridPlacement::Span(3));
    }

    #[test]
    fn the_gap_pair_is_swapped_at_the_crossing() {
        // The scene spells `(row, column)` after CSS's shorthand; taffy spells
        // `(width, height)`, which is `(column, row)`. A gap that came through
        // unswapped would separate rows by the column gap.
        let layout = LayoutStyle {
            gap: (Length::Points(4.0), Length::Points(9.0)),
            ..LayoutStyle::default()
        };

        let style = super::to_taffy_style(&layout);

        assert_eq!(style.gap.width, taffy::LengthPercentage::length(9.0));
        assert_eq!(style.gap.height, taffy::LengthPercentage::length(4.0));
    }

    #[test]
    fn the_offered_space_reaches_the_measurer_in_this_crates_vocabulary() {
        assert_eq!(
            super::to_available(taffy::AvailableSpace::Definite(7.0)),
            Available::Definite(7.0)
        );
        assert_eq!(
            super::to_available(taffy::AvailableSpace::MinContent),
            Available::MinContent
        );
        assert_eq!(
            super::to_available(taffy::AvailableSpace::MaxContent),
            Available::MaxContent
        );
    }

    /// Every cell of the bad-value grid that layout owns, in one place.
    ///
    /// **Each row fails if the normalisation is removed**, which is the point:
    /// measured against Chrome 151, an invalid declaration is dropped and the
    /// property takes its unset value, and before this the values reached
    /// taffy untouched -- 23 of 48 sampled cells drew nothing at all.
    /// A percentage against a content-sized parent is no constraint at all.
    ///
    /// **Both halves are here on purpose.** Dropping every percentage passes
    /// the indefinite row and breaks the definite ones -- measured against
    /// that repair, a `min-height: 200%` child of a 60, 120 and 200 tall box
    /// gave 20 where Chrome gives 120, 240 and 400 -- so the definite rows are
    /// what makes this test constrain the fix rather than restate it.
    #[test]
    fn a_percentage_height_resolves_only_against_a_definite_one() {
        fn probe(parent_height: Dimension) -> f32 {
            let (mut scene, page) = scene_with_page(400.0, 400.0);
            let parent = scene
                .push(page, Node::new(meo_canvas_scene::node::NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(parent) {
                node.layout.size = (Dimension::Points(200.0), parent_height);
                node.layout.align_items = Some(Align::FlexStart);
                // **Without this the page stretches it and `auto` stops being
                // indefinite**: a stretched flex item has a definite cross
                // size, so the percentage would resolve against the page's 400
                // and this row would be measuring the page rather than the
                // parent. Chrome does the same, which is why the browser probe
                // for this row sets `align-items: flex-start` as well.
                node.layout.align_self = Some(Align::FlexStart);
            }
            let child = scene
                .push(parent, Node::new(meo_canvas_scene::node::NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(child) {
                node.layout.size =
                    (Dimension::Points(30.0), Dimension::Points(20.0));
                node.layout.min_size =
                    (Dimension::Auto, Dimension::Percent(2.0));
            }
            solved(&scene, page)
                .get(child)
                .map_or(0.0, |rect| rect.size.height)
        }

        // A content-sized parent: Chrome ignores the percentage, and so does
        // this -- the child keeps the 20 it declared. Compared by bits, as the
        // rest of this module does: every number here is a length taffy passed
        // through rather than one it computed, so the claim is identity.
        assert_eq!(probe(Dimension::Auto).to_bits(), 20.0_f32.to_bits());
        // A definite parent: the percentage resolves against it. These are the
        // rows a blanket "ignore percentages" repair breaks.
        assert_eq!(
            probe(Dimension::Points(60.0)).to_bits(),
            120.0_f32.to_bits()
        );
        assert_eq!(
            probe(Dimension::Points(120.0)).to_bits(),
            240.0_f32.to_bits()
        );
    }

    #[test]
    fn an_unusable_value_is_dropped_where_it_becomes_layout_input() {
        let bad = [f32::INFINITY, f32::NAN, -20.0];
        for value in bad {
            let style = LayoutStyle {
                size: (Dimension::Points(value), Dimension::Points(value)),
                min_size: (Dimension::Points(value), Dimension::Points(value)),
                max_size: (Dimension::Points(value), Dimension::Points(value)),
                flex_basis: Dimension::Points(value),
                aspect_ratio: Some(value),
                flex_grow: value,
                flex_shrink: value,
                padding: Sides::all(Length::Points(value)),
                gap: (Length::Points(value), Length::Points(value)),
                border: Sides::all(value),
                ..LayoutStyle::default()
            };
            let taffy = super::to_taffy_style(&style);

            assert_eq!(taffy.size.width, taffy::Dimension::auto(), "{value}");
            assert_eq!(
                taffy.min_size.height,
                taffy::LengthPercentageAuto::auto()
            );
            assert_eq!(
                taffy.max_size.width,
                taffy::LengthPercentageAuto::auto()
            );
            assert_eq!(taffy.flex_basis, taffy::Dimension::auto());
            assert_eq!(taffy.aspect_ratio, None, "{value}");
            // The two factors fall back to *their own* initial values, which
            // differ: a shrink that fell back to 0 would stop overflowing
            // items shrinking at all. Compared by bits rather than by value,
            // because the claim is that the fallback is passed through
            // untouched -- identity, not nearness.
            assert_eq!(taffy.flex_grow.to_bits(), 0.0_f32.to_bits(), "{value}");
            assert_eq!(
                taffy.flex_shrink.to_bits(),
                1.0_f32.to_bits(),
                "{value}"
            );
            assert_eq!(
                taffy.padding.left,
                taffy::LengthPercentage::length(0.0)
            );
            assert_eq!(taffy.gap.width, taffy::LengthPercentage::length(0.0));
            assert_eq!(taffy.border.top, taffy::LengthPercentage::length(0.0));
        }
    }

    /// The other half, and the half a blanket repair breaks.
    ///
    /// **A negative margin and a negative inset are valid CSS**, measured
    /// against Chrome and kept. A repair that rejected every negative would
    /// pass the test above and fail this one -- which is the whole reason it
    /// is written as its own test rather than as more rows in that one.
    #[test]
    fn a_negative_margin_and_a_negative_inset_survive() {
        let style = LayoutStyle {
            position_type: PositionType::Relative,
            margin: Sides::all(Dimension::Points(-20.0)),
            inset: Sides::all(Some(Length::Points(-20.0))),
            ..LayoutStyle::default()
        };
        let taffy = super::to_taffy_style(&style);

        assert_eq!(
            taffy.margin.left,
            taffy::LengthPercentageAuto::length(-20.0)
        );
        assert_eq!(taffy.inset.top, taffy::LengthPercentageAuto::length(-20.0));

        // And a non-finite one is still dropped -- a margin to zero rather
        // than to `auto`, which would absorb free space and centre a box that
        // asked for nothing of the kind; an inset to `auto`, which is absence.
        let broken = LayoutStyle {
            position_type: PositionType::Relative,
            margin: Sides::all(Dimension::Points(f32::NAN)),
            inset: Sides::all(Some(Length::Points(f32::INFINITY))),
            ..LayoutStyle::default()
        };
        let taffy = super::to_taffy_style(&broken);

        assert_eq!(taffy.margin.left, taffy::LengthPercentageAuto::length(0.0));
        assert_eq!(taffy.inset.top, taffy::LengthPercentageAuto::auto());
    }

    #[test]
    fn the_scenes_defaults_are_csss_not_taffys() {
        // The scene's `LayoutStyle::default()` is CSS's: a row direction and a
        // shrink of 1. This is the test that fails if the mapping ever leans on
        // `taffy::Style::default()` for a field the scene carries.
        let style = super::to_taffy_style(&LayoutStyle::default());

        assert_eq!(style.display, taffy::Display::Flex);
        assert_eq!(style.flex_direction, taffy::FlexDirection::Row);
        // Compared against the scene's own value rather than a literal, and by
        // bits rather than by value: the claim is that the mapping passes the
        // number through untouched, which is identity rather than nearness.
        assert_eq!(
            style.flex_shrink.to_bits(),
            LayoutStyle::default().flex_shrink.to_bits()
        );
        assert_eq!(style.box_sizing, taffy::BoxSizing::BorderBox);
    }

    #[test]
    fn child_rectangles_are_absolute_not_parent_relative() {
        let (mut scene, page) = scene_with_page(200.0, 200.0);
        let outer = scene
            .push(page, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));
        let inner = scene
            .push(outer, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));

        {
            let outer_style = &mut scene
                .get_mut(outer)
                .unwrap_or_else(|| unreachable!("the node was just created"))
                .layout;
            outer_style.padding = Sides {
                top: Length::Points(10.0),
                right: Length::Points(10.0),
                bottom: Length::Points(10.0),
                left: Length::Points(10.0),
            };
        }

        let result = solve(&scene, page, &mut Fixed::new(5.0, 5.0))
            .unwrap_or_else(|error| unreachable!("{error}"));

        let inner_rect = result
            .get(inner)
            .unwrap_or_else(|| unreachable!("the inner box is laid out"));
        assert_eq!(inner_rect.origin, Point { x: 10.0, y: 10.0 });
    }
}
