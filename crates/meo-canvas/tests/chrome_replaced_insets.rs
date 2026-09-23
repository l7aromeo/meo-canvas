//! What insets do to a replaced element (`l7aromeo/meo-canvas#92`): Chrome
//! keeps an image's intrinsic size and drops an inset rather than stretching
//! it. Read from ink, and every replaced row asserts 60x40 exactly, since the
//! defect's rows moved in opposite directions.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        layout::{Align, Display, PositionType},
        paint::{Color, ObjectFit},
    },
};

/// The page. Big enough that a box overflowing its parent is still on it.
const PAGE: f32 = 400.0;

/// The art, and the size every replaced row must come back as.
const ART: (f32, f32) = (60.0, 40.0);

/// The containing block: wider and shorter than the art, so stretching shows.
const BLOCK: (f32, f32) = (200.0, 30.0);

/// Where the block sits, so a negative offset is still on the page.
const ORIGIN: f32 = 100.0;

const INK: (u8, u8, u8) = (204, 0, 0);
const PAPER: (u8, u8, u8) = (255, 255, 255);

/// A 60x40 image, rendered by this renderer from one filled box rather than
/// embedded, so the intrinsic size under test is one the suite can restate.
fn art() -> Vec<u8> {
    let mut scene = Scene::new(Size::new(ART.0, ART.1));
    scene.nodes[0].paint.background_color = Color::rgb(INK.0, INK.1, INK.2);
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// The recorded Chrome rows, which `chrome_tables_are_read` requires be read.
const TABLE: &str = include_str!("assets/chrome/replaced-insets.tsv");

/// Chrome's `(x, y, width, height)` for one case. The columns after the fourth
/// are declarations and a note, prose that is deliberately not read.
fn chrome(case: &str) -> (i32, i32, u32, u32) {
    for line in TABLE.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut columns = line.split('\t');
        if columns.next() != Some(case) {
            continue;
        }
        let mut number = || -> f32 {
            columns
                .next()
                .and_then(|cell| cell.parse::<f32>().ok())
                .unwrap_or_else(|| unreachable!("{case} has a malformed cell"))
        };
        let (x, y, width, height) = (number(), number(), number(), number());
        #[expect(
            clippy::cast_possible_truncation,
            reason = "every recorded extent is a whole number of pixels"
        )]
        return (
            x as i32,
            y as i32,
            width.round() as u32,
            height.round() as u32,
        );
    }
    unreachable!("{case} is not a row of replaced-insets.tsv")
}

fn push(scene: &mut Scene, parent: NodeId, node: Node) -> NodeId {
    scene
        .push(parent, node)
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// The inked rectangle, parent-relative: `(x, y, width, height)`.
fn painted(scene: &Scene) -> (i32, i32, u32, u32) {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let bytes = renderer
        .render_to_buffer(scene, ImageFormat::Raw, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let side = PAGE as u32;
    let (mut left, mut top, mut right, mut bottom) = (side, side, 0_u32, 0_u32);
    let mut seen = false;
    for y in 0..side {
        for x in 0..side {
            let at = ((y * side + x) * 4) as usize;
            if (bytes[at], bytes[at + 1], bytes[at + 2]) == INK {
                seen = true;
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
        }
    }
    if !seen {
        return (0, 0, 0, 0);
    }
    // `cast_signed` rather than `as`: every value here is a pixel index inside
    // a 400-square page, so the conversion cannot wrap -- but `-D warnings`
    // does not know that and an `as` would be the reviewer's problem forever.
    let origin = (ORIGIN as u32).cast_signed();
    (
        left.cast_signed() - origin,
        top.cast_signed() - origin,
        right - left + 1,
        bottom - top + 1,
    )
}

/// A page carrying the containing block at [`ORIGIN`], and the node inside it.
fn scene_with(inner: Node) -> Scene {
    let mut scene = Scene::new(Size::new(PAGE, PAGE));
    scene.nodes[0].paint.background_color =
        Color::rgb(PAPER.0, PAPER.1, PAPER.2);
    scene.nodes[0].layout.padding = meo_canvas_scene::Sides {
        top: Length::Points(ORIGIN),
        right: Length::Points(ORIGIN),
        bottom: Length::Points(ORIGIN),
        left: Length::Points(ORIGIN),
    };

    let mut block = Node::new(NodeKind::Box);
    block.layout.size =
        (Dimension::Points(BLOCK.0), Dimension::Points(BLOCK.1));
    block.layout.position_type = PositionType::Relative;
    // `FlexStart`, or an in-flow child's cross size is the line's: under
    // `stretch` the overflow control reads 200x40 whether or not the renderer
    // clamps.
    block.layout.align_items = Some(Align::FlexStart);
    let block = push(&mut scene, NodeId::ROOT, block);
    push(&mut scene, block, inner);
    scene
}

/// An absolutely positioned image with the given insets.
fn image(insets: [Option<f32>; 4]) -> Node {
    let mut node = Node::new(NodeKind::Image {
        source: ImageSource::Bytes(art()),
        fit: ObjectFit::Fill,
        position: (Length::ZERO, Length::ZERO),
        frame: None,
    });
    node.layout.position_type = PositionType::Absolute;
    node.layout.inset = meo_canvas_scene::Sides {
        top: insets[0].map(Length::Points),
        right: insets[1].map(Length::Points),
        bottom: insets[2].map(Length::Points),
        left: insets[3].map(Length::Points),
    };
    node
}

/// Every shape of inset leaves a replaced element at its intrinsic size, 60x40
/// in Chrome for each row.
#[test]
fn insets_never_size_a_replaced_element() {
    let cases: [(&str, [Option<f32>; 4]); 5] = [
        ("img inset 0", [Some(0.0), Some(0.0), Some(0.0), Some(0.0)]),
        ("img top 0 only", [Some(0.0), None, None, None]),
        ("img left 0 right 0", [None, Some(0.0), None, Some(0.0)]),
        ("img top 0 bottom 0", [Some(0.0), None, Some(0.0), None]),
        ("img no insets", [None, None, None, None]),
    ];
    for (name, insets) in cases {
        let (_, _, width, height) = painted(&scene_with(image(insets)));
        let (_, _, want_width, want_height) = chrome(name);
        assert_eq!(
            (width, height),
            (want_width, want_height),
            "{name}: painted {width}x{height}, and Chrome paints \
             {want_width}x{want_height} -- insets do not size a replaced \
             element"
        );
    }
}

/// A lone end inset positions and is not dropped: Chrome puts `right: 0` at
/// x=140 in a 200-wide block and `bottom: 0` at y=-10 in a 30-tall one. Red if
/// the rule is simplified to dropping every end inset.
#[test]
fn a_lone_end_inset_still_positions() {
    let (x, _, width, height) =
        painted(&scene_with(image([None, Some(0.0), None, None])));
    let (want_x, _, want_width, want_height) = chrome("img right 0 only");
    assert_eq!(
        (x, width, height),
        (want_x, want_width, want_height),
        "a lone `right: 0` put the image at x={x}; it should sit against the \
         right edge at its intrinsic size"
    );

    let (_, y, width, height) =
        painted(&scene_with(image([None, None, Some(0.0), None])));
    let (_, want_y, want_width, want_height) = chrome("img bottom 0 only");
    assert_eq!(
        (y, width, height),
        (want_y, want_width, want_height),
        "a lone `bottom: 0` put the image at y={y}; it should sit against the \
         bottom edge and overflow the top"
    );
}

/// The control for the whole rule: a non-replaced box still stretches to its
/// insets, 200x30 in Chrome, which fails a fix keyed on insets alone.
#[test]
fn a_non_replaced_box_still_stretches_to_its_insets() {
    let mut inner = Node::new(NodeKind::Box);
    inner.paint.background_color = Color::rgb(INK.0, INK.1, INK.2);
    inner.layout.position_type = PositionType::Absolute;
    inner.layout.inset = meo_canvas_scene::Sides {
        top: Some(Length::ZERO),
        right: Some(Length::ZERO),
        bottom: Some(Length::ZERO),
        left: Some(Length::ZERO),
    };

    let (_, _, width, height) = painted(&scene_with(inner));
    let (_, _, want_width, want_height) = chrome("div inset 0");
    assert_eq!(
        (width, height),
        (want_width, want_height),
        "a Box with `inset: 0` painted {width}x{height} against Chrome's \
         {want_width}x{want_height}; insets DO size a non-replaced element \
         and this row is what says the rule is scoped"
    );
}

/// An in-flow replaced element overflows a shorter container, 60x40 in a 200x30
/// block. A fact, not a guard: the `no insets` case of
/// `insets_never_size_a_replaced_element` is what pins the clamp removal.
#[test]
fn an_in_flow_replaced_element_overflows_a_shorter_container() {
    let mut inner = Node::new(NodeKind::Image {
        source: ImageSource::Bytes(art()),
        fit: ObjectFit::Fill,
        position: (Length::ZERO, Length::ZERO),
        frame: None,
    });
    inner.layout.position_type = PositionType::Relative;

    // The height is the axis under test: the block is 30 tall and the art 40.
    // The width comes back 200 against Chrome's 60, since a block container
    // treats the image as block-level, a separate sizing rule, so it is
    // deliberately not asserted.
    let (_, _, _, height) = painted(&scene_with(inner));
    assert_eq!(
        height, ART.1 as u32,
        "an in-flow image in a {}-tall block painted {height} tall; it should \
         overflow at its intrinsic {} rather than be narrowed to fit",
        BLOCK.1, ART.1
    );
}

/// A declared size still wins over the intrinsic one: `inset: 0` with
/// `width/height: 100%` gives the containing block, 200x30, as in Chrome.
#[test]
fn a_declared_size_still_wins() {
    let mut inner = image([Some(0.0), Some(0.0), Some(0.0), Some(0.0)]);
    inner.layout.size = (Dimension::Percent(1.0), Dimension::Percent(1.0));

    let (_, _, width, height) = painted(&scene_with(inner));
    assert_eq!(
        (width, height),
        (BLOCK.0 as u32, BLOCK.1 as u32),
        "a declared `100%` painted {width}x{height}; an explicit size wins \
         over the intrinsic one"
    );
}

/// An `Image` with children is a container, not a replaced element, and has no
/// Chrome row since an `<img>` cannot have children. `is_replaced` asks arity:
/// keyed on the kind alone, removing the inset left the box sized by a child
/// waiting on that height, and it painted 0.
#[test]
fn an_image_with_children_is_a_container() {
    let mut inner = image([Some(0.0), None, Some(0.0), None]);
    inner.layout.display = Display::Flex;
    let mut scene = scene_with(inner);

    let mut child = Node::new(NodeKind::Box);
    child.paint.background_color = Color::rgb(INK.0, INK.1, INK.2);
    child.layout.size = (Dimension::Points(10.0), Dimension::Percent(1.0));
    push(&mut scene, NodeId::new(2), child);

    let (_, _, _, height) = painted(&scene);
    assert_eq!(
        height, BLOCK.1 as u32,
        "a `height: 100%` child of an absolutely positioned image with \
         `top`/`bottom` painted {height} tall; the image has children, so it is \
         a container and its height is the distance between its insets"
    );
}
