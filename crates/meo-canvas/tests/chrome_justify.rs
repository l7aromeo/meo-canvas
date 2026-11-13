//! Which lines a render spreads under `TextAlign::Justify`, against
//! `justify.tsv`: a line Chrome spreads ends flush with the box and differs
//! from the same line set left, and a line it leaves alone, the last or one
//! a newline ends, is drawn exactly as set left. Where each character goes
//! is the core's `a_justified_line_spreads_where_chrome_spreads_it`.

use std::io::{self, Write as _};

use meo_canvas::{
    Display, Element, Format, LineHeight, Renderer, Root, Styled, Text,
    TextAlign, hex_rgb, px, scene::Color,
};

/// The recorded Chrome rows.
const TABLE: &str = include_str!("assets/chrome/justify.tsv");

/// The faces the table names, under the families it names them by.
const FACES: [(&str, &str); 2] = [
    (
        "Trim",
        "../meo-canvas-core/tests/assets/fonts/NotoSansCJKsc-Trim.otf",
    ),
    (
        "Wide",
        "../meo-canvas-core/tests/assets/fonts/NotoSansSC-Subset.ttf",
    ),
];

/// The block's width where a row states none.
const WIDTH: f32 = 120.0;

/// The page, wider than any row's block and tall enough for three lines.
const PAGE: (usize, usize) = (140, 60);

/// The line height, which puts line `n` at `n * LINE`.
const LINE: usize = 20;

/// A pixel darker than this in red is glyph.
const GLYPH: u8 = 128;

/// How far from the box's right edge a spread line's ink may end: the right
/// side bearing of its last glyph, a pixel or two for these faces, before any
/// letter-spacing, which Chrome's last rect holds after the glyph.
const FLUSH: f32 = 3.0;

/// How close to the block's width Chrome's last rect on a line must reach
/// for Chrome to have spread it: one of its layout units.
const SPREAD: f32 = 1.0 / 64.0;

fn property<'css>(css: &'css str, name: &str) -> Option<&'css str> {
    css.split(';').find_map(|declaration| {
        declaration
            .split_once(':')
            .filter(|(key, _)| *key == name)
            .map(|(_, value)| value)
    })
}

fn pixels(value: &str) -> f32 {
    value
        .strip_suffix("px")
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| unreachable!("{value} is not in pixels"))
}

/// Renders `text` in `face`, `width` wide and aligned by `align`.
fn render(
    text: &str,
    face: &str,
    css: &str,
    width: f32,
    align: TextAlign,
) -> Vec<u8> {
    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte.
    renderer.set_gpu(false);
    for (family, path) in FACES {
        renderer
            .register_font(family, path)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    let mut node: Element = Text::new(text)
        .font_family(face)
        .font_size(16.0)
        .line_height(LineHeight::Length(20.0))
        .color(Color::rgb(0, 0, 0))
        .text_align(align)
        .width(px(width));
    if let Some(spacing) = property(css, "letter-spacing") {
        node = node.letter_spacing(px(pixels(spacing)));
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "the page's sides are written above and small"
    )]
    let mut canvas = Root::new(PAGE.0 as f32)
        .height(PAGE.1 as f32)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(node)
        .render(&renderer)
        .unwrap_or_else(|error| unreachable!("{error}"));
    canvas
        .to_buffer(Format::Raw)
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// Line `line`'s pixels, and the last column of glyph in it.
fn line_of(bytes: &[u8], line: usize) -> (&[u8], Option<usize>) {
    let rows = &bytes[line * LINE * PAGE.0 * 4..(line + 1) * LINE * PAGE.0 * 4];
    let right = (0..PAGE.0)
        .rev()
        .find(|x| (0..LINE).any(|y| rows[(y * PAGE.0 + x) * 4] < GLYPH));
    (rows, right)
}

#[test]
fn a_render_spreads_the_lines_chrome_spreads() {
    let mut failures = Vec::new();
    let (mut spread, mut left) = (0, 0);
    for row in TABLE.lines().filter(|line| line.starts_with("line\t")) {
        let cells: Vec<&str> = row.split('\t').collect();
        let [_, face, written, css, characters] = cells.as_slice() else {
            unreachable!("{row} does not have five cells")
        };
        // `text-align-last`, which the scene has no way to say.
        if css.contains("text-align-last:justify") {
            continue;
        }
        let text = written.trim_matches('"').replace("\\n", "\n");
        let width = property(css, "width").map_or(WIDTH, pixels);
        // Chrome's right edge per line, from its rects.
        let mut edges: Vec<f32> = Vec::new();
        for cell in characters.split(' ') {
            let parts: Vec<f32> = cell
                .split(':')
                .filter_map(|part| part.parse().ok())
                .collect();
            if let [line, x, w] = parts[..] {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a line index the tool wrote as a small whole number"
                )]
                let line = line as usize;
                if edges.len() <= line {
                    edges.resize(line + 1, 0.0);
                }
                edges[line] = edges[line].max(x + w);
            }
        }
        let justified = render(&text, face, css, width, TextAlign::Justify);
        let plain = render(&text, face, css, width, TextAlign::Left);
        let last = edges.len().saturating_sub(1);
        for (line, edge) in edges.iter().enumerate() {
            let (ours, right) = line_of(&justified, line);
            let (set_left, _) = line_of(&plain, line);
            let chrome_spreads = line != last && (width - edge).abs() <= SPREAD;
            let spacing = property(css, "letter-spacing").map_or(0.0, pixels);
            #[expect(
                clippy::cast_precision_loss,
                reason = "a column of a 140-pixel page is exact in an f32"
            )]
            let flush = right
                .is_some_and(|right| right as f32 + FLUSH + spacing >= width);
            if chrome_spreads {
                spread += 1;
                if ours == set_left || !flush {
                    failures.push(format!(
                        "{text:?} {css} line {line}: Chrome spreads it to {edge}; ours ends at {right:?}"
                    ));
                }
            } else {
                left += 1;
                if ours != set_left {
                    failures.push(format!(
                        "{text:?} {css} line {line}: Chrome leaves it at {edge}; ours is not as set left"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        spread >= 15 && left >= 15,
        "{spread} spread lines and {left} others read"
    );
}

/// The darkness-weighted mean column of line `line`'s ink between `from` and
/// `from + 16`, one em: where a glyph sits, to a fraction of a pixel, however
/// its rasteriser splits or shades it.
fn centroid(bytes: &[u8], line: usize, from: f32) -> f32 {
    let (rows, _) = line_of(bytes, line);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "an em's edge on a 140-pixel page, clamped at zero"
    )]
    let (first, last) = (
        from.max(0.0).floor() as usize,
        (from + 16.0).ceil() as usize,
    );
    let (mut weight, mut moment) = (0.0_f32, 0.0_f32);
    for x in first..last.min(PAGE.0) {
        for y in 0..LINE {
            let dark = f32::from(255 - rows[(y * PAGE.0 + x) * 4]);
            weight += dark;
            #[expect(
                clippy::cast_precision_loss,
                reason = "a column of a 140-pixel page"
            )]
            let column = x as f32 + 0.5;
            moment += dark * column;
        }
    }
    moment / weight
}

/// How far a glyph's ink may move other than as Chrome's rect moves it: half a
/// pixel, the most a rasteriser setting glyphs on whole pixels shifts one. On
/// macOS the worst is 0.12; spacing a tenth short puts five glyphs past it.
const INK_PLACE: f32 = 0.5;

#[test]
fn each_ideograph_is_drawn_where_chrome_sets_it() {
    // Each ideograph's ink centroid, justified, moves from where it sits set
    // left as Chrome's rect moves: a difference of centroids cancels the
    // platform's glyph shape and shading, which a first ink column does not.
    let (mut read, mut worst) = (0, 0.0_f32);
    let mut failures = Vec::new();
    for row in TABLE.lines().filter(|line| line.starts_with("line\t")) {
        let cells: Vec<&str> = row.split('\t').collect();
        let [_, face, written, css, characters] = cells.as_slice() else {
            unreachable!("{row} does not have five cells")
        };
        let text = written.trim_matches('"');
        if css.contains("text-align-last")
            || !text.chars().all(|c| matches!(c, '漢' | '字'))
        {
            continue;
        }
        let width = property(css, "width").map_or(WIDTH, pixels);
        // Each ideograph's advance set left: its em and the row's spacing.
        let advance =
            16.0 + property(css, "letter-spacing").map_or(0.0, pixels);
        let chrome: Vec<f32> = characters
            .split(' ')
            .filter_map(|cell| {
                let parts: Vec<f32> = cell
                    .split(':')
                    .filter_map(|part| part.parse().ok())
                    .collect();
                match parts[..] {
                    [0.0, x, _] => Some(x),
                    _ => None,
                }
            })
            .collect();
        let justified = render(text, face, css, width, TextAlign::Justify);
        let plain = render(text, face, css, width, TextAlign::Left);
        for (index, x) in chrome.iter().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "a glyph index")]
            let natural = advance * index as f32;
            let moved =
                centroid(&justified, 0, *x) - centroid(&plain, 0, natural);
            let expected = x - natural;
            worst = worst.max((moved - expected).abs());
            if (moved - expected).abs() > INK_PLACE {
                failures.push(format!(
                    "{text:?} {css} glyph {index}: moved {moved}, Chrome moves it {expected}"
                ));
            }
            read += 1;
        }
    }
    // Past libtest's capture, so a passing run reports each platform's spread.
    let report = writeln!(
        io::stderr(),
        "ideograph ink: worst {worst}px of {INK_PLACE} over {read}"
    );
    assert!(report.is_ok(), "stderr is writable in a test");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(read, 19, "the three rows' first lines are read");
}
