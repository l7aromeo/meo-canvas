//! What a render does when a URL cannot be fetched.
//!
//! **Behind `net`, because the behaviour only exists there.** With the feature
//! off a URL is `Error::UnresolvedSource` -- a statement about the build rather
//! than about the world -- and that is deliberately not softened, so there is
//! nothing here to check. `just net-check` runs this on Linux in CI.
//!
//! Every URL below points at `127.0.0.1:1`, which refuses immediately: a test
//! that waited for a real timeout would be one people learn to skip.
#![cfg(feature = "net")]

use meo_canvas_core::{
    FetchFailure, ImageFormat, Renderer, encode::EncodeOptions,
};
use meo_canvas_scene::{
    OnImageError, Scene, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        paint::{Color, ObjectFit},
    },
};

/// A URL nothing will answer, refused rather than timed out.
const DEAD: &str = "http://127.0.0.1:1/never.png";

/// The colour of the sibling that must survive the failure.
const SIBLING: (u8, u8, u8) = (0, 200, 0);
/// The ground, chosen to share no channel with [`SIBLING`].
const GROUND: (u8, u8, u8) = (40, 0, 90);

/// A row: one image at `source`, then a coloured box after it.
fn row(source: ImageSource, policy: OnImageError) -> Scene {
    let mut scene = Scene::new(Size::new(120.0, 60.0));
    scene.on_image_error = policy;
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = Color::rgb(GROUND.0, GROUND.1, GROUND.2);
        root.layout.flex_direction =
            meo_canvas_scene::style::layout::FlexDirection::Row;
    }
    let image = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Image {
                source,
                frame: None,
                fit: ObjectFit::Fill,
                position: (Length::ZERO, Length::ZERO),
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(image) {
        node.layout.size = (Dimension::Points(40.0), Dimension::Points(40.0));
    }
    let after = scene
        .push(NodeId::ROOT, Node::new(NodeKind::Box))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(after) {
        node.layout.size = (Dimension::Points(40.0), Dimension::Points(40.0));
        node.paint.background_color =
            Color::rgb(SIBLING.0, SIBLING.1, SIBLING.2);
    }
    scene
}

/// A 2x2 PNG that decodes, for the control.
fn working_bytes() -> Vec<u8> {
    let mut scene = Scene::new(Size::new(2.0, 2.0));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = Color::rgb(255, 255, 255);
    }
    Renderer::new()
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// Every pixel of a rendered PNG, as `(r, g, b)`.
fn pixels(png: &[u8]) -> (usize, Vec<(u8, u8, u8)>) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8()
            | png::Transformations::ALPHA,
    );
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut buffer = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut buffer)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let out = buffer
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| (p[0], p[1], p[2]))
        .collect::<Vec<_>>();
    (info.width as usize, out)
}

/// Where the sibling's colour appears, as `(x, y)` pairs.
fn sibling_at(png: &[u8]) -> Vec<(usize, usize)> {
    let (width, all) = pixels(png);
    all.iter()
        .enumerate()
        .filter(|(_, p)| **p == SIBLING)
        .map(|(i, _)| (i % width, i / width))
        .collect()
}

#[test]
fn a_dead_url_lets_the_render_finish_and_is_recorded() {
    let scene =
        row(ImageSource::Url(DEAD.to_owned()), OnImageError::Placeholder);
    let rendered = Renderer::new()
        .render(&scene)
        .unwrap_or_else(|error| unreachable!("the render failed: {error}"));

    let warnings = rendered.warnings();
    assert_eq!(warnings.len(), 1, "expected one warning, got {warnings:?}");
    assert_eq!(warnings[0].url, DEAD);
    // The reason is a value, not a sentence: a caller branches on this rather
    // than matching a message.
    assert!(
        matches!(
            warnings[0].failure,
            FetchFailure::Transport | FetchFailure::HostNotFound
        ),
        "a refused connection should classify as reachable-but-failed, got {:?}",
        warnings[0].failure
    );
}

#[test]
fn the_sibling_after_a_failing_image_lands_where_it_always_would() {
    // The property the report actually depends on: not merely that the render
    // finished, but that the node *after* the failure is painted exactly where
    // a working image of the same size would have put it.
    let dead = Renderer::new()
        .render_to_buffer(
            &row(ImageSource::Url(DEAD.to_owned()), OnImageError::Placeholder),
            ImageFormat::Png,
            &EncodeOptions::default(),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    let working = Renderer::new()
        .render_to_buffer(
            &row(
                ImageSource::Bytes(working_bytes()),
                OnImageError::Placeholder,
            ),
            ImageFormat::Png,
            &EncodeOptions::default(),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));

    let (dead_at, working_at) = (sibling_at(&dead), sibling_at(&working));
    assert!(
        !working_at.is_empty(),
        "the control drew no sibling, so this comparison checks nothing"
    );
    assert_eq!(
        dead_at, working_at,
        "the sibling moved when the image before it failed"
    );
}

#[test]
fn throw_is_the_behaviour_every_earlier_version_had() {
    let Err(error) = Renderer::new()
        .render(&row(ImageSource::Url(DEAD.to_owned()), OnImageError::Throw))
    else {
        unreachable!("a dead URL under `throw` must fail the render")
    };
    assert!(
        format!("{error}").contains(DEAD),
        "the error should name the URL: {error}"
    );
}

#[test]
fn ignore_draws_nothing_and_still_records_the_warning() {
    let scene = row(ImageSource::Url(DEAD.to_owned()), OnImageError::Ignore);
    let rendered = Renderer::new()
        .render(&scene)
        .unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(
        rendered.warnings().len(),
        1,
        "turning the drawing off must not turn the knowing off"
    );
}

#[test]
fn ignore_and_placeholder_differ_only_in_what_is_drawn() {
    let render = |policy| {
        Renderer::new()
            .render_to_buffer(
                &row(ImageSource::Url(DEAD.to_owned()), policy),
                ImageFormat::Png,
                &EncodeOptions::default(),
            )
            .unwrap_or_else(|error| unreachable!("{error}"))
    };
    assert_ne!(
        render(OnImageError::Ignore),
        render(OnImageError::Placeholder),
        "`placeholder` must actually draw something `ignore` does not"
    );
}

#[test]
fn a_path_that_cannot_be_read_still_fails_whatever_the_policy_says() {
    // The caller is holding this input and can check it before rendering, so
    // it is not the engine's to soften -- and softening it would make the
    // silent path reachable with no network in it at all.
    assert!(
        Renderer::new()
            .render(&row(
                ImageSource::Path("/no/such/file.png".to_owned()),
                OnImageError::Placeholder,
            ))
            .is_err(),
        "an unreadable path must fail even under `placeholder`"
    );
}

#[test]
fn bytes_that_will_not_decode_still_fail_whatever_the_policy_says() {
    assert!(
        Renderer::new()
            .render(&row(
                ImageSource::Bytes(vec![0, 1, 2, 3, 4, 5, 6, 7]),
                OnImageError::Placeholder,
            ))
            .is_err(),
        "undecodable bytes must fail even under `placeholder`"
    );
}

/// A single failing image of `size`, alone on a ground it must not touch.
fn alone(size: Option<f32>, radius: f32) -> Vec<u8> {
    let mut scene = Scene::new(Size::new(60.0, 60.0));
    scene.on_image_error = OnImageError::Placeholder;
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = Color::rgb(GROUND.0, GROUND.1, GROUND.2);
        root.layout.padding = meo_canvas_scene::Sides {
            top: Length::Points(10.0),
            right: Length::Points(10.0),
            bottom: Length::Points(10.0),
            left: Length::Points(10.0),
        };
    }
    let image = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Image {
                source: ImageSource::Url(DEAD.to_owned()),
                frame: None,
                fit: ObjectFit::Fill,
                position: (Length::ZERO, Length::ZERO),
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(image) {
        if let Some(size) = size {
            node.layout.size =
                (Dimension::Points(size), Dimension::Points(size));
        }
        node.paint.border_radius = meo_canvas_scene::Corners {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        };
    }
    Renderer::new()
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// Every pixel that is not the ground, as `(x, y)`.
fn marked(png: &[u8]) -> Vec<(usize, usize)> {
    let (width, all) = pixels(png);
    all.iter()
        .enumerate()
        .filter(|(_, p)| **p != GROUND)
        .map(|(i, _)| (i % width, i / width))
        .collect()
}

#[test]
fn the_placeholder_stays_inside_the_box_it_was_given() {
    // 10px of padding all round a 40px box, so the mark's own rectangle is
    // 10..50 on both axes and every pixel outside it must still be the
    // ground. A mark that overflowed by a pixel fails here rather than in a
    // screenshot somebody has to look at.
    let painted = marked(&alone(Some(40.0), 0.0));
    assert!(
        !painted.is_empty(),
        "nothing was drawn at all, so this test would pass on a no-op"
    );
    let stray: Vec<_> = painted
        .iter()
        .filter(|(x, y)| !(10..50).contains(x) || !(10..50).contains(y))
        .collect();
    assert!(
        stray.is_empty(),
        "the placeholder painted outside its box at {:?}",
        &stray[..stray.len().min(8)]
    );
}

#[test]
fn a_box_with_no_extent_is_drawn_as_nothing() {
    // `auto` on both axes with no bitmap to size it collapses to 0x0, and
    // Chrome draws nothing there. Drawing a mark would invent an extent
    // layout did not give the node.
    assert_eq!(
        marked(&alone(None, 0.0)),
        Vec::new(),
        "a collapsed box should be untouched"
    );
}

#[test]
fn a_radius_clips_the_placeholder() {
    // The corners of a 40px box with a 20px radius are outside the rounded
    // shape, so a mark clipped to it leaves them as ground. Without the clip
    // the wash fills the square and the corner is painted.
    let (width, all) = pixels(&alone(Some(40.0), 20.0));
    let corner = all[11 * width + 11];
    assert_eq!(
        corner, GROUND,
        "the corner outside a 20px radius was painted {corner:?}"
    );
    // And the middle is painted, so the comparison above is about the clip
    // rather than about nothing having been drawn.
    let middle = all[30 * width + 30];
    assert_ne!(middle, GROUND, "nothing was drawn inside the rounded box");
}
