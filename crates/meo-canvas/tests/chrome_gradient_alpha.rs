//! How a gradient mixes stops that are not opaque, against Chrome's
//! (`gradient-alpha.tsv`): a box's, the root's and a mask's, as CSS does, with
//! the alpha multiplied in, so red to `transparent` stays red; a path's fill or
//! stroke, as SVG does, straight, with a stop at alpha 0 keeping its colour.
//! Read at every page depth that keeps colour, through PNG, since raw bytes
//! differ by depth.

use meo_canvas::{
    Box, ColorType, Display, Element, Format, Path, PositionType, Renderer,
    Root, Styled, hex_rgb, px,
    scene::{
        Color, Gradient, GradientGeometry, GradientStop, LinearDirection, Mask,
        PathPaint,
    },
};

/// How far a channel may sit from Chrome's: two, one for Chrome's dither (see
/// `chrome_gradients.rs`) and one for rounding to a byte. Every defect this
/// guards moves a channel by 40 or more.
const TOLERANCE: i32 = 2;

/// The page depths read: every one that keeps three colour channels. `Gray8`,
/// `Alpha8` and the one- and two-channel depths cannot show a hue.
const DEPTHS: [ColorType; 6] = [
    ColorType::Uint8,
    ColorType::F16,
    ColorType::F32,
    ColorType::Srgba8888,
    ColorType::Bgra8888,
    ColorType::Rgba1010102,
];

const RED: Color = Color::rgb(255, 0, 0);
const BLUE: Color = Color::rgb(0, 0, 255);

/// The gradient a table case names: its ramp, after the role prefixes, along
/// the geometry its name says. CSS's `transparent` is `rgba(0, 0, 0, 0)`.
fn gradient(case: &str) -> Gradient {
    let ramp = ["root ", "mask ", "svg ", "stroke ", "radial ", "conic "]
        .iter()
        .fold(case, |name, prefix| {
            name.strip_prefix(prefix).unwrap_or(name)
        });
    let colors: &[Color] = match ramp {
        "opaque" => &[RED, BLUE],
        "zero same" => &[RED, Color::rgba(255, 0, 0, 0)],
        "transparent" => &[RED, Color::TRANSPARENT],
        // 0.01 of 255, which Chrome stores as 2.55 and this scene as a byte.
        "near zero" => &[RED, Color::rgba(255, 0, 0, 3)],
        "zero other" => &[RED, Color::rgba(0, 0, 255, 0)],
        "zero first" => &[Color::rgba(255, 0, 0, 0), BLUE],
        "through transparent" => &[RED, Color::TRANSPARENT, BLUE],
        // 0.2 of 255. Under a mask only the alpha is read, so its colours are
        // the box's ramp rather than the table's black.
        "partial" => &[RED, Color::rgba(0, 0, 255, 51)],
        other => unreachable!("`{other}` is not a ramp this test draws"),
    };
    let geometry = if case.contains("radial") {
        GradientGeometry::Radial {
            at: GradientGeometry::CENTER,
        }
    } else if case.contains("conic") {
        // The seam where CSS's twelve o'clock start meets Skia's three falls
        // between two stops, so `paint.rs` mixes it itself.
        GradientGeometry::Conic {
            at: GradientGeometry::CENTER,
            from: 0.0,
        }
    } else {
        GradientGeometry::Linear {
            direction: LinearDirection::Angle(90.0),
        }
    };
    let last = colors.len().saturating_sub(1).max(1);
    #[expect(clippy::cast_precision_loss, reason = "a stop index under ten")]
    let stops = colors
        .iter()
        .enumerate()
        .map(|(index, color)| GradientStop {
            offset: index as f32 / last as f32,
            color: (*color).into(),
        })
        .collect();
    Gradient { geometry, stops }
}

/// The case's element: a box with the gradient as its background or, for a
/// `mask` row, a red box under it; for an `svg` row a path over the same box
/// filled with it, or a line across it stroked with it.
fn painted(case: &str, (width, height): (f32, f32)) -> Element {
    let paint = Some(PathPaint::Gradient(gradient(case)));
    if case.starts_with("svg stroke") {
        return Path::d(format!("M0 {} H{width}", height / 2.0))
            .display(Display::Block)
            .size(px(width), px(height))
            .fill(None)
            .stroke(paint)
            .line_width(height);
    }
    if case.starts_with("svg") {
        return Path::d(format!("M0 0 H{width} V{height} H0 Z"))
            .display(Display::Block)
            .size(px(width), px(height))
            .fill(paint);
    }
    let block = Box::new()
        .display(Display::Block)
        .size(px(width), px(height));
    if case.starts_with("mask") {
        return block
            .background_color(RED)
            .mask(Mask::Gradient(gradient(case)));
    }
    block.gradient(gradient(case))
}

/// The box drawn over a white page at `depth`, as 8-bit RGB rows.
fn drawn(case: &str, size: (u32, u32), depth: ColorType) -> Vec<u8> {
    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte.
    renderer.set_gpu(false);
    #[expect(clippy::cast_precision_loss, reason = "a side of 100")]
    let (width, height) = (size.0 as f32, size.1 as f32);
    let scene = Root::new(width)
        .height(height)
        .color_type(depth)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff));
    // A `root` row paints the root itself, which Chrome draws on the canvas.
    let scene = if case.starts_with("root") {
        scene.gradient(gradient(case))
    } else {
        scene.children(painted(case, (width, height)))
    };
    let mut canvas = scene.render(&renderer).unwrap_or_else(|error| {
        unreachable!("the scene did not render: {error}")
    });
    let png = canvas.to_buffer(Format::Png).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    });
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap_or_else(|error| {
        unreachable!("the PNG did not decode: {error}")
    });
    let mut pixels = vec![0; reader.output_buffer_size().unwrap_or_default()];
    let frame = reader.next_frame(&mut pixels).unwrap_or_else(|error| {
        unreachable!("the PNG did not decode: {error}")
    });
    let channels = frame.color_type.samples();
    pixels
        .chunks(channels)
        .take((size.0 * size.1) as usize)
        .flat_map(|pixel| pixel.iter().copied().take(3))
        .collect()
}

fn number(text: &str) -> u32 {
    text.parse()
        .unwrap_or_else(|error| unreachable!("`{text}`: {error}"))
}

#[test]
fn a_transparent_stop_adds_no_colour_of_its_own() {
    let table = include_str!("assets/chrome/gradient-alpha.tsv");
    let mut wrong = Vec::new();
    let mut compared = 0_usize;
    for depth in DEPTHS {
        let mut last: Option<(String, Vec<u8>)> = None;
        for line in table.lines().filter(|line| !line.starts_with('#')) {
            let field: Vec<&str> = line.split('\t').collect();
            if field.len() != 10 {
                continue;
            }
            let case = field[0].trim_matches('"');
            let size = (number(field[1]), number(field[2]));
            if last.as_ref().is_none_or(|(name, _)| name != case) {
                last = Some((case.to_owned(), drawn(case, size, depth)));
            }
            let Some((_, pixels)) = last.as_ref() else {
                unreachable!("drawn just above")
            };
            let (x, y) = (number(field[4]), number(field[5]));
            let at = ((y * size.0 + x) * 3) as usize;
            let ours = &pixels[at..at + 3];
            let chrome = [field[6], field[7], field[8]].map(number);
            compared += 1;
            if ours.iter().zip(chrome).any(|(a, b)| {
                (i32::from(*a) - b.cast_signed()).abs() > TOLERANCE
            }) {
                wrong.push(format!(
                    "{depth:?} {case} {}: ours {ours:?}, chrome {chrome:?}",
                    field[3]
                ));
            }
        }
    }
    assert!(compared > 0, "the table holds no samples");
    assert!(
        wrong.is_empty(),
        "{} of {compared} samples differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
