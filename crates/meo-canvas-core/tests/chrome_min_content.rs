//! What a truncation does to an intrinsic width, against Chrome.
//!
//! # The rule, and why a browser was asked
//!
//! CSS Sizing 3 §5.1 derives min-content from the content, and Flexbox 1 §4.5
//! floors a flex item there. `text-overflow: ellipsis` and `-webkit-line-clamp`
//! are used-value behaviour -- what is *drawn* once the width is settled -- so
//! neither should lower the number the floor is taken from. That reading is
//! what the table checks, rather than being assumed by it.
//!
//! # The control is the row that makes the others mean something
//!
//! `text-overflow: ellipsis` needs `white-space: nowrap` to do anything, and
//! `nowrap` raises min-content to the whole run **on its own** by removing
//! every break opportunity. A table holding only `plain` and `ellipsis` would
//! show a difference and credit it to the marker. So the harness measures
//! `nowrap` without `text-overflow` as well, and the reading is that
//! `ellipsis` matches **`nowrap`**, not `plain`: 106.50 against 106.50, where
//! `plain` is 49.91. The marker changed nothing; the `nowrap` did.
//!
//! `-webkit-line-clamp` is the one that truncates while still wrapping, and so
//! is the analogue of this renderer's `max_lines`. Chrome leaves it at the
//! plain min-content -- 49.91 -- which is the row this crate's behaviour is
//! actually pinned against.
//!
//! Measured through `just conformance`;
//! `crates/meo-canvas/tests/assets/chrome/min-content.tsv`.

use meo_canvas_core::{
    measure::{Available, Measure, SceneMeasurer},
    resolve::{Fonts, Resolved},
};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::text::{LineHeight, ParagraphStyle, TextSegment, TextStyle},
};

/// The face the harness embedded and the fixtures register.
const FONT: (&str, &str) =
    ("Fixture", "tests/assets/fonts/Oswald-VariableFont_wght.ttf");

/// The marker, U+2026.
const MARKER: &str = "\u{2026}";

const TABLE: &str =
    include_str!("../../meo-canvas/tests/assets/chrome/min-content.tsv");

/// How far our number may sit from Chrome's.
///
/// Tight on purpose. The agreeing rows come in within 0.03 -- `171.81` is
/// exact -- and the defect this file exists for moves a width by 4 to 40
/// pixels, so a tolerance loose enough to be safe would be loose enough to
/// pass the bug.
const TOLERANCE: f32 = 0.25;

/// One row of the table.
struct Row {
    text: String,
    size: f32,
    variant: String,
    min: f32,
    max: f32,
}

/// The table, parsed. Comment lines and blanks are skipped.
fn rows() -> Vec<Row> {
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut column = line.split('\t');
            let mut next = || {
                column.next().unwrap_or_else(|| {
                    unreachable!("a row of min-content.tsv is short: {line}")
                })
            };
            // The text column is a JSON string, so a value containing a space
            // survives the round trip and is read back as what was measured.
            let quoted = next();
            let text = quoted.trim_matches('"').to_owned();
            let number = |value: &str| {
                value.parse::<f32>().unwrap_or_else(|error| {
                    unreachable!("{value:?} is not a number: {error}")
                })
            };
            Row {
                text,
                size: number(next()),
                variant: next().to_owned(),
                min: number(next()),
                max: number(next()),
            }
        })
        .collect()
}

/// The width this crate reports for `text` at `size` when asked `available`.
fn ours(
    text: &str,
    size: f32,
    paragraph: ParagraphStyle,
    available: Available,
) -> f32 {
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

    let fonts = Fonts::new();
    fonts.register_path(FONT.0, FONT.1).unwrap_or_else(|error| {
        unreachable!("the face did not register: {error}")
    });
    let resolved = Resolved::new(&scene, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    measurer
        .measure(node, (None, None), (available, Available::MaxContent))
        .size
        .width
}

/// The paragraph this crate spells each measured variant with.
///
/// `nowrap` and `ellipsis` have **no spelling here**: this renderer has no
/// `white-space` property, so those two rows are read as Chrome-internal
/// evidence rather than compared against us. `clamp` is the one that maps,
/// because `-webkit-line-clamp` truncates a run that still wraps, which is
/// exactly what `max_lines` with an ellipsis does.
fn spelling(variant: &str) -> Option<ParagraphStyle> {
    match variant {
        "plain" => Some(ParagraphStyle::default()),
        "clamp" => Some(ParagraphStyle {
            max_lines: Some(1),
            ellipsis: Some(MARKER.to_owned()),
        }),
        _ => None,
    }
}

#[test]
fn our_intrinsic_widths_are_chromes() {
    let mut compared = 0_usize;
    for row in rows() {
        let Some(paragraph) = spelling(&row.variant) else {
            continue;
        };
        let min = ours(
            &row.text,
            row.size,
            paragraph.clone(),
            Available::MinContent,
        );
        let max = ours(&row.text, row.size, paragraph, Available::MaxContent);
        assert!(
            (min - row.min).abs() < TOLERANCE,
            "{:?} at {}px as {}: our min-content is {min}, Chrome's is {}",
            row.text,
            row.size,
            row.variant,
            row.min
        );
        assert!(
            (max - row.max).abs() < TOLERANCE,
            "{:?} at {}px as {}: our max-content is {max}, Chrome's is {}",
            row.text,
            row.size,
            row.variant,
            row.max
        );
        compared += 1;
    }
    // A parse that silently matched nothing would pass every assertion above.
    assert_eq!(
        compared, 6,
        "expected three strings in two comparable variants"
    );
}

#[test]
fn chrome_does_not_lower_an_intrinsic_width_to_the_marker() {
    // The reading of the table, asserted rather than left in a comment: this
    // is the premise the fix rests on, and if a re-measure ever contradicts it
    // the fix is wrong rather than the test.
    let rows = rows();
    let find = |text: &str, variant: &str| {
        rows.iter()
            .find(|row| row.text == text && row.variant == variant)
            .unwrap_or_else(|| unreachable!("no {variant} row for {text:?}"))
    };

    for text in ["HP", "Flower of Paradise", "Antidisestablishmentarianism"] {
        let plain = find(text, "plain");
        let nowrap = find(text, "nowrap");
        let ellipsis = find(text, "ellipsis");
        let clamp = find(text, "clamp");

        // `text-overflow: ellipsis` against its own control, which is the
        // only comparison that isolates the marker from the `nowrap` it needs.
        assert!(
            (ellipsis.min - nowrap.min).abs() < TOLERANCE,
            "{text:?}: Chrome's min-content is {} with an ellipsis and {} \
             without, on otherwise identical `nowrap` boxes",
            ellipsis.min,
            nowrap.min
        );
        // And the clamp, which is the one this renderer spells, against the
        // untruncated baseline.
        assert!(
            (clamp.min - plain.min).abs() < TOLERANCE,
            "{text:?}: Chrome's min-content is {} clamped and {} plain",
            clamp.min,
            plain.min
        );
        // Neither ever drops below the baseline. The direction is the claim.
        assert!(
            ellipsis.min >= plain.min - TOLERANCE
                && clamp.min >= plain.min - TOLERANCE,
            "{text:?}: a truncation lowered Chrome's min-content below its \
             plain {}",
            plain.min
        );
    }

    // The suite has to contain one string whose two intrinsic widths differ,
    // or every row above is satisfied by a rule that returns max-content for
    // everything.
    let plain = find("Flower of Paradise", "plain");
    assert!(
        plain.max - plain.min > 1.0,
        "no measured string distinguishes min-content from max-content"
    );
}
