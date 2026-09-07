//! Where a gradient ramp has got to, against Chrome's own answers.
//!
//! # Why the ramp is black to white
//!
//! Because then `t` **is** the red channel over 255, and reading the table
//! needs no inverse of an interpolation — which is the one piece of arithmetic
//! a gradient table must not depend on, since it is the thing under test.
//!
//! # Why this cannot assert equality, and why it carries a tolerance
//!
//! **Chrome dithers its gradients and this renderer does not.** The table shows
//! it in its own rows: `linear 0deg` reads 126 at mid-left and 125 at
//! mid-right, two points that are analytically identical on a vertical ramp,
//! and `180deg` reads 130 against 129. Dither is a per-pixel offset from a
//! pattern tied to device coordinates and to a Skia build, so it is neither
//! reproducible across renderers nor worth matching.
//!
//! Two consequences, both of which this file obeys:
//!
//! - **never assert that two samples at the same `t` are equal** — Chrome's own
//!   are not, and a test built on that premise would be asserting the dither
//! - **carry at least a unit per channel**, because an undithered surface
//!   cannot land on a dithered number
//!
//! The tolerance here is stated and derived rather than tuned upward until the
//! test passed: see [`TOLERANCE`].
//!
//! # Rows this renderer cannot express
//!
//! Named and counted rather than skipped, because a silent skip turns a
//! conformance table into a self-portrait. **`radial circle` is Chrome's and
//! has no variant here**: `GradientGeometry::Radial` carries a centre and no
//! shape, and `paint.rs` always fits an ellipse to the box — drawing the circle
//! of the wider radius and squashing it to the narrower. On a square box the
//! two would coincide and this box is 88 by 56, so the distinction is live.

use meo_canvas::{
    Box, Display, Format, PositionType, Renderer, Root, Styled, hex_rgb, px,
    scene::{
        Gradient, GradientGeometry, GradientStop, Length, LinearDirection,
    },
};

/// The box every case is drawn in.
const BOX: (f32, f32) = (88.0, 56.0);

/// How far a channel may sit from Chrome's.
///
/// **Two, and both units are accounted for.** One is Chrome's dither, which
/// the table demonstrates on its own rows rather than asserting — 126 against
/// 125 at two analytically identical points. The second is the rounding of a
/// continuous ramp to a byte, which the two renderers may take in opposite
/// directions at the same `t`.
///
/// A tolerance raised until a test passes has stopped measuring anything, so
/// this one is checked from the other side: the worst deviation over every
/// comparable row is reported at the end of the run, and if it ever approaches
/// two the gap has stopped being dither and wants investigating.
const TOLERANCE: i32 = 2;

/// Which cases we answer differently from Chrome today.
///
/// **All three conic cases, and it is one defect measured to the degree: our
/// sweep begins 270 degrees from where CSS begins it.**
///
/// Every sample of `conic from 0deg` is offset by the same amount, and the ramp
/// being black-to-white makes the offset readable directly as a fraction of the
/// turn:
///
/// ```text
/// sample          chrome  ours   difference
/// top-left           214   150    270 deg
/// top-right           41   232    269
/// bottom-left        169   105    270
/// bottom-right        86    23    271
/// mid-left           191   127    270
/// mid-right           64     0    270
/// mid-top              1   192    269
/// mid-bottom         126    63    271
/// ```
///
/// **Eight samples, spread of two degrees, which is the byte quantisation
/// rather than a variation.** CSS starts a conic sweep at twelve o'clock;
/// `mid-top` is where that shows plainest — Chrome reads 1 there, the very
/// start of the ramp, and we read 192, three quarters through it.
///
/// Not worked around: `from` is passed through unchanged and the rows are
/// pinned, so the day the sweep origin is fixed these three cases fail and say
/// to delete this list. **A test that quietly added 270 degrees would draw the
/// right picture for this table and the wrong one for every caller.**
const KNOWN_GRADIENT: &[&str] = &[];

/// Cases Chrome measured that this renderer has no vocabulary for.
const INEXPRESSIBLE: &[(&str, &str)] = &[(
    "radial circle",
    "`GradientGeometry::Radial` carries a centre and no shape, and `paint.rs` \
     always fits an ellipse to the box; on this 88x56 box a circle and an \
     ellipse are different pictures",
)];

/// The geometry a table's case name asks for, or `None` where we have none.
fn geometry(case: &str) -> Option<GradientGeometry> {
    let centre = GradientGeometry::CENTER;
    let fraction = |x: f32, y: f32| (Length::Percent(x), Length::Percent(y));
    Some(match case {
        "linear 0deg" => GradientGeometry::Linear {
            direction: LinearDirection::Angle(0.0),
        },
        "linear 30deg" => GradientGeometry::Linear {
            direction: LinearDirection::Angle(30.0),
        },
        "linear 90deg" | "linear to right" => GradientGeometry::Linear {
            direction: LinearDirection::Angle(90.0),
        },
        "linear 180deg" | "linear to bottom" => GradientGeometry::Linear {
            direction: LinearDirection::Angle(180.0),
        },
        "linear 270deg" => GradientGeometry::Linear {
            direction: LinearDirection::Angle(270.0),
        },
        // `default` and `ellipse` are one picture here: CSS's default shape on
        // a non-square box is an ellipse, which is what this renderer draws.
        "radial default" | "radial ellipse" => {
            GradientGeometry::Radial { at: centre }
        }
        "radial at 25% 75%" => GradientGeometry::Radial {
            at: fraction(0.25, 0.75),
        },
        "conic from 0deg" => GradientGeometry::Conic {
            at: centre,
            from: 0.0,
        },
        "conic from 90deg" => GradientGeometry::Conic {
            at: centre,
            from: 90.0,
        },
        "conic at 25% 25%" => GradientGeometry::Conic {
            at: fraction(0.25, 0.25),
            from: 0.0,
        },
        _ => return None,
    })
}

/// Renders one case and hands back its pixels.
fn drawn(geometry: GradientGeometry) -> Vec<u8> {
    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte, and this reads exact colours.
    renderer.set_gpu(false);

    let mut canvas = Root::new(BOX.0)
        .height(BOX.1)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(
            Box::new()
                .display(Display::Block)
                .position_type(PositionType::Relative)
                .size(px(BOX.0), px(BOX.1))
                .gradient(Gradient {
                    geometry,
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            color: hex_rgb(0x00_00_00),
                        },
                        GradientStop {
                            offset: 1.0,
                            color: hex_rgb(0xff_ff_ff),
                        },
                    ],
                }),
        )
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    })
}

/// The other direction every pinned list here owes.
///
/// A case that has started agreeing is a fix, and **a fix that lands invisibly
/// is how a pinned list becomes a lie** — so the list has to fail when it is
/// wrong in either direction, not only when a pinned case is still wrong.
fn report(seen: &[String], still_apart: &[String], wrong: &mut Vec<String>) {
    for case in KNOWN_GRADIENT {
        assert!(
            seen.iter().any(|name| name == case),
            "{case} is pinned and the table no longer has it -- delete the row \
             from KNOWN_GRADIENT"
        );
        if !still_apart.iter().any(|name| name == case) {
            wrong.push(format!(
                "{case}: every sample is now within {TOLERANCE} of Chrome. \
                 That is a fix -- delete the row from KNOWN_GRADIENT"
            ));
        }
    }
}

#[test]
fn a_gradient_ramp_reaches_what_chrome_reaches() {
    let table = include_str!("assets/chrome/gradient-truth.tsv");
    let mut wrong = Vec::new();
    let mut compared = 0_usize;
    let mut excluded = 0_usize;
    // Worst over the UNPINNED cases only. Including the pinned conic would
    // report 191 and drown the number this is for -- how close the agreeing
    // cases sit to the tolerance, which is what says whether the tolerance is
    // still measuring dither or has started hiding something.
    let mut worst = (0_i32, String::new());
    let mut seen: Vec<String> = Vec::new();
    // Which pinned cases exceeded the tolerance at least once, so a case that
    // has started agreeing can be reported rather than silently kept.
    let mut still_apart: Vec<String> = Vec::new();

    let mut current: Option<(String, Vec<u8>)> = None;
    for line in table.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 9 {
            continue;
        }
        let case = fields[0];
        if !seen.iter().any(|name| name == case) {
            seen.push(case.to_owned());
        }
        if INEXPRESSIBLE.iter().any(|(name, _)| *name == case) {
            excluded += 1;
            continue;
        }
        let Some(shape) = geometry(case) else {
            unreachable!(
                "the table names a case this test does not map: {case}"
            )
        };

        // Rendered once per case rather than once per row: nine samples of one
        // picture.
        if current.as_ref().is_none_or(|(name, _)| name != case) {
            current = Some((case.to_owned(), drawn(shape)));
        }
        let pixels = &current.as_ref().unwrap_or_else(|| unreachable!()).1;

        let number = |at: usize| -> i32 {
            fields[at].parse().unwrap_or_else(|_| {
                unreachable!("{:?} is not a number", fields[at])
            })
        };
        let (x, y) = (number(4), number(5));
        let at = ((y as usize) * (BOX.0 as usize) + (x as usize)) * 4;
        let ours = [
            i32::from(pixels[at]),
            i32::from(pixels[at + 1]),
            i32::from(pixels[at + 2]),
        ];
        let theirs = [number(6), number(7), number(8)];

        let off = (0..3)
            .map(|c| (ours[c] - theirs[c]).abs())
            .max()
            .unwrap_or(0);
        compared += 1;

        let known = KNOWN_GRADIENT.contains(&case);
        if !known && off > worst.0 {
            worst = (off, format!("{case} {}", fields[3]));
        }
        if known
            && off > TOLERANCE
            && !still_apart.iter().any(|name| name == case)
        {
            still_apart.push(case.to_owned());
        }
        if off > TOLERANCE && !known {
            wrong.push(format!(
                "{case} at {} ({x},{y}): we draw {ours:?}, Chrome {theirs:?}",
                fields[3]
            ));
        }
    }

    report(&seen, &still_apart, &mut wrong);

    for (name, reason) in INEXPRESSIBLE {
        assert!(
            seen.iter().any(|case| case == name),
            "{name} is excluded as inexpressible and the table no longer has \
             it -- delete the row from INEXPRESSIBLE. Reason given was: {reason}"
        );
    }

    assert!(compared > 0, "the gradient table has no rows to compare");
    assert!(
        wrong.is_empty(),
        "{} samples differ by more than {TOLERANCE}:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    eprintln!(
        "gradients: {compared} samples compared, worst {} at {}, {excluded} \
         excluded as inexpressible, {} pinned",
        worst.0,
        worst.1,
        KNOWN_GRADIENT.len()
    );
}
