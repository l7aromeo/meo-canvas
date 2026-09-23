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
//! taffy rounds to whole pixels with no configurable scale factor, so layout
//! always solves at scale 1 and the device scale is applied at paint time. A
//! caller that wants layout itself to snap at a device scale does not get it.
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
        paint::BorderStyle,
    },
};
// The one trait imported from taffy rather than named through it:
// resolving a `LengthPercentage` against its containing block is a method,
// and a method needs its trait in scope.
use taffy::ResolveOrZero as _;

use crate::{
    Error, FINITE_CEILING,
    measure::{Available, Measure},
};

/// The scale layout solves at: one, always. taffy rounds to whole pixels on the
/// coordinates it is given, so a device scale would round twice; paint applies
/// [`Scene::scale`] to the whole drawing instead, rounding included.
const LAYOUT_SCALE: f32 = 1.0;

/// Where every node ended up.
///
/// **`#[non_exhaustive]` because it grows**, and a caller reads it rather than
/// building one, so a field can be added without a breaking change.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct LayoutResult {
    /// Absolute rectangle per node, in logical pixels at scale 1.
    pub rects: HashMap<NodeId, Rect>,
    /// How far a node's own content sits inside that rectangle: its border
    /// plus its padding, per edge.
    ///
    /// **Kept rather than re-derived.** Percentage padding resolves against
    /// the containing block's width, so working it out at paint time would be
    /// a second implementation of a rule layout has already applied. taffy
    /// computes both and hands them back with the rectangle.
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
    /// its picture 8px in on every edge, and with both, 16.
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
        // The page's own height is definite unless the caller asked for a
        // content height, when a percentage against it has nothing to resolve
        // against. The page has no parent, so both answers are this one.
        FromAbove {
            heights: Definite {
                parent: !scene.content_height,
                own: !scene.content_height,
            },
            parent: None,
            measure,
        },
    )?;
    for node in viewport.into_iter().chain(orphans) {
        tree.add_child(root, node)
            .map_err(|error| Error::Layout(error.to_string()))?;
    }
    pin_page_root(scene, page, root, &mut tree)?;

    // A content height is solved, so the height axis is offered `MaxContent`:
    // text breaks against the width, which must be known first, and the height
    // is only a consequence of that measuring.
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

    // [WORKAROUND] the compensations compare solved sizes and taffy rounds each
    // edge, so these passes run unrounded: rounded, a `339.20` minimum missed a
    // `340` width. `l7aromeo/meo-canvas#140`; retires with them, rounding being
    // taffy doing what it is asked. No solve precedes this line.
    tree.disable_rounding();
    collapse_definite_bases(&mut tree, root)?;
    // [WORKAROUND] the ratios come off before the first solve, so it reports a
    // fit-content inline size, and go back on in `compensate_ratio_direction`,
    // which names the defect and its probe.
    let candidates = ratio_direction_candidates(scene, &tree, &to_scene);
    clear_ratios(&mut tree, &candidates)?;
    solve_once(&mut tree, root, available, measure, &mut baselines)?;
    compensate_ratio_direction(
        &mut tree,
        &candidates,
        root,
        available,
        measure,
        &mut baselines,
    )?;

    // [WORKAROUND] a column container's automatic main size drops negative
    // margins. `DioxusLabs/taffy#1162` and `DioxusLabs/taffy#1163`, tracked as
    // `l7aromeo/meo-canvas#107`; see `compensate_dropped_margins`, probed by
    // `crates/meo-canvas-core/tests/taffy_negative_margin.rs`.
    compensate_dropped_margins(
        scene,
        &mut tree,
        &to_scene,
        root,
        available,
        measure,
        &mut baselines,
    )?;

    // One extra rounded pass rather than re-enabling rounding before whichever
    // compensation solves last, which would make their order load-bearing.
    tree.enable_rounding();
    baselines.clear();
    solve_once(&mut tree, root, available, measure, &mut baselines)?;

    let mut rects = HashMap::with_capacity(to_scene.len());
    let mut insets = HashMap::with_capacity(to_scene.len());
    collect(&tree, root, &to_scene, 0.0, 0.0, &mut rects, &mut insets)?;
    bottom_align_reversed_wraps(scene, page, &insets, &mut rects);

    Ok(LayoutResult {
        rects,
        insets,
        baselines,
    })
}

/// Moves an overflowing `wrap-reverse` line stack to the bottom of its box, as
/// Chrome does: taffy packs it from the top once it overflows, css-align-3's
/// safe fallback, so six 28x44 children in 88x56 sit at 0 and 44, not -32 and
/// 12. Only in-flow children move.
fn bottom_align_reversed_wraps(
    scene: &Scene,
    page: NodeId,
    insets: &HashMap<NodeId, Sides<f32>>,
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

        // The content box's own bottom, read from [`collect`]'s insets: the
        // edges taffy reserved, where a re-derivation resolved a percentage
        // against the wrong box. A node with no entry has no rectangle either,
        // and returned above.
        let Some(inset) = insets.get(&id).map(|sides| sides.bottom) else {
            continue;
        };
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

/// What a node needs from the walk above it, as one argument: `build` recurses,
/// and the parent, the two heights and the measurer travel together.
struct FromAbove<'above, M: ?Sized> {
    /// Which heights are definite, one level apart. See [`Definite`].
    heights: Definite,
    /// The node this one hangs under, or `None` for the page root, read for
    /// its `display`: a block parent stretches an `auto` width and a flex
    /// one does not.
    parent: Option<&'above meo_canvas_scene::node::Node>,
    /// The measurer this solve is running with.
    ///
    /// Here so a leaf's intrinsic size can be asked for while its style is
    /// being built, which is before taffy has asked anything.
    measure: &'above mut M,
}

/// Two answers, one level apart: `parent` is what this node's own percentages
/// resolve against, and `own` is what its children's do.
#[derive(Debug, Clone, Copy)]
struct Definite {
    /// Whether the containing block's height is definite.
    parent: bool,
    /// Whether this node's height is, for the children inside it.
    own: bool,
}

/// Whether a percentage inside a child can resolve against its height. `auto`
/// counts where flex settles it, stretched or grown, and not out of flow:
/// Chrome gives a `min-height: 200%` grandchild 240 and 20 in the two cases. A
/// `min-height` alone is not enough.
fn child_height_is_definite(
    parent: &meo_canvas_scene::node::Node,
    child: &meo_canvas_scene::node::Node,
    parent_is_definite: bool,
) -> bool {
    match child.layout.size.1 {
        Dimension::Points(_) => true,
        // An out-of-flow box does not contribute to its containing block's
        // height, so nothing is circular. `parent_is_definite` answers about
        // the flex parent, which for such a box is not its containing block.
        Dimension::Percent(_) => out_of_flow(child) || parent_is_definite,
        Dimension::Auto => {
            insets_settle_it(child)
                || ratio_settles_it(child)
                || parent_is_definite && flex_settles_it(parent, child)
        }
    }
}

/// Whether this node is replaced in CSS's sense: a childless `Image`, whose
/// `auto` size is intrinsic and whose over-constrained insets drop rather than
/// stretch it (`replaced-insets.tsv`). Childless, matching the measurer; Chrome
/// cannot arbitrate, an `<img>` having no children.
const fn is_replaced(node: &meo_canvas_scene::node::Node) -> bool {
    node.children.is_empty()
        && matches!(node.kind, meo_canvas_scene::node::NodeKind::Image { .. })
}

/// Drops the end inset on an over-constrained axis of a replaced element, as
/// Chrome does (`replaced-insets.tsv`); taffy has no notion of replaced. A lone
/// end inset still positions, as `a_lone_end_inset_still_positions` pins, and a
/// declared size wins.
const fn unstretch_replaced(
    style: &mut taffy::Style,
    source: &meo_canvas_scene::node::Node,
) {
    if !is_replaced(source) || !out_of_flow(source) {
        return;
    }
    let layout = &source.layout;
    // Asked of the scene's own style rather than of taffy's, for the same
    // reason `insets_settle_it` does: `Dimension::Auto` and `Option::None` are
    // this crate's vocabulary, and reading them back out of the converted
    // struct would be a second, weaker statement of what was just written.
    if matches!(layout.size.0, Dimension::Auto)
        && layout.inset.left.is_some()
        && layout.inset.right.is_some()
    {
        style.inset.right = taffy::LengthPercentageAuto::auto();
    }
    if matches!(layout.size.1, Dimension::Auto)
        && layout.inset.top.is_some()
        && layout.inset.bottom.is_some()
    {
        style.inset.bottom = taffy::LengthPercentageAuto::auto();
    }
}

/// Whether this box is taken out of the flow.
const fn out_of_flow(node: &meo_canvas_scene::node::Node) -> bool {
    matches!(
        node.layout.position_type,
        PositionType::Absolute | PositionType::Fixed
    )
}

/// Whether an out-of-flow box's height comes from its insets: `top` and
/// `bottom` both, which state a height as `height` does, where either alone
/// states a position. Both cases are rows in `absolute-percentage.tsv`.
const fn insets_settle_it(node: &meo_canvas_scene::node::Node) -> bool {
    // Not for a replaced element: insets do not settle its height
    // (`replaced-insets.tsv`). Its intrinsic height may serve a percentage
    // child all the same; that is unmeasured.
    !is_replaced(node)
        && out_of_flow(node)
        && node.layout.inset.top.is_some()
        && node.layout.inset.bottom.is_some()
}

/// Gives taffy a replaced element's intrinsic ratio, so its block algorithm
/// derives the height (`l7aromeo/meo-canvas#94`), and with both axes `auto`
/// under a block parent its intrinsic width, which block flow would stretch:
/// 60x40, not 200x133. An author's ratio wins.
fn intrinsic_sizes_it<M>(
    style: &mut taffy::Style,
    node: NodeId,
    source: &meo_canvas_scene::node::Node,
    parent: Option<&meo_canvas_scene::node::Node>,
    measure: &mut M,
) where
    M: Measure + ?Sized,
{
    if !is_replaced(source) {
        return;
    }
    // Asked of the measurer, which with neither axis known answers with the
    // intrinsic size (`fit_intrinsic`'s last arm), rather than of the image by
    // a second route.
    let intrinsic = measure
        .measure(
            node,
            (None, None),
            (Available::MaxContent, Available::MaxContent),
        )
        .size;
    if intrinsic.width <= 0.0 || intrinsic.height <= 0.0 {
        return;
    }
    let ratio = intrinsic.width / intrinsic.height;
    if !usable_ratio(ratio) {
        return;
    }
    if style.aspect_ratio.is_none() {
        style.aspect_ratio = Some(ratio);
    }
    if !out_of_flow(source)
        && parent.is_some_and(|parent| parent.layout.display == Display::Block)
        && matches!(source.layout.size.0, Dimension::Auto)
        && matches!(source.layout.size.1, Dimension::Auto)
    {
        style.size.width =
            taffy::Dimension::length(intrinsic.width * LAYOUT_SCALE);
    }
}

/// Whether a ratio derives this box's height from its width, which need not be
/// declared: a shrink-to-fit width serves in Chrome
/// (`aspect-ratio-percentage.tsv`). [`usable_ratio`] decides what counts, as in
/// [`to_taffy_style`].
fn ratio_settles_it(node: &meo_canvas_scene::node::Node) -> bool {
    node.layout.aspect_ratio.is_some_and(usable_ratio)
}

/// How far a derived cross size may sit from `main x ratio`: float residue, the
/// operands being unrounded. Below `0.6`, the tables' smallest real difference,
/// and above their accumulated error; no absolute value serves up to
/// `FINITE_CEILING`, as `l7aromeo/meo-canvas#142` records.
const DERIVED_TOLERANCE: f32 = 0.01;

/// Whether a declared ratio is one: positive and finite, or Chrome lays the box
/// out as though none were declared (120x50 under `0`, `-2` or
/// `calc(infinity)`). Shared by [`to_taffy_style`] and [`ratio_settles_it`].
const fn usable_ratio(ratio: f32) -> bool {
    ratio.is_finite() && ratio > 0.0
}

/// Whether flex layout gives this child a height its own contents did not.
fn flex_settles_it(
    parent: &meo_canvas_scene::node::Node,
    child: &meo_canvas_scene::node::Node,
) -> bool {
    // An out-of-flow box is not a flex item, so an `auto` height is its
    // content's: a `min-height: 200%` child of an absolutely positioned box is
    // 20 in Chrome, not 40.
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

/// Gives the page root the scene's extent on any axis it leaves to content: a
/// page is the canvas, so percentages and centring beneath it resolve against
/// the surface. An explicit size on the root is honoured.
fn pin_page_root(
    scene: &Scene,
    page: NodeId,
    root: taffy::NodeId,
    tree: &mut taffy::TaffyTree<NodeId>,
) -> Result<(), Error> {
    let source = scene.get(page).ok_or_else(|| {
        Error::Layout(format!("node {} is not in the scene", page.get()))
    })?;

    let mut style = to_taffy_style(&source.layout, source.paint.border_style);
    unstretch_replaced(&mut style, source);
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

/// Whether a node is a containing block for the absolute boxes beneath it:
/// positioned, or transformed (CSS Transforms 1 §3). Shared with the painter,
/// whose clip follows the same rule. A fixed box passes positioned ancestors,
/// so its caller tests the transform alone.
pub(crate) const fn is_containing_block(
    node: &meo_canvas_scene::node::Node,
) -> bool {
    !matches!(node.layout.position_type, PositionType::Static)
        || node.effects.transform.is_some()
}

/// The measure closure's body, as a function both passes of the workaround
/// below can call.
fn measure_leaf<M>(
    inputs: taffy::LayoutInput,
    context: Option<&mut NodeId>,
    style: &taffy::Style,
    measure: &mut M,
    baselines: &mut HashMap<NodeId, f32>,
) -> taffy::LayoutOutput
where
    M: Measure + ?Sized,
{
    // A node with no context is a container taffy sizes from its
    // children; only the leaves built with `new_leaf_with_context`
    // reach the measurer.
    let node = context.map(|context| *context);

    // `compute_leaf_layout` gives the leaf its padding, border, box sizing,
    // ratio and clamps, as every other leaf gets. Its `calc` resolver answers
    // zero: `calc` is off, so nothing reaches it.
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

    // A measurer works in the content box and CSS measures a flex item's
    // baseline from its border box, so the leaf's top padding and border are
    // added, percentages resolving against the containing block's inline size.
    output.baselines =
        taffy::Baselines::from_first(first_baseline.map(|baseline| {
            let top = style
                .padding
                .top
                .resolve_or_zero(inputs.parent_size.width, |_, _| 0.0)
                + style
                    .border
                    .top
                    .resolve_or_zero(inputs.parent_size.width, |_, _| 0.0);
            baseline + top
        }));
    output
}

/// One solve of the whole page.
fn solve_once<M>(
    tree: &mut taffy::TaffyTree<NodeId>,
    root: taffy::NodeId,
    available: taffy::Size<taffy::AvailableSpace>,
    measure: &mut M,
    baselines: &mut HashMap<NodeId, f32>,
) -> Result<(), Error>
where
    M: Measure + ?Sized,
{
    tree.compute_layout_with_measure(
        root,
        available,
        |inputs, _taffy_node, context, style| {
            measure_leaf(inputs, context, style, measure, baselines)
        },
    )
    .map_err(|error| Error::Layout(error.to_string()))
}

/// The boxes the ratio workaround applies to.
///
/// A usable ratio and nothing stated on either axis, minus any box entangled
/// with another such box. See [`compensate_ratio_direction`].
fn ratio_direction_candidates(
    scene: &Scene,
    tree: &taffy::TaffyTree<NodeId>,
    to_scene: &HashMap<taffy::NodeId, NodeId>,
) -> Vec<(taffy::NodeId, f32)> {
    let found: Vec<(taffy::NodeId, f32)> = to_scene
        .iter()
        .filter_map(|(taffy_id, scene_id)| {
            let source = scene.get(*scene_id)?;
            let ratio =
                source.layout.aspect_ratio.filter(|r| usable_ratio(*r))?;
            (matches!(source.layout.size.0, Dimension::Auto)
                && matches!(source.layout.size.1, Dimension::Auto))
            .then_some((*taffy_id, ratio))
        })
        .collect();

    // A ratio box nested in another is left alone: the ratio-free solve
    // misreads both (the outer sees 10, not 35.28), and taffy already agrees
    // with Chrome on the pair, `nested-ratio-outer` 35.28 and
    // `nested-ratio-inner` 83.00.
    let entangled: Vec<taffy::NodeId> = found
        .iter()
        .filter_map(|(id, _)| {
            let mut walk = tree.parent(*id);
            while let Some(parent) = walk {
                if found.iter().any(|(other, _)| *other == parent) {
                    return Some(vec![*id, parent]);
                }
                walk = tree.parent(parent);
            }
            None
        })
        .flatten()
        .collect();
    found
        .into_iter()
        .filter(|(id, _)| !entangled.contains(id))
        .collect()
}

/// Takes the ratio off every candidate, so the first solve reports a
/// fit-content inline size.
fn clear_ratios(
    tree: &mut taffy::TaffyTree<NodeId>,
    candidates: &[(taffy::NodeId, f32)],
) -> Result<(), Error> {
    for (id, _) in candidates {
        let mut style = tree
            .style(*id)
            .map_err(|error| Error::Layout(error.to_string()))?
            .clone();
        style.aspect_ratio = None;
        tree.set_style(*id, style)
            .map_err(|error| Error::Layout(error.to_string()))?;
    }
    Ok(())
}

// [WORKAROUND] taffy derives a ratio box's width from its height where Chrome
// derives the height from a fit-content width. `DioxusLabs/taffy#804`, tracked
// as `l7aromeo/meo-canvas#97`, probed by
// `crates/meo-canvas-core/tests/taffy_ratio_direction.rs`.
/// Restores the ratios, pins the ratio-free solve's fit-content width where
/// the derived height wins, and solves again; where content or a `min-height`
/// wins, taffy is already right. Retires when
/// `a_ratio_box_derives_its_width_from_its_height` fails.
fn compensate_ratio_direction<M>(
    tree: &mut taffy::TaffyTree<NodeId>,
    candidates: &[(taffy::NodeId, f32)],
    root: taffy::NodeId,
    available: taffy::Size<taffy::AvailableSpace>,
    measure: &mut M,
    baselines: &mut HashMap<NodeId, f32>,
) -> Result<(), Error>
where
    M: Measure + ?Sized,
{
    if candidates.is_empty() {
        return Ok(());
    }

    // What the ratio-free solve reported, kept because the second workaround
    // below compares against it. A `Vec` rather than a map because the
    // candidate list is the iteration order in both places.
    let mut cleared: Vec<(taffy::NodeId, f32, f32)> = Vec::new();
    let mut pins: Vec<(taffy::NodeId, f32)> = Vec::new();
    // [WORKAROUND] taffy transfers a ratio only to an item with its own cross
    // size, so a grown empty item is 0 wide where Chrome gives 248.
    // `DioxusLabs/taffy#804`, `l7aromeo/meo-canvas#123`; retires when its
    // `a_grown_main_size_never_reaches_the_ratio` probe fails.
    let mut derive: Vec<(taffy::NodeId, taffy::Size<f32>)> = Vec::new();
    // [WORKAROUND] taffy gives a stretched ratio item no automatic minimum
    // (Flexbox 1 §4.5): 424x248 where three engines give 424x424.
    // `DioxusLabs/taffy#351`, `l7aromeo/meo-canvas#147`; retires with the
    // `align-items: stretch` row of that same probe.
    let mut stretched: Vec<(taffy::NodeId, f32)> = Vec::new();
    for (id, ratio) in candidates {
        let solved = tree
            .layout(*id)
            .map_err(|error| Error::Layout(error.to_string()))?;
        cleared.push((*id, solved.size.width, solved.size.height));
        // The stretch owns this item's cross size, which §9.8 makes definite
        // and the ratio transfers from; the `l7aromeo/meo-canvas#123`
        // derivation is for an item with none. Firing it here takes `ratio 2`
        // in `ratio-stretch-main.tsv` to 496x248, not 424x248.
        if let Some(minimum) =
            stretched_ratio_minimum(tree, *id, *ratio, solved)
        {
            stretched.push((*id, minimum));
            continue;
        }
        // A `min-width` equal to the solved width is the minimum winning, which
        // taffy already transfers (`l7aromeo/meo-canvas#126`), so no pin. An
        // identity test, so no band; the solved width, not the minimum's
        // presence, decides.
        let min_binds = tree
            .style(*id)
            .ok()
            .and_then(|style| {
                style.min_size.width.resolve_to_option(
                    containing_inline_size(tree, *id),
                    |_, _| 0.0,
                )
            })
            .is_some_and(|value| value >= solved.size.width);
        if solved.size.width / ratio >= solved.size.height && !min_binds {
            pins.push((*id, solved.size.width));
            // The pin leaves here, so no node takes both arms. They do not
            // partition the candidates: one whose cross size is already the
            // ratio's takes neither.
            continue;
        }
        if let Some(size) = derived_cross(tree, *id, *ratio, solved.size) {
            derive.push((*id, size));
        }
    }

    for (id, ratio) in candidates {
        let mut style = tree
            .style(*id)
            .map_err(|error| Error::Layout(error.to_string()))?
            .clone();
        style.aspect_ratio = Some(*ratio);
        // [WORKAROUND] the transferred minimum of `l7aromeo/meo-canvas#147`,
        // written as a length so the re-solve carries it. The block above
        // carries the derivation and the probe.
        if let Some((_, minimum)) =
            stretched.iter().find(|(other, _)| *other == *id)
        {
            if parent_is_row(tree, *id) {
                style.min_size.width =
                    taffy::LengthPercentageAuto::length(*minimum);
            } else {
                style.min_size.height =
                    taffy::LengthPercentageAuto::length(*minimum);
            }
        }
        // The derivation taffy did not make, on both axes, for the re-solve. A
        // written cross drags the main down only where a `max-width` transfers
        // through the ratio, which is why clamping here does not fix
        // `l7aromeo/meo-canvas#129`.
        let cross_is_height = parent_is_row(tree, *id);
        if let Some((_, size)) = derive.iter().find(|(node, _)| node == id) {
            let mut size = *size;
            // Away from the pin's boundary this clamp is a no-op. At it, `(L *
            // r) / r` misses `L` by a ULP in `f32`, the node lands here, and it
            // bites by at most one ULP of the limit; kept as one statement
            // about the cross axis.
            if let Some(limit) = take_cross_maximum(&mut style, cross_is_height)
            {
                if cross_is_height {
                    size.height = size.height.min(limit);
                } else {
                    size.width = size.width.min(limit);
                }
            }
            style.size.width = taffy::Dimension::length(size.width);
            style.size.height = taffy::Dimension::length(size.height);
        }
        if let Some((_, width)) = pins.iter().find(|(pinned, _)| pinned == id) {
            if let Some(limit) = take_cross_maximum(&mut style, cross_is_height)
            {
                // In a row the pin writes the *main* axis, so the cross is
                // what the ratio derives from it; in a column the pinned
                // width is itself the cross.
                let cross = if cross_is_height {
                    *width / ratio
                } else {
                    *width
                };
                let clamped = cross.min(limit);
                if cross_is_height {
                    style.size.height = taffy::Dimension::length(clamped);
                } else {
                    style.size.width = taffy::Dimension::length(clamped);
                }
            }
            // The pinned width stays on the style: `collect` reads the layout,
            // but a later pass reading `tree.style()` would see a width the
            // author never wrote.
            style.size.width = taffy::Dimension::length(*width);
        }
        tree.set_style(*id, style)
            .map_err(|error| Error::Layout(error.to_string()))?;
    }

    // Cleared because the second pass re-measures, and a stale baseline draws
    // text at the wrong `y`. Safe while it measures every leaf the first did:
    // the pin sets the inline axis alone, so a block size still comes from
    // measuring.
    baselines.clear();
    solve_once(tree, root, available, measure, baselines)?;

    floor_ratio_heights(
        tree, candidates, &cleared, &pins, root, available, measure, baselines,
    )
}

/// Every container whose own main size dropped a child's negative margin, and
/// by how much. Chrome applies every margin in all 33 measured cells, so each
/// clause below describes taffy's defect and none a CSS rule.
fn dropped_margin_candidates(
    scene: &Scene,
    tree: &taffy::TaffyTree<NodeId>,
    to_scene: &HashMap<taffy::NodeId, NodeId>,
) -> Vec<(taffy::NodeId, f32)> {
    to_scene
        .iter()
        .filter_map(|(taffy_id, scene_id)| {
            let container = scene.get(*scene_id)?;
            // Column containers with an automatic main size, which is where
            // both defects live: a definite main size is correct in every
            // measured cell, and so is every row-direction cell.
            if !matches!(container.layout.display, Display::Flex)
                || !matches!(
                    container.layout.flex_direction,
                    FlexDirection::Column | FlexDirection::ColumnReverse
                )
                || !matches!(container.layout.size.1, Dimension::Auto)
            {
                return None;
            }
            // The containing block's content-box width, which a percentage
            // margin resolves against: `-10%` with 20px of padding is Chrome's
            // `-86.30`, pinned by
            // `a_percentage_margin_resolves_against_the_content_box`.
            let basis = content_inline_size(tree, *taffy_id);
            let dropped = children_of(tree, *taffy_id)
                .into_iter()
                .filter_map(|child| {
                    let source = scene.get(*to_scene.get(&child)?)?;
                    let margin = main_axis_margin(&source.layout, basis);
                    (margin < 0.0
                        && (source.layout.flex_grow > 0.0
                            || holds_clipping_grid_item(
                                scene, tree, to_scene, child,
                            )))
                    .then_some(margin)
                })
                .sum::<f32>();
            (dropped < 0.0).then_some((*taffy_id, dropped))
        })
        .collect()
}

/// A node's own main-axis margin in a column, in points. A percentage resolves
/// against `basis`, the container's inline size, since taffy drops it as it
/// drops a length. An `auto` edge counts zero and the other edge still counts:
/// Chrome gives 176 for `auto` beside `-24` in a 200 column.
fn main_axis_margin(layout: &LayoutStyle, basis: f32) -> f32 {
    let resolved = |dimension: Dimension| match dimension {
        Dimension::Points(points) => points,
        Dimension::Percent(fraction) => fraction * basis,
        // Zero rather than absent: every edge resolves now, so this returns a
        // number rather than an `Option` nothing could make empty. The earlier
        // signature said *this may not resolve* and, once the `auto` edge
        // stopped being a reason to skip the child, nothing could produce that.
        Dimension::Auto => 0.0,
    };
    resolved(layout.margin.top) + resolved(layout.margin.bottom)
}

/// Whether a subtree holds a grid container with a `Hidden` or `Scroll` direct
/// item, the values taffy gives an automatic minimum of zero. [`clips`] asks
/// about scroll containers; the two part on `Clip`, which `Overflow` lacks.
fn holds_clipping_grid_item(
    scene: &Scene,
    tree: &taffy::TaffyTree<NodeId>,
    to_scene: &HashMap<taffy::NodeId, NodeId>,
    id: taffy::NodeId,
) -> bool {
    let Some(source) =
        to_scene.get(&id).and_then(|scene_id| scene.get(*scene_id))
    else {
        return false;
    };
    let children = children_of(tree, id);
    if matches!(source.layout.display, Display::Grid)
        && children.iter().any(|child| {
            to_scene
                .get(child)
                .and_then(|scene_id| scene.get(*scene_id))
                .is_some_and(|item| {
                    let zero_minimum = |overflow| {
                        matches!(overflow, Overflow::Hidden | Overflow::Scroll)
                    };
                    zero_minimum(item.layout.overflow.0)
                        || zero_minimum(item.layout.overflow.1)
                })
        })
    {
        return true;
    }
    // Depth does not matter: measured with none, one and two plain boxes
    // between the margined child and the grid, and the ancestor is short in all
    // three.
    children
        .into_iter()
        .any(|child| holds_clipping_grid_item(scene, tree, to_scene, child))
}

// [WORKAROUND] taffy resolves a column's automatic main size without the
// negative margins it should apply. `DioxusLabs/taffy#1162` and
// `DioxusLabs/taffy#1163`, tracked as `l7aromeo/meo-canvas#107`; retires when
// `crates/meo-canvas-core/tests/taffy_negative_margin.rs` fails.
/// Writes the main size taffy should have reached, and solves again. One handle
/// serves both defects, which share the repair; they are collected apart
/// because they rest on opposite facts about the tree.
fn compensate_dropped_margins<M>(
    scene: &Scene,
    tree: &mut taffy::TaffyTree<NodeId>,
    to_scene: &HashMap<taffy::NodeId, NodeId>,
    root: taffy::NodeId,
    available: taffy::Size<taffy::AvailableSpace>,
    measure: &mut M,
    baselines: &mut HashMap<NodeId, f32>,
) -> Result<(), Error>
where
    M: Measure + ?Sized,
{
    let candidates = dropped_margin_candidates(scene, tree, to_scene);
    if candidates.is_empty() {
        return Ok(());
    }
    for (id, dropped) in &candidates {
        let solved = tree
            .layout(*id)
            .map_err(|error| Error::Layout(error.to_string()))?
            .size
            .height;
        let mut style = tree
            .style(*id)
            .map_err(|error| Error::Layout(error.to_string()))?
            .clone();
        style.size.height = taffy::Dimension::length(solved + dropped);
        tree.set_style(*id, style)
            .map_err(|error| Error::Layout(error.to_string()))?;
    }
    solve_once(tree, root, available, measure, baselines)
}

/// A node's children, or nothing if taffy will not say.
fn children_of(
    tree: &taffy::TaffyTree<NodeId>,
    id: taffy::NodeId,
) -> Vec<taffy::NodeId> {
    tree.children(id).unwrap_or_default()
}

/// The inline size a percentage on this node resolves against: the parent's
/// content box, as Chrome gives `min-width: 70%` of a 440 box with 8px padding
/// `296.80`, not `308`. Zero without a parent, which binds nothing.
fn containing_inline_size(
    tree: &taffy::TaffyTree<NodeId>,
    id: taffy::NodeId,
) -> f32 {
    tree.parent(id)
        .map_or(0.0, |parent| content_inline_size(tree, parent))
}

// [WORKAROUND] taffy sizes a flex container from its items' content where
// Flexbox §9.2 uses their hypothetical main sizes, so `flex-basis: 0` with
// `min-height: 0` is ignored. `l7aromeo/meo-canvas#145`, no upstream issue;
// probed by `crates/meo-canvas-core/tests/taffy_definite_basis.rs`.
/// Writes §9.2's hypothetical main size as a definite size where it owes
/// nothing to the content; §9.7 still grows it from there. Not for a grid item,
/// which has no `flex-basis`, nor on an inline main axis, which §9.9 governs.
fn collapse_definite_bases(
    tree: &mut taffy::TaffyTree<NodeId>,
    root: taffy::NodeId,
) -> Result<(), Error> {
    let mut pending = vec![root];
    let mut writes: Vec<(taffy::NodeId, bool, f32)> = Vec::new();
    while let Some(id) = pending.pop() {
        let children = tree
            .children(id)
            .map_err(|error| Error::Layout(error.to_string()))?;
        pending.extend(children.iter().copied());

        let style = tree
            .style(id)
            .map_err(|error| Error::Layout(error.to_string()))?;
        // Flex only: the exclusion above, expressed as the one thing it is
        // about rather than as a list of displays.
        if style.display != taffy::Display::Flex {
            continue;
        }
        // The inline-axis exclusion: that main axis is sized by max-content,
        // which §9.9 governs (`DioxusLabs/taffy#351`, `DioxusLabs/taffy#1182`).
        // `row-direction mirror` in `flex-basis-collapse.tsv` pins it at 1024
        // in Chrome and taffy.
        if !matches!(
            style.flex_direction,
            taffy::FlexDirection::Column | taffy::FlexDirection::ColumnReverse
        ) {
            continue;
        }

        // Named here rather than assumed by the helper: the guard above is
        // the whole of the scope decision, so widening it shows up in the
        // `row-direction mirror` control instead of being caught a second
        // time by a helper that only ever read the block axis.
        let row = matches!(
            style.flex_direction,
            taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
        );

        for child in children {
            let child_style = tree
                .style(child)
                .map_err(|error| Error::Layout(error.to_string()))?;
            let Some(hypothetical) = hypothetical_main_size(child_style, row)
            else {
                continue;
            };
            writes.push((child, row, hypothetical));
        }
    }

    for (id, row, size) in writes {
        let mut style = tree
            .style(id)
            .map_err(|error| Error::Layout(error.to_string()))?
            .clone();
        if row {
            style.size.width = taffy::Dimension::length(size);
        } else {
            style.size.height = taffy::Dimension::length(size);
        }
        tree.set_style(id, style)
            .map_err(|error| Error::Layout(error.to_string()))?;
    }
    Ok(())
}

/// §9.2's hypothetical main size where it owes nothing to the content: `None`
/// for an `auto` basis, a percentage basis against an indefinite container, and
/// an `auto` minimum, §4.5's floor at the content.
fn hypothetical_main_size(style: &taffy::Style, row: bool) -> Option<f32> {
    let taffy::ExpandedDimension::Length(basis) = style.flex_basis.expand()
    else {
        return None;
    };
    // A percentage minimum against an indefinite main size is a floor of zero,
    // where on a basis it is `auto`: `min 0%` and `basis 0%` in
    // `flex-basis-collapse.tsv` separate them, Chrome collapsing one only.
    let main_min = if row {
        style.min_size.width
    } else {
        style.min_size.height
    };
    let floor = match main_min.expand() {
        taffy::ExpandedLengthPercentageAuto::Length(value) => value,
        taffy::ExpandedLengthPercentageAuto::Percent(_) => 0.0,
        taffy::ExpandedLengthPercentageAuto::Auto => return None,
    };
    let main_max = if row {
        style.max_size.width
    } else {
        style.max_size.height
    };
    let ceiling = match main_max.expand() {
        taffy::ExpandedLengthPercentageAuto::Length(value) => Some(value),
        _ => None,
    };
    // Clamped the way §9.2 clamps: the minimum last, so a minimum above the
    // maximum wins, which is what CSS says and what a bare `clamp` would get
    // backwards.
    let mut size = basis;
    if let Some(ceiling) = ceiling
        && size > ceiling
    {
        size = ceiling;
    }
    if size < floor {
        size = floor;
    }
    Some(size)
}

/// A solved node's own content-box width, which a child's percentage resolves
/// against: the border box less padding, border and any scrollbar.
fn content_inline_size(
    tree: &taffy::TaffyTree<NodeId>,
    id: taffy::NodeId,
) -> f32 {
    tree.layout(id).map_or(0.0, |solved| {
        solved.size.width
            - solved.padding.left
            - solved.padding.right
            - solved.border.left
            - solved.border.right
            - solved.scrollbar_size.width
    })
}

/// The main-axis minimum a stretched item's ratio owes it by Flexbox §4.5, each
/// `None` a sentence of the text with its row in `ratio-stretch-main.tsv`.
/// Returned where it does not bind too: `Some` means stretched, which must not
/// reach the `l7aromeo/meo-canvas#123` derivation.
fn stretched_ratio_minimum(
    tree: &taffy::TaffyTree<NodeId>,
    id: taffy::NodeId,
    ratio: f32,
    solved: &taffy::Layout,
) -> Option<f32> {
    let parent = tree.parent(id)?;
    let container = tree.style(parent).ok()?;
    // §9.8 says *flex container* and says *single-line*: a wrapped line takes
    // its cross size from its items, so there is no container cross size for
    // the item to be stretched to. `container wrap`.
    if container.display != taffy::Display::Flex
        || container.flex_wrap != taffy::FlexWrap::NoWrap
    {
        return None;
    }
    let style = tree.style(id).ok()?;
    // Stretch is the initial value, so a container saying nothing is saying
    // stretch -- `align-items default` is what keeps the `None` arm here.
    // `no stretch` is what keeps the rest of the match.
    let align = style.align_self.or(container.align_items);
    if !align.is_none_or(|value| value == taffy::AlignItems::STRETCH) {
        return None;
    }
    let row = matches!(
        container.flex_direction,
        taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
    );
    let (min_main, overflow_main, max_main) = if row {
        (style.min_size.width, style.overflow.x, style.max_size.width)
    } else {
        (
            style.min_size.height,
            style.overflow.y,
            style.max_size.height,
        )
    };
    // §4.5 gives an automatic minimum only where the main-axis minimum is
    // `auto`; any definite minimum replaces it. `escape min-height 0`.
    if !matches!(min_main.expand(), taffy::ExpandedLengthPercentageAuto::Auto) {
        return None;
    }
    // §4.5: "for main-axis scroll containers the automatic minimum size is
    // zero, as usual". `escape overflow hidden`.
    if overflow_main != taffy::Overflow::Visible {
        return None;
    }
    // The transferred size suggestion. The ratio is width over height, so it
    // divides where the main axis is the block one and multiplies where it is
    // the inline one. `ratio 0.5` and `row container` are the two rows that
    // would swap if this were the other way round.
    let (cross, main) = if row {
        (solved.size.height, solved.size.width)
    } else {
        (solved.size.width, solved.size.height)
    };
    let transferred = if row { cross * ratio } else { cross / ratio };
    if !transferred.is_finite() {
        return None;
    }
    // The larger of the transferred and content suggestions, `main` carrying
    // the second, then §4.5's clamp by a definite maximum. `item content
    // taller` and `escape max-height 248px` pin the two; WebKit alone differs
    // on the clamp.
    let mut minimum = transferred.max(main);
    let ceiling = match max_main.expand() {
        taffy::ExpandedLengthPercentageAuto::Length(value) => Some(value),
        taffy::ExpandedLengthPercentageAuto::Percent(fraction) => {
            Some(fraction * content_main_size(tree, parent, row))
        }
        taffy::ExpandedLengthPercentageAuto::Auto => None,
    };
    if let Some(ceiling) = ceiling
        && minimum > ceiling
    {
        minimum = ceiling;
    }
    Some(minimum)
}

/// The content-box extent of a solved node on the axis named.
fn content_main_size(
    tree: &taffy::TaffyTree<NodeId>,
    id: taffy::NodeId,
    row: bool,
) -> f32 {
    if row {
        return content_inline_size(tree, id);
    }
    tree.layout(id).map_or(0.0, |solved| {
        solved.size.height
            - solved.padding.top
            - solved.padding.bottom
            - solved.border.top
            - solved.border.bottom
            - solved.scrollbar_size.height
    })
}

/// Whether this node's parent lays out as a row, so its cross axis is the
/// block one.
fn parent_is_row(tree: &taffy::TaffyTree<NodeId>, id: taffy::NodeId) -> bool {
    tree.parent(id)
        .and_then(|parent| tree.style(parent).ok())
        .is_some_and(|style| {
            matches!(
                style.flex_direction,
                taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
            )
        })
}

// [WORKAROUND] taffy clamps the main size with a cross maximum transferred
// through the ratio, where Chrome clamps only its own axis: `max-width: 100px`
// at ratio 1 is 100x100, not 100x248. `l7aromeo/meo-canvas#129`, probed by
// `crates/meo-canvas-core/tests/taffy_flex_ratio.rs`.
/// Removes the cross-axis maximum from `style` and returns it for the caller to
/// apply, leaving taffy nothing to transfer; the clamp reads `max_size`, so
/// pre-clamping does nothing. Lengths only: a percentage maximum is unmeasured.
fn take_cross_maximum(
    style: &mut taffy::Style,
    cross_is_height: bool,
) -> Option<f32> {
    let slot = if cross_is_height {
        &mut style.max_size.height
    } else {
        &mut style.max_size.width
    };
    let taffy::ExpandedLengthPercentageAuto::Length(limit) = slot.expand()
    else {
        return None;
    };
    *slot = taffy::LengthPercentageAuto::auto();
    Some(limit)
}

/// The size a ratio'd item should have taken, or `None` if it has it, read from
/// the solved tree so one condition covers every cause. Flex parents only, on
/// their main axis; `ratio_row_cross.rs` pins the row arm off ratio 1.
fn derived_cross(
    tree: &taffy::TaffyTree<NodeId>,
    id: taffy::NodeId,
    ratio: f32,
    solved: taffy::Size<f32>,
) -> Option<taffy::Size<f32>> {
    let parent = tree.parent(id)?;
    let parent_style = tree.style(parent).ok()?;
    if !matches!(parent_style.display, taffy::Display::Flex) {
        return None;
    }
    let row = matches!(
        parent_style.flex_direction,
        taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
    );
    let (main, cross) = if row {
        (solved.width, solved.height)
    } else {
        (solved.height, solved.width)
    };
    let want = if row { main / ratio } else { main * ratio };
    if (cross - want).abs() <= DERIVED_TOLERANCE {
        return None;
    }
    Some(if row {
        taffy::Size {
            width: main,
            height: want,
        }
    } else {
        taffy::Size {
            width: want,
            height: main,
        }
    })
}

/// Whether this box is a scroll container, which removes the automatic minimum:
/// Chrome gives 300 under `visible` and `clip` and 117.64 under `hidden`,
/// `scroll` and `auto`. `Overflow` has no `Clip`; see
/// [`holds_clipping_grid_item`].
const fn clips(overflow: taffy::Point<taffy::Overflow>) -> bool {
    !matches!(overflow.x, taffy::Overflow::Visible)
        || !matches!(overflow.y, taffy::Overflow::Visible)
}

// [WORKAROUND] taffy has one minimum where CSS has two: a content floor written
// to `min_size.height` transfers into the width like an author's `min-height`,
// 255 wide for a 100-wide box. `l7aromeo/meo-canvas#104`, probed by
// `crates/meo-canvas-core/tests/taffy_ratio_direction.rs`.
/// Restores the automatic minimum block size taffy omits, from the shared
/// ratio-free solve, where the inline size is not the ratio's outcome and the
/// box does not clip. Retires when
/// `a_content_derived_floor_is_transferred_into_the_width` fails.
#[expect(
    clippy::too_many_arguments,
    reason = "the second half of one workaround, sharing the first half's \
              solve; splitting the arguments into a struct would hide that \
              they are the same pass's outputs"
)]
fn floor_ratio_heights<M>(
    tree: &mut taffy::TaffyTree<NodeId>,
    candidates: &[(taffy::NodeId, f32)],
    cleared: &[(taffy::NodeId, f32, f32)],
    pins: &[(taffy::NodeId, f32)],
    root: taffy::NodeId,
    available: taffy::Size<taffy::AvailableSpace>,
    measure: &mut M,
    baselines: &mut HashMap<NodeId, f32>,
) -> Result<(), Error>
where
    M: Measure + ?Sized,
{
    let mut floored: Vec<taffy::NodeId> = Vec::new();
    for (id, ratio) in candidates {
        // A pinned node has a definite width by now, so its width cannot move
        // and the test below would answer `false` for the wrong reason.
        if pins.iter().any(|(pinned, _)| pinned == id) {
            continue;
        }
        let Some((_, was_wide, was_tall)) =
            cleared.iter().find(|(cleared_id, _, _)| cleared_id == id)
        else {
            continue;
        };
        let style = tree
            .style(*id)
            .map_err(|error| Error::Layout(error.to_string()))?;
        if clips(style.overflow) {
            continue;
        }
        let solved = tree
            .layout(*id)
            .map_err(|error| Error::Layout(error.to_string()))?;
        let moved = (solved.size.width - was_wide).abs() > f32::EPSILON;
        if !moved && *was_tall > solved.size.width / ratio {
            floored.push(*id);
        }
    }

    if floored.is_empty() {
        return Ok(());
    }

    for id in &floored {
        let mut style = tree
            .style(*id)
            .map_err(|error| Error::Layout(error.to_string()))?
            .clone();
        style.aspect_ratio = None;
        tree.set_style(*id, style)
            .map_err(|error| Error::Layout(error.to_string()))?;
    }

    baselines.clear();
    solve_once(tree, root, available, measure, baselines)
}

/// Builds the taffy tree for a subtree, leaving out `Display::None`. taffy
/// resolves an out-of-flow child against its parent, so `Fixed` goes to the
/// page root and `Absolute` to its nearest positioned ancestor; paint walks the
/// scene.
fn build<M>(
    scene: &Scene,
    node: NodeId,
    tree: &mut taffy::TaffyTree<NodeId>,
    to_scene: &mut HashMap<taffy::NodeId, NodeId>,
    orphans: &mut Vec<taffy::NodeId>,
    captive: &mut Vec<taffy::NodeId>,
    above: FromAbove<'_, M>,
) -> Result<taffy::NodeId, Error>
where
    M: Measure + ?Sized,
{
    let FromAbove {
        heights,
        parent,
        measure,
    } = above;
    let source = scene.get(node).ok_or_else(|| {
        Error::Layout(format!("node {} is not in the scene", node.get()))
    })?;

    let mut style = to_taffy_style(&source.layout, source.paint.border_style);
    unstretch_replaced(&mut style, source);
    intrinsic_sizes_it(&mut style, node, source, parent, measure);
    // A block-axis percentage against an indefinite containing block resolves
    // to `auto`, which taffy does not do. Out of flow the containing block is
    // not the flex parent and is settled first, so it stands
    // (`absolute-percentage.tsv`).
    if !heights.parent && !out_of_flow(source) {
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
                FromAbove {
                    // A dangling id has no style to read, so it inherits this
                    // node's answers rather than being given ones of its own.
                    heights: Definite {
                        parent: heights.own,
                        own: heights.own,
                    },
                    parent: Some(source),
                    measure: &mut *measure,
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
            FromAbove {
                heights: Definite {
                    parent: heights.own,
                    own: child_height_is_definite(
                        source,
                        child_source,
                        heights.own,
                    ),
                },
                parent: Some(source),
                measure: &mut *measure,
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

    // A positioned or transformed box is a containing block (CSS Transforms 1
    // §3): Chrome places an absolute child at 50,20 under a `translateZ(0)`
    // clipper and 30,20 without. Anything unclaimed goes further up.
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

    // A childless node gets the measurer's context whatever it draws, since
    // layout holds no intrinsic sizes. `is_replaced` classifies rather than
    // sizes: a measured `Text` still stretches to opposing insets, 200x30 in
    // Chrome too.
    let created = if children.is_empty() {
        tree.new_leaf_with_context(style, node)
    } else {
        tree.new_with_children(style, &children)
    }
    .map_err(|error| Error::Layout(error.to_string()))?;

    to_scene.insert(created, node);
    Ok(created)
}

/// Walks the solved tree, summing taffy's parent-relative locations into
/// absolute rectangles, the form paint wants and taffy's rounding assumed.
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
pub fn to_taffy_style(
    layout: &LayoutStyle,
    border_style: BorderStyle,
) -> taffy::Style {
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
        aspect_ratio: layout.aspect_ratio.filter(|ratio| usable_ratio(*ratio)),

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
            let used = used_border(layout.border, border_style);
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

        // The scene spells `gap` `(row, column)`, as CSS does, and taffy
        // `(width, height)`: swapped at the crossing, the one place that has to
        // know.
        gap: taffy::Size {
            width: to_length(spacing(layout.gap.1)),
            height: to_length(spacing(layout.gap.0)),
        },

        flex_direction: to_flex_direction(layout.flex_direction),
        flex_wrap: to_flex_wrap(layout.flex_wrap),
        // A negative or non-finite factor is dropped to its own initial value,
        // 0 for grow and 1 for shrink.
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

/// The `inset` taffy is given: dropped for a static node, taffy having no
/// `Static`. Chrome keeps a static child with `top: 30px` at its flow position
/// in block, flex and grid alike.
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

/// taffy has two positions where CSS has three: `Static` and `Relative` both
/// become `Relative`, and inset and z-order, where they differ, are settled in
/// [`to_taffy_inset`] and `stacks_by_z_index` in [`crate::paint`].
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

/// Cross-axis placement of an item. `SpaceBetween` and `SpaceAround` belong to
/// `align-content`, so they fall to `stretch`, the initial value, as a browser
/// discards an undefined value.
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

/// Cross-axis distribution of wrapped lines. `Baseline` is not an
/// `align-content` value and falls to `stretch`, as in [`to_align_items`].
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

/// Chrome's layout grid, sixty-fourths of a pixel: snapped once, then summed
/// exactly, so five `10.3` boxes reach `51.484375` where exact sums tie at
/// `51.5`. Floored, as `LayoutUnit` truncates; see `rounding_drift.rs`.
const LAYOUT_GRID: f32 = 64.0;

/// A measured content size, snapped outward onto [`LAYOUT_GRID`]: a floored
/// measurement would claim the content fits a box it does not, and the next
/// pass would wrap the last word.
fn contains(points: f32) -> f32 {
    if points.is_finite() {
        (points * LAYOUT_GRID).ceil() / LAYOUT_GRID
    } else {
        points
    }
}

/// One length, snapped into [`LAYOUT_GRID`]. Percentages are not: Chrome snaps
/// the resolved value, and snapping a fraction would quantise a ratio.
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

/// A border width as CSS uses it: Chrome floors it to an integer at both device
/// scales, `3.5` to 3, but never below 1, so a `0.1px` hairline draws. Layout
/// and the painter both read it here.
pub(crate) fn used_border(
    border: Sides<f32>,
    style: BorderStyle,
) -> Sides<f32> {
    // `none` is a used width of zero, so a border that draws nothing reserves
    // nothing, the half no ink comparison can see.
    if style == BorderStyle::None {
        return Sides::all(0.0);
    }
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

/// A length for taffy's `LengthPercentageAuto` fields, where `Auto` is a
/// free-space margin, a flex item's automatic minimum, or no maximum, by field.
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

/// A size, min-size, max-size or flex basis with an unusable value neutralised
/// where both surfaces meet, `const` having no error to report. `NaN` is
/// dropped, infinity bounded as `calc(infinity)` clamps, and a negative
/// dropped.
const fn sized(dimension: Dimension) -> Dimension {
    match dimension {
        Dimension::Points(points) => {
            let points = bounded(points);
            if !points.is_finite() || points < 0.0 {
                Dimension::Auto
            } else {
                Dimension::Points(points)
            }
        }
        Dimension::Percent(fraction) => {
            let fraction = bounded(fraction);
            if !fraction.is_finite() || fraction < 0.0 {
                Dimension::Auto
            } else {
                Dimension::Percent(fraction)
            }
        }
        kept @ Dimension::Auto => kept,
    }
}

/// An infinite value replaced by the largest this engine carries, before each
/// guard below drops a `NaN`. Clamped as `calc(infinity)` clamps; dropped, a
/// `flex-shrink: Infinity` box left the layout.
const fn bounded(value: f32) -> f32 {
    if value.is_infinite() {
        return FINITE_CEILING.copysign(value);
    }
    value
}

/// A margin edge with an unusable value dropped. A negative margin is valid and
/// survives; a non-finite one falls to zero, not `auto`, which would absorb
/// free space and centre the box.
const fn margin(dimension: Dimension) -> Dimension {
    match dimension {
        Dimension::Points(points) if bounded(points).is_nan() => {
            Dimension::Points(0.0)
        }
        Dimension::Percent(fraction) if bounded(fraction).is_nan() => {
            Dimension::Points(0.0)
        }
        Dimension::Points(points) => Dimension::Points(bounded(points)),
        Dimension::Percent(fraction) => Dimension::Percent(bounded(fraction)),
        kept @ Dimension::Auto => kept,
    }
}

/// A padding or gap length with an unusable value dropped.
///
/// Both refuse negatives in CSS and both have an initial value of zero, so
/// there is one fallback rather than a choice.
const fn spacing(length: Length) -> Length {
    match length {
        Length::Points(points)
            if !bounded(points).is_finite() || points < 0.0 =>
        {
            Length::ZERO
        }
        Length::Percent(fraction)
            if !bounded(fraction).is_finite() || fraction < 0.0 =>
        {
            Length::ZERO
        }
        // No `kept` arm: both `Length` variants carry a number to bound, and a
        // third added upstream fails to compile here.
        Length::Points(points) => Length::Points(bounded(points)),
        Length::Percent(fraction) => Length::Percent(bounded(fraction)),
    }
}

/// A flex factor with an unusable value dropped to the initial value given.
///
/// The caller passes the initial value because CSS's differ: `flex-grow`
/// starts at 0 and `flex-shrink` at 1.
fn factor(value: f32, initial: f32) -> f32 {
    let value = bounded(value);
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        initial
    }
}

/// An inset edge with an unusable value dropped: a negative inset is valid, and
/// a non-finite one becomes `auto`, the edge taffy places.
const fn inset(edge: Option<Length>) -> Option<Length> {
    match edge {
        Some(Length::Points(points)) if bounded(points).is_nan() => None,
        Some(Length::Percent(fraction)) if bounded(fraction).is_nan() => None,
        Some(Length::Points(points)) => Some(Length::Points(bounded(points))),
        Some(Length::Percent(fraction)) => {
            Some(Length::Percent(bounded(fraction)))
        }
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

/// A track's sizing function as CSS's `minmax()` pair: `auto` is `minmax(auto,
/// auto)`, a length fills both bounds, and `<n>fr` is `minmax(auto, <n>fr)`.
/// Wrapped at the call site, taffy's alias being private.
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

/// A grid item's placement on one axis, `<line> / span <n>`: no start is
/// auto-placement, and no span is one track.
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
    use crate::FINITE_CEILING;

    /// Chrome's grid: sixty-fourths of a CSS pixel.
    const GRID: f32 = 64.0;

    #[test]
    fn a_length_is_truncated_into_sixty_fourths_rather_than_rounded() {
        // Here because the snap is private and a `LayoutResult` is rounded.
        // Chrome's `getBoundingClientRect().height`: `10.0234375`, an exact tie
        // at `641.5`, takes `641`, which no rounding mode gives; the last three
        // rows are controls.
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
            paint::BorderStyle,
        },
    };

    use super::{LayoutResult, solve};
    use crate::measure::{Available, Measure, MeasuredLeaf};

    /// A measurer that answers one size for every leaf and no baseline, for a
    /// solve without fonts; test-only, since every caller is in this crate.
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

    /// A measurer shaped like a text run: `MaxContent` is the run on one line
    /// and `MinContent` its longest unbreakable piece, a field because flexbox
    /// floors an item there (§4.5). `chrome_min_content.rs` measures the rule.
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

    /// A scene with one page whose root is a plain box laid out as flex, named
    /// because the scene's default is `block` and both surfaces build flex.
    fn scene_with_page(width: f32, height: f32) -> (Scene, NodeId) {
        let mut scene = Scene::new(Size::new(width, height));
        let page = scene
            .push_page()
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(page) {
            node.layout.display = Display::Flex;
        }
        (scene, page)
    }

    fn solved(scene: &Scene, page: NodeId) -> LayoutResult {
        solve(scene, page, &mut Fixed::new(0.0, 0.0))
            .unwrap_or_else(|error| unreachable!("{error}"))
    }

    /// A growing child's negative margin reaches the container's own height;
    /// without `compensate_dropped_margins` both come out at the child's
    /// height.
    #[test]
    fn a_growing_child_s_negative_margin_reaches_the_container() {
        let (mut scene, page) = scene_with_page(903.0, 2000.0);
        // A column page, content-sized: the shape the defect needs is a
        // container whose own main size is automatic. A row page stretches its
        // children on the cross axis, which is the height here, and the
        // container never resolves its own.
        scene.content_height = true;
        if let Some(node) = scene.get_mut(page) {
            node.layout.flex_direction = FlexDirection::Column;
        }
        let container = scene
            .push(page, Node::new(meo_canvas_scene::node::NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(container) {
            node.layout.display = Display::Flex;
            node.layout.flex_direction = FlexDirection::Column;
            node.layout.size = (Dimension::Points(903.0), Dimension::Auto);
        }
        let child = scene
            .push(container, Node::new(meo_canvas_scene::node::NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(child) {
            node.layout.size =
                (Dimension::Points(476.0), Dimension::Points(500.0));
            node.layout.flex_grow = 1.0;
            node.layout.margin.top = Dimension::Points(-24.0);
        }

        // A second child at `-10` beside `-24` tells a sum from a `min`: Chrome
        // gives 366 and a fold with `f32::min` 376.
        let second = scene
            .push(container, Node::new(meo_canvas_scene::node::NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(second) {
            node.layout.size =
                (Dimension::Points(476.0), Dimension::Points(200.0));
            node.layout.flex_grow = 1.0;
            node.layout.margin.top = Dimension::Points(-10.0);
        }
        // An `auto` edge beside the length one, which the first version of
        // `main_axis_margin` dropped the whole child for.
        if let Some(node) = scene.get_mut(second) {
            node.layout.margin.bottom = Dimension::Auto;
        }

        let result = solved(&scene, page);
        let container_height = result.rects[&container].size.height;
        let child_height = result.rects[&child].size.height;
        // Chrome's answer for this scene, measured on the page: the
        // container, both children, and the positions.
        assert!(
            (container_height - 666.0).abs() < 0.01,
            "children carry margins of -24 and -10 and the container came out \
             {container_height}; Chrome gives 666. 676 is what reaches this \
             assertion when only one of the two is applied, by whichever \
             route"
        );
        assert!(
            (child_height - 500.0).abs() < 0.01,
            "child {child_height}, Chrome 500"
        );
    }

    /// A percentage margin reaches the container too: taffy drops it on a
    /// growing child as it drops a length, 500 against Chrome's 409.70.
    #[test]
    fn a_percentage_margin_reaches_the_container() {
        let (mut scene, page) = scene_with_page(903.0, 2000.0);
        scene.content_height = true;
        if let Some(node) = scene.get_mut(page) {
            node.layout.flex_direction = FlexDirection::Column;
        }
        let container = scene
            .push(page, Node::new(meo_canvas_scene::node::NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(container) {
            node.layout.display = Display::Flex;
            node.layout.flex_direction = FlexDirection::Column;
            node.layout.size = (Dimension::Points(903.0), Dimension::Auto);
        }
        let child = scene
            .push(container, Node::new(meo_canvas_scene::node::NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(child) {
            node.layout.size =
                (Dimension::Points(476.0), Dimension::Points(500.0));
            node.layout.flex_grow = 1.0;
            node.layout.margin.top = Dimension::Percent(-0.10);
        }

        // Half a pixel: this path rounds and Chrome's 409.70 is not integral.
        // Uncompensated this is 500.
        let result = solved(&scene, page);
        let height = result.rects[&container].size.height;
        assert!(
            (height - 409.70).abs() < 0.5,
            "container {height}, Chrome 409.70"
        );
    }

    /// A percentage margin resolves against the container's content box: 20px
    /// of padding gives Chrome's `-86.30` and 453.70 where the border box gives
    /// 450.00, and at `content-box` it returns to `-90.30`.
    #[test]
    fn a_percentage_margin_resolves_against_the_content_box() {
        let (mut scene, page) = scene_with_page(1200.0, 2000.0);
        scene.content_height = true;
        if let Some(node) = scene.get_mut(page) {
            node.layout.flex_direction = FlexDirection::Column;
        }
        let container = scene
            .push(page, Node::new(meo_canvas_scene::node::NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(container) {
            node.layout.display = Display::Flex;
            node.layout.flex_direction = FlexDirection::Column;
            node.layout.size = (Dimension::Points(903.0), Dimension::Auto);
            node.layout.box_sizing = BoxSizing::BorderBox;
            for edge in [
                &mut node.layout.padding.left,
                &mut node.layout.padding.right,
                &mut node.layout.padding.top,
                &mut node.layout.padding.bottom,
            ] {
                *edge = Length::Points(20.0);
            }
        }
        let child = scene
            .push(container, Node::new(meo_canvas_scene::node::NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(child) {
            node.layout.size =
                (Dimension::Points(476.0), Dimension::Points(500.0));
            node.layout.flex_grow = 1.0;
            node.layout.margin.top = Dimension::Percent(-0.10);
        }

        let result = solved(&scene, page);
        let height = result.rects[&container].size.height;
        assert!(
            (height - 453.70).abs() < 0.5,
            "container {height}, Chrome 453.70 -- reading the border box \
             instead of the content box gives 450.00, which is what this row \
             exists to refuse"
        );
    }

    /// A clipping grid item stops an ancestor ignoring a negative margin, where
    /// only the ancestor's size is wrong. Deleting the call, or dropping
    /// `Overflow::Scroll`, reddens this.
    #[test]
    fn a_clipping_grid_item_reaches_an_ancestor_s_height() {
        for overflow in [Overflow::Hidden, Overflow::Scroll] {
            let (mut scene, page) = scene_with_page(400.0, 2000.0);
            scene.content_height = true;
            if let Some(node) = scene.get_mut(page) {
                node.layout.flex_direction = FlexDirection::Column;
            }
            let parent = scene
                .push(page, Node::new(meo_canvas_scene::node::NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(parent) {
                node.layout.display = Display::Flex;
                node.layout.flex_direction = FlexDirection::Column;
            }
            let strip = scene
                .push(parent, Node::new(meo_canvas_scene::node::NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(strip) {
                node.layout.display = Display::Flex;
                node.layout.flex_direction = FlexDirection::Column;
                node.layout.margin.top = Dimension::Points(-32.0);
            }
            let grid = scene
                .push(strip, Node::new(meo_canvas_scene::node::NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(grid) {
                node.layout.display = Display::Grid;
                node.layout.grid_template_columns =
                    vec![TrackSize::Points(100.0)];
            }
            let item = scene
                .push(grid, Node::new(meo_canvas_scene::node::NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(item) {
                node.layout.display = Display::Flex;
                node.layout.flex_direction = FlexDirection::Column;
                node.layout.overflow = (overflow, overflow);
            }
            let content = scene
                .push(item, Node::new(meo_canvas_scene::node::NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(content) {
                node.layout.size =
                    (Dimension::Points(100.0), Dimension::Points(100.0));
            }

            let height = solved(&scene, page).rects[&parent].size.height;
            assert!(
                (height - 68.0).abs() < 0.01,
                "{overflow:?}: ancestor {height}, Chrome 68"
            );
        }
    }

    /// A box as wide as the ceiling fills its container, which tells
    /// `FINITE_CEILING` from `1.0e9`, inside the band above `2^27` where a
    /// solved box falls short. The `snapped` grid is not the cause: `2^17` to
    /// `2^27` fill.
    #[test]
    fn a_box_as_wide_as_the_ceiling_still_fills_its_container() {
        let (mut scene, page) = scene_with_page(200.0, 100.0);
        let child = scene
            .push(page, Node::new(meo_canvas_scene::node::NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(child) {
            node.layout.size =
                (Dimension::Points(FINITE_CEILING), Dimension::Points(20.0));
        }

        let width = solved(&scene, page)
            .get(child)
            .map_or(0.0, |rect| rect.size.width);

        // Equal to the container, not merely large: the failure this catches is
        // a box eight pixels short of one, which every "greater than" passes.
        assert!(
            (width - 200.0).abs() < f32::EPSILON,
            "a box of {FINITE_CEILING} laid out {width} wide in a 200 container"
        );
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

    /// A page root's own definite size survives layout, so beginning a page at
    /// it moves no scene: `pin_page_root` substitutes `scene.size` only for
    /// `auto`.
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
        // Chrome floors a border width at used-value time and layout sees it: a
        // 20-tall content box in a `3.5` border is 27, not 27.5, at either
        // scale.
        let (mut scene, page) = scene_with_page(100.0, 60.0);
        let root = scene
            .get_mut(page)
            .unwrap_or_else(|| unreachable!("the page root was just created"));
        root.layout.size = (Dimension::Points(40.0), Dimension::Points(20.0));
        root.layout.box_sizing = BoxSizing::ContentBox;
        root.layout.border = Sides::all(3.5);
        // Without a style the used width is zero and this test rounds nothing
        // -- it would pass and stop measuring, inside the function the style
        // gate lives in.
        root.paint.border_style = BorderStyle::Solid;

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
        // What `a_leaf_wider_than_its_container_shrinks_to_it` cannot see:
        // narrower than the longest piece, only the automatic minimum (§4.5)
        // decides, so the item keeps 12 in 8. A mock minimum of zero would pass
        // a collapsing paragraph.
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

    /// Maps every variant of an enum and asserts no two collapse onto one taffy
    /// value, so a new variant falling into an existing arm fails.
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
            super::to_taffy_style(&style, BorderStyle::Solid).inset,
            taffy::Rect::auto(),
            "a static inset is dropped"
        );

        style.position_type = PositionType::Relative;
        assert_eq!(
            super::to_taffy_style(&style, BorderStyle::Solid).inset.top,
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
            // A wrap is a flex concept and the scene's default is `block`, so
            // the container says flex; otherwise `bottom_align_reversed_wraps`
            // is never reached.
            outer.layout.display = Display::Flex;
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

    /// A percentage padding resolves against the containing block, not the
    /// node: the node's width is held and the parent's varied, since varying
    /// the node changes how many children fit. The default shape agrees either
    /// way.
    #[test]
    fn a_percentage_padding_resolves_against_the_containing_block() {
        use meo_canvas_scene::style::{Length, layout::FlexWrap};

        // The node is 200 wide in every scene; only the containing block moves.
        // Six 80x44 children wrap into three lines inside 200 either way -- two
        // fit across -- so the flow the correction shifts is identical and the
        // padding is the one thing that differs.
        let placed = |containing: f32| {
            let mut scene = Scene::new(Size::new(900.0, 600.0));
            let mut parent = Node::container();
            parent.layout.display = Display::Flex;
            parent.layout.size =
                (Dimension::Points(containing), Dimension::Points(400.0));
            let parent = scene
                .push(NodeId::ROOT, parent)
                .unwrap_or_else(|error| unreachable!("{error}"));

            let mut outer = Node::container();
            outer.layout.display = Display::Flex;
            outer.layout.flex_wrap = FlexWrap::WrapReverse;
            outer.layout.size =
                (Dimension::Points(200.0), Dimension::Points(56.0));
            outer.layout.padding.bottom = Length::Percent(0.10);
            let outer = scene
                .push(parent, outer)
                .unwrap_or_else(|error| unreachable!("{error}"));

            let mut ids = Vec::new();
            for _ in 0..6 {
                let mut child = Node::container();
                child.layout.size =
                    (Dimension::Points(80.0), Dimension::Points(44.0));
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

        // Three lines of 44 stack to 132 in a 56-tall border box, so the
        // correction shifts them up by `content_bottom - 132`. A containing
        // block of 200 gives a padding of 20 and a content bottom of 36; one
        // of 400 gives 40 and 16. The pair differs by exactly that 20.
        assert_eq!(
            placed(200.0),
            vec![-8, -8, -52, -52, -96, -96],
            "10% of a 200-wide containing block is 20"
        );
        assert_eq!(
            placed(400.0),
            vec![-28, -28, -72, -72, -116, -116],
            "10% of a 400-wide containing block is 40, and the node's own 200 \
             is not what the percentage resolves against"
        );
    }

    #[test]
    fn a_fixed_node_resolves_against_the_page_and_an_absolute_one_against_its_parent()
     {
        // `fixed` resolves against the page and `absolute` against its
        // containing block; a padded, offset parent separates them.
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
        // An absolute node resolves against its nearest positioned ancestor,
        // skipping static boxes, so this is settled by where it is attached;
        // misplaced, it lands at the static parent's (50, 50).
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

        let style = super::to_taffy_style(&layout, BorderStyle::Solid);

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

    /// A ratio the renderer will not use settles no height, asserted on
    /// [`ratio_settles_it`] since a rendered row cannot tell who discarded it.
    /// `-2` and `Infinity` pass the npm writer, so they arrive from either
    /// door.
    #[test]
    fn a_ratio_this_renderer_will_not_use_settles_nothing() {
        let parent = Node::new(meo_canvas_scene::node::NodeKind::Box);
        let mut child = Node::new(meo_canvas_scene::node::NodeKind::Box);
        child.layout.size = (Dimension::Points(30.0), Dimension::Auto);

        for bad in [-2.0, 0.0, f32::NAN, f32::INFINITY] {
            child.layout.aspect_ratio = Some(bad);
            assert!(
                !super::child_height_is_definite(&parent, &child, false),
                "a ratio of {bad} was treated as settling the height"
            );
        }

        // The control, and it is what stops this passing for a predicate that
        // refuses every ratio: the one usable value must still settle it.
        child.layout.aspect_ratio = Some(0.85);
        assert!(super::child_height_is_definite(&parent, &child, false));
    }

    /// A percentage against a content-sized parent is no constraint, and
    /// against a definite one it resolves: dropping every percentage gives 20
    /// where Chrome gives 120, 240 and 400.
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
                // Unstretched, so `auto` stays indefinite; stretched, the row
                // would measure the page. The browser probe sets `align-items:
                // flex-start` too.
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

    /// Every cell of the bad-value grid that layout owns: a `NaN` is dropped
    /// and the property takes its unset value, where 23 of 48 sampled cells
    /// drew nothing. Infinity is clamped, in
    /// `an_infinite_value_is_clamped_rather_than_dropped`.
    #[test]
    fn an_unusable_value_is_dropped_where_it_becomes_layout_input() {
        let bad = [f32::NAN, -20.0];
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
            // `Solid`, because `used_border` zeroes a `None` border whatever
            // its width, and this would pass without the non-finite guard it
            // pins.
            let taffy = super::to_taffy_style(&style, BorderStyle::Solid);

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
            // The factors fall back to their own initial values, which differ;
            // compared by bits, the claim being identity.
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

    /// An infinity becomes the largest value this engine carries, as `calc`
    /// clamps. `-Infinity` is dropped for being negative on a size or factor,
    /// and kept, bounded, on a margin or inset.
    #[test]
    fn an_infinite_value_is_clamped_rather_than_dropped() {
        let style = LayoutStyle {
            size: (
                Dimension::Points(f32::INFINITY),
                Dimension::Points(f32::INFINITY),
            ),
            flex_basis: Dimension::Points(f32::INFINITY),
            flex_grow: f32::INFINITY,
            flex_shrink: f32::INFINITY,
            padding: Sides::all(Length::Points(f32::INFINITY)),
            margin: Sides::all(Dimension::Points(f32::NEG_INFINITY)),
            ..LayoutStyle::default()
        };
        let taffy = super::to_taffy_style(&style, BorderStyle::Solid);

        assert_ne!(taffy.size.width, taffy::Dimension::auto());
        assert_ne!(taffy.flex_basis, taffy::Dimension::auto());
        // The factors keep their value rather than falling back to CSS's
        // initial, which is what `an_unusable_value_is_dropped...` asserts for
        // the values that are still dropped. Compared against the initials so
        // that a repair reinstating the drop fails here by name.
        assert_ne!(taffy.flex_grow.to_bits(), 0.0_f32.to_bits());
        assert_ne!(taffy.flex_shrink.to_bits(), 1.0_f32.to_bits());
        assert_ne!(taffy.padding.left, taffy::LengthPercentage::length(0.0));
        // A margin accepts negatives, so an infinite one is bounded rather
        // than zeroed -- the arm that would be missed by a repair written only
        // for the properties that refuse a negative.
        assert_ne!(taffy.margin.left, taffy::LengthPercentageAuto::length(0.0));

        // And every one of them is finite, which is the point rather than a
        // side effect: an unbounded value reaching taffy is what collapsed the
        // boxes this repair exists for.
        assert!(FINITE_CEILING.is_finite());
    }

    /// The other half: a negative margin and inset are valid CSS and kept,
    /// which a repair rejecting every negative would fail.
    #[test]
    fn a_negative_margin_and_a_negative_inset_survive() {
        let style = LayoutStyle {
            position_type: PositionType::Relative,
            margin: Sides::all(Dimension::Points(-20.0)),
            inset: Sides::all(Some(Length::Points(-20.0))),
            ..LayoutStyle::default()
        };
        // `Solid` because this test says nothing about borders and a style
        // that zeroed them would make that silence look like a result.
        let taffy = super::to_taffy_style(&style, BorderStyle::Solid);

        assert_eq!(
            taffy.margin.left,
            taffy::LengthPercentageAuto::length(-20.0)
        );
        assert_eq!(taffy.inset.top, taffy::LengthPercentageAuto::length(-20.0));

        // A `NaN` is still dropped, a margin to zero rather than `auto` and an
        // inset to `auto`. `NAN`, since an infinity is clamped and would test
        // nothing here.
        let broken = LayoutStyle {
            position_type: PositionType::Relative,
            margin: Sides::all(Dimension::Points(f32::NAN)),
            inset: Sides::all(Some(Length::Points(f32::NAN))),
            ..LayoutStyle::default()
        };
        let taffy = super::to_taffy_style(&broken, BorderStyle::Solid);

        assert_eq!(taffy.margin.left, taffy::LengthPercentageAuto::length(0.0));
        assert_eq!(taffy.inset.top, taffy::LengthPercentageAuto::auto());
    }

    #[test]
    fn the_scenes_defaults_are_csss_not_taffys() {
        // The scene's `LayoutStyle::default()` is CSS's: block, a row direction
        // and a shrink of 1. This fails if the mapping leans on
        // `taffy::Style::default()`, whose display is `Flex`.
        let style =
            super::to_taffy_style(&LayoutStyle::default(), BorderStyle::Solid);

        assert_eq!(style.display, taffy::Display::Block);
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
