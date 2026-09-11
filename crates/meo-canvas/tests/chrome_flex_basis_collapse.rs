//! Whether a flex item with a definite base size collapses into its line.
//!
//! `l7aromeo/meo-canvas#145`: a row whose height a sibling sets, holding a
//! column with no cross size of its own, holding an item that says *ignore my
//! content and take your size from the line*. Chrome collapses the item and the
//! sibling sets the row; taffy lets the content dictate it.
//!
//! **The pair is a definite `flex-basis` with a definite minimum, and neither
//! has to be zero.** `1px`, `120px` and `min: 0%` all collapse in Chrome; `0%`
//! does not, because a percentage basis against a container with no definite
//! main size resolves as `auto`. The rule is definiteness rather than
//! magnitude, and those rows are what tell the two readings apart.
//!
//! **Solved rectangles rather than ink**, for the reason
//! `chrome_flex_ratio_cross.rs` gives: a collapsed item paints nothing, and an
//! ink scan would report the absence rather than the size.
//!
//! Compensated in `layout.rs` by `collapse_definite_bases`;
//! `crates/meo-canvas-core/tests/taffy_definite_basis.rs` pins what taffy does
//! on its own.

use meo_canvas_core::{Available, Measure, MeasuredLeaf, layout::solve};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        layout::{
            Align, BoxSizing, Display, FlexDirection, FlexWrap, Overflow,
        },
    },
};

/// Every scene here is boxes, so nothing needs measuring.
struct NoLeaves;
impl Measure for NoLeaves {
    fn measure(
        &mut self,
        _: NodeId,
        _: (Option<f32>, Option<f32>),
        _: (Available, Available),
    ) -> MeasuredLeaf {
        MeasuredLeaf::EMPTY
    }
}

const TABLE: &str = include_str!("assets/chrome/flex-basis-collapse.tsv");

/// The two rows that say the compensation does not over-reach.
///
/// **Not divergences.** `known` elsewhere in this tree marks a row where we
/// deliberately do not match Chrome; both of these match it, and they are here
/// to go red if the compensation ever starts firing on them.
///
/// **`container grid`**: `flex-basis` does not apply to a grid item, so there
/// is no §9.2 hypothetical main size to write and a rule that wrote one would
/// be inventing flex semantics inside grid.
///
/// **`row-direction mirror`**: the container's main axis is the inline one and
/// is sized by max-content, where a flex container's main size is §9.9
/// Intrinsic Sizes -- a different computation that accounts for flex factors,
/// unimplemented upstream as `DioxusLabs/taffy#351`. That is a missing step
/// rather than this one.
///
/// **Both sentences are about which specification governs rather than about a
/// number**, which is what makes them survive a re-measurement.
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
    deeper: bool,
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
}

impl Case {
    const fn new() -> Self {
        Self {
            column: Column::Stretch,
            basis: Some(Dimension::Points(0.0)),
            min: Some(Dimension::Points(0.0)),
            content: Content::Tall,
            deeper: false,
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
        }
    }
}

/// The column every row shares, with the row's own knobs on it.
///
/// **Split from [`solved`] along what a row varies**, which is the container on
/// one side and the item on the other: the two are set by different columns of
/// the table and grouping them put three container arguments and three item
/// ones on one body.
fn column_of(case: Case, across: bool) -> Node {
    let mut column = Node::new(NodeKind::Box);
    column.layout.display = if case.column == Column::Grid {
        Display::Grid
    } else {
        Display::Flex
    };
    column.layout.flex_direction = if across {
        FlexDirection::Column
    } else {
        FlexDirection::Row
    };
    column.layout.flex_grow = 1.0;
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
    item.layout.flex_grow = 1.0;
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

    let parent = if case.deeper {
        let mut middle = Node::new(NodeKind::Box);
        middle.layout.display = Display::Flex;
        middle.layout.flex_direction = if across {
            FlexDirection::Column
        } else {
            FlexDirection::Row
        };
        middle.layout.flex_grow = 1.0;
        scene
            .push(column, middle)
            .unwrap_or_else(|error| unreachable!("{error}"))
    } else {
        column
    };

    let item = scene
        .push(parent, item_of(case, across))
        .unwrap_or_else(|error| unreachable!("{error}"));

    if case.content != Content::Empty {
        let mut tall = Node::new(NodeKind::Box);
        tall.layout.size = if across {
            (Dimension::Points(100.0), Dimension::Points(1024.0))
        } else {
            (Dimension::Points(1024.0), Dimension::Points(100.0))
        };
        let holder = if case.content == Content::Nested {
            let wrapper = Node::new(NodeKind::Box);
            scene
                .push(item, wrapper)
                .unwrap_or_else(|error| unreachable!("{error}"))
        } else {
            item
        };
        scene
            .push(holder, tall)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }

    let result = solve(&scene, NodeId::ROOT, &mut NoLeaves)
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
                deeper: true,
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
/// divergence: eighteen match Chrome because the compensation makes them and
/// twelve because they already did.
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

/// **The scope controls, asserted a second time for what they defend.**
///
/// The row above already checks them, because they agree with Chrome like
/// every other row. This one exists so the failure says what actually went
/// wrong: a move here is the compensation reaching a case it was scoped out
/// of, which is a different defect from a row drifting, and a message naming
/// the specification is what stops the next reader repairing the scope instead
/// of the cause.
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

/// The rows that combine the pair with what a caller writes beside it.
///
/// **The rows above vary one property and callers do not.** Each of these
/// reaches the same mechanism from a different side: `overflow` deletes §4.5's
/// automatic minimum, a maximum and a minimum on one axis is where CSS's
/// resolution order shows, `flex-shrink` is §9.7 from the other direction, and
/// a gap or a padding changes the free space the item resolves against.
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
