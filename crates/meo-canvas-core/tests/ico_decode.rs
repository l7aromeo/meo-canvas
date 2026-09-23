//! An ICO handed back to us decodes to the pixels we put in it. `encode.rs`
//! reads back the directory it writes, and nothing else reads an ICO in. Built
//! by our own encoder, for the reason `shared_decode.rs` gives.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        paint::{Color, ObjectFit},
    },
};

/// The colour the icon is filled with, and the one the render must show.
const ICON: (u8, u8, u8) = (192, 57, 43);

/// The colour behind the icon, differing from [`ICON`] on every channel, so an
/// ICO that decoded to nothing or to transparency shows as a failure.
const BEHIND: (u8, u8, u8) = (0, 128, 255);

/// A single-page ICO of flat [`ICON`], written by this crate's own encoder.
fn icon() -> Vec<u8> {
    let mut scene = Scene::new(Size::new(16.0, 16.0));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = Color::rgb(ICON.0, ICON.1, ICON.2);
    }
    Renderer::new()
        .render_to_buffer(&scene, ImageFormat::Ico, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("the ico did not encode: {error}"))
}

/// The centre pixel of a PNG, as `(r, g, b)`.
fn centre(png: Vec<u8>) -> (u8, u8, u8) {
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
    let (width, height) = (info.width as usize, info.height as usize);
    let start = (((height / 2) * width) + (width / 2)) * 4;
    (pixels[start], pixels[start + 1], pixels[start + 2])
}

#[test]
fn an_ico_we_wrote_decodes_back_to_the_colour_we_put_in_it() {
    let bytes = icon();
    assert!(
        !bytes.is_empty(),
        "the encoder produced no bytes, so nothing below is decoding anything"
    );

    let mut scene = Scene::new(Size::new(16.0, 16.0));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = Color::rgb(BEHIND.0, BEHIND.1, BEHIND.2);
    }
    let id = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Image {
                source: ImageSource::Bytes(bytes),
                frame: None,
                fit: ObjectFit::Fill,
                position: (Length::ZERO, Length::ZERO),
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size = (Dimension::Points(16.0), Dimension::Points(16.0));
    }

    let png = Renderer::new()
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| {
            unreachable!("the png did not encode: {error}")
        });

    // **The colour, not merely the absence of an error.** A decoder that
    // returned an empty or transparent image would leave `BEHIND` showing and
    // a render that threw would never reach here -- so this separates "decoded
    // the pixels" from both of the ways it could fail quietly.
    assert_eq!(
        centre(png),
        ICON,
        "the ICO did not decode back to the colour it was written with"
    );
}
