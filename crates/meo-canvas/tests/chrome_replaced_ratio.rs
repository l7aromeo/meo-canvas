//! What a definite width does to a replaced element's `auto` height
//! (`l7aromeo/meo-canvas#94`): Chrome derives it from the intrinsic ratio. Read
//! from ink. The flex rows with no minimum or maximum are the controls: a
//! stretched flex item is 200x40, correct there and the defect in a block.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        layout::{
            Align, BoxSizing, Display, FlexDirection, FlexWrap, PositionType,
        },
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

/// The recorded Chrome rows, which `chrome_tables_are_read` requires be read.
const TABLE: &str = include_str!("assets/chrome/replaced-ratio.tsv");

/// Chrome's `(width, height)` for one case, rounded rather than truncated: a
/// 3:2 ratio against 100 gives 66.66, which covers 67 rows of pixels.
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

/// A 60x40 image, rendered by this renderer from one filled box rather than
/// embedded, so the intrinsic size is one the suite can restate.
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
    /// A plain box with the art's ratio, which takes no intrinsic size.
    Ratio,
}

/// One case: a container, and the element inside it.
struct Case {
    display: Display,
    tall: Dimension,
    absolute: bool,
    width: Dimension,
    height: Dimension,
    /// The element's minimum and maximum height.
    clamp: (Dimension, Dimension),
    /// The element's minimum and maximum width.
    width_clamp: (Dimension, Dimension),
    /// The element's `flex-grow`.
    grow: f32,
    /// The element's `flex-shrink`.
    shrink: f32,
    /// The container's direction and `align-items`, CSS's initial values
    /// unless the row's container says otherwise.
    flow: (FlexDirection, Option<Align>),
    /// The container's `flex-wrap`.
    wrap: FlexWrap,
    /// The element's padding on every side, and the box its sizes measure.
    padding: (f32, BoxSizing),
    kind: Kind,
}

/// The painted extent of the element: anything that is not the page,
/// antialiased edges included, which an exact-colour match would drop.
fn painted(case: &Case) -> (u32, u32) {
    let mut scene = Scene::new(Size::new(PAGE, PAGE));
    scene.nodes[0].layout.align_items = Some(Align::FlexStart);

    let mut container = Node::new(NodeKind::Box);
    container.layout.size = (Dimension::Points(BLOCK.0), case.tall);
    container.layout.display = case.display;
    (
        container.layout.flex_direction,
        container.layout.align_items,
    ) = case.flow;
    container.layout.flex_wrap = case.wrap;
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
        Kind::Ratio => {
            let mut node = Node::new(NodeKind::Box);
            node.layout.aspect_ratio = Some(ART.0 / ART.1);
            node
        }
    };
    element.layout.size = (case.width, case.height);
    element.layout.min_size = (case.width_clamp.0, case.clamp.0);
    element.layout.max_size = (case.width_clamp.1, case.clamp.1);
    element.layout.flex_grow = case.grow;
    element.layout.flex_shrink = case.shrink;
    let (padding, box_sizing) = case.padding;
    element.layout.padding.left = Length::Points(padding);
    element.layout.padding.right = Length::Points(padding);
    element.layout.padding.top = Length::Points(padding);
    element.layout.padding.bottom = Length::Points(padding);
    element.layout.box_sizing = box_sizing;
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
        clamp: (Dimension::Auto, Dimension::Auto),
        width_clamp: (Dimension::Auto, Dimension::Auto),
        grow: 0.0,
        shrink: 1.0,
        flow: (FlexDirection::Row, None),
        wrap: FlexWrap::NoWrap,
        padding: (0.0, BoxSizing::ContentBox),
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
fn two_declared_sizes_are_both_kept() {
    // `l7aromeo/meo-canvas#189`: a declared height below the derived one held
    // nonetheless, 200x133 for a declared 200x60, in block flow alone.
    for (key, width) in [
        ("block w200 h60", Dimension::Points(200.0)),
        ("block w50% h60", Dimension::Percent(0.5)),
    ] {
        agrees(
            key,
            &replaced(
                Display::Block,
                Dimension::Auto,
                width,
                Dimension::Points(60.0),
            ),
        );
    }
    agrees(
        "block w200 h150",
        &replaced(
            Display::Block,
            Dimension::Auto,
            Dimension::Points(200.0),
            Dimension::Points(150.0),
        ),
    );
    agrees(
        "flex w200 h60",
        &replaced(
            Display::Flex,
            Dimension::Auto,
            Dimension::Points(200.0),
            Dimension::Points(60.0),
        ),
    );
}

#[test]
fn a_maximum_height_clamps_the_derived_one() {
    // `l7aromeo/meo-canvas#189`: the derived 133.33 held over a maximum of 60
    // in block flow, as over a declared height.
    let clamped = |display, clamp| Case {
        clamp,
        ..replaced(
            display,
            Dimension::Auto,
            Dimension::Points(200.0),
            Dimension::Auto,
        )
    };
    agrees(
        "block w200 max60",
        &clamped(Display::Block, (Dimension::Auto, Dimension::Points(60.0))),
    );
    agrees(
        "block w200 max60 min80",
        &clamped(
            Display::Block,
            (Dimension::Points(80.0), Dimension::Points(60.0)),
        ),
    );
}

/// A replaced element under a minimum height of 160, which is 240 wide
/// through the ratio, in a container of content height.
const fn floored(display: Display, width: Dimension, absolute: bool) -> Case {
    Case {
        absolute,
        clamp: (Dimension::Points(160.0), Dimension::Auto),
        ..replaced(display, Dimension::Auto, width, Dimension::Auto)
    }
}

#[test]
fn a_minimum_height_does_not_widen_a_declared_width() {
    // `l7aromeo/meo-canvas#192`: that was the width, 240x160 for a declared
    // 200 in every container, and 120 wide at 50% in a grid.
    let (points, percent) = (Dimension::Points(200.0), Dimension::Percent(0.5));
    for (key, display, width) in [
        ("block w200 min160", Display::Block, points),
        ("block w50% min160", Display::Block, percent),
        ("flex w200 min160", Display::Flex, points),
        ("flex w50% min160", Display::Flex, percent),
        ("grid w50% min160", Display::Grid, percent),
    ] {
        agrees(key, &floored(display, width, false));
    }
    agrees(
        "abs block w200 min160",
        &floored(Display::Block, points, true),
    );
    agrees(
        "flex w200 min160%",
        &Case {
            clamp: (Dimension::Percent(1.6), Dimension::Auto),
            ..replaced(
                Display::Flex,
                Dimension::Points(100.0),
                points,
                Dimension::Auto,
            )
        },
    );
    for (key, flow, width) in [
        (
            "flex start w200 min160",
            (FlexDirection::Row, Some(Align::FlexStart)),
            points,
        ),
        ("column w200 min160", (FlexDirection::Column, None), points),
        ("column w50% min160", (FlexDirection::Column, None), percent),
    ] {
        agrees(
            key,
            &Case {
                flow,
                ..floored(Display::Flex, width, false)
            },
        );
    }
    for (key, box_sizing) in [
        ("block w200 min160 padded", BoxSizing::ContentBox),
        ("block w200 min160 padded border-box", BoxSizing::BorderBox),
    ] {
        agrees(
            key,
            &Case {
                padding: (10.0, box_sizing),
                ..floored(Display::Block, points, false)
            },
        );
    }
}

#[test]
fn a_minimum_height_still_sizes_what_it_did() {
    // An auto width takes the minimum through the ratio, a minimum below the
    // derived height does nothing, and a grid item was right.
    let points = Dimension::Points(200.0);
    for (key, display, width) in [
        ("block w auto min160", Display::Block, Dimension::Auto),
        ("flex w auto min160", Display::Flex, Dimension::Auto),
        ("grid w200 min160", Display::Grid, points),
    ] {
        agrees(key, &floored(display, width, false));
    }
    agrees(
        "block w200 min-width250 min160",
        &Case {
            width_clamp: (Dimension::Points(250.0), Dimension::Auto),
            ..floored(Display::Block, points, false)
        },
    );
    agrees(
        "block w100 min20",
        &Case {
            clamp: (Dimension::Points(20.0), Dimension::Auto),
            ..replaced(
                Display::Block,
                Dimension::Auto,
                Dimension::Points(100.0),
                Dimension::Auto,
            )
        },
    );
    // A minimum that transfers below the declared width keeps an item free to
    // shrink: a box rather than an image, which a flex row does not shrink
    // past its declared width at all, `l7aromeo/meo-canvas#196`.
    for (key, floor) in [
        ("flex ratio div w300 min50", 50.0),
        ("flex ratio div w300 min110", 110.0),
    ] {
        agrees(
            key,
            &Case {
                clamp: (Dimension::Points(floor), Dimension::Auto),
                kind: Kind::Ratio,
                ..replaced(
                    Display::Flex,
                    Dimension::Points(100.0),
                    Dimension::Points(300.0),
                    Dimension::Auto,
                )
            },
        );
    }
}

/// A replaced element under a maximum height of 60, which is 90 wide through
/// the ratio, in a container of content height.
const fn capped(display: Display, width: Dimension) -> Case {
    Case {
        clamp: (Dimension::Auto, Dimension::Points(60.0)),
        ..replaced(display, Dimension::Auto, width, Dimension::Auto)
    }
}

#[test]
fn a_maximum_height_does_not_narrow_a_declared_width() {
    // `l7aromeo/meo-canvas#193`: that was the width, 90x60 for a declared 200
    // in every container but block flow, which `l7aromeo/meo-canvas#189` put
    // right for an image.
    let (points, percent) = (Dimension::Points(200.0), Dimension::Percent(0.5));
    for (key, display, width) in [
        ("flex w200 max60", Display::Flex, points),
        ("flex w50% max60", Display::Flex, percent),
        ("grid w200 max60", Display::Grid, points),
    ] {
        agrees(key, &capped(display, width));
    }
    agrees(
        "abs block w200 max60",
        &Case {
            absolute: true,
            ..capped(Display::Block, points)
        },
    );
    for (key, flow) in [
        (
            "flex start w200 max60",
            (FlexDirection::Row, Some(Align::FlexStart)),
        ),
        ("column w200 max60", (FlexDirection::Column, None)),
        (
            "column start w200 max60",
            (FlexDirection::Column, Some(Align::FlexStart)),
        ),
    ] {
        agrees(
            key,
            &Case {
                flow,
                ..capped(Display::Flex, points)
            },
        );
    }
    agrees(
        "flex w200 max60 min80",
        &Case {
            clamp: (Dimension::Points(80.0), Dimension::Points(60.0)),
            ..capped(Display::Flex, points)
        },
    );
    // Grown past its declared width: no maximum width holds it there.
    agrees(
        "flex grow w100 max60",
        &Case {
            grow: 1.0,
            ..capped(Display::Flex, Dimension::Points(100.0))
        },
    );
    for (key, display) in [
        ("block ratio div w200 max60", Display::Block),
        ("flex ratio div w200 max60", Display::Flex),
    ] {
        agrees(
            key,
            &Case {
                kind: Kind::Ratio,
                ..capped(display, points)
            },
        );
    }
}

#[test]
fn a_maximum_height_leaves_what_it_did() {
    // An auto width takes the maximum through the ratio, a declared maximum
    // width still wins, and a percentage maximum of an auto height is none,
    // so the ratio's 133.33 stands.
    let under = (Dimension::Auto, Dimension::Points(20.0));
    agrees(
        "flex start w auto max20",
        &Case {
            clamp: under,
            flow: (FlexDirection::Row, Some(Align::FlexStart)),
            ..capped(Display::Flex, Dimension::Auto)
        },
    );
    agrees(
        "abs block w auto max20",
        &Case {
            clamp: under,
            absolute: true,
            ..capped(Display::Block, Dimension::Auto)
        },
    );
    agrees(
        "flex w200 max-width150 max60",
        &Case {
            width_clamp: (Dimension::Auto, Dimension::Points(150.0)),
            ..capped(Display::Flex, Dimension::Points(200.0))
        },
    );
    agrees(
        "flex w200 max200%",
        &Case {
            clamp: (Dimension::Auto, Dimension::Percent(2.0)),
            ..capped(Display::Flex, Dimension::Points(200.0))
        },
    );
}

/// An image in block flow with both sizes `auto`, under `width_clamp` and
/// `clamp`, each a minimum then a maximum.
const fn natural(
    width_clamp: (Dimension, Dimension),
    clamp: (Dimension, Dimension),
) -> Case {
    Case {
        clamp,
        width_clamp,
        ..replaced(
            Display::Block,
            Dimension::Auto,
            Dimension::Auto,
            Dimension::Auto,
        )
    }
}

/// Every case laid out apart from its Chrome row, one line each, so a failure
/// names them all rather than the first.
fn apart(cases: impl IntoIterator<Item = (&'static str, Case)>) -> String {
    cases
        .into_iter()
        .filter_map(|(key, case)| {
            let (ours, chrome) = (painted(&case), chrome(key));
            (ours != chrome)
                .then(|| format!("{key}: ours {ours:?}, chrome {chrome:?}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn an_automatic_width_in_block_flow_takes_a_bound_through_the_ratio() {
    // `l7aromeo/meo-canvas#198`: the natural width held under every bound but
    // a minimum, 60x20 for Chrome's 30x20 under a maximum height of 20.
    let (auto, px, percent) =
        (Dimension::Auto, Dimension::Points, Dimension::Percent);
    let padded = |padding, box_sizing| Case {
        padding: (padding, box_sizing),
        ..natural((auto, auto), (auto, px(20.0)))
    };
    let wrong = apart([
        (
            "block w auto max20",
            natural((auto, auto), (auto, px(20.0))),
        ),
        (
            "block w auto max60",
            natural((auto, auto), (auto, px(60.0))),
        ),
        (
            "block w auto max-width30",
            natural((auto, px(30.0)), (auto, auto)),
        ),
        (
            "block w auto max-width10%",
            natural((auto, percent(0.1)), (auto, auto)),
        ),
        (
            "block w auto min-width90",
            natural((px(90.0), auto), (auto, auto)),
        ),
        (
            "block w auto max200%",
            natural((auto, auto), (auto, percent(2.0))),
        ),
        (
            "block w auto max20 padded",
            padded(10.0, BoxSizing::ContentBox),
        ),
        (
            "block w auto max20 padded border-box",
            padded(5.0, BoxSizing::BorderBox),
        ),
    ]);
    assert!(wrong.is_empty(), "{wrong}");
}

#[test]
fn two_bounds_on_an_automatic_width_follow_the_table() {
    // CSS 2.2 §10.4's table, which Chrome follows: the ratio gives way only
    // where the two bounds disagree. `l7aromeo/meo-canvas#198`: seven of these
    // nine were laid out apart from Chrome.
    let (auto, px) = (Dimension::Auto, Dimension::Points);
    let wrong = apart([
        (
            "block w auto max20 min30",
            natural((auto, auto), (px(30.0), px(20.0))),
        ),
        (
            "block w auto max-width30 min30",
            natural((auto, px(30.0)), (px(30.0), auto)),
        ),
        (
            "block w auto max20 min-width50",
            natural((px(50.0), auto), (auto, px(20.0))),
        ),
        (
            "block w auto min-width90 max20",
            natural((px(90.0), auto), (auto, px(20.0))),
        ),
        (
            "block w auto max-width30 min60",
            natural((auto, px(30.0)), (px(60.0), auto)),
        ),
        (
            "block w auto max-width40 max20",
            natural((auto, px(40.0)), (auto, px(20.0))),
        ),
        (
            "block w auto max-width20 max30",
            natural((auto, px(20.0)), (auto, px(30.0))),
        ),
        (
            "block w auto min-width90 min50",
            natural((px(90.0), auto), (px(50.0), auto)),
        ),
        (
            "block w auto min-width70 min80",
            natural((px(70.0), auto), (px(80.0), auto)),
        ),
    ]);
    assert!(wrong.is_empty(), "{wrong}");
}

#[test]
fn a_stretched_flex_item_is_not_sized_from_its_maximum() {
    // `l7aromeo/meo-canvas#197`: the item's maximum height, or one its maximum
    // width transfers, sized it in a line of indefinite height, 90x60 where
    // Chrome keeps the natural 60x40.
    let (auto, px) = (Dimension::Auto, Dimension::Points);
    let flex = |width_clamp, clamp| Case {
        clamp,
        width_clamp,
        ..replaced(Display::Flex, auto, auto, auto)
    };
    let wrong = apart([
        ("flex w auto max60", flex((auto, auto), (auto, px(60.0)))),
        (
            "flex w auto max-width90",
            flex((auto, px(90.0)), (auto, auto)),
        ),
        (
            "flex w auto max60 min50",
            flex((auto, auto), (px(50.0), px(60.0))),
        ),
        (
            "flex wrap w auto max60",
            Case {
                wrap: FlexWrap::Wrap,
                ..flex((auto, auto), (auto, px(60.0)))
            },
        ),
        (
            "flex w auto max-width30",
            flex((auto, px(30.0)), (auto, auto)),
        ),
        (
            "flex w auto max-width25%",
            flex((auto, Dimension::Percent(0.25)), (auto, auto)),
        ),
        ("flex w auto max20", flex((auto, auto), (auto, px(20.0)))),
        (
            "flex start w auto max60",
            Case {
                flow: (FlexDirection::Row, Some(Align::FlexStart)),
                ..flex((auto, auto), (auto, px(60.0)))
            },
        ),
        (
            "flex w auto max60 tall100",
            Case {
                tall: px(100.0),
                ..flex((auto, auto), (auto, px(60.0)))
            },
        ),
        (
            "column w auto max-width90",
            Case {
                flow: (FlexDirection::Column, None),
                ..flex((auto, px(90.0)), (auto, auto))
            },
        ),
        (
            "column w auto max-width120",
            Case {
                flow: (FlexDirection::Column, None),
                ..flex((auto, px(120.0)), (auto, auto))
            },
        ),
        (
            "column w auto max-width30",
            Case {
                flow: (FlexDirection::Column, None),
                ..flex((auto, px(30.0)), (auto, auto))
            },
        ),
        (
            "column w auto max-width120 min0",
            Case {
                flow: (FlexDirection::Column, None),
                ..flex((auto, px(120.0)), (px(0.0), auto))
            },
        ),
    ]);
    assert!(wrong.is_empty(), "{wrong}");
}

#[test]
fn a_flex_item_wider_than_its_line_shrinks_to_it() {
    // `l7aromeo/meo-canvas#196`: an image held its declared width, 300x200
    // where Chrome shrinks it to 200x133.33, and a box with the ratio shrank
    // but kept the height of its declared width.
    let (auto, px) = (Dimension::Auto, Dimension::Points);
    let wide = |width| replaced(Display::Flex, auto, width, auto);
    let floored = |floor| Case {
        clamp: (px(floor), auto),
        ..wide(px(300.0))
    };
    let wrong = apart([
        ("flex w300", wide(px(300.0))),
        ("flex w150%", wide(Dimension::Percent(1.5))),
        ("flex w300 min20", floored(20.0)),
        ("flex w300 min100", floored(100.0)),
        ("flex w300 min160", floored(160.0)),
        (
            "flex start w300",
            Case {
                flow: (FlexDirection::Row, Some(Align::FlexStart)),
                ..wide(px(300.0))
            },
        ),
        (
            "flex w300 min-width0",
            Case {
                width_clamp: (px(0.0), auto),
                ..wide(px(300.0))
            },
        ),
        (
            "flex ratio div w300",
            Case {
                kind: Kind::Ratio,
                ..wide(px(300.0))
            },
        ),
        (
            "flex w300 shrink0",
            Case {
                shrink: 0.0,
                ..wide(px(300.0))
            },
        ),
        (
            "flex w300 tall100",
            Case {
                tall: px(100.0),
                ..wide(px(300.0))
            },
        ),
        (
            "flex start w300 tall100",
            Case {
                tall: px(100.0),
                flow: (FlexDirection::Row, Some(Align::FlexStart)),
                ..wide(px(300.0))
            },
        ),
        (
            "flex w300 tall300",
            Case {
                tall: px(300.0),
                ..wide(px(300.0))
            },
        ),
        (
            "flex w300 max-width100 min160",
            Case {
                width_clamp: (auto, px(100.0)),
                ..floored(160.0)
            },
        ),
        (
            "flex w300 min160 min-width0",
            Case {
                width_clamp: (px(0.0), auto),
                ..floored(160.0)
            },
        ),
        (
            "flex w300 max100",
            Case {
                clamp: (auto, px(100.0)),
                ..wide(px(300.0))
            },
        ),
    ]);
    assert!(wrong.is_empty(), "{wrong}");
}

#[test]
fn a_flex_column_shrinks_a_declared_height() {
    // `l7aromeo/meo-canvas#196` in the other direction: the automatic minimum
    // was the declared height, and unstretched, the width stayed the one the
    // declared height gives.
    let (auto, px) = (Dimension::Auto, Dimension::Points);
    let wrong = apart([
        (
            "column h150 tall100",
            Case {
                flow: (FlexDirection::Column, None),
                ..replaced(Display::Flex, px(100.0), auto, px(150.0))
            },
        ),
        (
            "column h150 tall140",
            Case {
                flow: (FlexDirection::Column, None),
                ..replaced(Display::Flex, px(140.0), auto, px(150.0))
            },
        ),
        (
            "column start h150 tall100",
            Case {
                flow: (FlexDirection::Column, Some(Align::FlexStart)),
                ..replaced(Display::Flex, px(100.0), auto, px(150.0))
            },
        ),
    ]);
    assert!(wrong.is_empty(), "{wrong}");
}

#[test]
fn a_picture_keeps_its_ratio_in_the_content_box() {
    // `l7aromeo/meo-canvas#217`: padding counted in the ratio in a flex
    // container, 80x73 for Chrome's 80x60, and a border-box width gave its
    // whole box the ratio, 100x67 for 100x73.33.
    let (auto, px) = (Dimension::Auto, Dimension::Points);
    let padded = |display, width, (padding, box_sizing)| Case {
        padding: (padding, box_sizing),
        ..replaced(display, auto, width, auto)
    };
    let (content, border) =
        ((10.0, BoxSizing::ContentBox), (10.0, BoxSizing::BorderBox));
    let (flex, block) = (Display::Flex, Display::Block);
    let wrong = apart([
        ("flex w auto padded", padded(flex, auto, content)),
        (
            "flex start w auto padded",
            Case {
                flow: (FlexDirection::Row, Some(Align::FlexStart)),
                ..padded(flex, auto, content)
            },
        ),
        ("block w auto padded", padded(block, auto, content)),
        ("flex w auto padded border-box", padded(flex, auto, border)),
        (
            "block w auto padded border-box",
            padded(block, auto, border),
        ),
        ("flex w100 padded", padded(flex, px(100.0), content)),
        ("block w100 padded", padded(block, px(100.0), content)),
        (
            "flex w100 padded border-box",
            padded(flex, px(100.0), border),
        ),
        (
            "block w100 padded border-box",
            padded(block, px(100.0), border),
        ),
        (
            "block w50% padded border-box",
            padded(block, Dimension::Percent(0.5), border),
        ),
        (
            "column w auto padded",
            Case {
                flow: (FlexDirection::Column, None),
                ..padded(flex, auto, content)
            },
        ),
        (
            "flex w auto max60 padded",
            Case {
                clamp: (auto, px(60.0)),
                ..padded(flex, auto, content)
            },
        ),
        (
            "flex w auto max60 padded border-box",
            Case {
                clamp: (auto, px(60.0)),
                ..padded(flex, auto, (5.0, BoxSizing::BorderBox))
            },
        ),
        (
            "ratio div w100 padded border-box",
            Case {
                kind: Kind::Ratio,
                ..padded(block, px(100.0), border)
            },
        ),
        (
            "ratio div w100 padded",
            Case {
                kind: Kind::Ratio,
                ..padded(block, px(100.0), content)
            },
        ),
    ]);
    assert!(wrong.is_empty(), "{wrong}");
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
                clamp: (Dimension::Auto, Dimension::Auto),
                width_clamp: (Dimension::Auto, Dimension::Auto),
                grow: 0.0,
                shrink: 1.0,
                flow: (FlexDirection::Row, None),
                wrap: FlexWrap::NoWrap,
                padding: (0.0, BoxSizing::ContentBox),
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
                clamp: (Dimension::Auto, Dimension::Auto),
                width_clamp: (Dimension::Auto, Dimension::Auto),
                grow: 0.0,
                shrink: 1.0,
                flow: (FlexDirection::Row, None),
                wrap: FlexWrap::NoWrap,
                padding: (0.0, BoxSizing::ContentBox),
                kind: Kind::Box,
            },
        );
    }
}
