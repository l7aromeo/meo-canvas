//! Text numbers against Chrome's HTML layout on one face at 16px. Each is a
//! constant equal to its row of `text-truth.tsv`, so a re-measure that moves
//! one fails until it is edited. They pin that the strut is the face, not the
//! string, and that letter spacing is one unit per character.

use meo_canvas_core::{
    lines::{METRICS_STRING, Metrics, RunStyle, TextMeasurer, layout},
    resolve::{Fonts, ResolvedText},
};
use meo_canvas_scene::style::text::{
    LineHeight, ParagraphStyle, TextSegment, TextStyle,
};

/// The face the fixtures register, and the one Chrome was asked about.
const FONT: (&str, &str) =
    ("Fixture", "tests/assets/fonts/Oswald-VariableFont_wght.ttf");

/// The em size every number here was measured at.
const SIZE: f32 = 16.0;

/// Chrome's baseline below the top of a `line-height: normal` box at [`SIZE`],
/// whole. This face reports 19.088, and using that would make each line box
/// 23.712 against Chrome's 24, drifting nearly 3px of baseline by the tenth
/// line.
const CHROME_ASCENT: f32 = 19.0;

/// The rest of that box below the baseline, whole as the ascent is.
const CHROME_DESCENT: f32 = 5.0;

/// Chrome's `line-height: normal` box at [`SIZE`], exactly the sum of the two
/// above. The identity is what matters: two metrics can each agree to a tenth
/// and still build a line box that drifts.
const CHROME_LINE_BOX: f32 = 24.0;

/// Chrome's width for [`SIXTEEN`] with no letter spacing.
const CHROME_PLAIN: f32 = 102.750;

/// Chrome's width for [`SIXTEEN`] at 2px letter spacing.
///
/// Exactly [`CHROME_PLAIN`] plus 32: sixteen characters, sixteen units.
const CHROME_SPACED: f32 = 134.750;

/// Chrome's width for a single space at [`SIZE`].
const CHROME_SPACE: f32 = 3.672;

/// The letter spacing the width pair was measured at.
const SPACING: f32 = 2.0;

/// A string of exactly sixteen characters, so the per-character question has
/// an answer that cannot be confused with a per-run one.
const SIXTEEN: &str = "abcdefghijklmnop";

/// How far from Chrome a width may fall: a tenth of a pixel. The sixteen-letter
/// run measures 0.08 under Chrome's, spaced or not.
const WIDTH_SLACK: f32 = 0.1;

/// A resolved style in the fixture face, with the fonts registered.
fn fixture_style() -> (Fonts, ResolvedText) {
    let fonts = Fonts::new();
    fonts.register_path(FONT.0, FONT.1).unwrap_or_else(|error| {
        unreachable!("the face did not register: {error}")
    });
    let base = ResolvedText {
        family: FONT.0.to_owned(),
        size: SIZE,
        ..ResolvedText::initial()
    };
    (fonts, base)
}

#[test]
fn the_strut_is_the_face_and_not_the_string() {
    let (_fonts, base) = fixture_style();
    let style = RunStyle::base(&base);
    let mut measurer = TextMeasurer::new();

    let strut = measurer.measure(&style, 0.0, METRICS_STRING);
    assert!(
        (strut.ascent - CHROME_ASCENT).abs() < 0.01,
        "ascent {} is not Chrome's {CHROME_ASCENT}",
        strut.ascent
    );
    assert!(
        (strut.descent - CHROME_DESCENT).abs() < 0.01,
        "descent {} is not Chrome's {CHROME_DESCENT}",
        strut.descent
    );
    assert!(
        (strut.ascent + strut.descent - CHROME_LINE_BOX).abs() < 0.01,
        "a normal line box of {} is not Chrome's {CHROME_LINE_BOX}",
        strut.ascent + strut.descent
    );

    // The four strings Chrome was asked about, including one that is all
    // descenders. Chrome gives one answer for all four; so must this.
    for text in ["x", "ABCDEFG", "gjpqy"] {
        let other = measurer.measure(&style, 0.0, text);
        assert!(
            (other.ascent - strut.ascent).abs() < f32::EPSILON
                && (other.descent - strut.descent).abs() < f32::EPSILON,
            "{text:?} reports {other:?} where the strut reports {strut:?}"
        );
    }
}

#[test]
fn a_run_carries_one_letter_spacing_per_character_as_chrome_does() {
    let (_fonts, base) = fixture_style();
    let style = RunStyle::base(&base);
    let mut measurer = TextMeasurer::new();

    let plain = measurer.run_width(&style, 0.0, SIXTEEN);
    assert!(
        (plain - CHROME_PLAIN).abs() < WIDTH_SLACK,
        "unspaced {plain} is not Chrome's {CHROME_PLAIN}"
    );

    // The backend adds one spacing per character, as the Canvas standard does,
    // so sixteen characters gain sixteen units and nothing here corrects
    // it.
    let backend = measurer.measure(&style, SPACING, SIXTEEN).width;
    let backend_delta = backend - plain;
    assert!(
        SPACING.mul_add(-16.0, backend_delta).abs() < WIDTH_SLACK,
        "the backend added {backend_delta}, which is not 16 units"
    );

    // Sixteen units, which is Chrome's number.
    let width = measurer.run_width(&style, SPACING, SIXTEEN);
    assert!(
        (width - CHROME_SPACED).abs() < WIDTH_SLACK,
        "spaced {width} is not Chrome's {CHROME_SPACED}"
    );
    // **And `run_width` adds nothing of its own.** Asserting the total alone
    // passes for a backend at fifteen units with a correction of one, which is
    // what this file pinned before the bump; the equality is what says the
    // compensation is gone rather than merely balanced.
    assert!(
        (width - backend).abs() < f32::EPSILON,
        "run_width returned {width} where the backend measured {backend}"
    );
}

#[test]
fn a_space_is_as_wide_as_chrome_makes_it() {
    let (_fonts, base) = fixture_style();
    let style = RunStyle::base(&base);
    let mut measurer = TextMeasurer::new();

    let space = measurer.space_width(&style, 0.0);
    assert!(
        (space - CHROME_SPACE).abs() < WIDTH_SLACK,
        "a space is {space} where Chrome makes it {CHROME_SPACE}"
    );
    // With spacing it gains its own unit like any other run: a single
    // character has nothing for the backend to space it between, so the whole
    // unit is the correction's.
    let spaced = measurer.space_width(&style, SPACING);
    assert!(
        (spaced - space - SPACING).abs() < WIDTH_SLACK,
        "a spaced space is {spaced} where {} was expected",
        space + SPACING
    );
}

/// Chrome's `(line-height, box, baseline from box top)` at 16px; `None` is
/// `normal`. Read with a zero-height inline-block whose bottom edge is the
/// baseline. At `0.5` that ruler holds the block open to 11, so the box is the
/// bare line's 8; `Some(1.0)` is measured and agrees with the leading model.
const CHROME_LINE_BOXES: [(Option<f32>, f32, f32); 4] = [
    (None, 24.0, 19.0),
    (Some(1.0), 16.0, 15.0),
    (Some(2.0), 32.0, 23.0),
    (Some(0.5), 8.0, 11.0),
];

/// A line box is the face's metrics with the leading split above and below: the
/// baseline is `(box - content) / 2 + ascent`, Chrome's 23 at `2` and 11 at
/// `0.5` -- below an 8px box. Skia's paragraph scales the metrics instead, to
/// 25.76 and 6.44: a different model, not a different rounding.
#[test]
fn a_line_box_places_its_baseline_where_chrome_does() {
    let (_fonts, mut base) = fixture_style();
    let mut measurer = TextMeasurer::new();
    let segments = vec![TextSegment {
        text: "Hxgp".to_owned(),
        style: TextStyle::default(),
    }];

    for (multiple, box_height, baseline) in CHROME_LINE_BOXES {
        base.line_height = multiple.map(LineHeight::Number);
        let block = layout(
            &mut measurer,
            &base,
            &segments,
            1000.0,
            &ParagraphStyle::default(),
            Metrics::of(&base),
        );
        let line = &block.lines[0];
        assert!(
            (line.height - box_height).abs() < 0.01,
            "line-height {multiple:?}: a box of {} is not Chrome's \
             {box_height}",
            line.height
        );
        assert!(
            (line.baseline_from_top() - baseline).abs() < 0.01,
            "line-height {multiple:?}: a baseline at {} is not Chrome's \
             {baseline}",
            line.baseline_from_top()
        );
    }
}

/// A unitless line height and a length are one model: Chrome gives identical
/// boxes for `2` and `32px` at 16px. The scene carries only the multiple, so
/// this doubles the size and halves the multiple, which must match.
#[test]
fn a_multiple_and_a_length_are_the_same_line_box() {
    let (_fonts, mut base) = fixture_style();
    let mut measurer = TextMeasurer::new();
    let segments = vec![TextSegment {
        text: "Hxgp".to_owned(),
        style: TextStyle::default(),
    }];

    base.line_height = Some(LineHeight::Number(2.0));
    let doubled = layout(
        &mut measurer,
        &base,
        &segments,
        1000.0,
        &ParagraphStyle::default(),
        Metrics::of(&base),
    );
    // 32 pixels asked for as a multiple of a 16px font, and again as the same
    // multiple of the same font read the other way round: 4.0 x 8px would be a
    // different face size and so a different content height, which is why this
    // holds the size and varies nothing else.
    assert!((doubled.lines[0].height - 32.0).abs() < 0.01);

    // `None`, not `Some(1.0)`. This asked for the face's own metrics and
    // spelled it with the sentinel; `Some(1.0)` now means a box of exactly one
    // em, which is a different request and a different number.
    base.line_height = None;
    let natural = layout(
        &mut measurer,
        &base,
        &segments,
        1000.0,
        &ParagraphStyle::default(),
        Metrics::of(&base),
    );
    assert!((natural.lines[0].height - CHROME_LINE_BOXES[0].1).abs() < 0.01);
}

/// `(string, Chrome's width)`, a `white-space: pre` span's. This crate sums
/// words and a measured space, which Chrome gives as equal to within the
/// [`SNAP`] each span's width falls on; `text-truth.tsv` carries both.
const CHROME_WIDTHS: [(&str, f32); 7] = [
    ("a", 6.828),
    ("Hxgp quick", 64.203),
    ("brown fox", 56.609),
    ("jumps over", 63.219),
    ("the lazy dog", 69.250),
    ("Hxgpquickbrown", 95.922),
    ("Hxgp quick brown", 103.250),
];

/// The most any of those may fall short before this stops being a rounding.
///
/// A tenth of a pixel. The measured deficit is 0.018 to 0.060, and the point
/// of the bound is that it cannot grow quietly.
const DEFICIT_CEILING: f32 = 0.1;

/// Every width here is a shade under Chrome's: about 0.004 per glyph (a space
/// is 3.660 against 3.672) plus a per-string part (`"a"` is 0.018 short).
/// Recorded, not fixed: it has one sign, moves no wrap point, and belongs to
/// the backend's glyph advance rather than to line assembly.
#[test]
fn our_widths_run_a_known_shade_under_chromes() {
    let (_fonts, base) = fixture_style();
    let style = RunStyle::base(&base);
    let mut measurer = TextMeasurer::new();
    let space = measurer.space_width(&style, 0.0);

    for (text, chrome) in CHROME_WIDTHS {
        let words: Vec<&str> = text.split(' ').collect();
        let gaps = words.len() - 1;
        let ours = words
            .iter()
            .map(|word| measurer.run_width(&style, 0.0, word))
            .sum::<f32>()
            + space * gaps as f32;
        let deficit = chrome - ours;
        assert!(
            deficit >= 0.0,
            "{text:?} measures {ours}, over Chrome's {chrome} -- the deficit \
             has changed sign, which is a different defect from the one this \
             records"
        );
        assert!(
            deficit < DEFICIT_CEILING,
            "{text:?} measures {ours} against Chrome's {chrome}, short by \
             {deficit} where {DEFICIT_CEILING} is the recorded ceiling"
        );
    }
}

/// Chrome's measurements, which `chrome_tables_are_read` requires be read.
const TABLE: &str =
    include_str!("../../meo-canvas/tests/assets/chrome/text-truth.tsv");

/// The grid Chrome lays a span's width out on, a 64th of a pixel: why a run
/// summed from its words can fall a few of them from the whole.
const SNAP: f32 = 1.0 / 64.0;

/// How close a constant is to its row: the table's three decimals.
const PRINTED: f32 = 0.000_5;

/// Every constant here against its row of `text-truth.tsv`, in both directions:
/// a constant whose row moved fails, and so does a row no constant reads.
#[test]
fn every_constant_is_its_row_of_text_truth_tsv() {
    let rows: Vec<Vec<&str>> = TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| line.split('\t').collect())
        .collect();
    let number = |cell: &str| {
        cell.parse::<f32>()
            .unwrap_or_else(|_| unreachable!("{cell} is not a number"))
    };
    let mut read = vec![false; rows.len()];
    let mut row = |kind: &str, key: &str, argument: &str| {
        let quoted = format!("{key:?}");
        let at = rows
            .iter()
            .position(|cells| {
                cells[0] == kind
                    && (cells[1] == quoted || cells[1] == key)
                    && (cells[2] == argument
                        || cells[2] == format!("{argument:?}"))
            })
            .unwrap_or_else(|| {
                unreachable!(
                    "text-truth.tsv has no {kind} row {key} {argument}"
                )
            });
        read[at] = true;
        (number(rows[at][3]), rows[at][4])
    };
    let close = |ours: f32, chrome: f32| (ours - chrome).abs() < PRINTED;

    let mut wrong = Vec::new();
    for (text, spacing, constant) in [
        (SIXTEEN, "0", CHROME_PLAIN),
        (SIXTEEN, "2", CHROME_SPACED),
        (" ", "0", CHROME_SPACE),
    ] {
        let (chrome, _) = row("width", text, spacing);
        if !close(constant, chrome) {
            wrong.push(format!(
                "{text:?} at {spacing}: {constant}, row {chrome}"
            ));
        }
    }
    for (text, constant) in CHROME_WIDTHS {
        let (chrome, parts) = row("width", text, "0");
        if !close(constant, chrome) {
            wrong.push(format!("{text:?}: {constant}, row {chrome}"));
        }
        if parts != "-" {
            let spans = text.split(' ').count() * 2 - 1;
            #[expect(
                clippy::cast_precision_loss,
                reason = "a span count of a few words"
            )]
            let bound = SNAP * spans as f32;
            if (number(parts) - chrome).abs() > bound {
                wrong.push(format!(
                    "{text:?}: its parts sum to {parts}, beyond {bound} of {chrome}"
                ));
            }
        }
    }
    for text in ["Ag|``", "x", "ABCDEFG", "gjpqy"] {
        let (chrome_box, baseline) = row("line", "normal", text);
        let baseline = number(baseline);
        if !(close(chrome_box, CHROME_LINE_BOX)
            && close(baseline, CHROME_ASCENT)
            && close(chrome_box - baseline, CHROME_DESCENT))
        {
            wrong.push(format!("{text:?}: a {chrome_box} box at {baseline}"));
        }
    }
    for (multiple, box_height, baseline) in CHROME_LINE_BOXES {
        let key =
            multiple.map_or_else(|| "normal".to_owned(), |m| m.to_string());
        let (chrome_box, chrome_baseline) = row("line", &key, "Hxgp");
        if !(close(box_height, chrome_box)
            && close(baseline, number(chrome_baseline)))
        {
            wrong.push(format!(
                "line-height {key}: ({box_height}, {baseline}), row \
                 ({chrome_box}, {chrome_baseline})"
            ));
        }
    }
    let unread = read.iter().filter(|read| !**read).count();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert_eq!(
        unread, 0,
        "{unread} row(s) of text-truth.tsv no constant reads"
    );
}
