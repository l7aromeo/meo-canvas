//! What a ratio does to a grown flex item's cross size.
//!
//! `l7aromeo/meo-canvas#123`: an item with `flex-grow: 1` and
//! `aspect-ratio: 1` in a column came out with no width at all, where the ratio
//! should turn its grown height into one. Compensated in `layout.rs`; upstream
//! is `DioxusLabs/taffy#804` and `taffy_flex_ratio.rs` pins what taffy does.
//!
//! **Solved rectangles rather than ink.** A flex item centred on a cross axis
//! with no content is zero wide, and a zero-wide box paints nothing whatever
//! its height is -- an ink scan reports `0x0` for a box that grew, which is how
//! three divergences were reported that were the instrument.
//!
//! **The item's own `display` is our axis and not CSS's.** Chrome gives
//! `248x248` for all four combinations of display and content; taffy derives
//! only for a block item that has a contribution. The table records which each
//! row used.
//!
//! **Construction: every row here is hand-assembled**, so every item is
//! `Display::Block`. The sweep behind this table measured all twenty-seven
//! under both constructions and **exactly one moved** -- a content-bearing
//! item, `248x248` as a block and `30x248` as a flex one -- so
//! [`the_construction_axis_moves_one_row`] carries that pair and the rest do
//! not pay for it. A row that does not name its construction is a row someone
//! re-derives.

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

const TABLE: &str = include_str!("assets/chrome/flex-ratio-cross.tsv");

/// Rows this renderer answers differently, each with its reason.
///
/// **`uncompensated stretch`** wants a main size derived from the stretched
/// cross size, and Chrome's answer overflows its line -- `424x424` in a
/// 248-tall content box. Correct, and `DioxusLabs/taffy#1182` proposes to make
/// it taffy's own answer -- open rather than merged, so nothing about when it
/// arrives is settled. Shipping it here first is a layout change nobody asked
/// for.
///
/// **`known max-width`** is the asymmetry between a binding minimum and a
/// binding maximum. Chrome does not re-derive from a clamped maximum and taffy
/// does, so a max that binds the cross axis takes the other axis with it here:
/// `100x100` against Chrome's `100x248`. Before the compensation the row was
/// `0x248` -- neither dominates, and a zero-wide box paints nothing where a
/// short one is at least visible.
///
/// **`known max-width amplified`** is the same divergence at its far end, and
/// it is here because the family is a continuum rather than a case. The height
/// produced is the maximum itself where Chrome keeps the line's 248, so the
/// error is `248 - max`: `1x1` against Chrome's `1x248` at `max-width: 1px`.
///
/// **The near end is deliberately not a row.** At `max-width: 247px` this
/// renderer gives `247x247` against Chrome's `247x248` and the error is one
/// pixel, which `SLACK` swallows -- so the row would pass, inside a family
/// that diverges, and a sweep landing there would conclude the family agrees.
/// A row that cannot fail next to rows that do is worse than the prose.
const KNOWN: &[&str] = &[
    "uncompensated stretch",
    "known max-width",
    "known max-width amplified",
];

/// One row of the table.
fn chrome(case: &str) -> (f32, f32) {
    for line in TABLE.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut columns = line.split('\t');
        if columns.next() != Some(case) {
            continue;
        }
        let mut number = || -> f32 {
            columns
                .next()
                .and_then(|cell| cell.parse::<f32>().ok())
                .unwrap_or_else(|| unreachable!("{case} has a malformed cell"))
        };
        return (number(), number());
    }
    unreachable!("{case} is not in flex-ratio-cross.tsv")
}

/// What a row varies. Everything else is the 440x264 container.
///
/// **One field per axis the sweep measured, and the booleans are independent
/// rather than a state.** A bitfield or an enum would group axes that have no
/// relationship -- an item that clips and an item that wraps are not two values
/// of one thing -- and a reader checking a row against the table would have to
/// decode it.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a separate axis of the sweep, and grouping them would \
              hide that they vary independently"
)]
#[derive(Clone, Copy)]
struct Case {
    align: Align,
    row: bool,
    tall: bool,
    ratio: Option<f32>,
    grow: f32,
    pct_height: bool,
    min_w: Option<f32>,
    max_w: Option<f32>,
    item_clips: bool,
    container_clips: bool,
    content: bool,
    item_flex: bool,
    padding: f32,
    border: f32,
    wrap: bool,
    sibling: bool,
}

impl Case {
    const fn new() -> Self {
        Self {
            align: Align::Center,
            row: false,
            tall: true,
            ratio: Some(1.0),
            grow: 1.0,
            pct_height: false,
            min_w: None,
            max_w: None,
            item_clips: false,
            container_clips: false,
            content: false,
            item_flex: false,
            padding: 0.0,
            border: 0.0,
            wrap: false,
            sibling: false,
        }
    }
}

/// The container every row shares, with the row's own knobs on it.
fn container_of(case: Case) -> Node {
    let mut container = Node::new(NodeKind::Box);
    container.layout.display = Display::Flex;
    container.layout.flex_direction = if case.row {
        FlexDirection::Row
    } else {
        FlexDirection::Column
    };
    container.layout.align_items = Some(case.align);
    container.layout.size.0 = Dimension::Points(440.0);
    if case.tall {
        container.layout.size.1 = Dimension::Points(264.0);
    }
    container.layout.box_sizing = BoxSizing::BorderBox;
    for edge in [
        &mut container.layout.padding.left,
        &mut container.layout.padding.right,
        &mut container.layout.padding.top,
        &mut container.layout.padding.bottom,
    ] {
        *edge = Length::Points(8.0);
    }
    if case.container_clips {
        container.layout.overflow = (Overflow::Hidden, Overflow::Hidden);
    }
    if case.wrap {
        container.layout.flex_wrap = FlexWrap::Wrap;
    }
    container
}

/// The item under test.
fn item_of(case: Case) -> Node {
    let mut item = Node::new(NodeKind::Box);
    if case.item_flex {
        item.layout.display = Display::Flex;
    }
    item.layout.flex_grow = case.grow;
    item.layout.aspect_ratio = case.ratio;
    if case.pct_height {
        item.layout.size.1 = Dimension::Percent(1.0);
    }
    if let Some(value) = case.min_w {
        item.layout.min_size.0 = Dimension::Points(value);
    }
    if let Some(value) = case.max_w {
        item.layout.max_size.0 = Dimension::Points(value);
    }
    if case.item_clips {
        item.layout.overflow = (Overflow::Hidden, Overflow::Hidden);
    }
    if case.padding > 0.0 {
        for edge in [
            &mut item.layout.padding.left,
            &mut item.layout.padding.right,
            &mut item.layout.padding.top,
            &mut item.layout.padding.bottom,
        ] {
            *edge = Length::Points(case.padding);
        }
    }
    if case.border > 0.0 {
        for edge in [
            &mut item.layout.border.left,
            &mut item.layout.border.right,
            &mut item.layout.border.top,
            &mut item.layout.border.bottom,
        ] {
            *edge = case.border;
        }
    }
    item
}

/// The item's solved rectangle.
fn solved(case: Case) -> (f32, f32) {
    let mut scene = Scene::new(Size::new(1400.0, 1400.0));
    let container = scene
        .push(NodeId::ROOT, container_of(case))
        .unwrap_or_else(|error| unreachable!("{error}"));
    let item = scene
        .push(container, item_of(case))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if case.content {
        let mut child = Node::new(NodeKind::Box);
        child.layout.size = (Dimension::Points(30.0), Dimension::Points(10.0));
        scene
            .push(item, child)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    if case.sibling {
        let mut sibling = Node::new(NodeKind::Box);
        sibling.layout.flex_grow = case.grow;
        sibling.layout.aspect_ratio = case.ratio;
        scene
            .push(container, sibling)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    let result = solve(&scene, NodeId::ROOT, &mut NoLeaves)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let rect = result
        .get(item)
        .unwrap_or_else(|| unreachable!("no rectangle"));
    (rect.size.width, rect.size.height)
}

/// A pixel, because taffy rounds each edge and a size is the difference of two
/// of them. The same magnitude `DERIVED_TOLERANCE` carries, and for the same
/// reason.
const SLACK: f32 = 1.0;

/// The rows the compensation has to move.
fn rows_that_fire() -> Vec<(&'static str, Case)> {
    vec![
        ("center", Case::new()),
        (
            "flex-start",
            Case {
                align: Align::FlexStart,
                ..Case::new()
            },
        ),
        (
            "flex-end",
            Case {
                align: Align::FlexEnd,
                ..Case::new()
            },
        ),
        (
            "baseline",
            Case {
                align: Align::Baseline,
                ..Case::new()
            },
        ),
        (
            "ratio 0.5",
            Case {
                ratio: Some(0.5),
                ..Case::new()
            },
        ),
        (
            "ratio 2",
            Case {
                ratio: Some(2.0),
                ..Case::new()
            },
        ),
        (
            "ratio 8",
            Case {
                ratio: Some(8.0),
                ..Case::new()
            },
        ),
        (
            "two siblings",
            Case {
                sibling: true,
                ..Case::new()
            },
        ),
        (
            "item padding",
            Case {
                padding: 10.0,
                ..Case::new()
            },
        ),
        (
            "item border",
            Case {
                border: 5.0,
                ..Case::new()
            },
        ),
        (
            "item block with content",
            Case {
                content: true,
                ..Case::new()
            },
        ),
        (
            "item clips",
            Case {
                item_clips: true,
                ..Case::new()
            },
        ),
        (
            "container clips",
            Case {
                container_clips: true,
                ..Case::new()
            },
        ),
        (
            "wrap",
            Case {
                wrap: true,
                ..Case::new()
            },
        ),
    ]
}

/// The rows nothing should move, each removing one suspect.
///
/// **A table whose every row exercises the change would agree with a renderer
/// that fired everywhere.** These are what make the fourteen above mean
/// something: no growth, no ratio, nothing to grow into, a percentage main size
/// that already reaches the ratio, a row container where the axes swap, and the
/// `DioxusLabs/taffy#1081` shape we do not diverge on.
fn rows_that_control() -> Vec<(&'static str, Case)> {
    vec![
        (
            "control no grow",
            Case {
                grow: 0.0,
                ..Case::new()
            },
        ),
        (
            "control no ratio",
            Case {
                ratio: None,
                ..Case::new()
            },
        ),
        (
            "control auto container",
            Case {
                tall: false,
                ..Case::new()
            },
        ),
        (
            "control height 100%",
            Case {
                grow: 0.0,
                pct_height: true,
                ..Case::new()
            },
        ),
        (
            "control row container",
            Case {
                row: true,
                ..Case::new()
            },
        ),
        (
            "control stretch no main",
            Case {
                align: Align::Stretch,
                tall: false,
                ..Case::new()
            },
        ),
    ]
}

/// The rows that sit on a threshold, and the two that stay divergent.
fn rows_at_a_boundary() -> Vec<(&'static str, Case)> {
    vec![
        (
            "derived-tolerance-rounds",
            Case {
                grow: 0.0,
                pct_height: true,
                ratio: Some(0.333_333),
                ..Case::new()
            },
        ),
        (
            "pin-fires-min-width",
            Case {
                min_w: Some(300.0),
                ..Case::new()
            },
        ),
        (
            "pin-min-width ratio 0.5",
            Case {
                min_w: Some(300.0),
                ratio: Some(0.5),
                ..Case::new()
            },
        ),
        (
            "pin-min-width no grow",
            Case {
                min_w: Some(300.0),
                grow: 0.0,
                ..Case::new()
            },
        ),
        (
            "min-width slack",
            Case {
                min_w: Some(100.0),
                ..Case::new()
            },
        ),
        ("pin-quiet-empty", Case::new()),
        (
            "uncompensated stretch",
            Case {
                align: Align::Stretch,
                ..Case::new()
            },
        ),
        (
            "known max-width",
            Case {
                max_w: Some(100.0),
                ..Case::new()
            },
        ),
        (
            "known max-width amplified",
            Case {
                max_w: Some(1.0),
                ..Case::new()
            },
        ),
    ]
}

fn rows() -> Vec<(&'static str, Case)> {
    let mut all = rows_that_fire();
    all.extend(rows_that_control());
    all.extend(rows_at_a_boundary());
    all
}

#[test]
fn every_row_agrees_with_chrome_or_is_known() {
    let mut wrong = Vec::new();
    let mut stale = Vec::new();
    for (key, case) in rows() {
        let (width, height) = solved(case);
        let (want_width, want_height) = chrome(key);
        let agrees = (width - want_width).abs() <= SLACK
            && (height - want_height).abs() <= SLACK;
        if !agrees && !KNOWN.contains(&key) {
            wrong.push(format!(
                "{key}: chrome {want_width} x {want_height}, here {width} x {height}"
            ));
        }
        if agrees && KNOWN.contains(&key) {
            stale.push(key);
        }
    }
    assert!(
        wrong.is_empty(),
        "{} row(s) disagree:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    assert!(
        stale.is_empty(),
        "{stale:?} now agree with Chrome -- delete them from KNOWN, and if that \
         empties it the compensation covers everything this table asks"
    );
}

#[test]
fn the_construction_axis_moves_one_row() {
    // **The only row of the twenty-seven that the item's own display changes.**
    // Hand-assembled from `Node` values an item is `Display::Block`, and
    // through any factory on either surface it is `Display::Flex`; taffy
    // derives the cross size for the first and not the second. Chrome gives
    // `248x248` for both, so this is a fact about taffy rather than about
    // the browser -- which is why the compensation's predicate reads the
    // solved outcome and not the style.
    //
    // Both must now be Chrome's answer. Before the compensation the flex one
    // was `30x248`, the content's own contribution.
    let (chrome_width, chrome_height) = chrome("item flex with content");
    for item_flex in [false, true] {
        let (width, height) = solved(Case {
            content: true,
            item_flex,
            ..Case::new()
        });
        assert!(
            (width - chrome_width).abs() <= SLACK
                && (height - chrome_height).abs() <= SLACK,
            "item_flex {item_flex}: chrome {chrome_width} x {chrome_height}, \
             here {width} x {height}"
        );
    }
}

#[test]
fn the_tolerance_has_a_row_on_each_side() {
    // **`DERIVED_TOLERANCE` is a pixel and neither row is near it.** A correct
    // derivation sits 0.67 from `main x ratio` because taffy rounds each edge;
    // a real divergence sat 218 away. The threshold is in a gap of two orders
    // of magnitude rather than tuned to either.
    // **Ours, not Chrome's.** The browser reports the unrounded 82.66; the
    // quantity the tolerance guards is what *this* renderer solves, which is
    // 82 because taffy rounds each edge. Comparing Chrome's number here would
    // have measured the browser's precision and called it our slack.
    let (rounds_width, _) = solved(Case {
        grow: 0.0,
        pct_height: true,
        ratio: Some(0.333_333),
        ..Case::new()
    });
    let ideal = 248.0 * 0.333_333_f32;
    assert!(
        (rounds_width - ideal).abs() < 1.0
            && (rounds_width - ideal).abs() > 0.1,
        "the rounding row is {} from main x ratio, which is no longer both \
         inside the tolerance and outside an epsilon",
        (rounds_width - ideal).abs()
    );
    let (diverges_width, _) = chrome("derived-tolerance-diverges");
    assert!(
        (diverges_width - 30.0).abs() > 100.0,
        "the diverging row no longer sits far from the contribution it had"
    );
}
