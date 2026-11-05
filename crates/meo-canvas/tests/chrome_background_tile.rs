//! Where a background picture's tiles land, against Chrome's
//! (`background-tile.tsv`): rounded to whole pixels on an axis that does not
//! repeat, fractional on one that does. Compared as the half-coverage crossings
//! of the picture's edges and of its red half along one row and one column.

use meo_canvas::{
    Box, Display, Format, PositionType, Renderer, Root, Styled, hex_rgb, pct,
    px,
    scene::{
        BackgroundImage, BackgroundRepeat, BackgroundSize, Dimension,
        ImageSource, Length,
    },
    sides,
};

/// The picture: 40x20, its left half red and its right half blue.
const SPLIT_TILE: &[u8] = include_bytes!("assets/split-tile.png");

/// The page, and where the box sits on it, as the table's.
const PAGE: (usize, usize) = (420, 220);
const AT: f32 = 10.0;

/// How far a crossing may sit from Chrome's: unscaled edges agree exactly, and
/// a scaled tile's red edge is softened by resampling on both sides by up to a
/// sixth of a pixel; every defect this guards moves an edge by 0.3 or more.
const TOLERANCE: f32 = 0.2;

/// One row of the table.
struct Case {
    name: String,
    size: (f32, f32),
    background: BackgroundImage,
    row: usize,
    column: usize,
    want: [Vec<f32>; 4],
}

/// One length of `background-size` or `background-position`, as CSS spells it.
fn length(text: &str) -> Dimension {
    if text == "auto" {
        Dimension::Auto
    } else if let Some(percent) = text.strip_suffix('%') {
        Dimension::Percent(number(percent) / 100.0)
    } else if let Some(points) = text.strip_suffix("px") {
        Dimension::Points(number(points))
    } else {
        unreachable!("`{text}` is not a length this table writes")
    }
}

fn number(text: &str) -> f32 {
    text.parse()
        .unwrap_or_else(|error| unreachable!("`{text}`: {error}"))
}

fn position(text: &str) -> Length {
    match length(text) {
        Dimension::Percent(fraction) => pct(fraction * 100.0),
        Dimension::Points(points) => px(points),
        Dimension::Auto => unreachable!("a position is never `auto`"),
    }
}

fn crossings_of(text: &str) -> Vec<f32> {
    text.split(',')
        .filter(|value| !value.is_empty())
        .map(number)
        .collect()
}

fn rows() -> Vec<Case> {
    let table = include_str!("assets/chrome/background-tile.tsv");
    table
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let field: Vec<&str> = line.split('\t').collect();
            assert_eq!(field.len(), 12, "`{line}` is not twelve fields");
            let pair = |text: &str| -> (String, String) {
                let mut parts = text.split(' ');
                let first = parts.next().unwrap_or_default().to_owned();
                let second = parts.next().unwrap_or(&first).to_owned();
                (first, second)
            };
            let size = match field[3] {
                "contain" => BackgroundSize::Contain,
                "cover" => BackgroundSize::Cover,
                text => {
                    let (width, height) = pair(text);
                    BackgroundSize::PerAxis(length(&width), length(&height))
                }
            };
            let (x, y) = pair(field[4]);
            let repeat = match field[5] {
                "no-repeat" => BackgroundRepeat::NoRepeat,
                "repeat" => BackgroundRepeat::Repeat,
                "repeat-x" => BackgroundRepeat::RepeatX,
                "repeat-y" => BackgroundRepeat::RepeatY,
                "space" => BackgroundRepeat::Space,
                "round" => BackgroundRepeat::Round,
                other => {
                    unreachable!("a repeat this table does not write: {other}")
                }
            };
            #[expect(
                clippy::cast_sign_loss,
                clippy::cast_possible_truncation,
                reason = "a pixel index inside the page"
            )]
            let index = |text: &str| number(text) as usize;
            Case {
                name: field[0].trim_matches('"').to_owned(),
                size: (number(field[1]), number(field[2])),
                background: BackgroundImage {
                    source: ImageSource::Bytes(SPLIT_TILE.to_vec()),
                    repeat,
                    size,
                    position: (position(&x), position(&y)),
                },
                row: index(field[6]),
                column: index(field[7]),
                want: [
                    crossings_of(field[8]),
                    crossings_of(field[9]),
                    crossings_of(field[10]),
                    crossings_of(field[11]),
                ],
            }
        })
        .collect()
}

/// The page with the box and its background, as raw RGBA.
fn drawn(row: &Case) -> Vec<u8> {
    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte.
    renderer.set_gpu(false);
    #[expect(
        clippy::cast_precision_loss,
        reason = "a page side in the hundreds"
    )]
    let page = (PAGE.0 as f32, PAGE.1 as f32);
    let mut canvas = Root::new(page.0)
        .height(page.1)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(
            Box::new()
                .display(Display::Block)
                .position_type(PositionType::Absolute)
                .position(sides(Some(px(AT)), None, None, Some(px(AT))))
                .width(px(row.size.0))
                .height(px(row.size.1))
                .background_color(hex_rgb(0xff_ff_ff))
                .background_image(row.background.clone()),
        )
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    })
}

/// Every place `value` crosses a half along `n` samples, in box pixels.
fn crossings(n: usize, value: impl Fn(usize) -> f32) -> Vec<f32> {
    let mut out = Vec::new();
    for at in 0..n.saturating_sub(1) {
        let (here, next) = (value(at), value(at + 1));
        if (here - 0.5) * (next - 0.5) < 0.0 {
            #[expect(clippy::cast_precision_loss, reason = "a pixel index")]
            let centre = at as f32 + 0.5;
            out.push(centre + (0.5 - here) / (next - here) - AT);
        }
    }
    out
}

#[test]
fn background_tiles_land_where_chrome_puts_them() {
    let rows = rows();
    // The table can fail only if it holds both kinds of axis.
    assert!(
        rows.iter()
            .any(|row| row.background.repeat == BackgroundRepeat::NoRepeat)
    );
    assert!(
        rows.iter()
            .any(|row| row.background.repeat == BackgroundRepeat::RepeatX)
    );

    let mut wrong = Vec::new();
    for row in &rows {
        let pixels = drawn(row);
        let channel = |x: usize, y: usize, index: usize| {
            f32::from(pixels[(y * PAGE.0 + x) * 4 + index]) / 255.0
        };
        // Offsets in the table are box pixels; the page has the box at `AT`.
        #[expect(
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation,
            reason = "a whole offset"
        )]
        let at = AT as usize;
        let (y, x) = (row.row + at, row.column + at);
        let got = [
            crossings(PAGE.0, |x| 1.0 - channel(x, y, 1)),
            crossings(PAGE.0, |x| 1.0 - channel(x, y, 2)),
            crossings(PAGE.1, |y| 1.0 - channel(x, y, 1)),
            crossings(PAGE.1, |y| 1.0 - channel(x, y, 2)),
        ];
        let agrees = got.iter().zip(&row.want).all(|(ours, theirs)| {
            ours.len() == theirs.len()
                && ours
                    .iter()
                    .zip(theirs)
                    .all(|(a, b)| (a - b).abs() <= TOLERANCE)
        });
        if !agrees {
            wrong.push(format!(
                "{}:\n  ours   {got:?}\n  chrome {:?}",
                row.name, row.want
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} rows differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
