//! Renders a scene and decodes the bytes back.
//!
//! The only assertion that means anything about an encoder: a byte length
//! proves nothing, because a one-frame GIF and a three-frame GIF are both some
//! bytes. These decode the output and count what is in it.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{Scene, Size};

/// A scene of one page at a known size, drawing nothing.
fn blank(width: f32, height: f32) -> Scene {
    Scene::new(Size::new(width, height))
}

#[test]
fn a_png_decodes_to_the_scenes_pixel_size() {
    let scene = blank(8.0, 4.0);
    let image = Renderer::new()
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let decoder = png::Decoder::new(std::io::Cursor::new(&image));
    let reader = decoder.read_info().unwrap_or_else(|error| {
        unreachable!("the bytes are not a PNG: {error}")
    });
    let info = reader.info();

    assert_eq!((info.width, info.height), (8, 4));
}

#[test]
fn the_scale_multiplies_the_pixels_and_not_the_layout() {
    let mut scene = blank(8.0, 4.0);
    scene.scale = 2.0;

    let image = Renderer::new()
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let decoder = png::Decoder::new(std::io::Cursor::new(&image));
    let reader = decoder.read_info().unwrap_or_else(|error| {
        unreachable!("the bytes are not a PNG: {error}")
    });

    assert_eq!((reader.info().width, reader.info().height), (16, 8));
}

#[test]
fn a_gif_carries_one_frame_per_page() {
    let mut scene = blank(4.0, 4.0);
    scene
        .push_page()
        .unwrap_or_else(|error| unreachable!("{error}"));
    scene
        .push_page()
        .unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(scene.pages.len(), 3);

    let image = Renderer::new()
        .render_to_buffer(&scene, ImageFormat::Gif, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder =
        options.read_info(image.as_slice()).unwrap_or_else(|error| {
            unreachable!("the bytes are not a GIF: {error}")
        });

    let mut frames = 0;
    while decoder
        .read_next_frame()
        .unwrap_or_else(|error| unreachable!("{error}"))
        .is_some()
    {
        frames += 1;
    }

    assert_eq!(frames, 3, "one frame per page");
}

#[test]
fn a_still_format_writes_one_page_of_a_multi_page_scene() {
    let mut scene = blank(4.0, 4.0);
    scene
        .push_page()
        .unwrap_or_else(|error| unreachable!("{error}"));

    let image = Renderer::new()
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let decoder = png::Decoder::new(std::io::Cursor::new(&image));
    let reader = decoder.read_info().unwrap_or_else(|error| {
        unreachable!("the bytes are not a PNG: {error}")
    });

    assert_eq!((reader.info().width, reader.info().height), (4, 4));
}

#[test]
fn a_frame_rate_named_for_a_png_is_refused_before_anything_is_drawn() {
    let scene = blank(4.0, 4.0);
    let options = EncodeOptions {
        fps: Some(24.0),
        ..EncodeOptions::default()
    };

    let refused =
        Renderer::new().render_to_buffer(&scene, ImageFormat::Png, &options);

    assert!(refused.is_err(), "a PNG has no clock");
}
