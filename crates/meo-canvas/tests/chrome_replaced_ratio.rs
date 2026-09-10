//! What a definite width does to a replaced element's `auto` height.
//!
//! `l7aromeo/meo-canvas#94`: an `Image` with an intrinsic 60x40 and
//! `width: 200` was 200x40 in a block container, the intrinsic height arriving
//! untouched. Chrome derives the other axis from the intrinsic ratio, which is
//! CSS 2.2 §10.3.2 and §10.6.2 and is what `aspect-ratio: auto` means.
//!
//! **Ink, not `LayoutResult`.** The report is about a picture, and a test
//! reading the solved box would pass on a renderer that laid the image out
//! correctly and painted it somewhere else. The node carries a background
//! colour, so what is measured is the rectangle the painter actually filled.
//!
//! **The flex rows are controls and they are the point of the table.** A flex
//! item with an `auto` cross size stretches to the line, so `200x40` is
//! correct in a flex container and the defect in a block one. Two measurements
//! of those two rows were read against each other as a divergence before
//! Chrome was asked, and Chrome says both. A repair scored without them can
//! trade one for the other and look like progress.
//!
//! The `div` rows are the other contrast: a non-replaced block box fills its
//! container and must keep filling it.

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

/// The page. Big enough that a box overflowing its container is still on it.
const PAGE: f32 = 400.0;

/// The art, and the intrinsic size every ratio here is derived from.
const ART: (f32, f32) = (60.0, 40.0);

/// The container: 200 wide, and shorter than the ratio would derive.
const BLOCK: (f32, f32) = (200.0, 40.0);

const INK: (u8, u8, u8) = (204, 0, 0);

/// The recorded Chrome rows, so the assertions compare against a measurement
/// rather than against numbers retyped into this file.
///
/// `chrome_tables_are_read` requires it: a table nobody reads is a table nobody
/// checks, and the pairing is by `include_str!` of the path.
const TABLE: &str = include_str!("assets/chrome/replaced-ratio.tsv");

/// Chrome's `(width, height)` for one case of the table.
///
/// **Rounded rather than truncated, and the two differ here.** A 3:2 ratio
/// against 100 gives Chrome `66.66`, and a box 66.66 tall covers 67 rows of
/// pixels — so truncation would compare 66 against an ink extent of 67 and
/// fail on the arithmetic rather than on the renderer. Floor a sample point;
/// round a reported value.
fn chrome(case: &str) -> (u32, u32) {
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
        // `x` and `y` are recorded for the same reason the sibling table
        // records them and are not read here: every case in this file is at
        // the container's origin, so they discriminate nothing.
        let (_x, _y, width, height) = (number(), number(), number(), number());
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a rounded extent on a 400-square page is a small \
                      non-negative whole number"
        )]
        return (width.round() as u32, height.round() as u32);
    }
    unreachable!("{case} is not in replaced-ratio.tsv")
}

/// A 60x40 image, rendered rather than embedded.
///
/// The bytes come from this renderer encoding a scene of one filled box, so the
/// intrinsic size under test is one the suite can restate rather than a number
/// in a file nobody re-derives.
fn art() -> Vec<u8> {
    let mut scene = Scene::new(Size::new(ART.0, ART.1));
    scene.nodes[0].paint.background_color = Color::rgb(INK.0, INK.1, INK.2);
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// What the element under test is.
#[derive(Clone, Copy)]
enum Kind {
    /// An image with an intrinsic size: the replaced element.
    Replaced,
    /// A plain box: the contrast, which fills its container.
    Box,
}

/// One case: a container, and the element inside it.
struct Case {
    display: Display,
    tall: Dimension,
    absolute: bool,
    width: Dimension,
    height: Dimension,
    kind: Kind,
}

/// The painted extent of the element, in pixels.
///
/// The element carries a background colour and the page carries none, so
/// "anything that is not the page" is the element's own rectangle — including
/// the antialiased edge columns, which an exact-colour match drops and
/// under-reports the box by two pixels for.
fn painted(case: &Case) -> (u32, u32) {
    let mut scene = Scene::new(Size::new(PAGE, PAGE));
    scene.nodes[0].layout.align_items = Some(Align::FlexStart);

    let mut container = Node::new(NodeKind::Box);
    container.layout.size = (Dimension::Points(BLOCK.0), case.tall);
    container.layout.display = case.display;
    container.layout.position_type = PositionType::Relative;
    let container = scene
        .push(NodeId::ROOT, container)
        .unwrap_or_else(|error| unreachable!("{error}"));

    let mut element = match case.kind {
        Kind::Replaced => Node::new(NodeKind::Image {
            source: ImageSource::Bytes(art()),
            fit: ObjectFit::Fill,
            position: (Length::Percent(0.5), Length::Percent(0.5)),
            frame: None,
        }),
        Kind::Box => Node::new(NodeKind::Box),
    };
    element.layout.size = (case.width, case.height);
    element.paint.background_color = Color::rgb(INK.0, INK.1, INK.2);
    if case.absolute {
        element.layout.position_type = PositionType::Absolute;
        element.layout.inset.top = Some(Length::Points(0.0));
        element.layout.inset.left = Some(Length::Points(0.0));
    }
    scene
        .push(container, element)
        .unwrap_or_else(|error| unreachable!("{error}"));

    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let bytes = renderer
        .render_to_buffer(&scene, ImageFormat::Raw, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let side = PAGE as u32;
    // The page's own colour, sampled rather than named: `ImageFormat::Raw` is
    // not white, and a `!= white` predicate matches every pixel on the page.
    let far = ((side - 1) * side + side - 1) as usize * 4;
    let paper = (bytes[far], bytes[far + 1], bytes[far + 2]);

    let (mut left, mut top, mut right, mut bottom) = (side, side, 0_u32, 0_u32);
    let mut seen = false;
    for y in 0..side {
        for x in 0..side {
            let at = ((y * side + x) * 4) as usize;
            if (bytes[at], bytes[at + 1], bytes[at + 2]) != paper {
                seen = true;
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
        }
    }
    if !seen {
        return (0, 0);
    }
    (right - left + 1, bottom - top + 1)
}

/// Asserts one case against the row Chrome wrote for it.
fn agrees(key: &str, case: &Case) {
    assert_eq!(painted(case), chrome(key), "{key}");
}

const fn replaced(
    display: Display,
    tall: Dimension,
    width: Dimension,
    height: Dimension,
) -> Case {
    Case {
        display,
        tall,
        absolute: false,
        width,
        height,
        kind: Kind::Replaced,
    }
}

#[test]
fn a_definite_width_derives_the_height_in_block_flow() {
    agrees(
        "block w200",
        &replaced(
            Display::Block,
            Dimension::Points(BLOCK.1),
            Dimension::Points(200.0),
            Dimension::Auto,
        ),
    );
    agrees(
        "block w50%",
        &replaced(
            Display::Block,
            Dimension::Points(BLOCK.1),
            Dimension::Percent(0.5),
            Dimension::Auto,
        ),
    );
    agrees(
        "block tall auto w200",
        &replaced(
            Display::Block,
            Dimension::Auto,
            Dimension::Points(200.0),
            Dimension::Auto,
        ),
    );
}

#[test]
fn a_declared_height_derives_the_width() {
    agrees(
        "block w auto h80",
        &replaced(
            Display::Block,
            Dimension::Points(BLOCK.1),
            Dimension::Auto,
            Dimension::Points(80.0),
        ),
    );
    agrees(
        "flex w auto h80",
        &replaced(
            Display::Flex,
            Dimension::Points(BLOCK.1),
            Dimension::Auto,
            Dimension::Points(80.0),
        ),
    );
}

#[test]
fn neither_axis_definite_is_the_intrinsic_size() {
    agrees(
        "block w auto",
        &replaced(
            Display::Block,
            Dimension::Points(BLOCK.1),
            Dimension::Auto,
            Dimension::Auto,
        ),
    );
    agrees(
        "block tall auto w auto",
        &replaced(
            Display::Block,
            Dimension::Auto,
            Dimension::Auto,
            Dimension::Auto,
        ),
    );
}

#[test]
fn a_flex_item_still_stretches_to_its_line() {
    agrees(
        "flex w200",
        &replaced(
            Display::Flex,
            Dimension::Points(BLOCK.1),
            Dimension::Points(200.0),
            Dimension::Auto,
        ),
    );
    agrees(
        "flex w50%",
        &replaced(
            Display::Flex,
            Dimension::Points(BLOCK.1),
            Dimension::Percent(0.5),
            Dimension::Auto,
        ),
    );
    agrees(
        "flex w auto",
        &replaced(
            Display::Flex,
            Dimension::Points(BLOCK.1),
            Dimension::Auto,
            Dimension::Auto,
        ),
    );
    agrees(
        "flex tall auto w200",
        &replaced(
            Display::Flex,
            Dimension::Auto,
            Dimension::Points(200.0),
            Dimension::Auto,
        ),
    );
}

#[test]
fn an_out_of_flow_replaced_element_is_unchanged() {
    for (key, display, width) in [
        ("abs block w200", Display::Block, Dimension::Points(200.0)),
        ("abs block w auto", Display::Block, Dimension::Auto),
        ("abs flex w200", Display::Flex, Dimension::Points(200.0)),
    ] {
        agrees(
            key,
            &Case {
                display,
                tall: Dimension::Points(BLOCK.1),
                absolute: true,
                width,
                height: Dimension::Auto,
                kind: Kind::Replaced,
            },
        );
    }
}

#[test]
fn a_non_replaced_box_still_fills_its_container() {
    for (key, width) in [
        ("div block w auto", Dimension::Auto),
        ("div block w200", Dimension::Points(200.0)),
    ] {
        agrees(
            key,
            &Case {
                display: Display::Block,
                tall: Dimension::Points(BLOCK.1),
                absolute: false,
                width,
                height: Dimension::Points(20.0),
                kind: Kind::Box,
            },
        );
    }
}
