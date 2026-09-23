//! A source is decoded once per distinct source and shared by every node
//! drawing it. What sharing risks is carrying one node's choice to another; the
//! only such choice is an animated source's frame, asserted through the
//! renderer, since the picture is what a caller sees.

use meo_canvas_core::{Error, ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        paint::{Color, ObjectFit},
    },
};

/// Two pages of flat colour as an animated GIF, built by our own encoder: bytes
/// we wrote are bytes we can decode, where a hand-rolled GIF once read back as
/// one frame.
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

#[test]
fn a_frame_past_the_last_is_refused_naming_both_numbers() {
    // Frame 3 of a two-frame source is something the source cannot answer.
    let mut scene = Scene::new(Size::new(8.0, 8.0));
    scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Image {
                source: ImageSource::Bytes(two_frames()),
                frame: Some(3),
                fit: ObjectFit::Fill,
                position: (Length::ZERO, Length::ZERO),
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));

    let refused = Renderer::new().render_to_buffer(
        &scene,
        ImageFormat::Png,
        &EncodeOptions::default(),
    );
    let Err(error) = refused else {
        unreachable!("frame 3 of a two-frame source drew")
    };
    assert!(
        matches!(
            error,
            Error::FrameOutOfRange {
                index: 3,
                frames: 2,
                ..
            }
        ),
        "frame 3 of two came back as {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "node 1 asks for frame 3 of an image with 2 frames"
    );
}
