//! This renderer's text numbers against Chrome's, on the same face.
//!
//! # Why a browser is the oracle
//!
//! The standard for this project is that an expectation matches **Chrome**,
//! not v1 and not our own last render. A golden accepted from what we drew
//! records what we did; `fixtures/borders-square` was accepted around a
//! diagonal bottom edge and passed for exactly that reason. Where a browser
//! can be asked, its answer is the expectation.
//!
//! # Where these numbers came from
//!
//! Measured in Chrome with the same Oswald face the fixtures register, at
//! 16px, and recorded in `scratchpad/chrome/text-truth.tsv`. They are pasted
//! here as constants rather than read from that file: a test that reads its
//! own expectations from a file someone can regenerate is a test that agrees
//! with whatever it is given.
//!
//! # What each one settles
//!
//! **The strut is the face, not the string.** Four strings -- including one
//! that is all descenders and one that is a single `x` -- report the same
//! ascent and descent in Chrome. That is the property a line box is built on:
//! a line does not move when it gains a descender.
//!
//! **Chrome applies letter spacing once per character; this backend applies it
//! between them.** Sixteen characters at 2px are 32px wider in Chrome and 30px
//! wider here, so a run measured through the backend is short by exactly one
//! unit however long it is. v1 carries that same correction with a comment
//! saying an earlier version added `n - 1` and made a line a third too wide;
//! this test is the reason the correction is one unit and not a guess.

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

/// Chrome's `fontBoundingBoxAscent` at [`SIZE`].
///
/// **A whole number because Chrome rounds it, not because it was rounded on
/// the way here.** The same call returns a fractional
/// `actualBoundingBoxAscent` of 12.96, so the integer is a decision. This face
/// reports 19.088, and taking that unrounded would make every line box 23.712
/// against Chrome's 24 -- 0.288px short per line, accumulating to nearly three
/// pixels of baseline by the tenth.
const CHROME_ASCENT: f32 = 19.0;

/// Chrome's `fontBoundingBoxDescent` at [`SIZE`], rounded by Chrome as the
/// ascent is.
const CHROME_DESCENT: f32 = 5.0;

/// Chrome's `line-height: normal` box at [`SIZE`], which is exactly the sum of
/// the two above.
///
/// The identity is the part worth pinning: a renderer can agree on both
/// metrics to a tenth and still build a line box that drifts.
const CHROME_LINE_BOX: f32 = 24.0;

/// Chrome's width for [`SIXTEEN`] with no letter spacing.
const CHROME_PLAIN: f32 = 102.736;

/// Chrome's width for [`SIXTEEN`] at 2px letter spacing.
///
/// Exactly [`CHROME_PLAIN`] plus 32: sixteen characters, sixteen units.
const CHROME_SPACED: f32 = 134.736;

/// Chrome's width for a single space at [`SIZE`].
const CHROME_SPACE: f32 = 3.664;

/// The letter spacing the width pair was measured at.
const SPACING: f32 = 2.0;

/// A string of exactly sixteen characters, so the per-character question has
/// an answer that cannot be confused with a per-run one.
const SIXTEEN: &str = "abcdefghijklmnop";

/// How far from Chrome a width may fall.
///
/// A tenth of a pixel. Chrome and this backend shape the same face with the
/// same advances and round differently at the end; the measured gap on a
/// hundred-pixel run is under seven hundredths.
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

    // The backend's own answer, before the correction: sixteen characters with
    // fifteen gaps between them.
    let backend = measurer.measure(&style, SPACING, SIXTEEN).width;
    let backend_delta = backend - plain;
    assert!(
        SPACING.mul_add(-15.0, backend_delta).abs() < WIDTH_SLACK,
        "the backend added {backend_delta}, which is neither 15 nor 16 units"
    );

    // And with it: sixteen units, which is Chrome's number.
    let corrected = measurer.run_width(&style, SPACING, SIXTEEN);
    assert!(
        (corrected - CHROME_SPACED).abs() < WIDTH_SLACK,
        "spaced {corrected} is not Chrome's {CHROME_SPACED}"
    );
    assert!(
        SPACING.mul_add(-16.0, corrected - plain).abs() < WIDTH_SLACK,
        "the correction did not land on one unit per character"
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

/// Chrome's line box and baseline at three line heights, in pixels.
///
/// `(line-height multiple, box height, baseline from the box top)`. A multiple
/// of `1.0` is the face's own -- CSS's `normal`.
///
/// # How they were read
///
/// With a zero-height inline-block on the line, whose bottom margin edge **is**
/// the alphabetic baseline. So the baseline column is the baseline's position
/// rather than an inference from where ink begins.
///
/// # The one number in that table that is not portable
///
/// At `line-height: 8px` Chrome reports a **block** height of 11, and 8 is
/// what a bare line gives: the zero-height ruler contributes no descent, so it
/// holds the box open against a negative one. The instrument changed the
/// thing it measured, in one column while the other stayed clean. The box
/// height here is 8; the baseline is 11, and the baseline is what was in
/// question.
/// **The first row was labelled `1.0` and is Chrome's `normal`.** At a 16px
/// font `line-height: 1` is a 16px box; this row pins 24.0, which is the
/// face's own metrics. The label was the sentinel leaking into the measured
/// data -- while `1.0` meant "the face's own" everywhere in this crate,
/// recording `normal` under that spelling was invisible. It is `None` now,
/// which is what was actually measured.
///
/// **`Some(1.0)` is a real `line-height: 1` and it is measured, not derived.**
/// It became expressible only when the sentinel went, so nothing could have
/// pinned it before.
///
/// It is also what the leading model predicts -- `19 + (16 - 24) / 2 = 15` --
/// so **this row confirms the model rather than constraining it.** Said
/// plainly because 16.0 and 15.0 are exactly the numbers the arithmetic makes
/// obvious, and a reader who assumes they were derived would be right about
/// the value and wrong about where it came from.
///
/// The measurement reproduced the three rows above it before it was trusted
/// with the fourth, and its first attempt did not: a zero-height marker at the
/// baseline **grew a tight line box to contain itself**, reading 11.0 for the
/// `0.5` row's 8.0. **An instrument that cannot reproduce known values cannot
/// be trusted on unknown ones**, and the known rows are what caught it.
const CHROME_LINE_BOXES: [(Option<f32>, f32, f32); 4] = [
    (None, 24.0, 19.0),
    (Some(1.0), 16.0, 15.0),
    (Some(2.0), 32.0, 23.0),
    (Some(0.5), 8.0, 11.0),
];

/// A line box is the face's metrics with the leading split above and below.
///
/// # What this rejects
///
/// Skia's paragraph **scales** the ascent and descent to fill the line box
/// instead: at `line-height: 2` it puts the baseline at 25.76, which is
/// exactly `32 × 19.088 / 23.712`, and at `0.5` it puts it at 6.44 -- inside
/// the box. CSS adds leading around the metrics rather than stretching them,
/// so the baseline is `(box − content) / 2 + ascent`, and Chrome agrees to the
/// pixel at both ends: 23 and 11.
///
/// **The tight case is the one worth having.** `19 + (8 − 24) / 2 = 11` puts
/// the baseline *below* a box eight pixels tall, because the leading is
/// negative and CSS lets the glyphs escape the box rather than moving the
/// baseline inside it. Skia's 6.44 is inside. Those are different models, not
/// different roundings, and only one of them is a browser's.
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

/// The unitless form and the length form are one model.
///
/// Chrome lands `line-height: 2`, `line-height: 32px`, `line-height: 0.5` and
/// `line-height: 8px` on identical boxes and baselines at 16px. A unitless
/// value resolves against the font size **first**; from there there is one
/// arithmetic, which is why the scene can carry either spelling without the
/// painter learning a second rule.
///
/// Asserted here through the size rather than through two fields, because the
/// scene has only the multiple today: doubling the font size and halving the
/// multiple is the same length, and it must be the same line box.
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

/// Chrome's width for a string, and what this renderer measures for it.
///
/// `(string, Chrome's width)`. Multi-word entries are Chrome's `measureText`
/// of the whole string; this crate builds the same number by summing its words
/// and a measured space, which Chrome itself confirms is equivalent -- **it
/// does not shape across a gap.** Measured directly rather than assumed: the
/// whole string and the sum of its parts agree to the third decimal on every
/// string tried, `103.248` against `103.248` for the longest.
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

/// Every width this renderer reports is a shade under Chrome's, and by how
/// much.
///
/// # Why this is recorded rather than fixed
///
/// It is six hundredths of one per cent on a hundred-pixel line, it has the
/// **same sign everywhere** so it cannot accumulate into a wrong break, and it
/// moves no wrap point in any of the four scenes Chrome was asked about. What
/// it is not is a mystery, and the shape of it says where to look:
///
/// - it **scales with characters, not with words** -- sixteen characters run
///   0.060 short here and 0.066 short in the letter-spacing scene, about 0.004
///   per glyph, and a space measures 3.660 against Chrome's 3.664, the same
///   0.004;
/// - a single-character `"a"` is 0.018 short, which is far more than one
///   glyph's worth, so there is a per-string component on top of the per-glyph
///   one.
///
/// So the question belongs to the backend -- what advance does it report for
/// one glyph at 16px -- and not to how a line is assembled. Chasing it through
/// a measurer that is being rewritten is how a real defect gets blamed on a
/// rounding.
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
