//! Where a path's `viewBox` maps its drawing, against the transform Chrome
//! reports for an `<svg>` (`view-box.tsv`): into the content box, at its size
//! before rounding. Read off the edges of a bar reaching far down the page, so
//! a scale wrong by a sixth of a percent moves its far edge by pixels.

use meo_canvas::{
    BorderStyle, Format, Path, PositionType, Renderer, Root, Styled, all,
    hex_rgb, px, scene::PathPaint, sides,
};

/// The bar, in the `0 0 100 100` space: ten in from each side and the top, and
/// ten times the space's height down, where the scale is amplified tenfold.
const BAR: &str = "M10 10 H90 V1000 H10 Z";

/// Where the bar's edges sit in the `viewBox` space.
const BAR_EDGES: (f32, f32, f32, f32) = (10.0, 90.0, 10.0, 1000.0);

/// A page wide enough for every box and tall enough for the bar's far edge.
const PAGE: (usize, usize) = (400, 2100);

/// How far an edge may sit from Chrome's: our antialiasing's 0.5 crossing on a
/// straight edge lands within a tenth, and the defects this guards move the
/// far edge by more than three pixels and every edge of a padded box by ten.
const TOLERANCE: f32 = 0.25;

/// One row of the table.
struct Row {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    padding: f32,
    border: f32,
    scale: (f32, f32),
    translate: (f32, f32),
}

fn rows() -> Vec<Row> {
    let table = include_str!("assets/chrome/view-box.tsv");
    let rows: Vec<Row> = table
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let field: Vec<f32> = line
                .split('\t')
                .map(|value| {
                    value.parse().unwrap_or_else(|error| {
                        unreachable!("`{value}` in `{line}`: {error}")
                    })
                })
                .collect();
            assert_eq!(field.len(), 10, "`{line}` is not ten numbers");
            Row {
                left: field[0],
                top: field[1],
                width: field[2],
                height: field[3],
                padding: field[4],
                border: field[5],
                scale: (field[6], field[7]),
                translate: (field[8], field[9]),
            }
        })
        .collect();
    // The table can fail only if it holds a fractional box and a padded one
    // beside the integer control both engines agree on.
    assert!(rows.iter().any(|row| row.height.fract() != 0.0));
    assert!(rows.iter().any(|row| row.padding > 0.0 && row.border > 0.0));
    rows
}

/// The bar drawn in `row`'s box, as raw RGBA.
fn drawn(row: &Row) -> Vec<u8> {
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
            Path::d(BAR)
                .view_box(Some((0.0, 0.0, 100.0, 100.0)))
                .fill(Some(PathPaint::Solid(hex_rgb(0xff_00_00).into())))
                .position_type(PositionType::Absolute)
                .position(sides(
                    Some(px(row.top)),
                    None,
                    None,
                    Some(px(row.left)),
                ))
                .width(px(row.width))
                .height(px(row.height))
                .padding(all(px(row.padding)))
                .border(all(row.border))
                .border_style(BorderStyle::Solid)
                .border_color(hex_rgb(0xff_ff_ff)),
        )
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    })
}

/// Every place red coverage crosses a half along `n` samples, interpolated.
fn crossings(n: usize, coverage: impl Fn(usize) -> f32) -> Vec<f32> {
    let mut out = Vec::new();
    for at in 0..n.saturating_sub(1) {
        let (here, next) = (coverage(at), coverage(at + 1));
        if (here - 0.5) * (next - 0.5) < 0.0 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a pixel index in the thousands"
            )]
            let centre = at as f32 + 0.5;
            out.push(centre + (0.5 - here) / (next - here));
        }
    }
    out
}

#[test]
fn a_view_box_maps_where_chrome_maps_it() {
    let mut wrong = Vec::new();
    for row in rows() {
        let pixels = drawn(&row);
        // Red on white: the green channel is the uncovered fraction.
        let coverage = |x: usize, y: usize| {
            1.0 - f32::from(pixels[(y * PAGE.0 + x) * 4 + 1]) / 255.0
        };
        let (a, d) = row.scale;
        let (e, f) = row.translate;
        let want = (
            BAR_EDGES.0.mul_add(a, e),
            BAR_EDGES.1.mul_add(a, e),
            BAR_EDGES.2.mul_add(d, f),
            BAR_EDGES.3.mul_add(d, f),
        );
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a point inside the page, a few hundred pixels in"
        )]
        let (column, line) = (
            want.0.midpoint(want.1) as usize,
            want.2.midpoint(want.3) as usize,
        );
        let across = crossings(PAGE.0, |x| coverage(x, line));
        let down = crossings(PAGE.1, |y| coverage(column, y));
        let ([left, right], [top, bottom]) =
            (across.as_slice(), down.as_slice())
        else {
            wrong.push(format!(
                "{}x{} at {},{}: edges {across:?} across, {down:?} down",
                row.width, row.height, row.left, row.top
            ));
            continue;
        };
        let got = (*left, *right, *top, *bottom);
        let off = [
            got.0 - want.0,
            got.1 - want.1,
            got.2 - want.2,
            got.3 - want.3,
        ];
        if off.iter().any(|delta| delta.abs() > TOLERANCE) {
            wrong.push(format!(
                "{}x{} at {},{} padding {} border {}: edges {got:?}, Chrome's {want:?}",
                row.width, row.height, row.left, row.top, row.padding, row.border
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
