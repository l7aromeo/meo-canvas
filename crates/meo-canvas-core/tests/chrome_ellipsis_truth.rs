//! What a line keeps when it does not fit, against Chrome.
//!
//! # Why the string and not the width
//!
//! A word-boundary rule and a character rule disagree about **content**, not
//! about width: `Flower of…` and `Flower of Par…` both fit a 90-pixel box, and
//! only one of them is what a browser draws. So these rows record the string,
//! and the test reconstructs ours from the runs rather than measuring ink.
//!
//! Chrome does not expose the string it drew for `text-overflow: ellipsis`,
//! which is why the harness measures with `measureText` instead and computes
//! the longest prefix that fits with the marker appended. That is the same
//! question asked in a form the browser will answer.
//!
//! Measured through `just conformance`;
//! `crates/meo-canvas/tests/assets/chrome/ellipsis.tsv`.

use meo_canvas_core::{
    lines::{Line, Metrics, RunStyle, TextMeasurer, layout, line_width},
    resolve::{Fonts, ResolvedText},
};
use meo_canvas_scene::style::text::{ParagraphStyle, TextSegment, TextStyle};

/// The face the fixtures register and the one Chrome was asked about.
const FONT: (&str, &str) =
    ("Fixture", "tests/assets/fonts/Oswald-VariableFont_wght.ttf");

/// The marker, U+2026, which is what CSS uses and what the harness measured.
const MARKER: &str = "\u{2026}";

/// One row: the text, its size, the width it is given, and what Chrome draws.
struct Row {
    /// What the caller wrote.
    text: &'static str,
    /// The em size.
    size: f32,
    /// The width available to the line.
    width: f32,
    /// The string Chrome keeps, marker included.
    drawn: &'static str,
}

/// Chrome's answers, verbatim from the harness.
///
/// The fourth and fifth rows are the pair that matters most. **A word with no
/// space in it is cut mid-word** -- wrapping cannot break it and there is no
/// line after it, so a truncation triggered by the line count never runs. And
/// **a space before the marker survives when it fits**, which v1 strips: at
/// 22px in 90 the browser draws `Flower of …`, space and all.
const CHROME: [Row; 6] = [
    Row {
        text: "Flower of Paradise",
        size: 16.0,
        width: 60.0,
        drawn: "Flower o\u{2026}",
    },
    Row {
        text: "Flower of Paradise",
        size: 16.0,
        width: 90.0,
        drawn: "Flower of Par\u{2026}",
    },
    // Wide enough for all of it: the marker must not appear at all.
    Row {
        text: "Flower of Paradise",
        size: 16.0,
        width: 120.0,
        drawn: "Flower of Paradise",
    },
    Row {
        text: "Flower of Paradise",
        size: 16.0,
        width: 150.0,
        drawn: "Flower of Paradise",
    },
    Row {
        text: "Antidisestablishmentarianism",
        size: 16.0,
        width: 90.0,
        drawn: "Antidisestabli\u{2026}",
    },
    Row {
        text: "Flower of Paradise",
        size: 22.0,
        width: 90.0,
        drawn: "Flower of \u{2026}",
    },
];

/// The text of a line, with a space wherever a space run sits.
///
/// The runs carry a space as a run of no width -- the gap it stands for is
/// arithmetic -- so joining their texts alone would run two words together.
fn read(line: &Line) -> String {
    line.runs
        .iter()
        .map(|run| {
            if run.is_space() {
                " "
            } else {
                run.text.as_str()
            }
        })
        .collect()
}

#[test]
fn a_clamped_line_keeps_what_chrome_keeps() {
    let fonts = Fonts::new();
    fonts.register_path(FONT.0, FONT.1).unwrap_or_else(|error| {
        unreachable!("the face did not register: {error}")
    });
    let mut measurer = TextMeasurer::new();

    for row in CHROME {
        let base = ResolvedText {
            family: FONT.0.to_owned(),
            size: row.size,
            ..ResolvedText::initial()
        };
        let segments = vec![TextSegment {
            text: row.text.to_owned(),
            style: TextStyle::default(),
        }];
        // One line and a marker, which is what `text-overflow: ellipsis` on a
        // non-wrapping line asks for.
        let paragraph = ParagraphStyle {
            max_lines: Some(1),
            ellipsis: Some(MARKER.to_owned()),
        };
        let metrics = Metrics::of(&base);

        let block = layout(
            &mut measurer,
            &base,
            &segments,
            row.width,
            &paragraph,
            metrics,
        );
        assert_eq!(
            block.lines.len(),
            1,
            "{:?} kept more than one line",
            row.text
        );
        let drawn = read(&block.lines[0]);
        assert_eq!(
            drawn, row.drawn,
            "{:?} at {}px in {}: we draw {drawn:?} where Chrome draws {:?}",
            row.text, row.size, row.width, row.drawn
        );

        // And it has to fit, which is the half a string comparison cannot see:
        // a rule that kept the right characters and measured them wrongly
        // would pass the assertion above.
        let space = measurer
            .space_width(&RunStyle::base(&base), metrics.letter_spacing);
        let width = line_width(&block.lines[0], space, metrics.word_spacing);
        assert!(
            width <= row.width + 0.5,
            "{:?} at {}px is {width} wide in a box of {}",
            row.text,
            row.size,
            row.width
        );
    }
}
