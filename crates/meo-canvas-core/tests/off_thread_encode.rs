//! What `prepare_encode` promises, each claim able to fail: identical bytes
//! rather than equivalent ones; the calling thread's registered face survives,
//! since fonts are per thread and paint is lazy; and the canvas stays usable
//! afterwards.

use std::{path::PathBuf, thread};

use meo_canvas_core::{EncodeOptions, ImageFormat, Renderer};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId},
    style::text::TextStyle,
};

/// The family the tests register. Not a platform face: a scene naming one
/// would measure whichever fonts this machine happens to have.
const FAMILY: &str = "OffThread";

// The control is text with no family, drawn by the platform's face:
// registration is per thread, so a bare renderer here still finds `FAMILY`, and
// an unregistered family is refused by `Resolved::new`. If the two faces agree
// pixel for pixel, the assertion fails.

fn font_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets/fonts/Oswald-VariableFont_wght.ttf")
}

/// A renderer with the test face registered, or without it.
///
/// The GPU is off in both. A test whose backend depends on which features a
/// build happened to compile is a test of the build.
fn renderer(with_font: bool) -> Renderer {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    if with_font {
        renderer
            .register_font(FAMILY, font_path())
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    renderer
}

/// Text large enough that a substituted face changes the pixels.
fn text_scene(pages: usize) -> Scene {
    scene_in(pages, Some(FAMILY))
}

/// The same scene under a named family, or under none.
fn scene_in(pages: usize, family: Option<&str>) -> Scene {
    let mut scene = Scene::new(Size::new(320.0, 120.0));
    for page in 0..pages {
        let root = if page == 0 {
            NodeId::ROOT
        } else {
            scene
                .push_page()
                .unwrap_or_else(|error| unreachable!("{error}"))
        };
        let leaf = scene
            .push(root, Node::text(format!("Handoff {page}")))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(leaf) {
            node.text = TextStyle {
                font_family: family.map(str::to_owned),
                font_size: Some(48.0),
                ..TextStyle::default()
            };
        }
    }
    scene
}

#[test]
fn a_handle_encoded_on_a_worker_gives_the_bytes_the_canvas_would_have() {
    let renderer = renderer(true);
    let scene = text_scene(1);

    // Every format the addon can be asked for that is not vector or animated,
    // plus one of each of those: the point of the split is that it is the
    // whole encoder that moved, not the easy half of it.
    for format in [
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::Webp,
        ImageFormat::Raw,
        ImageFormat::Svg,
        ImageFormat::Pdf,
        ImageFormat::Gif,
    ] {
        let options = match format {
            // JPEG has no alpha channel, so it refuses a surface it cannot
            // flatten without being told what to flatten against.
            ImageFormat::Jpeg => EncodeOptions {
                matte: Some(0x00_00_00),
                ..EncodeOptions::default()
            },
            _ => EncodeOptions::default(),
        };

        let mut canvas = renderer
            .render(&scene)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let inline = canvas
            .to_buffer(format, &options)
            .unwrap_or_else(|error| unreachable!("{format}: {error}"));

        let mut canvas = renderer
            .render(&scene)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let prepared = canvas
            .prepare_encode(format, &options)
            .unwrap_or_else(|error| unreachable!("{format}: {error}"));
        let off_thread = thread::spawn(move || prepared.encode())
            .join()
            .unwrap_or_else(|_| unreachable!("the encoding thread panicked"))
            .unwrap_or_else(|error| unreachable!("{format}: {error}"));

        assert_eq!(
            inline, off_thread.bytes,
            "{format}: the two paths wrote different files"
        );
        assert_eq!(off_thread.format, format);
    }
}

#[test]
fn the_worker_draws_the_registered_face_and_not_a_fallback() {
    let scene = text_scene(1);
    let options = EncodeOptions::default();

    let registered = renderer(true);
    let mut canvas = registered
        .render(&scene)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let prepared = canvas
        .prepare_encode(ImageFormat::Png, &options)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let with_face = thread::spawn(move || prepared.encode())
        .join()
        .unwrap_or_else(|_| unreachable!("the encoding thread panicked"))
        .unwrap_or_else(|error| unreachable!("{error}"))
        .bytes;

    // The control: the same text at the same size with no family named, so
    // the platform's own face draws it. See the note above for why it is this
    // and not a renderer with nothing registered.
    let bare = renderer(false);
    let mut canvas = bare
        .render(&scene_in(1, None))
        .unwrap_or_else(|error| unreachable!("{error}"));
    let fallback = canvas
        .to_buffer(ImageFormat::Png, &options)
        .unwrap_or_else(|error| unreachable!("{error}"));

    assert_ne!(
        with_face, fallback,
        "the registered face and the platform's drew the same pixels, so this \
         test cannot tell them apart"
    );

    // And the off-thread bytes are the registered face's, which is the claim.
    let mut canvas = registered
        .render(&scene)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let inline = canvas
        .to_buffer(ImageFormat::Png, &options)
        .unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(with_face, inline);
}

#[test]
fn a_handle_does_not_spend_the_canvas() {
    let renderer = renderer(true);
    let scene = text_scene(1);
    let mut canvas = renderer
        .render(&scene)
        .unwrap_or_else(|error| unreachable!("{error}"));

    // Two handles from one canvas, taken for two formats, which is what the
    // type asks a caller to do rather than reusing one.
    let png = canvas
        .prepare_encode(ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));
    let webp = canvas
        .prepare_encode(ImageFormat::Webp, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let png = thread::spawn(move || png.encode())
        .join()
        .unwrap_or_else(|_| unreachable!("the encoding thread panicked"))
        .unwrap_or_else(|error| unreachable!("{error}"));
    let webp = thread::spawn(move || webp.encode())
        .join()
        .unwrap_or_else(|_| unreachable!("the encoding thread panicked"))
        .unwrap_or_else(|error| unreachable!("{error}"));

    assert_eq!(png.format, ImageFormat::Png);
    assert_eq!(webp.format, ImageFormat::Webp);
    assert_ne!(png.bytes, webp.bytes);

    // And the canvas is still there to encode from, which is the property
    // that makes two formats cost one paint.
    let third = canvas
        .to_buffer(ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(third, png.bytes);
}

#[test]
fn the_options_are_refused_before_the_snapshot_rather_than_after() {
    let renderer = renderer(true);
    let scene = text_scene(1);
    let mut canvas = renderer
        .render(&scene)
        .unwrap_or_else(|error| unreachable!("{error}"));

    // A frame rate means nothing to a PNG, and saying so is `prepare_encode`'s
    // job rather than the worker's: a caller who wrote it should learn at the
    // call that named it, while there is still a stack to throw from.
    let refused = canvas.prepare_encode(
        ImageFormat::Png,
        &EncodeOptions {
            fps: Some(30.0),
            ..EncodeOptions::default()
        },
    );
    assert!(
        refused.is_err(),
        "a frame rate on a still format reached the worker"
    );

    // The same options through the folded call fail identically, so the split
    // did not move where a caller learns about a bad argument.
    let folded = canvas.to_buffer(
        ImageFormat::Png,
        &EncodeOptions {
            fps: Some(30.0),
            ..EncodeOptions::default()
        },
    );
    assert!(folded.is_err());
}

#[test]
fn a_handle_reports_the_pages_it_holds() {
    let renderer = renderer(true);
    let scene = text_scene(3);
    let mut canvas = renderer
        .render(&scene)
        .unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(canvas.page_count(), 3);

    let prepared = canvas
        .prepare_encode(ImageFormat::Gif, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));
    // The pages the call selected, which for a handle with no range named is
    // every page the canvas holds.
    assert_eq!(prepared.page_count(), 3);
    assert_eq!(prepared.format(), ImageFormat::Gif);
}
