//! What a `viewBox` does to a path, and what it deliberately does not do to a
//! pen.
//!
//! # The behaviour this file exists to hold
//!
//! **A `viewBox` scales the drawing and leaves the stroke width alone.** In
//! SVG it does not: a two-pixel stroke in a box scaled five times is drawn ten
//! pixels wide, which is why `vector-effect: non-scaling-stroke` exists at all.
//! Ours is equivalent to SVG's `viewBox` **with** `non-scaling-stroke`.
//!
//! That is deliberate — a caller authoring a `d` in a unit square wants
//! `line_width` to mean pixels, and a chart's gridlines to stay hairlines
//! whatever the box — but **it was true by accident before this file existed**.
//! It falls out of transforming the path's geometry rather than the coordinate
//! system: `Path2D::transform` moves the points and leaves the pen. Nothing
//! asserted it, and a future change that scaled the pen would look like a bug
//! *fix* to whoever made it, since it would move us toward SVG.
//!
//! If something later wants scaled pens, `vector-effect` is the addable piece —
//! the same subset-of-a-standard-vocabulary move as `preserve_aspect_ratio`.

use meo_canvas::{
    Box as BoxNode, Format, Path, PositionType, Renderer, Root, Styled,
    hex_rgb, px, scene::PathPaint,
};

/// The page every case is drawn on.
const PAGE: (f32, f32) = (200.0, 200.0);

/// Renders one path in a box of `size`, and hands back the pixels.
fn render(view: Option<(f32, f32, f32, f32)>, size: (f32, f32)) -> Vec<u8> {
    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte.
    renderer.set_gpu(false);

    let mut canvas = Root::new(PAGE.0)
        .height(PAGE.1)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(
            BoxNode::new()
                .position_type(PositionType::Absolute)
                .position(meo_canvas::sides(
                    Some(px(0.0)),
                    None,
                    None,
                    Some(px(0.0)),
                ))
                .size(px(size.0), px(size.1))
                .children(
                    // A cross: one horizontal arm and one vertical, so a pen
                    // distorted by a non-uniform scale would show as two
                    // different thicknesses in one drawing.
                    Path::d("M2 10 H18 M10 2 V18")
                        .view_box(view)
                        .fill(None)
                        .stroke(Some(PathPaint::Solid(hex_rgb(0x00_00_00))))
                        .line_width(2.0)
                        .width(px(size.0))
                        .height(px(size.1)),
                ),
        )
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    })
}

/// How thick the ink is down a column, in pixels.
fn thickness_down(pixels: &[u8], x: usize) -> usize {
    (0..PAGE.1 as usize)
        .filter(|y| {
            let at = (y * PAGE.0 as usize + x) * 4;
            pixels[at] < 128
        })
        .count()
}

/// How thick the ink is across a row, in pixels.
fn thickness_across(pixels: &[u8], y: usize) -> usize {
    (0..PAGE.0 as usize)
        .filter(|x| {
            let at = (y * PAGE.0 as usize + x) * 4;
            pixels[at] < 128
        })
        .count()
}

#[test]
fn a_view_box_scales_the_drawing_and_not_the_pen() {
    // The same 20x20 drawing in a 40x40 box and in a 160x160 box: four times
    // the scale. Under SVG the second would stroke eight pixels wide.
    let small = render(Some((0.0, 0.0, 20.0, 20.0)), (40.0, 40.0));
    let large = render(Some((0.0, 0.0, 20.0, 20.0)), (160.0, 160.0));

    // Read across the horizontal arm, away from the crossing point.
    let thin = thickness_down(&small, 30);
    let thick = thickness_down(&large, 120);

    assert_eq!(
        thin, thick,
        "a viewBox scaled four times drew the pen {thick} wide against {thin} \
         -- the stroke is following the scale, which is SVG's behaviour and \
         not this renderer's. See this file's own doc before 'fixing' it"
    );
    assert_eq!(thin, 2, "the pen is the width the caller asked for");
}

#[test]
fn the_drawing_itself_does_scale() {
    // The control the assertion above needs: if nothing scaled, the two
    // renders would be identical and the test would pass for the wrong reason.
    let small = render(Some((0.0, 0.0, 20.0, 20.0)), (40.0, 40.0));
    let large = render(Some((0.0, 0.0, 20.0, 20.0)), (160.0, 160.0));

    let short = thickness_across(&small, 20);
    let long = thickness_across(&large, 80);
    assert!(
        long > short * 3,
        "the arm should grow with the box: {short} became {long}"
    );
}

#[test]
fn no_view_box_draws_in_absolute_coordinates() {
    // The arm is sixteen units long and stays sixteen pixels whatever the box,
    // which is what every path did before a viewBox existed.
    let small = render(None, (40.0, 40.0));
    let large = render(None, (160.0, 160.0));
    assert_eq!(thickness_across(&small, 10), thickness_across(&large, 10));
}
