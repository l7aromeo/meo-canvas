//! What two nodes drawing one picture get, and what they must not share.
//!
//! A source is decoded **once per distinct source** rather than once per node:
//! sixty nodes drawing one image decoded it sixty times, and nothing pointed
//! at it because every node got the right bytes -- just not the same ones.
//!
//! The risk that arrives with sharing is the opposite one, and it is what this
//! file is for: **a shared decode must not carry one node's choices to
//! another.** The only per-node choice a decode can carry is the frame of an
//! animated source, so that is the case asserted here, through the renderer
//! rather than against the cache -- a table that hands back the right object
//! and a picture that draws the right pixels are two claims, and only the
//! second is what a caller sees.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        paint::{Color, ObjectFit},
    },
};

/// Two pages of flat colour, encoded as an animated GIF.
///
/// Built by our own encoder rather than committed as an asset: bytes we wrote
/// are bytes we can decode by definition, and a hand-rolled GIF read back as
/// one frame once already.
fn two_frames() -> Vec<u8> {
    let mut scene = Scene::new(Size::new(8.0, 8.0));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = Color::rgb(255, 0, 0);
    }
    let second = scene
        .push_page()
        .unwrap_or_else(|error| unreachable!("the second page: {error}"));
    if let Some(page) = scene.get_mut(second) {
        page.paint.background_color = Color::rgb(0, 0, 255);
    }
    Renderer::new()
        .render_to_buffer(&scene, ImageFormat::Gif, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("the gif did not encode: {error}"))
}

#[test]
fn two_nodes_sharing_a_source_keep_their_own_frames() {
    let gif = two_frames();
    let mut scene = Scene::new(Size::new(16.0, 8.0));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = Color::rgb(255, 255, 255);
    }
    for frame in [0_u32, 1] {
        let id = scene
            .push(
                NodeId::ROOT,
                Node::new(NodeKind::Image {
                    source: ImageSource::Bytes(gif.clone()),
                    frame: Some(frame),
                    fit: ObjectFit::Fill,
                    position: (Length::ZERO, Length::ZERO),
                }),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(id) {
            node.layout.size = (Dimension::Points(8.0), Dimension::Points(8.0));
        }
    }

    let png = Renderer::new()
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));
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
    let at = |x: usize| {
        let start = ((4 * stride) + x) * 4;
        (pixels[start], pixels[start + 1], pixels[start + 2])
    };

    // The left node asked for frame 0 and the right for frame 1. Sharing one
    // decode between them is correct; sharing one *frame* is the bug, and it
    // would paint both halves the same colour.
    assert_ne!(
        at(4),
        at(12),
        "both halves drew the same frame of a shared source"
    );
}
