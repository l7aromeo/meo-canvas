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
    Rect, Scene, Size,
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
#[derive(Debug, Clone, Default)]
pub struct LayoutResult {
    /// Absolute rectangle per node, in logical pixels at scale 1.
    pub rects: HashMap<NodeId, Rect>,
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
    )?;
    for node in viewport.into_iter().chain(orphans) {
        tree.add_child(root, node)
            .map_err(|error| Error::Layout(error.to_string()))?;
    }
    pin_page_root(scene, page, root, &mut tree)?;

    let available = taffy::Size {
        width: taffy::AvailableSpace::Definite(scene.size.width * LAYOUT_SCALE),
        height: taffy::AvailableSpace::Definite(
            scene.size.height * LAYOUT_SCALE,
        ),
    };

    // Baselines are collected during the solve because this closure is the only
    // place they exist: taffy takes the size and drops the rest of what the
    // measurer returned.
    let mut baselines: HashMap<NodeId, f32> = HashMap::new();

    tree.compute_layout_with_measure(
        root,
        available,
        |known, space, _taffy_node, context, _style| {
            // A node with no context is a container taffy sizes from its
            // children; only the leaves built with `new_leaf_with_context`
            // reach the measurer.
            let Some(&node) = context.map(|context| &*context) else {
                return taffy::Size::ZERO;
            };

            let measured = measure.measure(
                node,
                (known.width, known.height),
                (to_available(space.width), to_available(space.height)),
            );

            if let Some(baseline) = measured.first_baseline {
                baselines.insert(node, baseline);
            }

            // Measured text is a used length like any other, so it enters
            // the grid at the same boundary the styled lengths do.
            taffy::Size {
                width: contains(measured.size.width),
                height: contains(measured.size.height),
            }
        },
    )
    .map_err(|error| Error::Layout(error.to_string()))?;

    let mut rects = HashMap::with_capacity(to_scene.len());
    collect(&tree, root, &to_scene, 0.0, 0.0, &mut rects)?;
    bottom_align_reversed_wraps(scene, page, &mut rects);

    Ok(LayoutResult { rects, baselines })
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
        let inset = node.layout.border.bottom + padding;
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
        style.size.height =
            taffy::Dimension::length(scene.size.height * LAYOUT_SCALE);
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
) -> Result<taffy::NodeId, Error> {
    let source = scene.get(node).ok_or_else(|| {
        Error::Layout(format!("node {} is not in the scene", node.get()))
    })?;

    let style = to_taffy_style(&source.layout);

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
    }

    let children = tree
        .children(node)
        .map_err(|error| Error::Layout(error.to_string()))?;
    for child in children {
        collect(tree, child, to_scene, x, y, rects)?;
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
            width: to_dimension(layout.size.0),
            height: to_dimension(layout.size.1),
        },
        min_size: taffy::Size {
            width: to_dimension(layout.min_size.0),
            height: to_dimension(layout.min_size.1),
        },
        max_size: taffy::Size {
            width: to_dimension(layout.max_size.0),
            height: to_dimension(layout.max_size.1),
        },
        aspect_ratio: layout.aspect_ratio,

        margin: taffy::Rect {
            left: to_margin(layout.margin.left),
            right: to_margin(layout.margin.right),
            top: to_margin(layout.margin.top),
            bottom: to_margin(layout.margin.bottom),
        },
        padding: taffy::Rect {
            left: to_length(layout.padding.left),
            right: to_length(layout.padding.right),
            top: to_length(layout.padding.top),
            bottom: to_length(layout.padding.bottom),
        },
        border: taffy::Rect {
            left: taffy::LengthPercentage::length(snapped(layout.border.left)),
            right: taffy::LengthPercentage::length(snapped(
                layout.border.right,
            )),
            top: taffy::LengthPercentage::length(snapped(layout.border.top)),
            bottom: taffy::LengthPercentage::length(snapped(
                layout.border.bottom,
            )),
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
            width: to_length(layout.gap.1),
            height: to_length(layout.gap.0),
        },

        flex_direction: to_flex_direction(layout.flex_direction),
        flex_wrap: to_flex_wrap(layout.flex_wrap),
        flex_grow: layout.flex_grow,
        flex_shrink: layout.flex_shrink,
        flex_basis: to_dimension(layout.flex_basis),

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

/// A margin, where `auto` is CSS's free-space-absorbing margin rather than an
/// absent value.
fn to_margin(dimension: Dimension) -> taffy::LengthPercentageAuto {
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

/// One `inset` edge, where absence is `auto` -- the edge taffy is free to place
/// rather than an edge pinned at zero.
fn to_inset(inset: Option<Length>) -> taffy::LengthPercentageAuto {
    match inset {
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
    #[derive(Debug, Clone, Copy)]
    struct Wrapping {
        /// The width the content takes with nothing constraining it.
        natural: f32,
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
                Available::Definite(extent) => self.natural.min(extent),
                Available::MinContent => 0.0,
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
            super::to_margin(Dimension::Auto),
            taffy::LengthPercentageAuto::auto()
        );
        assert_eq!(
            super::to_margin(Dimension::Points(3.0)),
            taffy::LengthPercentageAuto::length(3.0)
        );
        assert_eq!(
            super::to_margin(Dimension::Percent(0.1)),
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
