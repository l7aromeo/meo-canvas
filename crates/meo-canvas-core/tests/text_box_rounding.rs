//! What a box shrink-wrapping text measures, and why two rounding rules
//! cannot disagree about it here.
//!
//! # The defect this does not have
//!
//! v1 fixed a real bug (`meo-canvas-old`, `5815e11`): a box shrink-wrapping
//! text came out **a pixel shorter than the text inside it**, so the text's
//! own background spilled past its parent and a sibling laid out after it
//! started before the text ended. Yoga is where the two parted company --
//! setting a measure function makes a node `NodeType::Text` and Yoga **ceils**
//! such a node's edges so glyphs are never cut, while a plain parent
//! shrink-wrapping it **rounds to nearest**. Three lines measuring 63.3 gave
//! the text 64 and the box around it 63.
//!
//! **taffy has one rule and applies it to every node.** `round_layout`
//! (`taffy-0.13.0/src/compute/mod.rs:219`) carries no text special case, and
//! it rounds on *cumulative* coordinates:
//!
//! ```text
//! size.width = round(cumulative_x + width) - round(cumulative_x)
//! ```
//!
//! A child's rounded edge is derived from the same rounded absolute position
//! as its parent's, **so the two cannot disagree by construction**.
//!
//! **These assertions therefore cannot fail for the reason they were
//! written**, and that is why the mechanism is recorded above them rather than
//! left to be re-derived. They are kept because the next reader to meet v1's
//! commit will ask whether we carry the same bug, and one file answering that
//! is worth more than three that pass silently.
//!
//! # What is still open, and is not this
//!
//! Yoga ceils because a box shorter than its glyphs cuts them. **taffy rounds,
//! so text measuring 63.3 gets a 63-pixel box and the last row of ink is a
//! third of a pixel short.** Nothing disagrees; the box is simply smaller than
//! the ink. Which of those is right is a Chrome question and is not settled
//! here.
//!
//! # Chrome's answer, and it is none of the candidates
//!
//! Four rules were enumerated before the browser was asked -- round or ceil,
//! per line or on the total -- and a 16px face at `line-height: 1.4` over
//! three lines separates all four: 66, 67, 68, 69. **Chrome does none of
//! them.** It works in sixty-fourths of a pixel, **floors each line into that
//! grid and sums**, and never rounds the total:
//!
//! ```text
//! 22.4 x 64 = 1433.6 -> 1433 -> 22.390625      three lines -> 67.171875
//! ```
//!
//! So our 67 is `0.17` away and the difference is sub-pixel rather than whole.
//!
//! **The trap in measuring it was `offsetHeight`, which reports 67** -- an
//! integer API rounding a fractional layout and handing our own rule back to
//! us. A check that had stopped there would have closed on a false agreement
//! reached from a correct number.
//!
//! **This is therefore a layout question rather than a text one**: taffy
//! rounds every box and Chrome rounds none. It is tracked separately, and the
//! first thing to establish is whether it is observable at all -- one box's
//! sub-pixel difference vanishes into antialiasing, and only a stack of many
//! would drift far enough to see.
//!
//! **What does not follow: porting v1's whole-pixel measure.** It treats a
//! defect this crate does not have, and Chrome neither rounds nor ceils a
//! total, so it would move us further from the browser rather than closer.

use meo_canvas_core::{
    layout,
    measure::SceneMeasurer,
    resolve::{Fonts, Resolved},
};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{Dimension, layout::FlexDirection, text::TextSegment},
};

/// The face the fixtures register.
const FONT: (&str, &str) =
    ("Fixture", "tests/assets/fonts/Oswald-VariableFont_wght.ttf");

/// A size and multiple whose three-line total lands on a fraction well clear
/// of both `.0` and `.5` -- `22.4` a line, `67.2` in all.
const SIZE: f32 = 16.0;
/// The multiple that supplies the fraction. **The face cannot**: the strut's
/// ascent and descent are rounded (`lines.rs:367`), so a line box built from
/// metrics alone is always whole.
const LINE_HEIGHT: f32 = 1.4;

/// A text node above a plain sibling, both shrink-wrapping, in a fixed page.
fn solve(text: &str, width: f32) -> (f32, f32, f32) {
    let mut scene = Scene::new(Size::new(width, 400.0));
    if let Some(page) = scene.get_mut(NodeId::ROOT) {
        page.layout.flex_direction = FlexDirection::Column;
    }
    let column = scene
        .push(NodeId::ROOT, Node::new(NodeKind::Box))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(column) {
        node.layout.size = (Dimension::Points(width), Dimension::Auto);
        // A row is the default, and in a row the cross axis is vertical: the
        // children would stretch to the page's full height and the question
        // would never arise.
        node.layout.flex_direction = FlexDirection::Column;
    }
    let paragraph = scene
        .push(
            column,
            Node::new(NodeKind::Text {
                segments: vec![TextSegment {
                    text: text.to_owned(),
                    style: meo_canvas_scene::style::text::TextStyle::default(),
                }],
                paragraph:
                    meo_canvas_scene::style::text::ParagraphStyle::default(),
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(paragraph) {
        node.text.font_family = Some(FONT.0.to_owned());
        node.text.font_size = Some(SIZE);
        node.text.line_height = Some(LINE_HEIGHT);
    }
    let sibling = scene
        .push(column, Node::new(NodeKind::Box))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(sibling) {
        node.layout.size = (Dimension::Points(width), Dimension::Points(10.0));
    }

    let fonts = Fonts::new();
    fonts.register_path(FONT.0, FONT.1).unwrap_or_else(|error| {
        unreachable!("the face did not register: {error}")
    });
    let resolved = Resolved::new(&scene, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let solved = layout::solve(&scene, NodeId::ROOT, &mut measurer)
        .unwrap_or_else(|error| unreachable!("{error}"));

    let rect = |node| {
        solved
            .get(node)
            .unwrap_or_else(|| unreachable!("no rectangle for the node"))
    };
    let (text_box, wrapper, after) =
        (rect(paragraph), rect(column), rect(sibling));
    (
        text_box.size.height,
        wrapper.size.height,
        after.origin.y - (text_box.origin.y + text_box.size.height),
    )
}

#[test]
fn a_shrink_wrapping_box_is_the_height_of_its_text() {
    // v1's first case. Yoga gave 64 to the text and 63 to the box; taffy
    // rounds both from the same cumulative edge, so there is no pixel to
    // lose.
    let (text, wrapper, _) =
        solve("Flower of Paradise in a narrow column", 120.0);
    let sibling = 10.0;
    assert!(
        (wrapper - (text + sibling)).abs() < f32::EPSILON,
        "the box is {wrapper} around {text} of text and {sibling} of sibling"
    );
}

#[test]
fn a_sibling_starts_where_the_text_ends() {
    // v1's third case, and the one the disagreement actually cost: a sibling
    // laid out after the text began before the text ended.
    let (_, _, gap) = solve("Flower of Paradise in a narrow column", 120.0);
    assert!(
        gap.abs() < f32::EPSILON,
        "the sibling starts {gap} from where the text ends"
    );
}

#[test]
fn the_same_holds_when_the_text_is_padded() {
    // v1's second case. Padding moves the cumulative origin, which is what
    // taffy rounds against -- so it is the case that would break a rule
    // rounding sizes rather than edges.
    let (text, wrapper, gap) =
        solve("Flower of Paradise wrapped over lines", 90.0);
    assert!(text > 0.0, "the text measured nothing");
    assert!(wrapper >= text, "the box is shorter than its text");
    assert!(gap.abs() < f32::EPSILON, "the sibling overlaps by {gap}");
}

#[test]
fn a_fractional_total_rounds_once_at_the_end() {
    // **Where this crate sits in the four-way table, measured rather than
    // read off the code.** A 16px face at `line-height: 1.4` is 22.4 a line,
    // and the box comes back as the sum rounded once:
    //
    // ```text
    // lines   total    ours
    //   2      44.8      45
    //   3      67.2      67     <- the discriminating case
    //   4      89.6      90
    //   5     112.0     112
    // ```
    //
    // Three lines separates the four rules that were on the table before
    // Chrome was asked -- 66, 67, 68, 69. Chrome turned out to do none of
    // them: sixty-fourths of a pixel, floored per line, summed, `67.171875`.
    // The pin stays at our own number because it records *our* rule; the gap
    // to Chrome is sub-pixel and is tracked as a layout question.
    // **Two lines is the control**: 44.8 is 45 under both total rules, so it
    // must agree whatever Chrome turns out to do, and a disagreement there
    // means something other than rounding is happening.
    //
    // This pin fails the moment the rule changes, which is the point: when
    // Chrome's answer arrives, the number that moves says which rule replaced
    // which.
    for (width, expected) in
        [(120.0, 45.0), (100.0, 67.0), (80.0, 90.0), (60.0, 112.0)]
    {
        let (text, _, _) =
            solve("Flower of Paradise in a narrow column", width);
        assert!(
            (text - expected).abs() < f32::EPSILON,
            "at {width} wide the text box is {text} where {expected} is the \
             sum of its lines rounded once"
        );
    }
}
