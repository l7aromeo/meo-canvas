//! A text node's min-content width with and without a truncation: an ellipsis
//! or line clamp is used-value behaviour, not an input to intrinsic sizing, so
//! both report the widest word (CSS Sizing 3 §5.1). Each case is measured plain
//! and truncating; Chrome's answers are in `chrome_min_content.rs`.

use meo_canvas_core::{
    layout,
    measure::{Available, Measure, SceneMeasurer},
    resolve::{Fonts, Resolved},
};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        layout::{Display, FlexDirection, Justify},
        text::{
            FontWeight, LineHeight, ParagraphStyle, TextSegment, TextStyle,
        },
    },
};

/// The face the fixtures register and the one Chrome was asked about.
const FONT: (&str, &str) =
    ("Fixture", "tests/assets/fonts/Oswald-VariableFont_wght.ttf");

/// The marker, U+2026, which is what CSS uses.
const MARKER: &str = "\u{2026}";

/// Registers the fixture face. Every measurement here is of that face.
fn fonts() -> Fonts {
    let fonts = Fonts::new();
    fonts.register_path(FONT.0, FONT.1).unwrap_or_else(|error| {
        unreachable!("the face did not register: {error}")
    });
    fonts
}

/// A page holding one text node, and the node's id; wide enough to constrain
/// nothing, since what is asked below is an intrinsic width.
fn page_with_text(
    text: &str,
    size: f32,
    paragraph: ParagraphStyle,
) -> (Scene, NodeId) {
    let mut scene = Scene::new(Size::new(1000.0, 400.0));
    let node = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Text {
                segments: vec![TextSegment {
                    text: text.to_owned(),
                    style: TextStyle::default(),
                }],
                paragraph,
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(node) {
        node.text.font_family = Some(FONT.0.to_owned());
        node.text.font_size = Some(size);
        node.text.line_height = Some(LineHeight::Length(16.0));
    }
    (scene, node)
}

/// The width the measurer reports for `text` when asked `available`, through
/// [`Measure`] directly, since a solve exposes only the answer taffy kept.
fn intrinsic(
    text: &str,
    size: f32,
    paragraph: ParagraphStyle,
    available: Available,
) -> f32 {
    let (scene, node) = page_with_text(text, size, paragraph);
    let fonts = fonts();
    let resolved = Resolved::new(&scene, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    measurer
        .measure(node, (None, None), (available, Available::MaxContent))
        .size
        .width
}

/// A truncating paragraph: one line, with a marker. `text-overflow: ellipsis`.
fn clamped() -> ParagraphStyle {
    ParagraphStyle {
        max_lines: Some(1),
        ellipsis: Some(MARKER.to_owned()),
    }
}

/// A paragraph with no truncation at all. The control.
fn plain() -> ParagraphStyle {
    ParagraphStyle::default()
}

#[test]
fn a_clamp_does_not_change_min_content_width() {
    // The three strings cover the three shapes the rule has to answer for:
    // one word with no break opportunity (`HP` -- the reported case), a run
    // whose min-content and max-content genuinely differ, and a single word
    // wider than any sensible container.
    for (text, size) in [
        ("HP", 12.0),
        ("Flower of Paradise", 16.0),
        ("Antidisestablishmentarianism", 16.0),
    ] {
        let bare = intrinsic(text, size, plain(), Available::MinContent);
        let clipped = intrinsic(text, size, clamped(), Available::MinContent);
        assert!(
            (bare - clipped).abs() < 0.01,
            "{text:?} at {size}px reports {clipped} as its min-content width \
             with an ellipsis and {bare} without; a clamp is used-value \
             behaviour and must not change an intrinsic size"
        );
    }
}

#[test]
fn min_content_is_the_widest_word_not_the_marker() {
    // Pins the value, not only the agreement, which both halves collapsing
    // alike would pass: `Flower of Paradise` has min-content `Paradise`,
    // strictly between the marker and the whole run.
    let marker = intrinsic(MARKER, 16.0, plain(), Available::MaxContent);
    let widest = intrinsic("Paradise", 16.0, plain(), Available::MaxContent);
    let whole =
        intrinsic("Flower of Paradise", 16.0, plain(), Available::MaxContent);

    for (name, paragraph) in [("plain", plain()), ("clamped", clamped())] {
        let min = intrinsic(
            "Flower of Paradise",
            16.0,
            paragraph,
            Available::MinContent,
        );
        assert!(
            (min - widest).abs() < 0.01,
            "as {name}, min-content is {min}, and `Paradise` -- the widest \
             word -- is {widest}"
        );
        assert!(
            min > marker,
            "as {name}, min-content {min} collapsed to at most the marker's \
             {marker}"
        );
        assert!(
            min < whole,
            "as {name}, min-content {min} is the whole run's {whole}; a case \
             whose two intrinsic widths coincide measures nothing"
        );
    }
}

#[test]
fn a_word_with_no_break_opportunity_reports_its_whole_width() {
    // Stated rather than assumed, because it is the case the reported defect
    // is: a run with nowhere to break has min-content == max-content. It does
    // **not** shrink to the marker, and it does not shrink to zero.
    for (text, size) in [("HP", 12.0), ("Antidisestablishmentarianism", 16.0)] {
        for paragraph in [plain(), clamped()] {
            let min =
                intrinsic(text, size, paragraph.clone(), Available::MinContent);
            let max = intrinsic(text, size, paragraph, Available::MaxContent);
            assert!(
                (min - max).abs() < 0.01,
                "{text:?} has no break opportunity, so its min-content {min} \
                 must equal its max-content {max}"
            );
        }
    }
}

/// Lays out a `space-between` row holding `HP` and `46.6%` and reports each
/// text box's width. `clamp_label` is the only variable, and the two answers
/// are compared to each other rather than to a written number.
fn row(width: f32, clamp_label: bool) -> (f32, f32) {
    let mut scene = Scene::new(Size::new(400.0, 100.0));
    let row = scene
        .push(NodeId::ROOT, Node::new(NodeKind::Box))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(row) {
        node.layout.size = (Dimension::Points(width), Dimension::Auto);
        // A row is a flex container and says so: the scene's default display
        // is `block`, which both public surfaces override on every container
        // they build. A scene assembled node by node, as this one is, does not.
        node.layout.display = Display::Flex;
        node.layout.flex_direction = FlexDirection::Row;
        node.layout.justify_content = Some(Justify::SpaceBetween);
        node.layout.gap = (Length::Points(8.0), Length::ZERO);
    }

    let mut text = |parent, body: &str, size: f32, paragraph, weight| {
        let id = scene
            .push(
                parent,
                Node::new(NodeKind::Text {
                    segments: vec![TextSegment {
                        text: body.to_owned(),
                        style: TextStyle::default(),
                    }],
                    paragraph,
                }),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(id) {
            node.text.font_family = Some(FONT.0.to_owned());
            node.text.font_size = Some(size);
            node.text.font_weight = weight;
            node.text.line_height = Some(LineHeight::Length(16.0));
        }
        id
    };
    let label = text(
        row,
        "HP",
        12.0,
        if clamp_label { clamped() } else { plain() },
        None,
    );
    let value = text(row, "46.6%", 14.0, plain(), Some(FontWeight::new(600)));

    let fonts = fonts();
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
            .size
            .width
    };
    (rect(label), rect(value))
}

#[test]
fn a_clamped_label_is_floored_where_a_plain_one_is() {
    // Across the boundary, not only at the reported 150: a row with room to
    // spare never consults the automatic minimum. Narrower, it binds, which
    // is where a clamped label at the marker's 7 against the plain label's
    // 13 would show.
    for width in [150.0_f32, 60.0, 40.0, 25.0] {
        let (plain_label, plain_value) = row(width, false);
        let (clamped_label, clamped_value) = row(width, true);
        assert!(
            (plain_label - clamped_label).abs() < 0.01,
            "in a row of {width}, `HP` is laid out {clamped_label} wide with \
             an ellipsis and {plain_label} without"
        );
        assert!(
            (plain_value - clamped_value).abs() < 0.01,
            "in a row of {width}, the sibling is {clamped_value} wide with a \
             clamped label and {plain_value} with a plain one"
        );
    }
}

#[test]
fn the_label_holds_its_own_width_once_the_row_cannot_fit_it() {
    // The half the pair above cannot see: both spellings agreeing on a wrong
    // number would pass it. `HP` wants its full width and a row of 25 has
    // nowhere near enough for it and its sibling, so §4.5 says the label
    // keeps it and the row overflows instead.
    let wanted = intrinsic("HP", 12.0, plain(), Available::MaxContent);
    let (label, _) = row(25.0, true);
    assert!(
        (label - wanted).abs() < 1.0,
        "`HP` was squeezed to {label} in a row of 25; its automatic minimum \
         size is its min-content width of {wanted}, which it must keep"
    );
}
