//! A segment's own `letterSpacing`, asserted on advances rather than pixels:
//! spacing feeds the line breaker as well as the drawing, so run widths are
//! what both must agree on. An `em` resolves against the segment's own size,
//! not the paragraph's.

use meo_canvas_core::{
    lines::{Metrics, TextMeasurer, wrap},
    resolve::{Fonts, ResolvedText},
};
use meo_canvas_scene::style::text::{Spacing, TextSegment, TextStyle};

const FAMILY: &str = "SpacingProbe";
const FONT: &str = "tests/assets/fonts/Oswald-VariableFont_wght.ttf";

/// A paragraph style at `size`, in a family this file registers.
fn base(size: f32) -> ResolvedText {
    let fonts = Fonts::new();
    fonts
        .register_path(FAMILY, FONT)
        .unwrap_or_else(|error| unreachable!("{error}"));
    ResolvedText {
        family: FAMILY.to_owned(),
        size,
        ..ResolvedText::initial()
    }
}

/// The width of the first run of `segments`, laid out at an unconstrained
/// width.
fn first_run_width(node: &ResolvedText, segments: &[TextSegment]) -> f32 {
    let mut measurer = TextMeasurer::new();
    let lines = wrap(
        &mut measurer,
        node,
        segments,
        f32::INFINITY,
        Metrics::of(node),
    );
    let line = lines.first().unwrap_or_else(|| unreachable!("no line"));
    let run = line.runs.first().unwrap_or_else(|| unreachable!("no run"));
    run.width
}

/// One segment carrying `overlay`.
fn one(overlay: TextStyle) -> Vec<TextSegment> {
    vec![TextSegment {
        text: "AA".to_owned(),
        style: overlay,
    }]
}

#[test]
fn a_segment_spacing_widens_that_run() {
    let node = base(16.0);
    let plain = first_run_width(&node, &one(TextStyle::default()));
    let spaced = first_run_width(
        &node,
        &one(TextStyle {
            letter_spacing: Some(Spacing::Points(4.0)),
            ..TextStyle::default()
        }),
    );

    assert!(
        spaced > plain,
        "a segment's letter spacing did not reach the measurer: {plain} then \
         {spaced}"
    );
}

#[test]
fn an_em_spacing_resolves_against_the_segments_own_size() {
    // The scenes differ only in the paragraph's size and the segment fixes its
    // own in both: spacing resolved against the run gives equal runs,
    // against the paragraph different ones. No number is written down to
    // match.
    let overlay = TextStyle {
        font_size: Some(20.0),
        letter_spacing: Some(Spacing::Em(1.0)),
        ..TextStyle::default()
    };

    let under_small_paragraph =
        first_run_width(&base(10.0), &one(overlay.clone()));
    let under_large_paragraph = first_run_width(&base(40.0), &one(overlay));

    assert!(
        (under_small_paragraph - under_large_paragraph).abs() < 0.01,
        "an em spacing resolved against the paragraph rather than the \
         segment: {under_small_paragraph} under a 10px paragraph against \
         {under_large_paragraph} under a 40px one, where the segment fixes \
         its own size at 20 in both"
    );

    // The control shows the spacing does something, since the equality above
    // would also hold with zero spacing everywhere. Differing paragraphs
    // would not do: glyph sizes alone make them differ.
    let no_spacing = first_run_width(
        &base(10.0),
        &one(TextStyle {
            font_size: Some(20.0),
            ..TextStyle::default()
        }),
    );
    assert!(
        under_small_paragraph - no_spacing > 0.01,
        "the control failed: the em spacing added nothing, so the equality \
         above holds because both runs have no spacing rather than because \
         both resolved against the segment"
    );
}

#[test]
fn a_spacing_on_one_segment_leaves_its_neighbour_alone() {
    let node = base(16.0);
    let plain_first = first_run_width(&node, &one(TextStyle::default()));

    let two = vec![
        TextSegment {
            text: "AA".to_owned(),
            style: TextStyle::default(),
        },
        TextSegment {
            text: "AA".to_owned(),
            style: TextStyle {
                letter_spacing: Some(Spacing::Points(8.0)),
                ..TextStyle::default()
            },
        },
    ];
    let mut measurer = TextMeasurer::new();
    let lines = wrap(
        &mut measurer,
        &node,
        &two,
        f32::INFINITY,
        Metrics::of(&node),
    );
    let runs = &lines
        .first()
        .unwrap_or_else(|| unreachable!("no line"))
        .runs;

    assert!(
        (runs[0].width - plain_first).abs() < 0.01,
        "styling the second segment changed the first: {} against {}",
        runs[0].width,
        plain_first
    );
    assert!(
        runs[1].width > runs[0].width,
        "the styled segment is not wider than its unstyled neighbour"
    );
}
