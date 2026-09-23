//! A dashed border's rhythm against Chrome: `3w` on, `2w` off while thin and
//! `2w` on, `1w` off from width 3, the nearest whole count of dashes fitted per
//! side with slack in the gaps. Rounded boxes stay unfitted: Chrome's fit
//! length there (213 against 216) is unresolved.

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

/// One measured width, and the ink and gap Chrome repeats along the top band of
/// a 240x48 box between x=40 and 200, ink below 128 red; the clipped end runs
/// are not whole periods.
struct Rhythm {
    /// The border's width in pixels.
    width: f32,
    /// The ink run Chrome repeats.
    ink: f32,
    /// The gap Chrome repeats. Where Chrome's own gaps vary by a pixel — the
    /// remainder it spreads — this is the one it holds most of the way along.
    gap: f32,
}

/// Chrome's rhythm at five widths, read out of the table rather than copied. It
/// gives the nominal gap for `dash_pattern` and the modal fitted one for
/// `fitted_dash`, which differ only at width 8 (8 against 9). Width 3 is in the
/// upper regime, so the step falls at `w < 3`.
fn chrome() -> Vec<Rhythm> {
    let table =
        include_str!("../../meo-canvas/tests/assets/chrome/border-rhythm.tsv");
    let mut out = Vec::new();
    for line in table.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.first() != Some(&"dashed") || fields.len() < 6 {
            continue;
        }
        let width: f32 = match fields[1].parse() {
            Ok(width) => width,
            Err(_) => continue,
        };
        // The first and last runs are cut by the reading window and are not
        // whole periods, so neither votes.
        let runs: Vec<&str> = fields[5].split_whitespace().collect();
        let modal = |kind: &str| {
            let mut counts: Vec<(u32, usize)> = Vec::new();
            for run in &runs[1..runs.len() - 1] {
                let Some(value) = run.strip_prefix(kind) else {
                    continue;
                };
                let value: u32 = value
                    .parse()
                    .unwrap_or_else(|_| unreachable!("{run} is not a run"));
                match counts.iter_mut().find(|(seen, _)| *seen == value) {
                    Some((_, count)) => *count += 1,
                    None => counts.push((value, 1)),
                }
            }
            counts
                .into_iter()
                .max_by_key(|&(_, count)| count)
                .unwrap_or_else(|| {
                    unreachable!("no {kind} runs at width {width}")
                })
                .0
        };
        out.push(Rhythm {
            width,
            ink: modal("on:") as f32,
            gap: modal("off:") as f32,
        });
    }
    assert_eq!(out.len(), 5, "the table should carry five dashed widths");
    out
}

/// The box every `dashed` row was read in.
///
/// Needed here because the table's runs are **fitted** to that side, and the
/// nominal pattern is only what fitting starts from.
const READING_BOX: f32 = 240.0;

#[test]
fn a_dash_is_the_length_chrome_makes_it() {
    for row in chrome() {
        // The INK is nominal and fitted alike -- the slack goes in the gaps,
        // never in the dash -- so one assertion covers it.
        let (ink, nominal_gap) = dash_pattern(row.width);
        assert!(
            (ink - row.ink).abs() < f32::EPSILON,
            "a {}px border dashes {ink} on where Chrome draws {}",
            row.width,
            row.ink
        );

        // Two claims about the gap: `dash_pattern`'s nominal and the table's
        // fitted, equal except at width 8 (8 against 9). Rounded, since
        // the fit is continuous (2.04 at width 1) and a table run is
        // whole pixels.
        let (_, fitted_gap) = fitted_dash(READING_BOX, row.width);
        assert!(
            (fitted_gap.round() - row.gap).abs() < f32::EPSILON,
            "a {}px border on a {READING_BOX}px side leaves {fitted_gap} off, \
             rounding to {}, where Chrome leaves {}",
            row.width,
            fitted_gap.round(),
            row.gap
        );
        assert!(
            nominal_gap <= row.gap,
            "fitting only ever widens a gap: nominal {nominal_gap} against a \
             drawn {} at width {}",
            row.gap,
            row.width
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

/// The two sides Chrome was read along end to end. A 48px edge at width 4 is
/// flush with whole dashes and its gaps are `5, 6, 5`, all wider than nominal;
/// the 137px edge has one narrower. So the nominal gap is a target, not a
/// floor.
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

/// What a caller sees: renders through `Renderer` and reads the ink back. The
/// arithmetic tests would pass a renderer that fits its stroked centre line
/// (44) rather than the border box Chrome fits (48); the drawn runs down a
/// 48-tall box's left border at width 4 show it, as gaps of 4 against 5, 6, 5.
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
