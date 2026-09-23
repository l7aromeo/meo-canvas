//! Whether a stretched flex item derives its main size from its ratio
//! (`l7aromeo/meo-canvas#147`): its definite cross size transfers into the
//! automatic minimum, flooring the main size at `424` where the line offers
//! `248`. Three engines measured; `stretched_ratio_minimum` compensates.

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

const TABLE: &str = include_str!("assets/chrome/ratio-stretch-main.tsv");

/// The engine this renderer is compared against: Chromium rather than a vote,
/// so where engines differ the row is named in [`WEBKIT_ALONE`] instead of
/// averaged away.
const REFERENCE: &str = "chromium";

/// The engine that reads one family of rows differently from the other two.
const OUTLIER: &str = "webkit";

/// The rows [`OUTLIER`] alone reads differently: with `max-height`, `WebKit`
/// feeds the clamped main size back through the ratio, `248x248` against
/// `424x248`. §4.5 clamps only the minimum, so these compare against
/// [`REFERENCE`] like the rest.
const WEBKIT_ALONE: &[&str] =
    &["escape max-height 100%", "escape max-height 248px"];

/// One row of the table, for one engine.
fn measured(case: &str, engine: &str) -> (f32, f32) {
    for line in TABLE.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut columns = line.split('\t');
        if columns.next() != Some(case) || columns.next() != Some(engine) {
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
    unreachable!("{case} on {engine} is not in ratio-stretch-main.tsv")
}

/// Every engine named in the table, in the order it names them.
fn engines() -> Vec<&'static str> {
    let mut all: Vec<&'static str> = Vec::new();
    for line in TABLE.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut columns = line.split('\t');
        columns.next();
        if let Some(engine) = columns.next()
            && !all.contains(&engine)
        {
            all.push(engine);
        }
    }
    all
}

/// How the item is asked to stretch.
#[derive(Clone, Copy, PartialEq)]
enum Stretch {
    /// `align-items: stretch` on the container.
    Container,
    /// Nothing said anywhere, since stretch is the initial value.
    Unstated,
    /// `align-self: stretch` on the item, against a container that does not.
    Item,
    /// `align-items: flex-start`, so the cross size is not stretched at all.
    None,
}

/// What a row varies. Everything else is the 440x264 container.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a separate axis of the sweep and they vary \
              independently: an item that clips and a container that wraps \
              are not two values of one thing"
)]
#[derive(Clone, Copy)]
struct Case {
    stretch: Stretch,
    ratio: Option<f32>,
    grow: f32,
    /// The item's own main size, as `height: 100%`.
    pct_main: bool,
    min_h: Option<Dimension>,
    max_h: Option<Dimension>,
    min_w: Option<f32>,
    max_w: Option<f32>,
    clips: bool,
    /// A child of the item, as width by height.
    content: Option<(f32, f32)>,
    padding: f32,
    border_box: bool,
    sibling: bool,
    row: bool,
    /// The container has a cross size of its own.
    tall: bool,
    wrap: bool,
    nested: bool,
}

impl Case {
    const fn new() -> Self {
        Self {
            stretch: Stretch::Container,
            ratio: Some(1.0),
            grow: 1.0,
            pct_main: false,
            min_h: None,
            max_h: None,
            min_w: None,
            max_w: None,
            clips: false,
            content: None,
            padding: 0.0,
            border_box: false,
            sibling: false,
            row: false,
            tall: true,
            wrap: false,
            nested: false,
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
    container.layout.align_items = match case.stretch {
        Stretch::Container => Some(Align::Stretch),
        Stretch::Unstated => None,
        Stretch::Item | Stretch::None => Some(Align::FlexStart),
    };
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
    if case.wrap {
        container.layout.flex_wrap = FlexWrap::Wrap;
    }
    container
}

/// The item under test.
fn item_of(case: Case) -> Node {
    let mut item = Node::new(NodeKind::Box);
    item.layout.flex_grow = case.grow;
    item.layout.aspect_ratio = case.ratio;
    if case.stretch == Stretch::Item {
        item.layout.align_self = Some(Align::Stretch);
    }
    if case.pct_main {
        item.layout.size.1 = Dimension::Percent(1.0);
    }
    if let Some(min) = case.min_h {
        item.layout.min_size.1 = min;
    }
    if let Some(max) = case.max_h {
        item.layout.max_size.1 = max;
    }
    if let Some(value) = case.min_w {
        item.layout.min_size.0 = Dimension::Points(value);
    }
    if let Some(value) = case.max_w {
        item.layout.max_size.0 = Dimension::Points(value);
    }
    if case.clips {
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
    if case.border_box {
        item.layout.box_sizing = BoxSizing::BorderBox;
    }
    item
}

/// The item under test, its content, and the sibling sharing its line, split
/// from [`solved`] along what a row varies.
fn push_item(scene: &mut Scene, parent: NodeId, case: Case) -> NodeId {
    let item = scene
        .push(parent, item_of(case))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some((width, height)) = case.content {
        let mut child = Node::new(NodeKind::Box);
        child.layout.size =
            (Dimension::Points(width), Dimension::Points(height));
        scene
            .push(item, child)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    if case.sibling {
        scene
            .push(parent, item_of(case))
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    item
}

/// The item's solved rectangle.
fn solved(case: Case) -> (f32, f32) {
    let mut scene = Scene::new(Size::new(1400.0, 1400.0));
    let container = scene
        .push(NodeId::ROOT, container_of(case))
        .unwrap_or_else(|error| unreachable!("{error}"));

    let parent = if case.nested {
        let mut middle = Node::new(NodeKind::Box);
        middle.layout.display = Display::Flex;
        middle.layout.flex_direction = FlexDirection::Column;
        middle.layout.align_items = Some(Align::Stretch);
        middle.layout.flex_grow = 1.0;
        scene
            .push(container, middle)
            .unwrap_or_else(|error| unreachable!("{error}"))
    } else {
        container
    };

    let item = push_item(&mut scene, parent, case);

    let result = solve(&scene, NodeId::ROOT, &mut NoLeaves)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let rect = result
        .get(item)
        .unwrap_or_else(|| unreachable!("the item is laid out"));
    (rect.size.width, rect.size.height)
}

/// **A pixel of slack**, which is what a solved size costs: taffy rounds each
/// edge and a size is the difference of two rounded numbers.
const SLACK: f32 = 1.0;

/// Every row of the table, in its order.
fn rows() -> Vec<(&'static str, Case)> {
    let mut all = reported_rows();
    all.extend(escape_rows());
    all.extend(item_rows());
    all.extend(container_and_bound_rows());
    all
}

/// The defect itself, and how the stretch is asked for.
fn reported_rows() -> Vec<(&'static str, Case)> {
    vec![
        ("stretch", Case::new()),
        (
            "align-self stretch",
            Case {
                stretch: Stretch::Item,
                ..Case::new()
            },
        ),
        (
            "align-items default",
            Case {
                stretch: Stretch::Unstated,
                ..Case::new()
            },
        ),
        (
            "no stretch",
            Case {
                stretch: Stretch::None,
                ..Case::new()
            },
        ),
    ]
}

/// What a caller writes to keep the earlier result: `min-height: 0` replaces
/// the automatic minimum; `height: 100%` and `overflow: hidden` also work.
/// `max-height` works here but not in Safari -- see [`WEBKIT_ALONE`].
fn escape_rows() -> Vec<(&'static str, Case)> {
    vec![
        (
            "escape min-height 0",
            Case {
                min_h: Some(Dimension::Points(0.0)),
                ..Case::new()
            },
        ),
        (
            "escape height 100%",
            Case {
                pct_main: true,
                ..Case::new()
            },
        ),
        (
            "escape max-height 100%",
            Case {
                max_h: Some(Dimension::Percent(1.0)),
                ..Case::new()
            },
        ),
        (
            "escape max-height 248px",
            Case {
                max_h: Some(Dimension::Points(248.0)),
                ..Case::new()
            },
        ),
        (
            "escape overflow hidden",
            Case {
                clips: true,
                ..Case::new()
            },
        ),
    ]
}

/// What the ratio is and what the item carries.
fn item_rows() -> Vec<(&'static str, Case)> {
    vec![
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
            "no ratio",
            Case {
                ratio: None,
                ..Case::new()
            },
        ),
        (
            "no grow",
            Case {
                grow: 0.0,
                ..Case::new()
            },
        ),
        (
            "item content",
            Case {
                content: Some((100.0, 100.0)),
                ..Case::new()
            },
        ),
        (
            "item content taller",
            Case {
                content: Some((100.0, 600.0)),
                ..Case::new()
            },
        ),
        (
            "item padding",
            Case {
                padding: 12.0,
                ..Case::new()
            },
        ),
        (
            "item border-box",
            Case {
                padding: 12.0,
                border_box: true,
                ..Case::new()
            },
        ),
        (
            "two items",
            Case {
                sibling: true,
                ..Case::new()
            },
        ),
    ]
}

/// The bounds on the stretched axis, and the container: a bound cross size is
/// still definite and still transfers, clamped by §4.5, so these rows sit at
/// `248` where the unbound ones sit at `424`.
fn container_and_bound_rows() -> Vec<(&'static str, Case)> {
    vec![
        (
            "max-width 100px",
            Case {
                max_w: Some(100.0),
                ..Case::new()
            },
        ),
        (
            "max-width 1px",
            Case {
                max_w: Some(1.0),
                ..Case::new()
            },
        ),
        (
            "min-width 600px",
            Case {
                min_w: Some(600.0),
                ..Case::new()
            },
        ),
        (
            "row container",
            Case {
                row: true,
                ..Case::new()
            },
        ),
        (
            "container auto cross",
            Case {
                tall: false,
                ..Case::new()
            },
        ),
        (
            "container wrap",
            Case {
                wrap: true,
                ..Case::new()
            },
        ),
        (
            "nested",
            Case {
                nested: true,
                ..Case::new()
            },
        ),
    ]
}

/// The engines are checked against each other first: a row nothing names where
/// any engine disagrees is refused, as is a named row where the other two stop
/// agreeing, and a named row [`OUTLIER`] now agrees with is stale.
#[test]
fn the_engines_agree_except_where_the_table_says_so() {
    let all = engines();
    assert!(
        all.len() >= 3,
        "the table names {} engine(s); the whole argument for this file is \
         that three of them agree, and two cannot make it",
        all.len()
    );
    assert!(
        all.contains(&OUTLIER) && REFERENCE != OUTLIER,
        "{OUTLIER} has to be a column of the table and cannot also be \
         {REFERENCE}, or naming a row in WEBKIT_ALONE would assert nothing"
    );
    let cells = |key: &str| -> String {
        all.iter()
            .map(|engine| {
                let (width, height) = measured(key, engine);
                format!("{engine} {width}x{height}")
            })
            .collect::<Vec<String>>()
            .join(", ")
    };
    let mut unexpected = Vec::new();
    let mut not_a_split = Vec::new();
    let mut stale = Vec::new();
    for (key, _) in rows() {
        let named = WEBKIT_ALONE.contains(&key);
        let expected_to_agree: Vec<&str> = all
            .iter()
            .copied()
            .filter(|engine| !named || *engine != OUTLIER)
            .collect();
        let first = measured(key, expected_to_agree[0]);
        let agree = expected_to_agree
            .iter()
            .all(|engine| approximately(measured(key, engine), first));
        if !agree {
            if named {
                not_a_split.push(format!("{key}: {}", cells(key)));
            } else {
                unexpected.push(format!("{key}: {}", cells(key)));
            }
        } else if named && approximately(measured(key, OUTLIER), first) {
            stale.push(key);
        }
    }
    assert!(
        unexpected.is_empty(),
        "{} row(s) the engines disagree on and WEBKIT_ALONE does not name:\n{}\n\
         A disagreement is a finding rather than a defect here. Do not \
         compensate it on a majority: say which engines gave what, and name \
         the sentence of the specification that settles it, if one does",
        unexpected.len(),
        unexpected.join("\n")
    );
    assert!(
        not_a_split.is_empty(),
        "{} row(s) WEBKIT_ALONE names where the engines other than {OUTLIER} \
         no longer agree either:\n{}\n\
         The list claims one engine reads these differently. That has stopped \
         being true, so the comparison against {REFERENCE} is no longer \
         standing on two engines and a specification sentence",
        not_a_split.len(),
        not_a_split.join("\n")
    );
    assert!(
        stale.is_empty(),
        "{stale:?} now agree across every engine -- delete them from \
         WEBKIT_ALONE, and check whether the release notes still need to warn \
         that Safari differs"
    );
}

/// Within [`SLACK`] on both axes.
fn approximately(left: (f32, f32), right: (f32, f32)) -> bool {
    (left.0 - right.0).abs() <= SLACK && (left.1 - right.1).abs() <= SLACK
}

/// Every row, with no exemptions, including the two [`WEBKIT_ALONE`] names:
/// they are the only rows reaching `stretched_ratio_minimum`'s clamp by a
/// definite maximum, so skipping them would leave it deletable.
#[test]
fn every_row_matches_the_reference() {
    let mut wrong = Vec::new();
    for (key, case) in rows() {
        let (want_width, want_height) = measured(key, REFERENCE);
        let (width, height) = solved(case);
        if !approximately((width, height), (want_width, want_height)) {
            wrong.push(format!(
                "{key}: {REFERENCE} {want_width} x {want_height}, here {width} x {height}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} row(s) disagree:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
