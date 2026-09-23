//! Where fractional boxes meet. Our boundaries land on whole pixels, so black
//! meeting white is a hard edge with no blended row; Chrome's land on
//! sixty-fourths and blend the seam's row. This asserts our half: the seams are
//! crisp.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{Dimension, layout::FlexDirection, paint::Color},
};

/// A height with no exact binary form, so every boundary but the first falls
/// between pixels before rounding.
const STEP: f32 = 10.3;
/// How many boxes are stacked.
const BOXES: usize = 8;

/// Renders the stack at `scale` and returns the column of pixels down its
/// middle.
fn column(scale: f32) -> Vec<u8> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "eight boxes is exactly representable"
    )]
    let height = STEP.mul_add(BOXES as f32, 4.0);
    let mut scene = Scene::new(Size::new(20.0, height));
    scene.scale = scale;
    if let Some(page) = scene.get_mut(NodeId::ROOT) {
        page.layout.flex_direction = FlexDirection::Column;
        page.paint.background_color = Color::rgb(255, 255, 255);
    }
    for index in 0..BOXES {
        let id = scene
            .push(NodeId::ROOT, Node::new(NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(id) {
            node.layout.size =
                (Dimension::Points(20.0), Dimension::Points(STEP));
            node.paint.background_color = if index % 2 == 0 {
                Color::rgb(0, 0, 0)
            } else {
                Color::rgb(255, 255, 255)
            };
        }
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
    let stride = info.width as usize;
    (0..info.height as usize)
        .map(|y| pixels[((y * stride) + stride / 2) * 4])
        .collect()
}

#[test]
fn every_seam_is_crisp_at_both_scales() {
    // No blended row anywhere: every boundary is on a whole device pixel -- at
    // scale 1 because layout rounded it, at 2 because a whole logical pixel is
    // a whole device pixel. Chrome's seam row is predicted to blend.
    for scale in [1.0_f32, 2.0] {
        let blended: Vec<(usize, u8)> = column(scale)
            .into_iter()
            .enumerate()
            .filter(|&(_, value)| value > 8 && value < 247)
            .collect();
        assert!(
            blended.is_empty(),
            "at scale {scale} the stack has blended rows at {blended:?}, so a \
             boundary did not land on a device pixel"
        );
    }
}

#[test]
fn the_stack_holds_the_colours_it_was_given() {
    // The control: a test that found no blended rows because it found no
    // rows at all would pass the assertion above. Eight boxes alternating
    // means at least seven changes of colour down the column.
    let column = column(1.0);
    let changes = column
        .windows(2)
        .filter(|pair| (i16::from(pair[0]) - i16::from(pair[1])).abs() > 100)
        .count();
    assert!(
        changes >= BOXES - 1,
        "only {changes} colour changes down a stack of {BOXES} boxes"
    );
}
