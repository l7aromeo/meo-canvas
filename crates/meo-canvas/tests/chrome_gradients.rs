//! Where a gradient ramp has got to, against Chrome's answers, on a
//! black-to-white ramp so `t` is red over 255. Chrome dithers and this renderer
//! does not, so samples at one `t` are never asserted equal and [`TOLERANCE`]
//! allows for it. `radial circle` has no variant here and is counted.

use meo_canvas::{
    Box, Display, Format, PositionType, Renderer, Root, Styled, hex_rgb, px,
    scene::{
        Gradient, GradientGeometry, GradientStop, Length, LinearDirection,
    },
};

/// The box every case is drawn in.
const BOX: (f32, f32) = (88.0, 56.0);

/// How far a channel may sit from Chrome's: two, one for Chrome's dither -- 126
/// against 125 at analytically identical points -- and one for rounding a ramp
/// to a byte. The worst deviation is reported each run.
const TOLERANCE: i32 = 2;

/// Which cases we answer differently from Chrome: empty. A pinned case that
/// starts agreeing fails and says to delete its entry.
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

/// The other direction a pinned list owes: a case that has started agreeing is
/// a fix, and must fail rather than land invisibly.
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
