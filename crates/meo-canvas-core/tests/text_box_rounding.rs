//! A box shrink-wrapping text is exactly its text's height: taffy rounds each
//! node from cumulative coordinates, `round(x + w) - round(x)`, so child and
//! parent edges share one rounded position. Chrome floors each line to 1/64 px
//! and sums: 67.171875 against our 67.

use meo_canvas_core::{
    layout,
    measure::SceneMeasurer,
    resolve::{Fonts, Resolved},
};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension,
        layout::FlexDirection,
        text::{LineHeight, TextSegment},
    },
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
        node.text.line_height = Some(LineHeight::Number(LINE_HEIGHT));
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
    // Text plus sibling is the box: both round from the same cumulative edge,
    // so no pixel is lost between them.
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
    // A sibling laid out after the text starts where the text ends, not
    // inside it.
    let (_, _, gap) = solve("Flower of Paradise in a narrow column", 120.0);
    assert!(
        gap.abs() < f32::EPSILON,
        "the sibling starts {gap} from where the text ends"
    );
}

#[test]
fn the_same_holds_when_the_text_is_padded() {
    // Padding moves the cumulative origin taffy rounds against, so this is the
    // case that would break a rule rounding sizes rather than edges.
    let (text, wrapper, gap) =
        solve("Flower of Paradise wrapped over lines", 90.0);
    assert!(text > 0.0, "the text measured nothing");
    assert!(wrapper >= text, "the box is shorter than its text");
    assert!(gap.abs() < f32::EPSILON, "the sibling overlaps by {gap}");
}

#[test]
fn a_fractional_total_rounds_once_at_the_end() {
    // Our rule: 22.4 a line, the total rounded once -- 45, 67, 90 and 112 for
    // two to five lines. Three lines separates the four round-or-ceil rules
    // (66 to 69); two is the control, 45 under both total rules. Chrome's
    // 67.171875 is sub-pixel from ours, so this pins our rule.
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

/// A text in a flex **row**, whose height says whether it wrapped.
///
/// The row is wide enough for the whole phrase several times over, so any
/// second line is the renderer's doing rather than the container's.
fn row_height(text: &str) -> f32 {
    let mut scene = Scene::new(Size::new(400.0, 200.0));
    if let Some(page) = scene.get_mut(NodeId::ROOT) {
        page.layout.flex_direction = FlexDirection::Column;
    }
    let row = scene
        .push(NodeId::ROOT, Node::new(NodeKind::Box))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(row) {
        node.layout.size = (Dimension::Points(400.0), Dimension::Auto);
        node.layout.flex_direction = FlexDirection::Row;
    }
    let paragraph = scene
        .push(
            row,
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
        node.text.line_height = Some(LineHeight::Number(1.0));
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
    solved
        .get(row)
        .unwrap_or_else(|| unreachable!("no rectangle for the row"))
        .size
        .height
}

/// A phrase in a roomy row stays on one line. A measured width is not floored
/// the way a styled length is: flooring it sizes the item a sixty-fourth short
/// and its last word wraps. The single word is the control, which a renderer
/// that never wraps would otherwise pass.
#[test]
fn a_phrase_in_a_roomy_row_does_not_wrap() {
    let one_word = row_height("CRITRate");
    assert!(
        (one_word - SIZE).abs() < 0.01,
        "a single word should be one line of {SIZE}, not {one_word}"
    );
    for phrase in ["CRIT Rate", "CRIT Rate Bonus", "Energy Recharge"] {
        let height = row_height(phrase);
        assert!(
            (height - one_word).abs() < 0.01,
            "{phrase:?} wrapped in a 400-wide row: {height} against \
             {one_word} for one word"
        );
    }
}
