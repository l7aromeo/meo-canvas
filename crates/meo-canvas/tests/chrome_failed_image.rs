//! A failed image's box against Chrome's `<img>` with no `alt` at the same
//! size and in the same container (`l7aromeo/meo-canvas#183`), through
//! resolve, measure and layout. The loaded rows are the control.

use meo_canvas::{
    Align, Box, Display, Element, FlexDirection, Image, PositionType, Root,
    Styled, TrackSize, px, sides,
};
use meo_canvas_core::{
    Fonts, ImageFormat, Renderer, encode::EncodeOptions, layout::solve,
    measure::SceneMeasurer, resolve::Resolved,
};
use meo_canvas_scene::{
    Scene, Size,
    node::{NodeId, NodeKind},
    style::paint::Color,
    surface::{ImageFetchAttempt, ImageFetchFailure},
};

const TABLE: &str = include_str!("assets/chrome/failed-image.tsv");

/// The URL every failed row names, recorded below as a fetch that failed.
const URL: &str = "https://origin.invalid/logo.png";

/// The status the table's failing source answered.
const STATUS: u16 = 522;

/// The container's width, and a grid's and an absolute container's height,
/// as in the table.
const CONTAINER: (f32, f32) = (300.0, 200.0);

/// The loaded control's intrinsic size, as in the table.
const LOADED: (f32, f32) = (64.0, 32.0);

/// The insets of the `absolute insets` rows.
const INSET: f32 = 10.0;

/// A picture that decodes, at the control's size.
fn loaded_png() -> Vec<u8> {
    let mut scene = Scene::new(Size::new(LOADED.0, LOADED.1));
    scene.nodes[0].paint.background_color = Color::rgb(0, 0, 0).into();
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"))
}

fn extent(text: &str) -> Option<f32> {
    (text != "-").then(|| {
        text.parse::<f32>()
            .unwrap_or_else(|error| unreachable!("`{text}`: {error}"))
    })
}

/// The image's laid-out size in one cell of the table.
fn laid_out(
    container: &str,
    size: (Option<f32>, Option<f32>),
    image: Element,
) -> (f32, f32) {
    let mut image = image;
    if let Some(width) = size.0 {
        image = image.width(px(width));
    }
    if let Some(height) = size.1 {
        image = image.height(px(height));
    }
    let mut boxed = Box::new().width(px(CONTAINER.0));
    match container {
        "block" => boxed = boxed.display(Display::Block),
        "flex column" | "flex row" => {
            let direction = if container == "flex row" {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            };
            boxed = boxed
                .display(Display::Flex)
                .flex_direction(direction)
                .align_items(Align::FlexStart);
        }
        "flex column stretch" => {
            boxed = boxed
                .display(Display::Flex)
                .flex_direction(FlexDirection::Column);
        }
        "grid" => {
            boxed = boxed
                .display(Display::Grid)
                .height(px(CONTAINER.1))
                .grid_template_columns(vec![TrackSize::Points(CONTAINER.0)]);
        }
        "absolute" | "absolute insets" => {
            boxed = boxed
                .display(Display::Block)
                .position_type(PositionType::Relative)
                .height(px(CONTAINER.1));
            image = image.position_type(PositionType::Absolute);
            if container == "absolute insets" {
                image = image.position(sides(
                    None,
                    Some(px(INSET)),
                    None,
                    Some(px(INSET)),
                ));
            }
        }
        other => unreachable!("a container this test does not build: {other}"),
    }
    let root = Root::new(CONTAINER.0)
        .height(CONTAINER.0)
        .display(Display::Block)
        .children(boxed.children(image));
    let (mut scene, _) = root
        .into_scene()
        .unwrap_or_else(|error| unreachable!("{error}"));
    scene.image_fetch_attempts = vec![ImageFetchAttempt {
        url: URL.into(),
        failure: ImageFetchFailure::Status(STATUS),
        detail: format!("{STATUS}"),
    }];
    let id = scene
        .nodes
        .iter()
        .position(|node| matches!(node.kind, NodeKind::Image { .. }))
        .and_then(|index| u32::try_from(index).ok())
        .map_or_else(|| unreachable!("the scene holds an image"), NodeId::new);

    let fonts = Fonts::new();
    let resolved = Resolved::new(&scene, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let page = scene.pages[0];
    let layout = solve(&scene, page, &mut measurer)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let rect = layout
        .get(id)
        .unwrap_or_else(|| unreachable!("the image was laid out"));
    (rect.size.width, rect.size.height)
}

#[test]
fn a_failed_image_takes_the_box_chrome_gives_a_broken_img_with_no_alt() {
    let loaded = loaded_png();
    let mut wrong = Vec::new();
    let mut rows = 0;
    for line in TABLE.lines().filter(|line| !line.starts_with('#')) {
        let columns: Vec<&str> = line.split('\t').collect();
        let [source, container, width, height, w, h] = columns[..] else {
            unreachable!("a row without its six columns: {line}");
        };
        rows += 1;
        let image = match source {
            "failed" => Image::url(URL),
            "loaded" => Image::bytes(loaded.clone()),
            other => unreachable!("a source this test does not read: {other}"),
        };
        let want = (extent(w), extent(h));
        let got = laid_out(container, (extent(width), extent(height)), image);
        let matches = |got: f32, want: Option<f32>| {
            want.is_some_and(|want| (got - want).abs() < 0.5)
        };
        if !matches(got.0, want.0) || !matches(got.1, want.1) {
            wrong.push(format!("{line}\n    got {got:?}"));
        }
    }
    // Seven containers, four sizes, failed and loaded: a table that lost its
    // rows would pass.
    assert_eq!(rows, 56, "the table has {rows} rows");
    assert!(
        wrong.is_empty(),
        "{} of {rows} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
