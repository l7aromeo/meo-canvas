//! Every blend mode, checked against its own formula over our own backdrop
//! rather than against Chrome's pinned output, which also carries where the
//! gradient lands. `saturation` and `color-dodge` amplify a backdrop error by
//! about twelve and three, so they are the cases that count.

use meo_canvas::{
    Box, Display, Element, Format, PositionType, Renderer, Root, Styled,
    hex_rgb, px,
    scene::{
        BlendMode, Color, Gradient, GradientGeometry, GradientStop,
        LinearDirection,
    },
    sides,
};

/// The cell every mode is drawn in.
const CELL: (f32, f32) = (56.0, 40.0);

/// The source drawn over it, and where.
const SOURCE: (f32, f32) = (36.0, 24.0);
const SOURCE_AT: (f32, f32) = (10.0, 8.0);

/// The source's colour, `#4090c0`.
const SOURCE_INK: Color = Color::rgb(0x40, 0x90, 0xc0);

/// The two points read inside the source, over a dark and a light backdrop: on
/// one flat backdrop `multiply` agrees with `darken` and `screen` with
/// `lighten`.
const POINTS: [(&str, usize, usize); 2] =
    [("over dark", 14, 20), ("over light", 42, 20)];

/// Chrome's backdrop at the two points, read from `blend-modes.tsv` rather than
/// transcribed, so a regenerated table cannot silently disagree with a copy.
fn chrome_backdrop() -> [(u8, u8, u8); 2] {
    let table = include_str!("assets/chrome/blend-modes.tsv");
    let mut found = [None, None];
    for line in table.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.first() != Some(&"none") || fields.len() < 7 {
            continue;
        }
        let at = match fields[1] {
            "over dark" => 0,
            "over light" => 1,
            _ => continue,
        };
        let channel = |index: usize| {
            fields[index].parse().unwrap_or_else(|_| {
                unreachable!("{:?} is not a channel", fields[index])
            })
        };
        found[at] = Some((channel(4), channel(5), channel(6)));
    }
    [
        found[0].unwrap_or_else(|| {
            unreachable!("the table has no `none` row over dark")
        }),
        found[1].unwrap_or_else(|| {
            unreachable!("the table has no `none` row over light")
        }),
    ]
}

/// The stops the ramp runs between, `#181838` to `#f0e0a0`.
const STOPS: [(f64, f64, f64); 2] = [(24.0, 24.0, 56.0), (240.0, 224.0, 160.0)];

/// Every mode this table covers, in the table's order.
const MODES: [(&str, BlendMode); 16] = [
    ("normal", BlendMode::Normal),
    ("multiply", BlendMode::Multiply),
    ("screen", BlendMode::Screen),
    ("overlay", BlendMode::Overlay),
    ("darken", BlendMode::Darken),
    ("lighten", BlendMode::Lighten),
    ("color-dodge", BlendMode::ColorDodge),
    ("color-burn", BlendMode::ColorBurn),
    ("hard-light", BlendMode::HardLight),
    ("soft-light", BlendMode::SoftLight),
    ("difference", BlendMode::Difference),
    ("exclusion", BlendMode::Exclusion),
    ("hue", BlendMode::Hue),
    ("saturation", BlendMode::Saturation),
    ("color", BlendMode::Color),
    ("luminosity", BlendMode::Luminosity),
];

/// The ramp under every cell.
fn backdrop() -> Gradient {
    Gradient {
        geometry: GradientGeometry::Linear {
            direction: LinearDirection::Angle(90.0),
        },
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: hex_rgb(0x18_18_38),
            },
            GradientStop {
                offset: 1.0,
                color: hex_rgb(0xf0_e0_a0),
            },
        ],
    }
}

/// One cell, with the source drawn over it in `mode` or not drawn at all.
///
/// `isolation: isolate` is what the browser page needed and what a cell here
/// gets for free: the source blends against the cell, not against the page.
fn cell(mode: Option<BlendMode>) -> Element {
    let cell = Box::new()
        .display(Display::Block)
        .position_type(PositionType::Relative)
        .size(px(CELL.0), px(CELL.1))
        .gradient(backdrop());
    match mode {
        None => cell,
        Some(mode) => cell.children(
            Box::new()
                .display(Display::Block)
                .position_type(PositionType::Relative)
                .size(px(SOURCE.0), px(SOURCE.1))
                .margin(sides(
                    px(SOURCE_AT.1),
                    px(0.0),
                    px(0.0),
                    px(SOURCE_AT.0),
                ))
                .background_color(SOURCE_INK)
                .mix_blend_mode(mode),
        ),
    }
}

/// How many pixels each sample covers: two, since the output oscillates with
/// period two -- `exclusion` reads 151 on macOS and 149 on Linux against
/// 150.01. Averaging over a full period cancels it; moving the sample to a null
/// point would fit the phase.
const PERIOD: usize = 2;

/// Renders one cell and reads a period-wide window at both sample points.
fn read(mode: Option<BlendMode>) -> [[(u8, u8, u8); PERIOD]; 2] {
    let mut renderer = Renderer::new();
    // Off for the reason every other pixel-reading test turns it off: two
    // rasterisers do not agree to the byte, and this reads exact colours.
    renderer.set_gpu(false);

    let mut canvas = Root::new(CELL.0)
        .height(CELL.1)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(cell(mode))
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    let bytes = canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    });

    POINTS.map(|(_, x, y)| {
        std::array::from_fn(|step| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the cell is a whole number of pixels, written above"
            )]
            let at = (y * (CELL.0 as usize) + x + step) * 4;
            (bytes[at], bytes[at + 1], bytes[at + 2])
        })
    })
}

/// Compositing 1 §10.2: the separable modes, one channel at a time.
fn separable(mode: &str, backdrop: f64, source: f64) -> f64 {
    match mode {
        "normal" => source,
        "multiply" => backdrop * source,
        "screen" => backdrop.mul_add(-source, backdrop + source),
        // Overlay is hard-light with the two swapped, which is the spec's own
        // definition rather than a shortcut.
        "overlay" => separable("hard-light", source, backdrop),
        "darken" => backdrop.min(source),
        "lighten" => backdrop.max(source),
        "color-dodge" => {
            if backdrop <= 0.0 {
                0.0
            } else if source >= 1.0 {
                1.0
            } else {
                (backdrop / (1.0 - source)).min(1.0)
            }
        }
        "color-burn" => {
            if backdrop >= 1.0 {
                1.0
            } else if source <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - backdrop) / source).min(1.0)
            }
        }
        "hard-light" => {
            if source <= 0.5 {
                separable("multiply", backdrop, 2.0 * source)
            } else {
                separable("screen", backdrop, 2.0f64.mul_add(source, -1.0))
            }
        }
        "soft-light" => {
            if source <= 0.5 {
                2.0f64
                    .mul_add(-source, 1.0)
                    .mul_add(-(backdrop * (1.0 - backdrop)), backdrop)
            } else {
                let lifted = if backdrop <= 0.25 {
                    16.0f64.mul_add(backdrop, -12.0).mul_add(backdrop, 4.0)
                        * backdrop
                } else {
                    backdrop.sqrt()
                };
                2.0f64
                    .mul_add(source, -1.0)
                    .mul_add(lifted - backdrop, backdrop)
            }
        }
        "difference" => (backdrop - source).abs(),
        "exclusion" => 2.0f64.mul_add(-(backdrop * source), backdrop + source),
        other => unreachable!("no separable formula for {other}"),
    }
}

/// Compositing 1 §10.3's `Lum`.
fn luminosity(color: [f64; 3]) -> f64 {
    0.11f64.mul_add(color[2], 0.3f64.mul_add(color[0], 0.59 * color[1]))
}

/// Compositing 1 §10.3's `ClipColor`.
fn clip(color: [f64; 3]) -> [f64; 3] {
    let lum = luminosity(color);
    let low = color[0].min(color[1]).min(color[2]);
    let high = color[0].max(color[1]).max(color[2]);
    let mut out = color;
    if low < 0.0 {
        for channel in &mut out {
            *channel = lum + (*channel - lum) * lum / (lum - low);
        }
    }
    if high > 1.0 {
        for channel in &mut out {
            *channel = lum + (*channel - lum) * (1.0 - lum) / (high - lum);
        }
    }
    out
}

/// Compositing 1 §10.3's `SetLum`.
fn with_luminosity(color: [f64; 3], want: f64) -> [f64; 3] {
    let shift = want - luminosity(color);
    clip([color[0] + shift, color[1] + shift, color[2] + shift])
}

/// Compositing 1 §10.3's `Sat`, the spread between the extreme channels: the
/// divisor that makes `saturation` an amplifier, eight units out of 255 at the
/// dark point.
fn spread(color: [f64; 3]) -> f64 {
    color[0].max(color[1]).max(color[2]) - color[0].min(color[1]).min(color[2])
}

/// Compositing 1 §10.3's `SetSat`.
fn with_spread(color: [f64; 3], want: f64) -> [f64; 3] {
    let low = color[0].min(color[1]).min(color[2]);
    let high = color[0].max(color[1]).max(color[2]);
    if high <= low {
        return [0.0; 3];
    }
    color.map(|channel| (channel - low) * want / (high - low))
}

/// Compositing 1 §10.3: the four non-separable modes, all three channels at
/// once.
fn non_separable(mode: &str, backdrop: [f64; 3], source: [f64; 3]) -> [f64; 3] {
    match mode {
        "hue" => with_luminosity(
            with_spread(source, spread(backdrop)),
            luminosity(backdrop),
        ),
        "saturation" => with_luminosity(
            with_spread(backdrop, spread(source)),
            luminosity(backdrop),
        ),
        "color" => with_luminosity(source, luminosity(backdrop)),
        "luminosity" => with_luminosity(backdrop, luminosity(source)),
        other => unreachable!("no non-separable formula for {other}"),
    }
}

/// What the spec says the mode produces over `backdrop`, in 0..255.
fn expected(mode: &str, backdrop: (u8, u8, u8)) -> (f64, f64, f64) {
    let scaled = [
        f64::from(backdrop.0) / 255.0,
        f64::from(backdrop.1) / 255.0,
        f64::from(backdrop.2) / 255.0,
    ];
    let source = [
        f64::from(SOURCE_INK.r) / 255.0,
        f64::from(SOURCE_INK.g) / 255.0,
        f64::from(SOURCE_INK.b) / 255.0,
    ];
    let blended = match mode {
        "hue" | "saturation" | "color" | "luminosity" => {
            non_separable(mode, scaled, source)
        }
        separably => [
            separable(separably, scaled[0], source[0]),
            separable(separably, scaled[1], source[1]),
            separable(separably, scaled[2], source[2]),
        ],
    };
    (blended[0] * 255.0, blended[1] * 255.0, blended[2] * 255.0)
}

/// How far a channel may sit from the formula: one unit, with every reading
/// within `0.49` here once sampled over [`PERIOD`]. Not tightened to match,
/// since that is one machine's number; `saturation` and `color-dodge` still
/// expose a fractional error.
const TOLERANCE: f64 = 1.0;

/// Which modes we blend differently from the formula today.
const KNOWN_BLEND: &[&str] = &[];

#[test]
fn every_blend_mode_follows_its_formula() {
    let ours = read(None);
    let mut wrong = Vec::new();
    let mut compared = 0_usize;
    let mut worst = (0.0_f64, String::new());

    for (name, mode) in MODES {
        let drawn = read(Some(mode));
        let known = KNOWN_BLEND.contains(&name);
        let mut apart = false;

        for (index, (point, _, _)) in POINTS.iter().enumerate() {
            // **The expectation is averaged per pixel, not computed from an
            // averaged backdrop.** The blend functions are not linear in the
            // backdrop, so `mean(f(b))` and `f(mean(b))` are different numbers
            // and only the first is what the window actually drew.
            let mean = |of: &dyn Fn(usize) -> (f64, f64, f64)| {
                let sum = (0..PERIOD).fold((0.0, 0.0, 0.0), |acc, step| {
                    let one = of(step);
                    (acc.0 + one.0, acc.1 + one.1, acc.2 + one.2)
                });
                #[expect(clippy::cast_precision_loss, reason = "PERIOD is 2")]
                let count = PERIOD as f64;
                (sum.0 / count, sum.1 / count, sum.2 / count)
            };
            let want = mean(&|step| expected(name, ours[index][step]));
            let got = mean(&|step| {
                let pixel = drawn[index][step];
                (f64::from(pixel.0), f64::from(pixel.1), f64::from(pixel.2))
            });
            let off = [
                (got.0 - want.0).abs(),
                (got.1 - want.1).abs(),
                (got.2 - want.2).abs(),
            ];
            let furthest = off[0].max(off[1]).max(off[2]);
            if furthest > worst.0 {
                worst = (furthest, format!("{name} {point}"));
            }
            compared += 1;

            if furthest > TOLERANCE {
                apart = true;
                wrong.push(format!(
                    "{name} {point}: over our own backdrop {:?} the formula \
                     gives ({:.2}, {:.2}, {:.2}) and we drew ({:.2}, {:.2}, \
                     {:.2}), each a mean over {PERIOD} pixels",
                    ours[index], want.0, want.1, want.2, got.0, got.1, got.2
                ));
            }
        }

        if apart && known {
            wrong.retain(|line| !line.starts_with(name));
        }
        if !apart && known {
            wrong.push(format!(
                "{name}: now follows the formula. That is a fix -- delete the \
                 row from KNOWN_BLEND"
            ));
        }
    }

    assert_eq!(
        compared,
        MODES.len() * POINTS.len(),
        "every mode, both points"
    );
    assert!(
        wrong.is_empty(),
        "{} readings differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    eprintln!(
        "blend modes: {compared} readings against Compositing 1, worst \
         {:.2} of {TOLERANCE:.1} allowed at {}, {} pinned",
        worst.0,
        worst.1,
        KNOWN_BLEND.len()
    );
}

/// Where our gradient lands: the analytic ramp, `(80, 76, 83)` at the dark
/// point, each channel rounded to nearest. Chrome's `(80, 75, 83)` is no
/// rounding rule, likely its gradient dither; this asserts only our side.
#[test]
fn the_gradient_under_the_blends_is_the_analytic_ramp() {
    let ours = read(None);

    // Every pixel of the window the blend rows average over, not just its
    // first: the backdrop under a mean has to be analytic everywhere the mean
    // reaches, or the blend rows are averaging over a ramp that is only right
    // where this row looked.
    for ((index, (point, x, _)), step) in POINTS
        .iter()
        .enumerate()
        .flat_map(|point| (0..PERIOD).map(move |step| (point, step)))
    {
        // The pixel *centre*, not its corner: a gradient is sampled where the
        // pixel is, and reading it at the integer coordinate shifts the whole
        // ramp by half a step.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a coordinate on a 56-pixel cell is exact in an f64"
        )]
        let along = ((*x + step) as f64 + 0.5) / f64::from(CELL.0);
        let want = [0, 1, 2].map(|channel| {
            let (from, to) = (STOPS[0], STOPS[1]);
            let (from, to) = match channel {
                0 => (from.0, to.0),
                1 => (from.1, to.1),
                _ => (from.2, to.2),
            };
            (to - from).mul_add(along, from)
        });
        // Compared as the eight-bit values they are, not as floats: the
        // question is which byte the ramp rounds to, and a float comparison
        // would be asking a different one.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a rounded channel of a ramp between two bytes is a byte"
        )]
        let nearest = want.map(|channel| channel.round() as u8);
        let got: [u8; 3] = ours[index][step].into();
        let theirs = chrome_backdrop()[index];

        assert_eq!(
            got, nearest,
            "{point} +{step}: the ramp is analytically ({:.2}, {:.2}, {:.2}), \
             which rounds to {nearest:?}, and we drew {got:?}. Chrome draws \
             {theirs:?} at the sample point -- but Chrome is not the reference \
             for this row, the arithmetic is, because Chrome dithers and we do \
             not",
            want[0], want[1], want[2]
        );
    }

    // Not an assertion about Chrome, a record of the one place it differs, so
    // that a later reader meeting the blend table's `none` row knows this was
    // measured rather than overlooked.
    eprintln!(
        "gradient under the blends: analytic at both points. Chrome agrees in \
         five channels of six and sits one unit low in green over dark, which \
         is the size and shape of its dither."
    );
}
