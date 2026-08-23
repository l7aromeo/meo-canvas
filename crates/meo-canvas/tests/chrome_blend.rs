//! Every blend mode, checked against its own formula rather than against a
//! pinned output.
//!
//! # Why not compare with Chrome's numbers directly
//!
//! Because they answer two questions at once and neither can be read off the
//! result. A blend mode's output carries **the backdrop it was given**, and
//! the backdrop here is a gradient — so a pinned output is a claim about the
//! blending *and* about where the gradient lands at that pixel, and a failing
//! row cannot say which one moved.
//!
//! They did move, separately: our gradient reads `80, 76, 83` at the dark
//! sample point where Chrome reads `80, 75, 83`, one unit in green, against an
//! analytic value of `79.93, 75.79, 82.93` that rounds to Chrome's. Put
//! through the modes, that one unit came out as `+1` on eleven of them, `+3`
//! on `color-dodge`'s green and `-4, +3, +3` on `saturation` — thirteen rows
//! that looked like thirteen defects and were one.
//!
//! So: **read our own backdrop and apply the formula to that.** These rows
//! then say only whether we blend correctly, and the gradient gets a row of
//! its own that says only where it lands. Two questions, two measurements,
//! neither carrying the other.
//!
//! # Why the amplifying modes matter more than the rest
//!
//! `saturation` divides by the backdrop's channel spread — eight units out of
//! 255 at the dark point — so a one-unit fault in the backdrop leaves it
//! twelve units wide. `color-dodge` divides by `1 - Cs` and multiplies the
//! same fault by three. Every other mode here has a gain near one, which is
//! why eleven of them hid the fault inside a tolerance that would have passed
//! them all. **The gain is readable off the formula before anything renders**,
//! and it is what makes a case worth having rather than a curiosity.
//!
//! # Where the source's alpha went
//!
//! Nowhere: the source is opaque and so is the backdrop, so the compositing
//! step of Compositing 1 §9 reduces to the blend function alone and every
//! formula below is `B(Cb, Cs)` with no `Sa`/`Da` term. A translucent source
//! would need the full equation, and there is no row here that has one.

use meo_canvas::{
    Box as BoxNode, Element, Format, PositionType, Renderer, Root, Styled,
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

/// The two points read inside the source.
///
/// One where the backdrop under it is dark and one where it is light. On a
/// flat backdrop `multiply` and `darken` agree wherever the backdrop is
/// lighter than the source, and `screen` and `lighten` agree wherever it is
/// darker; the ramp and the two points are what keep those four apart.
const POINTS: [(&str, usize, usize); 2] =
    [("over dark", 14, 20), ("over light", 42, 20)];

/// Chrome's backdrop at the two points, from `blend-modes.tsv`'s `none` row.
const CHROME_BACKDROP: [(u8, u8, u8); 2] = [(80, 75, 83), (188, 176, 135)];

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
    let cell = BoxNode::new()
        .position_type(PositionType::Relative)
        .size(px(CELL.0), px(CELL.1))
        .gradient(backdrop());
    match mode {
        None => cell,
        Some(mode) => cell.children(
            BoxNode::new()
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

/// Renders one cell and reads it at both sample points.
fn read(mode: Option<BlendMode>) -> [(u8, u8, u8); 2] {
    let mut renderer = Renderer::new();
    // Off for the reason every other pixel-reading test turns it off: two
    // rasterisers do not agree to the byte, and this reads exact colours.
    renderer.set_gpu(false);

    let mut canvas = Root::new(CELL.0, CELL.1)
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
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the cell is a whole number of pixels, written above"
        )]
        let at = (y * (CELL.0 as usize) + x) * 4;
        (bytes[at], bytes[at + 1], bytes[at + 2])
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

/// Compositing 1 §10.3's `Sat`: the spread between the extreme channels.
///
/// **This is the divisor that makes `saturation` an amplifier.** At the dark
/// sample point the backdrop spreads eight units out of 255, so a one-unit
/// error in a channel moves the result by about twelve.
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

/// How far a channel may sit from the formula before it is a defect.
///
/// **One unit, and it is load-bearing for exactly one mode.** Thirty of the
/// thirty-two readings land within `0.49`; `exclusion` lands at `0.99` over
/// the dark point and `0.78` over the light one, which is a separation rather
/// than a tail and is not something to bury under a round number.
///
/// It is not ours. Chrome's own `exclusion` rows, put through the same formula
/// over Chrome's own backdrop, are off by `0.99` and `0.78` — the same two
/// numbers to both decimals. Chrome rasterises with Skia and so do we, so this
/// is one implementation's rounding of `Cb + Cs - 2·Cb·Cs` showing up twice,
/// not a defect either of us has. The row is left inside the tolerance and
/// named here rather than pinned, because a pin would claim we differ from
/// Chrome and we do not.
///
/// A tolerance of one on *pinned Chrome outputs* would have passed eleven
/// modes carrying a real fault — which is what happened before this walker
/// changed currency. Against our own backdrop the only thing inside the
/// tolerance is the blend's own rounding, and a blend genuinely wrong by a
/// fraction of a unit still shows: `saturation` multiplies it by twelve at the
/// dark point and `color-dodge` by three.
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
            let want = expected(name, ours[index]);
            let got = drawn[index];
            let off = [
                (f64::from(got.0) - want.0).abs(),
                (f64::from(got.1) - want.1).abs(),
                (f64::from(got.2) - want.2).abs(),
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
                     gives ({:.2}, {:.2}, {:.2}) and we drew {got:?}",
                    ours[index], want.0, want.1, want.2
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

/// Where our gradient lands, which is the *other* question the blend rows used
/// to carry.
///
/// **We are the analytic value and Chrome is not**, which is the opposite of
/// what the blend comparison first suggested. At the dark point the ramp is
/// analytically `(79.93, 75.79, 82.93)`; we draw `(80, 76, 83)`, which is each
/// channel rounded to nearest, and Chrome draws `(80, 75, 83)` — one unit low
/// in green alone. `75.79` does not round to `75` under any rule that also
/// takes `79.93` to `80`, so Chrome's value is not a rounding rule at all.
///
/// The likely reason is the one worth naming rather than the one worth
/// asserting: **Chrome dithers gradients and we do not** (`PaintStyle`'s
/// `dither` defaults to `false`, and nothing in the blend scene sets it). A
/// dither displaces a channel by about a unit, which is the size and the shape
/// of what is here — five of the six channels across the two points identical
/// and the sixth off by one. Two pixels cannot prove it, so this test asserts
/// only our own side, which it can.
#[test]
fn the_gradient_under_the_blends_is_the_analytic_ramp() {
    let ours = read(None);

    for (index, (point, x, _)) in POINTS.iter().enumerate() {
        // The pixel *centre*, not its corner: a gradient is sampled where the
        // pixel is, and reading it at the integer coordinate shifts the whole
        // ramp by half a step.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a coordinate on a 56-pixel cell is exact in an f64"
        )]
        let along = (*x as f64 + 0.5) / f64::from(CELL.0);
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
        let got = [ours[index].0, ours[index].1, ours[index].2];
        let theirs = CHROME_BACKDROP[index];

        assert_eq!(
            got, nearest,
            "{point}: the ramp is analytically ({:.2}, {:.2}, {:.2}), which \
             rounds to {nearest:?}, and we drew {got:?}. Chrome draws \
             {theirs:?} here -- but Chrome is not the reference for this row, \
             the arithmetic is, because Chrome dithers and we do not",
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
