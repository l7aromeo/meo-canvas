//! Whether a flex item with a definite base size collapses into its line
//! (`l7aromeo/meo-canvas#145`): a definite `flex-basis` and minimum collapse in
//! Chrome at any magnitude, but `0%` against an indefinite main size resolves
//! as `auto`. Compensated by `collapse_definite_bases`.

use meo_canvas_core::{Available, Measure, MeasuredLeaf, layout::solve};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        layout::{
            Align, BoxSizing, Display, FlexDirection, FlexWrap, Justify,
            Overflow,
        },
    },
};

/// Every box states its own size, so only the row asking for a measured leaf
/// measures: it answers for one node, since a childless box is a leaf too and
/// would otherwise take the mock's 1024.
struct Leaves {
    /// The node standing in for Chrome's block of text, when the row has one.
    measured: Option<NodeId>,
}
impl Measure for Leaves {
    fn measure(
        &mut self,
        node: NodeId,
        _: (Option<f32>, Option<f32>),
        _: (Available, Available),
    ) -> MeasuredLeaf {
        if self.measured == Some(node) {
            // Chrome's leaf is 32 monospace lines at a line height of 32,
            // which is the same 1024 every other row states outright.
            return MeasuredLeaf::sized(Size::new(100.0, 1024.0));
        }
        MeasuredLeaf::EMPTY
    }
}

const TABLE: &str = include_str!("assets/chrome/flex-basis-collapse.tsv");

/// The two rows that say the compensation does not over-reach, both matching
/// Chrome: `flex-basis` does not apply to a grid item, and a row container's
/// main size is §9.9 intrinsic sizing, unimplemented upstream as
/// `DioxusLabs/taffy#351`.
const SCOPE_CONTROLS: &[&str] = &["container grid", "row-direction mirror"];

/// One row of the table: the extent and the axis it was measured on.
fn chrome(case: &str) -> (f32, bool) {
    for line in TABLE.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut columns = line.split('\t');
        if columns.next() != Some(case) {
            continue;
        }
        let extent = columns
            .next()
            .and_then(|cell| cell.parse::<f32>().ok())
            .unwrap_or_else(|| unreachable!("{case} has a malformed extent"));
        let axis = columns.next().unwrap_or_else(|| unreachable!("{case}"));
        return (extent, axis == "width");
    }
    unreachable!("{case} is not in flex-basis-collapse.tsv")
}

/// Every row of the table, against this renderer.
fn rows() -> Vec<(&'static str, Case)> {
    let mut all = container_rows();
    all.extend(value_rows());
    all.extend(combination_rows());
    all.extend(reach_rows());
    all
}

/// How the column gets -- or does not get -- a definite main size.
#[derive(Clone, Copy, PartialEq)]
enum Column {
    Stretch,
    Explicit,
    Percent,
    DefiniteAncestor,
    AlignSelfStart,
    Grid,
}

/// What sits inside the collapsing item.
#[derive(Clone, Copy, PartialEq)]
enum Content {
    Tall,
    Nested,
    Empty,
    /// The tall box under a grid container, inside the collapsing item.
    Grid,
    /// A leaf whose extent comes from the measurer rather than from a style.
    Measured,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a separate axis of the sweep and they vary \
              independently: an item that clips and a container that wraps \
              are not two values of one thing, and grouping them would hide \
              that a row sets one without the others"
)]
#[derive(Clone, Copy)]
struct Case {
    column: Column,
    basis: Option<Dimension>,
    min: Option<Dimension>,
    content: Content,
    /// How many flex containers sit between the column and the item.
    levels: u8,
    mirror: bool,
    /// The combination axes: what a caller writes beside the pair.
    clips: Option<Overflow>,
    max: Option<Dimension>,
    shrink: Option<f32>,
    gap: f32,
    padding: f32,
    border_box: bool,
    ratio: Option<f32>,
    wrap: bool,
    item_percent_main: bool,
    /// The reach axes: what a scene puts around the pair without touching it.
    second_item: bool,
    align_items: Option<Align>,
    justify: Option<Justify>,
    reverse: bool,
    margin_auto: bool,
    no_grow: bool,
    column_ratio: Option<f32>,
}

impl Case {
    const fn new() -> Self {
        Self {
            column: Column::Stretch,
            basis: Some(Dimension::Points(0.0)),
            min: Some(Dimension::Points(0.0)),
            content: Content::Tall,
            levels: 0,
            mirror: false,
            clips: None,
            max: None,
            shrink: None,
            gap: 0.0,
            padding: 0.0,
            border_box: false,
            ratio: None,
            wrap: false,
            item_percent_main: false,
            second_item: false,
            align_items: None,
            justify: None,
            reverse: false,
            margin_auto: false,
            no_grow: false,
            column_ratio: None,
        }
    }
}

/// The column every row shares, with the row's own knobs, split from [`solved`]
/// since container and item are set by different columns of the table.
fn column_of(case: Case, across: bool) -> Node {
    let mut column = Node::new(NodeKind::Box);
    column.layout.display = if case.column == Column::Grid {
        Display::Grid
    } else {
        Display::Flex
    };
    column.layout.flex_direction = match (across, case.reverse) {
        (true, false) => FlexDirection::Column,
        (true, true) => FlexDirection::ColumnReverse,
        (false, false) => FlexDirection::Row,
        (false, true) => FlexDirection::RowReverse,
    };
    column.layout.flex_grow = 1.0;
    column.layout.align_items = case.align_items;
    column.layout.justify_content = case.justify;
    if let Some(ratio) = case.column_ratio {
        column.layout.aspect_ratio = Some(ratio);
    }
    match case.column {
        Column::Explicit if across => {
            column.layout.size.1 = Dimension::Points(200.0);
        }
        Column::Explicit => column.layout.size.0 = Dimension::Points(200.0),
        Column::Percent if across => {
            column.layout.size.1 = Dimension::Percent(1.0);
        }
        Column::Percent => column.layout.size.0 = Dimension::Percent(1.0),
        Column::AlignSelfStart => {
            column.layout.align_self = Some(Align::FlexStart);
        }
        Column::Stretch | Column::DefiniteAncestor | Column::Grid => {}
    }
    if case.gap > 0.0 {
        column.layout.gap = (Length::Points(0.0), Length::Points(case.gap));
    }
    if case.padding > 0.0 {
        for edge in [
            &mut column.layout.padding.top,
            &mut column.layout.padding.bottom,
            &mut column.layout.padding.left,
            &mut column.layout.padding.right,
        ] {
            *edge = Length::Points(case.padding);
        }
    }
    if case.border_box {
        column.layout.box_sizing = BoxSizing::BorderBox;
    }
    if case.wrap {
        column.layout.flex_wrap = FlexWrap::Wrap;
    }
    column
}

/// The collapsing item, carrying whichever of the pair the row writes.
fn item_of(case: Case, across: bool) -> Node {
    let mut item = Node::new(NodeKind::Box);
    item.layout.flex_grow = if case.no_grow { 0.0 } else { 1.0 };
    if case.margin_auto {
        // The auto margin is on the main-axis start edge, which is where it
        // takes the free space before §9.7 can give it to the item.
        if across {
            item.layout.margin.top = Dimension::Auto;
        } else {
            item.layout.margin.left = Dimension::Auto;
        }
    }
    if let Some(basis) = case.basis {
        item.layout.flex_basis = basis;
    }
    if let Some(min) = case.min {
        if across {
            item.layout.min_size.1 = min;
        } else {
            item.layout.min_size.0 = min;
        }
    }
    if let Some(max) = case.max {
        if across {
            item.layout.max_size.1 = max;
        } else {
            item.layout.max_size.0 = max;
        }
    }
    if let Some(shrink) = case.shrink {
        item.layout.flex_shrink = shrink;
        item.layout.flex_grow = 0.0;
    }
    if let Some(overflow) = case.clips {
        item.layout.overflow = (overflow, overflow);
    }
    if let Some(ratio) = case.ratio {
        item.layout.aspect_ratio = Some(ratio);
    }
    if case.item_percent_main {
        if across {
            item.layout.size.1 = Dimension::Percent(1.0);
        } else {
            item.layout.size.0 = Dimension::Percent(1.0);
        }
    }
    item
}

/// What the collapsing item holds, and the node the measurer answers for, split
/// from [`solved`] as [`column_of`] and [`item_of`] are.
fn fill_item(
    scene: &mut Scene,
    item: NodeId,
    case: Case,
    across: bool,
) -> Option<NodeId> {
    if case.content == Content::Empty {
        return None;
    }
    let holder = match case.content {
        Content::Nested => scene
            .push(item, Node::new(NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}")),
        Content::Grid => {
            let mut grid = Node::new(NodeKind::Box);
            grid.layout.display = Display::Grid;
            scene
                .push(item, grid)
                .unwrap_or_else(|error| unreachable!("{error}"))
        }
        Content::Tall | Content::Measured | Content::Empty => item,
    };
    if case.content == Content::Measured {
        // A childless box with no stated size is what reaches the measurer,
        // which is the path Chrome's paragraph takes.
        return Some(
            scene
                .push(holder, Node::new(NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}")),
        );
    }
    let mut tall = Node::new(NodeKind::Box);
    tall.layout.size = if across {
        (Dimension::Points(100.0), Dimension::Points(1024.0))
    } else {
        (Dimension::Points(1024.0), Dimension::Points(100.0))
    };
    scene
        .push(holder, tall)
        .unwrap_or_else(|error| unreachable!("{error}"));
    None
}

/// The outer box's extent on the axis the table names.
fn solved(case: Case) -> f32 {
    let mut scene = Scene::new(Size::new(1400.0, 1400.0));
    let across = !case.mirror;

    let mut outer = Node::new(NodeKind::Box);
    outer.layout.display = Display::Flex;
    outer.layout.flex_direction = if across {
        FlexDirection::Row
    } else {
        FlexDirection::Column
    };
    if across {
        outer.layout.size.0 = Dimension::Points(600.0);
        if case.column == Column::DefiniteAncestor {
            outer.layout.size.1 = Dimension::Points(200.0);
        }
    } else {
        outer.layout.size.1 = Dimension::Points(600.0);
    }
    // The mirror's cross axis is the inline one, where a block box fills rather
    // than fits, so it needs a parent that shrinks to it or the measurement is
    // of the page. Chrome needed the same wrapper for the same reason.
    let host = if case.mirror {
        let mut wrapper = Node::new(NodeKind::Box);
        wrapper.layout.display = Display::Flex;
        wrapper.layout.align_items = Some(Align::FlexStart);
        scene
            .push(NodeId::ROOT, wrapper)
            .unwrap_or_else(|error| unreachable!("{error}"))
    } else {
        NodeId::ROOT
    };
    let outer = scene
        .push(host, outer)
        .unwrap_or_else(|error| unreachable!("{error}"));

    let mut sibling = Node::new(NodeKind::Box);
    sibling.layout.size = if across {
        (Dimension::Points(300.0), Dimension::Points(200.0))
    } else {
        (Dimension::Points(200.0), Dimension::Points(300.0))
    };
    scene
        .push(outer, sibling)
        .unwrap_or_else(|error| unreachable!("{error}"));

    let column = scene
        .push(outer, column_of(case, across))
        .unwrap_or_else(|error| unreachable!("{error}"));

    let mut parent = column;
    for _ in 0..case.levels {
        let mut middle = Node::new(NodeKind::Box);
        middle.layout.display = Display::Flex;
        middle.layout.flex_direction = if across {
            FlexDirection::Column
        } else {
            FlexDirection::Row
        };
        middle.layout.flex_grow = 1.0;
        parent = scene
            .push(parent, middle)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }

    let item = scene
        .push(parent, item_of(case, across))
        .unwrap_or_else(|error| unreachable!("{error}"));

    let measured = fill_item(&mut scene, item, case, across);

    if case.second_item {
        scene
            .push(parent, item_of(case, across))
            .unwrap_or_else(|error| unreachable!("{error}"));
    }

    let result = solve(&scene, NodeId::ROOT, &mut Leaves { measured })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let rect = result
        .get(outer)
        .unwrap_or_else(|| unreachable!("the outer box is laid out"));
    if across {
        rect.size.height
    } else {
        rect.size.width
    }
}

/// The rows that vary how the container is sized.
fn container_rows() -> Vec<(&'static str, Case)> {
    vec![
        ("baseline", Case::new()),
        (
            "container explicit",
            Case {
                column: Column::Explicit,
                ..Case::new()
            },
        ),
        (
            "container percentage",
            Case {
                column: Column::Percent,
                ..Case::new()
            },
        ),
        (
            "container definite ancestor",
            Case {
                column: Column::DefiniteAncestor,
                ..Case::new()
            },
        ),
        (
            "container align-self start",
            Case {
                column: Column::AlignSelfStart,
                ..Case::new()
            },
        ),
        (
            "container align-self flex-start",
            Case {
                column: Column::AlignSelfStart,
                ..Case::new()
            },
        ),
    ]
}

/// The rows that vary the basis, the minimum, and what the item holds.
fn value_rows() -> Vec<(&'static str, Case)> {
    vec![
        (
            "basis auto",
            Case {
                basis: None,
                ..Case::new()
            },
        ),
        (
            "basis 1px",
            Case {
                basis: Some(Dimension::Points(1.0)),
                ..Case::new()
            },
        ),
        (
            "basis 120px",
            Case {
                basis: Some(Dimension::Points(120.0)),
                ..Case::new()
            },
        ),
        (
            "basis 0%",
            Case {
                basis: Some(Dimension::Percent(0.0)),
                ..Case::new()
            },
        ),
        (
            "min auto",
            Case {
                min: None,
                ..Case::new()
            },
        ),
        (
            "min 1px",
            Case {
                min: Some(Dimension::Points(1.0)),
                ..Case::new()
            },
        ),
        (
            "min 0%",
            Case {
                min: Some(Dimension::Percent(0.0)),
                ..Case::new()
            },
        ),
        (
            "content nested",
            Case {
                content: Content::Nested,
                ..Case::new()
            },
        ),
        (
            "content empty",
            Case {
                content: Content::Empty,
                ..Case::new()
            },
        ),
        (
            "item one level deeper",
            Case {
                levels: 1,
                ..Case::new()
            },
        ),
        (
            "container grid",
            Case {
                column: Column::Grid,
                ..Case::new()
            },
        ),
        (
            "row-direction mirror",
            Case {
                mirror: true,
                ..Case::new()
            },
        ),
    ]
}

/// **A pixel of slack**, which is what a solved size costs: taffy rounds each
/// edge and a size is the difference of two rounded numbers.
const SLACK: f32 = 1.0;

/// **Every row, with no exemptions**, because no row here records a
/// divergence: thirty match Chrome because the compensation makes them and
/// fourteen because they already did. Both counts are measured -- commenting
/// out the call in `layout.rs` turns exactly thirty of them red.
#[test]
fn every_row_agrees_with_chrome() {
    let mut wrong = Vec::new();
    for (name, case) in rows() {
        let (want, _) = chrome(name);
        let got = solved(case);
        if (got - want).abs() > SLACK {
            wrong.push(format!("{name}: chrome {want}, here {got}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} row(s) disagree:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// The scope controls, asserted a second time so the failure names the
/// specification: a move here is the compensation reaching a case it was scoped
/// out of, not a row drifting.
#[test]
fn the_scope_controls_hold() {
    for name in SCOPE_CONTROLS {
        let case = rows()
            .into_iter()
            .find(|(row, _)| row == name)
            .unwrap_or_else(|| unreachable!("{name} has no case"))
            .1;
        let (want, _) = chrome(name);
        let got = solved(case);
        assert!(
            (got - want).abs() <= SLACK,
            "{name} agrees with Chrome at {want} and is now {got}. This row \
             is a scope control rather than a divergence: it is here to go red \
             exactly when the compensation reaches a case a different \
             specification governs -- grid has no `flex-basis`, and an \
             inline-axis container is §9.9 Intrinsic Sizes. Repair the scope, \
             not the row"
        );
    }
}

/// The rows combining the pair with what a caller writes beside it: `overflow`
/// deleting §4.5's automatic minimum, a minimum and maximum on one axis,
/// `flex-shrink`, and a gap or padding changing the free space.
fn combination_rows() -> Vec<(&'static str, Case)> {
    vec![
        (
            "combo overflow hidden",
            Case {
                clips: Some(Overflow::Hidden),
                ..Case::new()
            },
        ),
        (
            "combo overflow scroll",
            Case {
                clips: Some(Overflow::Scroll),
                ..Case::new()
            },
        ),
        (
            "combo overflow hidden, min auto",
            Case {
                clips: Some(Overflow::Hidden),
                min: None,
                ..Case::new()
            },
        ),
        (
            "combo max above min",
            Case {
                max: Some(Dimension::Points(400.0)),
                ..Case::new()
            },
        ),
        (
            "combo max below min",
            Case {
                min: Some(Dimension::Points(300.0)),
                max: Some(Dimension::Points(100.0)),
                ..Case::new()
            },
        ),
        (
            "combo shrink definite basis",
            Case {
                basis: Some(Dimension::Points(400.0)),
                shrink: Some(1.0),
                ..Case::new()
            },
        ),
        (
            "combo container gap",
            Case {
                gap: 24.0,
                ..Case::new()
            },
        ),
        (
            "combo container padding",
            Case {
                padding: 16.0,
                ..Case::new()
            },
        ),
        (
            "combo border-box padding",
            Case {
                padding: 16.0,
                border_box: true,
                ..Case::new()
            },
        ),
        (
            "combo item ratio",
            Case {
                ratio: Some(1.0),
                ..Case::new()
            },
        ),
        (
            "combo container wrap",
            Case {
                wrap: true,
                ..Case::new()
            },
        ),
        (
            "combo nested percentage",
            Case {
                column: Column::Percent,
                item_percent_main: true,
                ..Case::new()
            },
        ),
    ]
}

/// The rows varying what surrounds the pair, which the compensation does not
/// read, so they say how far it reaches. `container ratio` settles the
/// container's cross size; `grid inside the item` must not stop the collapse.
fn reach_rows() -> Vec<(&'static str, Case)> {
    vec![
        (
            "two items",
            Case {
                second_item: true,
                ..Case::new()
            },
        ),
        (
            "align-items center",
            Case {
                align_items: Some(Align::Center),
                ..Case::new()
            },
        ),
        (
            "justify-content center",
            Case {
                justify: Some(Justify::Center),
                ..Case::new()
            },
        ),
        (
            "column-reverse",
            Case {
                reverse: true,
                ..Case::new()
            },
        ),
        (
            "item margin auto",
            Case {
                margin_auto: true,
                ..Case::new()
            },
        ),
        (
            "no grow, definite basis",
            Case {
                no_grow: true,
                ..Case::new()
            },
        ),
        (
            "max-height 50%",
            Case {
                max: Some(Dimension::Percent(0.5)),
                ..Case::new()
            },
        ),
        (
            "container ratio",
            Case {
                column_ratio: Some(1.0),
                ..Case::new()
            },
        ),
        (
            "min-height 50%",
            Case {
                min: Some(Dimension::Percent(0.5)),
                ..Case::new()
            },
        ),
        (
            "align-items baseline",
            Case {
                align_items: Some(Align::Baseline),
                ..Case::new()
            },
        ),
        (
            "two levels deeper",
            Case {
                levels: 2,
                ..Case::new()
            },
        ),
        (
            "three levels deeper",
            Case {
                levels: 3,
                ..Case::new()
            },
        ),
        (
            "grid inside the item",
            Case {
                content: Content::Grid,
                ..Case::new()
            },
        ),
        (
            "measured leaf",
            Case {
                content: Content::Measured,
                ..Case::new()
            },
        ),
    ]
}
