//! That an ICO handed back to us decodes to the pixels we put in it.
//!
//! **The encode side was covered and the decode side was not.** `encode.rs`
//! reads an ICO's directory back out of the bytes it wrote -- header, count,
//! and each entry's size -- which is real coverage of the writer. Nothing read
//! an ICO *in*: there is no `.ico` anywhere in this repository, and the image
//! assets that exist as inputs are PNG and GIF.
//!
//! That gap mattered when `meo-skia-canvas` 0.14.0 moved ICO decoding out of
//! Skia and into a Rust decoder, because the path that changed was exactly the
//! one nothing exercised. An input source reaches a decoder through
//! `ImageSource::Bytes` the same way a `src` does, so the change is reachable
//! from an ordinary scene.
//!
//! Built by our own encoder rather than committed as an asset, for the reason
//! `shared_decode.rs` gives: bytes we wrote are bytes we can decode by
//! definition, and a hand-rolled fixture is a second thing that can be wrong.

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

/// The colour behind it, chosen to differ from [`ICON`] on every channel.
///
/// **This is what makes the assertion able to fail.** An ICO that decoded to
/// nothing, or to transparency, leaves the background showing -- so the test
/// distinguishes "decoded" from "did not throw" only because the two colours
/// share no channel.
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
