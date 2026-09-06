//! An SVG source is rasterised for the surface it lands on, not for its own
//! stated size.
//!
//! # Why this file exists
//!
//! The unit tests beside `raster` prove the document is rasterised at the size
//! it is *asked* for. **They cannot see what asks.** A painter that asked in
//! layout pixels would pass every one of them and still draw a page at
//! `scale: 2` from a raster half the surface's resolution -- which is the
//! bitmap upscale that keeping the document was meant to avoid.
//!
//! So this measures the pixels. A diagonal edge rasterised at device
//! resolution steps one pixel at a time; the same edge rasterised at layout
//! resolution and drawn twice as large steps two, and **every one of its
//! transitions lands on an even column**. That is a property of the upscale
//! rather than of the drawing, which is what makes it a test rather than a
//! golden.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension,
        paint::{Color, ObjectFit},
    },
};

/// A triangle, so the drawing has an edge that is neither horizontal nor
/// vertical at any scale.
const WEDGE: &str = concat!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" "##,
    r##"viewBox="0 0 20 20"><path d="M0 0 L20 20 L0 20 Z" fill="#000000"/>"##,
    r##"</svg>"##
);

/// Renders the wedge at `scale`, drawn into a 20x20 box, and returns the
/// pixels.
fn drawn(scale: f32) -> (usize, Vec<u8>) {
    let mut scene = Scene::new(Size::new(20.0, 20.0));
    scene.scale = scale;
    if let Some(page) = scene.get_mut(NodeId::ROOT) {
        page.paint.background_color = Color::rgb(255, 255, 255);
    }
    let id = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Image {
                source: ImageSource::Bytes(WEDGE.as_bytes().to_vec()),
                fit: ObjectFit::Fill,
                position: (
                    meo_canvas_scene::style::Length::Percent(0.5),
                    meo_canvas_scene::style::Length::Percent(0.5),
                ),
                frame: None,
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size = (Dimension::Points(20.0), Dimension::Points(20.0));
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
    let mut bytes = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut bytes)
        .unwrap_or_else(|error| unreachable!("{error}"));
    bytes.truncate(info.buffer_size());
    (info.width as usize, bytes)
}

/// How many pixels sit between ink and paper -- the softness of the edge.
///
/// **Two discriminators were tried before this one.** Where a row's ink
/// *starts* is column zero on every row, because the wedge's diagonal is its
/// right edge, so it could not separate anything. Where it *ends*, and whether
/// those ends step by one column or two, does separate a doubled raster in
/// principle and not in practice: the upscale is smoothed rather than blocky,
/// so its ends step by one as well and the mutation passed.
///
/// What does separate them is how far the smoothing spreads. A document
/// rasterised at device resolution has about one part-covered pixel per row
/// along the diagonal; the same document rasterised at half that and scaled up
/// has the blend spread over two, plus the interpolation's own ramp.
fn edge_softness(width: usize, bytes: &[u8]) -> usize {
    let height = bytes.len() / (width * 4);
    (0..height)
        .map(|y| {
            (0..width)
                .filter(|x| (40..=215).contains(&bytes[(y * width + x) * 4]))
                .count()
        })
        .sum()
}

#[test]
fn a_document_is_rasterised_for_the_surface_and_not_for_itself() {
    let (width, bytes) = drawn(2.0);
    assert_eq!(width, 40, "a 20pt page at scale 2 is 40 pixels wide");

    let softness = edge_softness(width, &bytes);
    // **Measured both ways rather than reasoned about.** Rasterising for the
    // surface gives 20 part-covered pixels over the whole edge; asking in
    // layout pixels and letting the draw call scale gives 118. The bound sits
    // between them with room on each side, so neither antialiasing noise nor
    // a change of one pixel's coverage moves the answer.
    assert!(
        softness < 60,
        "the wedge's edge is {softness} pixels of blend, where a document \
         rasterised for this surface is about 20 and one rasterised at half \
         the surface and scaled up is about 118: the rasterisation is being \
         asked for in layout pixels rather than device pixels"
    );
}

#[test]
fn the_same_document_at_scale_one_is_the_control() {
    // Without this the row above could pass on a page that ignored `scale`
    // entirely: 40 is asserted there, and here is the same scene at 1.
    let (width, bytes) = drawn(1.0);
    assert_eq!(width, 20);
    assert!(
        edge_softness(width, &bytes) > 0,
        "the wedge drew no edge at all at scale 1"
    );
}
