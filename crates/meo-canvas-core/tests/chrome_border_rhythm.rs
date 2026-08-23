//! The rhythm a dashed border is drawn with, against Chrome.
//!
//! # What is settled here and what is not
//!
//! **The ratio is.** Chrome runs two regimes — a thin border gets a longer
//! dash relative to its width — and the five measured widths pin both,
//! including the width 3 that decides where the step falls.
//!
//! **The fitting is, on a square box.** Chrome keeps the dash at its nominal
//! length, puts the slack in the gaps, and fits the *nearest* whole number of
//! dashes to each side — not the largest that fits. The two sides below say
//! so between them: a 48-pixel edge whose three gaps are all *wider* than
//! nominal, and a 137-pixel one with a gap *narrower*. Each side is stroked
//! as its own line corner to corner, so the phase restarts at a corner
//! because the path does.
//!
//! **The fitting on a rounded box is not.** A full dash begins exactly where
//! the arc ends, so a rounded side is fitted on its straight run rather than
//! carried round the corner — but the length Chrome fits it to is open. A
//! 240-wide box with a 12px radius at width 4 holds eighteen dashes and
//! seventeen gaps summing to 213, where `width - 2 * radius` is 216. Three
//! pixels is either the wrong length or a reading nibbled at the ends of
//! every run, and only the first and last ink offsets can say which. Until
//! then a rounded box keeps the whole-path stroke, unfitted.
//!
//! # The symmetry, which turned out not to need a mechanism
//!
//! Chrome distributes the remainder symmetrically — `5, 6, 5` and not
//! `6, 5, 5` — and this was written down as something a single dash array
//! could not express, on the reasoning that every gap in a pattern is one
//! fractional length. **That reasoning was wrong, and the renderer settles
//! it**: one gap of `16 / 3` puts its boundaries at 8, 13.33, 21.33, 26.67,
//! 34.67 and 40, and a rasteriser rounding each where it falls draws
//! `8, 5, 8, 6, 8, 5, 8` — Chrome's runs, symmetry and all. The symmetry is
//! the fractional gap seen through pixels, not a rule about remainders.
//!
//! `crates/meo-canvas/tests/assets/chrome/border-rhythm.tsv`, through
//! `just conformance`.

use meo_canvas_core::{
    ImageFormat, Renderer,
    encode::EncodeOptions,
    paint::{dash_pattern, fitted_dash},
};
use meo_canvas_scene::{
    Scene, Sides, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension,
        paint::{BorderStyle, Color},
    },
};

/// One measured width: the border, and the ink and gap Chrome repeats.
///
/// Read along the top band of a 240x48 box, between x=40 and x=200, counting
/// a pixel as ink below 128 in the red channel. The runs at the two ends of
/// that span are cut by the bounds and are not whole periods; these are the
/// values every whole period in between holds.
struct Rhythm {
    /// The border's width in pixels.
    width: f32,
    /// The ink run Chrome repeats.
    ink: f32,
    /// The gap Chrome repeats. Where Chrome's own gaps vary by a pixel — the
    /// remainder it spreads — this is the one it holds most of the way along.
    gap: f32,
}

/// Chrome's rhythm at the five widths the harness measured.
///
/// The jump is the whole point of the table: the ratio is `3w` on and `2w`
/// off while the border is thin and `2w` on and `1w` off once it is not, so a
/// single ratio cannot be right at both ends.
///
/// **Width 3 decides where the step falls, and it is measured**: `on:6 off:3`
/// puts it in the *upper* regime, so the boundary is `w < 3` rather than
/// `w <= 3`. It was a guess for an hour, and this row is what retired it —
/// the two readings differ by exactly one width, which is the one a reader
/// would otherwise have to take on trust.
const CHROME: [Rhythm; 5] = [
    Rhythm {
        width: 1.0,
        ink: 3.0,
        gap: 2.0,
    },
    Rhythm {
        width: 2.0,
        ink: 6.0,
        gap: 4.0,
    },
    Rhythm {
        width: 3.0,
        ink: 6.0,
        gap: 3.0,
    },
    Rhythm {
        width: 4.0,
        ink: 8.0,
        gap: 4.0,
    },
    Rhythm {
        width: 8.0,
        ink: 16.0,
        gap: 8.0,
    },
];

#[test]
fn a_dash_is_the_length_chrome_makes_it() {
    for row in CHROME {
        let (ink, gap) = dash_pattern(row.width);
        assert!(
            (ink - row.ink).abs() < f32::EPSILON,
            "a {}px border dashes {ink} on where Chrome draws {}",
            row.width,
            row.ink
        );
        assert!(
            (gap - row.gap).abs() < f32::EPSILON,
            "a {}px border leaves {gap} off where Chrome leaves {}",
            row.width,
            row.gap
        );
    }
}

#[test]
fn the_two_regimes_are_a_step_and_not_a_slope() {
    // The property that separates Chrome's rule from any single ratio: the
    // period per unit of width *falls* as the border thickens, from five
    // widths to three. A renderer with one ratio has a constant here, and
    // matching Chrome at one width would put it wrong at the other.
    let period = |width: f32| {
        let (ink, gap) = dash_pattern(width);
        (ink + gap) / width
    };
    assert!((period(1.0) - 5.0).abs() < f32::EPSILON);
    assert!((period(2.0) - 5.0).abs() < f32::EPSILON);
    // The step, measured rather than assumed: 3 is on the far side of it.
    assert!((period(3.0) - 3.0).abs() < f32::EPSILON);
    assert!((period(4.0) - 3.0).abs() < f32::EPSILON);
    assert!((period(8.0) - 3.0).abs() < f32::EPSILON);
}

/// One measured side: how long it is, the border's width, and the runs Chrome
/// draws along the whole of it.
struct Fit {
    /// The side's length in pixels.
    length: f32,
    /// The border's width.
    width: f32,
    /// Every run along the side, in order, ink first.
    runs: &'static [f32],
}

/// The two sides Chrome was read along end to end.
///
/// The first is the discriminating one: **a 48-pixel edge at width 4 begins
/// and ends flush with a whole dash** and its three gaps are `5, 6, 5` -- all
/// *wider* than the nominal 4. The second, a 137-pixel edge, has a gap
/// *narrower* than nominal. Together they say the nominal gap is a target
/// Chrome moves in either direction rather than a floor, which is the rule a
/// naive fit gets wrong.
const SIDES: [Fit; 2] = [
    Fit {
        length: 48.0,
        width: 4.0,
        runs: &[8.0, 5.0, 8.0, 6.0, 8.0, 5.0, 8.0],
    },
    // Read through a sixty-pixel window, so this is the start of the side and
    // not the whole of it: the counts below are derived from the length, and
    // only the leading runs are Chrome's own.
    Fit {
        length: 137.0,
        width: 4.0,
        runs: &[8.0, 4.0, 8.0, 3.0, 8.0, 4.0],
    },
];

#[test]
fn a_side_is_fitted_the_way_chrome_fits_it() {
    for side in SIDES {
        let (dash, gap) = fitted_dash(side.length, side.width);

        // The dash keeps its nominal length: the slack goes in the gaps.
        let (nominal, _) = dash_pattern(side.width);
        assert!(
            (dash - nominal).abs() < f32::EPSILON,
            "a {}px side dashes {dash} where the nominal is {nominal}",
            side.length
        );

        // The count Chrome drew, and the count this produces, are the same --
        // and the runs sum to the side, which is what "both ends flush" means
        // arithmetically.
        let count = side.runs.iter().step_by(2).count() as f32;
        let fitted = (side.length - count * dash) / (count - 1.0);
        assert!(
            (gap - fitted).abs() < 0.01 || side.runs.len() < 7,
            "a {}px side gaps {gap} where fitting {count} dashes wants \
             {fitted}",
            side.length
        );

        // Every gap Chrome drew is within a pixel of ours, in both
        // directions -- which is the property a fit that only padded would
        // fail on the first side and a fit that only shrank would fail on the
        // second.
        for chrome in side.runs.iter().skip(1).step_by(2) {
            assert!(
                (chrome - gap).abs() <= 1.0,
                "a {}px side: Chrome leaves {chrome} where we leave {gap}",
                side.length
            );
        }
    }
}

#[test]
fn a_whole_side_sums_to_the_side() {
    // The 48-pixel edge was read end to end, so its runs must add up. That is
    // the one row here that proves "both ends flush" rather than assuming it.
    let side = &SIDES[0];
    let total: f32 = side.runs.iter().sum();
    assert!(
        (total - side.length).abs() < f32::EPSILON,
        "the runs sum to {total} on a side of {}",
        side.length
    );
    // Odd number of runs: ink, gap, ink ... ink. A side that began or ended
    // with a gap would have an even count.
    assert_eq!(side.runs.len() % 2, 1);
}

/// Renders one dashed box on a white page and returns its pixels.
fn dashed(size: (f32, f32), width: f32) -> (usize, Vec<u8>) {
    let mut scene = Scene::new(Size::new(size.0 + 20.0, size.1 + 20.0));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = Color::rgb(255, 255, 255);
    }
    let id = scene
        .push(NodeId::ROOT, Node::new(NodeKind::Box))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size =
            (Dimension::Points(size.0), Dimension::Points(size.1));
        node.layout.border = Sides::all(width);
        node.paint.background_color = Color::rgb(255, 255, 255);
        node.paint.border_color_all = Color::rgb(0, 0, 0);
        node.paint.border_style = BorderStyle::Dashed;
    }

    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let png = renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("it did not render: {error}"));
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8()
            | png::Transformations::ALPHA,
    );
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut pixels = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut pixels)
        .unwrap_or_else(|error| unreachable!("{error}"));
    pixels.truncate(info.buffer_size());
    (info.width as usize, pixels)
}

/// The runs of ink and gap down one column, ink first.
fn runs_down(stride: usize, pixels: &[u8], x: usize, rows: usize) -> Vec<f32> {
    let mut runs: Vec<f32> = Vec::new();
    let mut inked = true;
    for y in 0..rows {
        let at = (y * stride + x) * 4;
        let here = pixels[at] < 128;
        if here == inked {
            if let Some(last) = runs.last_mut() {
                *last += 1.0;
            } else {
                runs.push(1.0);
            }
        } else {
            inked = here;
            runs.push(1.0);
        }
    }
    runs
}

/// What a caller sees, and the one assertion here that could have caught the
/// defect the others could not.
///
/// **The rest of this file asserts arithmetic and the renderer passed it a
/// different number.** `fitted_dash(48.0, 4.0)` is the right answer to the
/// wrong question if the renderer hands it 44 -- which it did, having fitted
/// the centre line it strokes rather than the border box Chrome fits. Same
/// dash count, gaps of 4 where Chrome leaves 5, 6, 5, and four pixels of edge
/// unaccounted for. Every test in this file passed throughout.
///
/// So this one goes through `Renderer` and reads the ink back out: the
/// subject is what is drawn, not what a helper computes.
///
/// A 48-tall box at width 4, read down the middle of its left border, which
/// is the edge Chrome was read along end to end.
#[test]
fn the_renderer_draws_the_runs_chrome_draws() {
    let side = &SIDES[0];
    let (stride, pixels) = dashed((80.0, side.length), side.width);
    let runs = runs_down(stride, &pixels, 2, side.length as usize);
    assert_eq!(
        runs, side.runs,
        "down a {}px edge at width {} we draw {runs:?} where Chrome draws \
         {:?}",
        side.length, side.width, side.runs
    );
}
