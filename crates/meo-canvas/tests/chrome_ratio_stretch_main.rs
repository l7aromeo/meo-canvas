//! Whether a stretched flex item derives its main size from its aspect ratio.
//!
//! `l7aromeo/meo-canvas#147`: a column container with a definite cross size,
//! holding one item at `flex-grow: 1; aspect-ratio: 1`. The item's cross size
//! comes from `stretch`, which makes it definite, and CSS transfers a definite
//! cross size through the ratio into the item's **automatic minimum** on the
//! main axis. That floors the main size at `424` where the line offers `248`,
//! so the item overflows its own line -- and three engines do exactly that.
//!
//! **Three engines rather than one.** Blink and `WebKit` share an ancestor
//! and Gecko shares none, so a row all three give is a reading of CSS rather
//! than of a codebase. The table carries one row per engine and this walker
//! compares against Chromium's, because
//! [`the_engines_agree_except_where_the_table_says_so`] has already refused
//! any row the three split on in a way [`WEBKIT_ALONE`] does not describe.
//!
//! **Solved rectangles rather than ink**, for the reason
//! `chrome_flex_ratio_cross.rs` gives: an item with no content paints nothing
//! whatever it was sized to, so an ink scan would measure the absence.
//!
//! **Construction: every row here is hand-assembled**, so the item is
//! `Display::Block` -- which is what a browser's `<div>` is, and what the
//! measured page used.
//!
//! Compensated in `layout.rs` by `stretched_ratio_minimum`; the
//! `align-items: stretch` row of `a_grown_main_size_never_reaches_the_ratio`
//! in `crates/meo-canvas-core/tests/taffy_flex_ratio.rs` pins what taffy does
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

const TABLE: &str = include_str!("assets/chrome/ratio-stretch-main.tsv");

/// The engine this renderer is compared against.
///
/// **Chromium rather than a vote**, because a majority would hide the thing
/// the three-engine measurement is for: where they differ, the row is a
/// finding rather than a target, and it is named in [`WEBKIT_ALONE`] with the
/// sentence that settles it, instead of being averaged away.
const REFERENCE: &str = "chromium";

/// The engine that reads one family of rows differently from the other two.
const OUTLIER: &str = "webkit";

/// The rows [`OUTLIER`] alone reads differently, and what settles them.
///
/// **`WebKit` feeds a clamped main size back through the ratio and the other
/// two do not.** With `max-height` on the item -- as a percentage or as a
/// length, which is the part that says it is not about percentages --
/// Chromium and Firefox clamp the main axis and leave the stretched cross
/// size alone, giving `424x248`; `WebKit` takes the clamped `248` back
/// through the ratio and gives `248x248`.
///
/// **Two against one is not what decides it; the specification is.** §4.5
/// ends the content-based minimum with "the size is clamped by the maximum
/// main size if it's definite" -- a clamp of the *minimum*, and neither §4.5
/// nor §9.8 sends a clamped main size back across the ratio to the cross
/// axis. So these rows are compared against [`REFERENCE`] like every other
/// row here, `stretched_ratio_minimum`'s own ceiling is what answers them,
/// and [`OUTLIER`] is recorded as the outlier rather than excused from being
/// one.
///
/// **`max-height` is still not the escape to recommend, for a different
/// reason than it was.** Not because the answer is unsettled, but because a
/// caller who writes it gets one box here, in Chromium and in Firefox, and a
/// different one in Safari -- which is a portability cost rather than an open
/// question. Both spellings are named because measuring only the percentage
/// would have read as a percentage-resolution difference, which engines do
/// differ about, and the length row is what rules that reading out.
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

/// The item under test, its content, and the sibling that shares its line.
///
/// **Split from [`solved`] along what a row varies**, the way [`container_of`]
/// and [`item_of`] already are: a row varies the container, the item, or what
/// is inside it.
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

/// What a caller writes to keep the result this renderer gave before.
///
/// **`min-height: 0` is the one to recommend**, and it is CSS rather than a
/// workaround: the floor is the item's *automatic* minimum, so naming any
/// definite minimum replaces it. `height: 100%` works for a different reason
/// -- a definite main size leaves nothing to derive -- and `overflow: hidden`
/// for a third, since a scroll container has no automatic minimum at all.
///
/// **`max-height` is on this list and is the one not to recommend**, which is
/// a different sentence from the other four: it gives the same box here as in
/// Chromium and Firefox, and a smaller one in Safari. [`WEBKIT_ALONE`] has
/// the split and the sentence of §4.5 that decides it.
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

/// The bounds on the stretched axis, and what the container does.
///
/// **The three `max-width` and `min-width` rows are the adjacent family**, the
/// one `flex-ratio-cross.tsv` carries as `max-width binds`. A bound cross size
/// is still definite, so it still transfers -- §4.5 says the transferred size
/// suggestion is the cross size "clamped by its minimum and maximum cross
/// sizes if they are definite" -- and the minimum it produces is then under
/// the line's own answer rather than over it, which is why these rows sit at
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

/// **The engines are checked against each other before this renderer is
/// checked against any of them.**
///
/// Three refusals rather than the two an exclusion list needs, because
/// [`WEBKIT_ALONE`] claims something narrower than "these rows are
/// unsettled": it claims [`OUTLIER`] differs *and the rest agree*. So a row
/// nothing names where any engine disagrees is refused; a named row where the
/// engines other than [`OUTLIER`] stop agreeing with each other is refused,
/// since the list would then be describing a split it does not describe; and
/// a named row [`OUTLIER`] has come to agree with is reported as stale, so
/// the naming does not outlive its reason.
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

/// **Every row, with no exemptions.**
///
/// There is no `KNOWN` list here and no row is skipped, and both are results
/// rather than omissions: the compensation covers every row of this family,
/// including the two [`WEBKIT_ALONE`] names -- those are compared against
/// [`REFERENCE`] like the rest, and are the only rows that reach
/// `stretched_ratio_minimum`'s clamp by a definite maximum main size. Skip
/// them and that clamp could be deleted with every row here still green.
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
