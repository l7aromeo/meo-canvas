//! A page as tall as what is in it, which is what v1 does when a height is
//! left out.
//!
//! # Why the picture is measured rather than the layout
//!
//! Because the claim is about the **surface**, not about the solve. Layout
//! could resolve a root to any height at all and still be painted onto a sheet
//! of the size the scene stated, which is exactly what happened before: the
//! surface was allocated from `scene.size` before layout had run, so a height
//! it derived had nowhere to go. Reading the encoded PNG's own header is the
//! only assertion that distinguishes "solved a height" from "made a canvas that
//! tall".
//!
//! # The control is the same scene with a stated height
//!
//! Content sizing that agreed with a stated height on every scene would be
//! indistinguishable from ignoring the flag, so each case is run twice: once
//! asking for the content's height and once stating a different one. A pair
//! that reports the same number twice is measuring nothing.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{Dimension, Length, layout::FlexDirection},
};

/// The width every case is laid out at. Never derived -- see `Scene::size`.
const WIDTH: f32 = 100.0;

/// The height of each stacked child.
const CHILD: f32 = 60.0;

/// The page's own padding, so the content's height and the page's differ by
/// something a wrong answer could not land on by accident.
const PADDING: f32 = 8.0;

/// A column page holding `count` children, sized by content or by `stated`.
fn page(count: usize, stated: Option<f32>, floor: f32) -> Scene {
    let mut scene = Scene::new(Size::new(WIDTH, stated.unwrap_or(floor)));
    scene.content_height = stated.is_none();
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.layout.flex_direction = FlexDirection::Column;
        root.layout.padding.top = Length::Points(PADDING);
        root.layout.padding.bottom = Length::Points(PADDING);
    }
    for _ in 0..count {
        let child = scene
            .push(NodeId::ROOT, Node::new(NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(child) {
            node.layout.size =
                (Dimension::Points(WIDTH), Dimension::Points(CHILD));
        }
    }
    scene
}

/// The rendered image's height in pixels, read from the PNG rather than from
/// us.
fn rendered_height(scene: &Scene) -> u32 {
    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: the two
    // rasterisers do not agree to the byte. Only the header is read, but a
    // backend that fails to allocate would fail differently, and this is one
    // fewer difference between this test and its neighbours.
    renderer.set_gpu(false);
    let bytes = renderer
        .render_to_buffer(scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    let reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap_or_else(|error| {
            unreachable!("the png did not decode: {error}")
        });
    reader.info().height
}

#[test]
fn a_page_takes_the_height_of_its_content() {
    // Two children of 60 in a page padded 8 top and bottom is 136, and one
    // child is 76. Neither is a number the stated-height control produces, so
    // a pass cannot come from the flag being ignored.
    for (count, expected) in [(1_usize, 76_u32), (2, 136), (3, 196)] {
        let height = rendered_height(&page(count, None, 0.0));
        assert_eq!(
            height, expected,
            "{count} children of {CHILD} in a page padded {PADDING}"
        );
    }
}

#[test]
fn a_stated_height_is_still_the_height() {
    // The control. The same scenes with a height stated come out that height
    // whatever they hold, including shorter than their content -- which is
    // what makes the pair above a measurement rather than a coincidence.
    for count in 1_usize..4 {
        let height = rendered_height(&page(count, Some(50.0), 0.0));
        assert_eq!(height, 50, "{count} children under a stated height of 50");
    }
}

#[test]
fn the_stated_size_is_a_floor_when_the_content_decides() {
    // A floor below the content changes nothing, and a floor above it is what
    // the page becomes. Both directions, because a floor that only ever raised
    // would be indistinguishable from a maximum in every case where it did.
    assert_eq!(
        rendered_height(&page(1, None, 10.0)),
        76,
        "a floor under the content's own height does not move it"
    );
    assert_eq!(
        rendered_height(&page(1, None, 300.0)),
        300,
        "a floor above the content's own height is the page"
    );
}
