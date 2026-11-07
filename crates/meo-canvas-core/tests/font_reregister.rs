//! Registering a face that is already registered, as a caller naming the same
//! fonts on every render does, leaves what a render draws unchanged.
use meo_canvas_core::{Fonts, ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Sides, Size,
    node::Node,
    style::{
        Length,
        paint::Color,
        text::{FontStyle, FontWeight, Spacing},
    },
};

const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/fonts/Oswald-VariableFont_wght.ttf"
);

/// A family nothing else in the workspace registers.
const FAMILY: &str = "MeoReregisterProbe";

/// How many renders, each registering the face again. A registry that adds a
/// typeface per registration moves Oswald's pixels at 96px and scale 2.4 on the
/// GPU from the fourth on (`l7aromeo/meo-skia-canvas#611`); twelve go past.
const RENDERS: usize = 12;

/// Large glyphs at a device scale: 96px at 2.4 moves, 48px at 2.4 does not.
const FONT_SIZE: f32 = 96.0;
const SCALE: f32 = 2.4;

fn scene() -> Scene {
    let mut scene = Scene::new(Size::new(420.0, 140.0));
    scene.scale = SCALE;
    scene.gpu = Some(true);
    scene.nodes[0].layout.padding = Sides::all(Length::Points(16.0));
    scene.nodes[0].paint.background_color = Color::rgb(0xEC, 0xEE, 0xF3);
    let root = scene
        .root()
        .unwrap_or_else(|| unreachable!("a new scene has a root"));
    let mut node = Node::text("N.T.E");
    node.text.font_family = Some(FAMILY.to_owned());
    node.text.font_size = Some(FONT_SIZE);
    node.text.font_weight = Some(FontWeight::new(900));
    node.text.font_style = Some(FontStyle::Italic);
    node.text.letter_spacing = Some(Spacing::Points(-2.0));
    node.text.color = Some(Color::rgb(0x3A, 0x4F, 0xD8));
    scene
        .push(root, node)
        .unwrap_or_else(|error| unreachable!("{error}"));
    scene
}

// The raster backend draws the same pixels however often a face is
// registered, so this checks something only on the GPU engine: macOS with the
// `metal` feature has one, and anywhere else it is skipped by name.
#[test]
#[cfg_attr(
    not(all(target_os = "macos", feature = "metal")),
    ignore = "a GPU engine is expected only on macOS with the `metal` feature, and on the CPU this checks nothing"
)]
fn registering_a_registered_face_again_draws_the_same_pixels_on_the_gpu_engine()
{
    let mut first: Option<Vec<u8>> = None;
    let mut differing = Vec::new();
    for render in 0..RENDERS {
        let mut renderer = Renderer::new();
        renderer.set_gpu(true);
        renderer
            .register_font(FAMILY, FONT)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut canvas = renderer
            .render(&scene())
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(
            canvas.engine(),
            "gpu",
            "render {render} came up on the {} engine, where this checks \
             nothing; macOS with `metal` is expected to give the GPU",
            canvas.engine()
        );
        let raw = canvas
            .to_buffer(ImageFormat::Raw, &EncodeOptions::default())
            .unwrap_or_else(|error| unreachable!("{error}"));
        match &first {
            None => first = Some(raw),
            Some(first) if *first != raw => differing.push(render),
            Some(_) => {}
        }
    }
    assert!(
        differing.is_empty(),
        "renders {differing:?} of {RENDERS} differ from the first"
    );
}

#[test]
fn a_second_registry_reports_the_face_it_registered() {
    let family = "MeoReregisterTwice";
    let first = Fonts::new();
    first
        .register_path(family, FONT)
        .unwrap_or_else(|error| unreachable!("{error}"));
    // The same bytes again, which this thread already holds.
    let second = Fonts::new();
    assert!(
        second.registered().is_empty(),
        "a fresh registry reported a face"
    );
    second
        .register_path(family, FONT)
        .unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(second.registered(), vec![family.to_owned()]);
    assert!(second.has(family));
    assert_eq!(first.registered(), vec![family.to_owned()]);
}

#[test]
fn a_file_that_cannot_be_read_reports_what_meo_skia_canvas_reports() {
    let family = "MeoReregisterMissing";
    let missing =
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/fonts/none.ttf");
    let theirs = meo_skia_canvas::FontLibrary::new()
        .register_font_from_path(family, missing)
        .map_or_else(
            |error| error.to_string(),
            |()| unreachable!("{missing} exists"),
        );
    let ours = Fonts::new().register_path(family, missing).map_or_else(
        |error| error.to_string(),
        |()| unreachable!("{missing} exists"),
    );
    assert_eq!(
        ours,
        format!("cannot register font family {family:?}: {theirs}")
    );
    assert!(
        ours.contains(missing),
        "the message does not name the file: {ours}"
    );
}
