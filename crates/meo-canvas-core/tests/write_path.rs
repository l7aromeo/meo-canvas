//! That writing a file and encoding a buffer answer the same questions.
//!
//! `PreparedEncode::write` exists so a page-spanning format streams into the
//! file rather than being held whole in memory. The risk it brings is not
//! performance: it is that a second path through the encoder can resolve
//! *which pages* differently from the first, and disagree silently. One file
//! with every frame in it is a plausible GIF, and so is one file with one
//! frame; nothing about either says which was asked for.
//!
//! So these compare the two paths rather than checking each alone, and they do
//! it on the case where the two rules collide — an [`EncodeOptions::page`]
//! naming one frame of a format that otherwise gathers them all.
//!
//! **The pages are given different colours on purpose.** Blank pages encode to
//! identical bytes, so a test over them cannot tell "wrote page 1" from "wrote
//! page 2" — it would pass on a path that always wrote the first page, and on
//! one that always wrote the last. The colours are what make the assertion
//! about the named page rather than about the count.

use std::fs;

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{Scene, Size, style::paint::Color};

/// Three pages, each a different flat colour.
fn three_colours() -> Scene {
    let mut scene = Scene::new(Size::new(4.0, 4.0));
    let colours = [
        Color::rgb(255, 0, 0),
        Color::rgb(0, 255, 0),
        Color::rgb(0, 0, 255),
    ];

    scene.nodes[0].paint.background_color = colours[0];
    for colour in &colours[1..] {
        let page = scene
            .push_page()
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(page) {
            node.paint.background_color = *colour;
        }
    }
    assert_eq!(scene.pages.len(), 3);
    scene
}

/// How many frames a GIF carries.
fn frames(bytes: &[u8]) -> usize {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder = options
        .read_info(bytes)
        .unwrap_or_else(|error| unreachable!("not a GIF: {error}"));
    let mut count = 0;
    while decoder
        .read_next_frame()
        .unwrap_or_else(|error| unreachable!("{error}"))
        .is_some()
    {
        count += 1;
    }
    count
}

/// A path in a directory this test owns and removes.
fn scratch(name: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join("meo-canvas-write-path");
    fs::create_dir_all(&directory)
        .unwrap_or_else(|error| unreachable!("{error}"));
    directory.join(name)
}

#[test]
fn the_written_file_is_the_encoded_buffer() {
    let scene = three_colours();
    let renderer = Renderer::new();

    // Every page, one page named, and each of the three named in turn. The
    // last three are what make this a statement about *which* page: with
    // identical pages they would all be the same bytes and prove nothing.
    for page in [None, Some(0), Some(1), Some(2)] {
        let options = EncodeOptions {
            page,
            ..EncodeOptions::default()
        };

        let mut canvas = renderer
            .render(&scene)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let encoded = canvas
            .prepare_encode(ImageFormat::Gif, &options)
            .and_then(meo_canvas_core::PreparedEncode::encode)
            .unwrap_or_else(|error| unreachable!("{page:?}: {error}"));

        let path = scratch(&format!("page-{page:?}.gif"));
        let mut canvas = renderer
            .render(&scene)
            .unwrap_or_else(|error| unreachable!("{error}"));
        canvas
            .prepare_encode(ImageFormat::Gif, &options)
            .and_then(|prepared| prepared.write(&path))
            .unwrap_or_else(|error| unreachable!("{page:?}: {error}"));
        let written =
            fs::read(&path).unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(
            encoded.bytes, written,
            "{page:?}: the file and the buffer are different documents"
        );
        let expected = if page.is_some() { 1 } else { 3 };
        assert_eq!(
            frames(&written),
            expected,
            "{page:?}: the file carries the wrong number of frames"
        );

        fs::remove_file(&path).unwrap_or_else(|error| unreachable!("{error}"));
    }
}

#[test]
fn the_three_pages_are_actually_different_so_the_comparison_can_fail() {
    // The control for the test above, and it earns its place: if the pages
    // encoded alike, `the_written_file_is_the_encoded_buffer` would pass on a
    // writer that always wrote page 0 and on one that always wrote page 2.
    let scene = three_colours();
    let renderer = Renderer::new();

    let bytes = |page: usize| {
        renderer
            .render_to_buffer(
                &scene,
                ImageFormat::Gif,
                &EncodeOptions {
                    page: Some(page),
                    ..EncodeOptions::default()
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"))
    };

    let (first, second, third) = (bytes(0), bytes(1), bytes(2));
    assert_ne!(first, second, "pages 0 and 1 encode alike");
    assert_ne!(second, third, "pages 1 and 2 encode alike");
    assert_ne!(first, third, "pages 0 and 2 encode alike");
}

#[test]
fn a_page_past_the_end_is_refused_on_the_writing_path_too() {
    let scene = three_colours();
    let renderer = Renderer::new();
    let mut canvas = renderer
        .render(&scene)
        .unwrap_or_else(|error| unreachable!("{error}"));

    let refused = canvas.prepare_encode(
        ImageFormat::Gif,
        &EncodeOptions {
            page: Some(3),
            ..EncodeOptions::default()
        },
    );
    assert!(
        refused.is_err(),
        "page 3 of a three-page scene was accepted"
    );

    // And nothing was written, because the refusal happens before a path is
    // ever named.
    assert!(!scratch("page-Some(3).gif").exists());
}
