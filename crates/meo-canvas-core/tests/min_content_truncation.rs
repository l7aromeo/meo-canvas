//! A text node's min-content width, with and without a truncation.
//!
//! # The claim
//!
//! CSS Flexbox 1 §4.5 floors a flex item at its automatic minimum size, which
//! for text is its min-content width. CSS Sizing 3 §5.1 defines that as the
//! narrowest the content can get "without overflowing" -- the widest word,
//! for a run that wraps at spaces.
//!
//! `text-overflow: ellipsis` and a line clamp are **used-value** behaviour:
//! they describe what is drawn when the used width is already below what the
//! content wants. They are not inputs to intrinsic sizing. So the same text
//! reports the same min-content width whether or not it carries a marker.
//!
//! # Why the pair
//!
//! A single string measured once proves nothing: any number is consistent
//! with any rule. Each case here is measured **twice**, plain and truncating,
//! and the assertion is that the two agree. A pair that came back equal for
//! the wrong reason -- because the rule ignored something both halves shared
//! -- is guarded against by the second axis: `Flower of Paradise` has a
//! min-content (its widest word) and a max-content (the whole run) that
//! genuinely differ, so a rule collapsing to either is visible.
//!
//! Chrome's own answers are in `chrome_min_content.rs`; this file is the
//! internal consistency argument and does not need a browser to fail.

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

/// A page holding one text node, and the node's id.
///
/// The page is wide enough that nothing is constrained by it: what is asked
/// for below is an *intrinsic* width, and a page narrow enough to clip would
/// answer a different question.
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

/// The width the measurer reports for `text` when asked `available`.
///
/// Asked through [`Measure`] directly rather than through a solve, because a
/// solve only ever exposes the *answer taffy kept*. The defect is in what it
/// was told, and this is the one place that question is legible.
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
    // The second axis. If min-content collapsed to the marker's width, the
    // assertion above would still pass whenever both halves collapsed the
    // same way -- so this pins the value, not just the agreement.
    //
    // `Flower of Paradise` breaks at spaces: min-content is `Paradise`, which
    // is strictly between the marker and the whole run.
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

/// Lays out a `space-between` row of the given width holding `HP` and
/// `46.6%`, and reports the width each text box was given.
///
/// `clamp_label` is the whole variable: the same row is solved with the label
/// truncating and not truncating, and the two answers are compared to each
/// other rather than to a number written down here.
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
    // The reported case, end to end -- but **not at the width it was reported
    // at**. A 150-pixel row has around 90 to spare, so nothing shrinks and
    // both spellings agree whatever the rule is; that width cannot fail. The
    // row has to be narrower than its contents before the automatic minimum
    // size is consulted at all, which is where the two spellings diverged:
    // the plain label held its 13 and the clamped one fell to the marker's 7.
    //
    // Run across the boundary rather than at one width, so the pair is
    // compared both where the floor is inert and where it binds.
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
