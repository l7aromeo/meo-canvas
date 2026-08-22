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

    let root = build(scene, page, &mut tree, &mut to_scene)?;
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

            taffy::Size {
                width: measured.size.width,
                height: measured.size.height,
            }
        },
    )
    .map_err(|error| Error::Layout(error.to_string()))?;

    let mut rects = HashMap::with_capacity(to_scene.len());
    collect(&tree, root, &to_scene, 0.0, 0.0, &mut rects)?;

    Ok(LayoutResult { rects, baselines })
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
fn build(
    scene: &Scene,
    node: NodeId,
    tree: &mut taffy::TaffyTree<NodeId>,
    to_scene: &mut HashMap<taffy::NodeId, NodeId>,
) -> Result<taffy::NodeId, Error> {
    let source = scene.get(node).ok_or_else(|| {
        Error::Layout(format!("node {} is not in the scene", node.get()))
    })?;

    let style = to_taffy_style(&source.layout);

    let children: Vec<taffy::NodeId> = source
        .children
        .iter()
        .filter(|child| {
            scene
                .get(**child)
                .is_none_or(|child| child.layout.display != Display::None)
        })
        .map(|child| build(scene, *child, tree, to_scene))
        .collect::<Result<_, _>>()?;

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

        inset: taffy::Rect {
            left: to_inset(layout.inset.left),
            right: to_inset(layout.inset.right),
            top: to_inset(layout.inset.top),
            bottom: to_inset(layout.inset.bottom),
        },
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
            left: taffy::LengthPercentage::length(layout.border.left),
            right: taffy::LengthPercentage::length(layout.border.right),
            top: taffy::LengthPercentage::length(layout.border.top),
            bottom: taffy::LengthPercentage::length(layout.border.bottom),
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

const fn to_position(position: PositionType) -> taffy::Position {
    match position {
        PositionType::Relative => taffy::Position::Relative,
        PositionType::Absolute => taffy::Position::Absolute,
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

const fn to_length(length: Length) -> taffy::LengthPercentage {
    match length {
        Length::Points(points) => taffy::LengthPercentage::length(points),
        Length::Percent(fraction) => taffy::LengthPercentage::percent(fraction),
    }
}

const fn to_dimension(dimension: Dimension) -> taffy::Dimension {
    match dimension {
        Dimension::Auto => taffy::Dimension::auto(),
        Dimension::Points(points) => taffy::Dimension::length(points),
        Dimension::Percent(fraction) => taffy::Dimension::percent(fraction),
    }
}

/// A margin, where `auto` is CSS's free-space-absorbing margin rather than an
/// absent value.
const fn to_margin(dimension: Dimension) -> taffy::LengthPercentageAuto {
    match dimension {
        Dimension::Auto => taffy::LengthPercentageAuto::auto(),
        Dimension::Points(points) => {
            taffy::LengthPercentageAuto::length(points)
        }
        Dimension::Percent(fraction) => {
            taffy::LengthPercentageAuto::percent(fraction)
        }
    }
}

/// One `inset` edge, where absence is `auto` -- the edge taffy is free to place
/// rather than an edge pinned at zero.
const fn to_inset(inset: Option<Length>) -> taffy::LengthPercentageAuto {
    match inset {
        None => taffy::LengthPercentageAuto::auto(),
        Some(Length::Points(points)) => {
            taffy::LengthPercentageAuto::length(points)
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
        maps_injectively(PositionType::ALL, super::to_position);
        maps_injectively(BoxSizing::ALL, super::to_box_sizing);
        maps_injectively(Direction::ALL, super::to_direction);
        maps_injectively(GridAutoFlow::ALL, super::to_grid_auto_flow);
        maps_injectively(Justify::ALL, super::to_justify);
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
