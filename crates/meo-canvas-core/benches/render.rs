//! What a render costs, on a tree shaped like a real one.
//!
//! An instrument rather than a gate: it answers "what is this worth" and never
//! "is this correct". It is outside the `ci` chain because a number that varies
//! with the machine cannot fail a build honestly, and the golden fixtures
//! already say whether a change moved a pixel.
//!
//! Two things it exists to answer. What a proposed allocation fix is actually
//! worth against paint and encode, so a tidier allocator profile is not
//! mistaken for a faster renderer. And the question AGENTS.md records as open:
//! how much re-laying-out a prepared paragraph saves against rebuilding it,
//! which has never been a number.
//!
//! The GPU is off. A bench whose backend depends on which features a build
//! happened to compile measures the build rather than the change.
//!
//! Setup failures abort with `unreachable!` rather than returning: a bench that
//! cannot build its scene has nothing to measure, and `clippy::panic` is denied
//! across the workspace while `unreachable` says the same thing about a case
//! the setup rules out.

use std::{hint::black_box, path::PathBuf};

use criterion::{Criterion, criterion_group, criterion_main};
use meo_canvas_core::{EncodeOptions, ImageFormat, Renderer};
use meo_canvas_scene::{
    Corners, Scene, Sides, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        layout::{Align, Display, FlexDirection, LayoutStyle},
        paint::{Color, ObjectFit, PaintStyle},
        text::TextStyle,
    },
};

/// The family the bench registers, and the only one its scenes may name.
///
/// A scene naming a platform face would measure this machine's font stack.
const FAMILY: &str = "Bench";

/// How many rows the tree has.
///
/// Twelve rows of eight children plus their containers is a little over four
/// hundred nodes, which is the order a real page reaches: v1's own report cards
/// and feature sheets sit in the hundreds. Small enough that a run finishes,
/// large enough that a per-node cost is visible against the fixed cost of
/// surface allocation and encoding.
const ROWS: usize = 12;

/// How many children each row holds.
const PER_ROW: usize = 8;

/// A 4x2 opaque red PNG, written out rather than read from disk so the bench
/// measures decoding and not the filesystem.
const RED_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
    0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x02,
    0x08, 0x06, 0x00, 0x00, 0x00, 0x7F, 0xA8, 0x7D, 0x63, 0x00, 0x00, 0x00,
    0x12, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x19, 0x33, 0xA0, 0x0B, 0x00, 0x00, 0x0F, 0x21, 0x0F, 0xF1, 0xFE,
    0x45, 0x14, 0x63, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn font_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets/fonts/Oswald-VariableFont_wght.ttf")
}

fn renderer() -> Renderer {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .register_font(FAMILY, font_path())
        .unwrap_or_else(|error| unreachable!("{error}"));
    renderer
}

/// A page of rows: text beside boxes, with two images and some styling, which
/// is the mix a real document has rather than one node kind repeated.
fn realistic_scene() -> Scene {
    let mut scene = Scene::new(Size::new(900.0, 1_200.0));
    scene.nodes[0].layout = LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        padding: Sides::all(Length::Points(16.0)),
        gap: (Length::Points(10.0), Length::ZERO),
        ..LayoutStyle::default()
    };
    scene.nodes[0].paint.background_color = Color::rgb(255, 255, 255);
    scene.nodes[0].text = TextStyle {
        font_family: Some(FAMILY.to_owned()),
        font_size: Some(15.0),
        ..TextStyle::default()
    };

    for row_index in 0..ROWS {
        let row = scene
            .push(NodeId::ROOT, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(row) {
            node.layout = LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: Some(Align::Center),
                gap: (Length::ZERO, Length::Points(8.0)),
                padding: Sides::all(Length::Points(6.0)),
                ..LayoutStyle::default()
            };
            node.paint = PaintStyle {
                background_color: Color::rgb(246, 247, 250),
                border_radius: Corners::all(6.0),
                ..PaintStyle::default()
            };
        }

        for column in 0..PER_ROW {
            // Every third child is a box rather than a run of text, so the
            // measure pass is exercised without the tree being nothing else.
            let child = if column % 3 == 0 {
                Node::container()
            } else {
                Node::text(format!("row {row_index} cell {column}"))
            };
            let id = scene
                .push(row, child)
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id)
                && matches!(node.kind, NodeKind::Box)
            {
                node.layout.size =
                    (Dimension::Points(40.0), Dimension::Points(18.0));
                node.paint.background_color = Color::rgb(210, 220, 235);
            }
        }
    }

    // Two images, which is what makes `resolve` do more than walk the tree.
    for _ in 0..2 {
        scene
            .push(
                NodeId::ROOT,
                Node::new(NodeKind::Image {
                    source: ImageSource::Bytes(RED_PNG.to_vec()),
                    fit: ObjectFit::Contain,
                    position: (Length::Percent(0.5), Length::Percent(0.5)),
                    frame: None,
                }),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
    }

    scene
}

/// A tree the size a large page reaches, for the passes whose cost is per node.
///
/// The pipeline benches stay on the smaller tree so a run finishes; the
/// allocation-sensitive passes are measured here as well, so a per-node claim
/// is extrapolated from two points rather than from one.
fn large_scene() -> Scene {
    let mut scene = realistic_scene();
    let rows: Vec<NodeId> = scene.nodes[0].children.clone();
    for _ in 0..4 {
        for &row in &rows {
            let copy = scene
                .push(NodeId::ROOT, Node::container())
                .unwrap_or_else(|error| unreachable!("{error}"));
            let children: Vec<NodeId> = scene
                .get(row)
                .map(|node| node.children.clone())
                .unwrap_or_default();
            for child in children {
                let kind = scene
                    .get(child)
                    .map_or(NodeKind::Box, |node| node.kind.clone());
                scene
                    .push(copy, Node::new(kind))
                    .unwrap_or_else(|error| unreachable!("{error}"));
            }
        }
    }
    scene
}

fn benches(c: &mut Criterion) {
    let renderer = renderer();
    let scene = realistic_scene();
    let nodes = scene.len();

    let mut group = c.benchmark_group("render");
    group.sample_size(20);

    // The whole pipeline, which is the number every other number is a fraction
    // of. Named with the node count so a comparison across changes cannot
    // silently compare different trees.
    group.bench_function(format!("pipeline/{nodes}-nodes"), |b| {
        b.iter(|| {
            let mut canvas = renderer
                .render(black_box(&scene))
                .unwrap_or_else(|error| unreachable!("{error}"));
            black_box(
                canvas
                    .to_buffer(ImageFormat::Png, &EncodeOptions::default())
                    .unwrap_or_else(|error| unreachable!("{error}")),
            )
        });
    });

    // Render without the encode, so the encode's share is the difference
    // between this and the line above.
    group.bench_function(format!("draw/{nodes}-nodes"), |b| {
        b.iter(|| {
            black_box(
                renderer
                    .render(black_box(&scene))
                    .unwrap_or_else(|error| unreachable!("{error}")),
            )
        });
    });

    // A second encode of an already-painted surface: what the render/encode
    // split buys, and the floor any allocation fix is measured against.
    group.bench_function("re-encode", |b| {
        let mut canvas = renderer
            .render(&scene)
            .unwrap_or_else(|error| unreachable!("{error}"));
        b.iter(|| {
            black_box(
                canvas
                    .to_buffer(ImageFormat::Png, &EncodeOptions::default())
                    .unwrap_or_else(|error| unreachable!("{error}")),
            )
        });
    });

    // The passes on their own, so a change to one is measured against its own
    // cost rather than against the pipeline it is a fraction of.
    group.bench_function(format!("resolve/{nodes}-nodes"), |b| {
        b.iter(|| {
            black_box(
                meo_canvas_core::Resolved::new(
                    black_box(&scene),
                    renderer.fonts(),
                )
                .unwrap_or_else(|error| unreachable!("{error}")),
            )
        });
    });

    // The same pass on a page-sized tree, so "per node" is two points.
    let large = large_scene();
    let large_nodes = large.len();
    group.bench_function(format!("resolve/{large_nodes}-nodes"), |b| {
        b.iter(|| {
            black_box(
                meo_canvas_core::Resolved::new(
                    black_box(&large),
                    renderer.fonts(),
                )
                .unwrap_or_else(|error| unreachable!("{error}")),
            )
        });
    });

    // The z-order step, written here as paint writes it, so the operation is
    // measured rather than the pass around it. Private in `paint`, so the
    // bench reproduces it against the same data rather than reaching in.
    group.bench_function(format!("z-order/{large_nodes}-nodes"), |b| {
        b.iter(|| {
            let mut total = 0_usize;
            for node in &black_box(&large).nodes {
                let mut children = node.children.clone();
                children.sort_by_key(|child| {
                    large.get(*child).map_or(0, |c| c.paint.z_index)
                });
                total += children.len();
            }
            black_box(total)
        });
    });

    group.finish();
}

criterion_group!(render, benches);
criterion_main!(render);
