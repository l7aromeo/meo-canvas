//! A `RichText` segment's own styling reaching the drawing.
//!
//! # Why this is a whole file
//!
//! Because the defect it guards was silent on both surfaces and the feature
//! demonstrably worked: `fontSize` and `fontWeight` on a segment have always
//! applied, so a caller who tried per-run styling with a bold run and shipped a
//! coloured one got no signal at all. The keys that failed were the ones with
//! no field to travel in -- `RunStyle` carried `family`, `size`, `weight`,
//! `italic` and `variant`, and those are exactly the keys that worked.
//!
//! # Every case is a pair
//!
//! A row asserting only that a styled segment differs from an unstyled one
//! would pass on a renderer that drew nothing at all. So each property is also
//! set at **node** level, which is the path that already worked, and the node
//! row is what proves the scene can show that property before the segment row
//! is allowed to mean anything.

use meo_canvas_core::{
    ImageFormat, Renderer, encode::EncodeOptions, resolve::Fonts,
};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension,
        paint::Color,
        text::{ParagraphStyle, TextDecoration, TextSegment, TextStyle},
    },
};

const FONT: (&str, &str) = (
    "SegmentProbe",
    "tests/assets/fonts/Oswald-VariableFont_wght.ttf",
);

/// The rendered bytes of one paragraph, as a hash.
///
/// A hash rather than a pixel read because the claim is only ever "these two
/// renders differ" or "these two agree"; no row here asserts a colour value.
fn render(segments: Vec<TextSegment>, node: &TextStyle) -> u64 {
    let fonts = Fonts::new();
    fonts
        .register_path(FONT.0, FONT.1)
        .unwrap_or_else(|error| unreachable!("{error}"));

    let mut scene = Scene::new(Size::new(160.0, 60.0));
    let id = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Text {
                segments,
                paragraph: ParagraphStyle::default(),
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node_mut) = scene.get_mut(id) {
        node_mut.layout.size =
            (Dimension::Points(160.0), Dimension::Points(60.0));
        node_mut.text = node.clone();
        node_mut.text.font_family = Some(FONT.0.to_owned());
        node_mut.text.font_size = Some(28.0);
    }

    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let bytes = renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in &bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

/// One segment, styled or not.
fn segment(overlay: TextStyle) -> Vec<TextSegment> {
    vec![TextSegment {
        text: "Ag".to_owned(),
        style: overlay,
    }]
}

/// The node's own style with `white` text, which every row starts from.
fn white() -> TextStyle {
    TextStyle {
        color: Some(Color::rgb(255, 255, 255)),
        ..TextStyle::default()
    }
}

#[test]
fn a_segment_colour_reaches_the_drawing() {
    let plain = render(segment(TextStyle::default()), &white());

    let on_segment = render(
        segment(TextStyle {
            color: Some(Color::rgb(255, 0, 0)),
            ..TextStyle::default()
        }),
        &white(),
    );

    // The control: the same colour at node level, which has always worked. If
    // this row does not move, the scene cannot show a colour at all and the
    // row above says nothing.
    let on_node = render(
        segment(TextStyle::default()),
        &TextStyle {
            color: Some(Color::rgb(255, 0, 0)),
            ..white()
        },
    );

    assert_ne!(
        plain, on_node,
        "the control failed: a node colour changed nothing, so this scene \
         cannot measure a colour"
    );
    assert_ne!(
        plain, on_segment,
        "a segment's colour did not reach the drawing"
    );
}

#[test]
fn a_segment_decoration_reaches_the_drawing() {
    let plain = render(segment(TextStyle::default()), &white());

    let on_segment = render(
        segment(TextStyle {
            text_decoration: Some(TextDecoration::Underline),
            ..TextStyle::default()
        }),
        &white(),
    );

    let on_node = render(
        segment(TextStyle::default()),
        &TextStyle {
            text_decoration: Some(TextDecoration::Underline),
            ..white()
        },
    );

    assert_ne!(
        plain, on_node,
        "the control failed: a node decoration changed nothing"
    );
    assert_ne!(
        plain, on_segment,
        "a segment's decoration did not reach the drawing"
    );
}

#[test]
fn one_styled_segment_leaves_its_neighbours_alone() {
    // **The row that a per-run repair can fail without failing the two above.**
    // Setting the colour on the whole paragraph instead of on the run would
    // satisfy both, because both compare against an unstyled render. Two
    // segments where only the second is styled cannot be satisfied that way.
    let red = TextStyle {
        color: Some(Color::rgb(255, 0, 0)),
        ..TextStyle::default()
    };

    let second_only = render(
        vec![
            TextSegment {
                text: "Ag".to_owned(),
                style: TextStyle::default(),
            },
            TextSegment {
                text: "Ag".to_owned(),
                style: red.clone(),
            },
        ],
        &white(),
    );
    let both = render(
        vec![
            TextSegment {
                text: "Ag".to_owned(),
                style: red.clone(),
            },
            TextSegment {
                text: "Ag".to_owned(),
                style: red,
            },
        ],
        &white(),
    );

    assert_ne!(
        second_only, both,
        "styling one segment coloured both of them, so the colour is being \
         applied to the paragraph rather than to the run"
    );
}
