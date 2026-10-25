//! Where `justify-items` and `justify-self` put one item on the inline axis, as
//! Chrome 153 does (`l7aromeo/meo-canvas#171`): every value, in grid, block and
//! flex containers, box and image, `ltr` and `rtl`, overflowing, absolutely
//! positioned, with its own direction and under a `legacy` ancestor.

use meo_canvas::{
    Box, Direction, Display, Element, Format, Image, JustifyItems, JustifySelf,
    LegacyPosition, OverflowPosition, PositionType, Renderer, Root,
    SelfPosition, Styled, TrackSize, hex_rgb, px,
};
use meo_canvas_core::{ImageFormat, encode::EncodeOptions};
use meo_canvas_scene::{Scene, Size, style::paint::Color};

const TABLE: &str = include_str!("assets/chrome/justify-items.tsv");

/// The page: the container's 200 and an overflowing item's 60 either side fit
/// inside its padding.
const PAGE: (usize, usize) = (600, 200);

/// The page's padding on each side, which keeps an item overflowing its
/// container by 60 on the page.
const GUTTER: f32 = 100.0;

/// The container, and each `outer` wrapper's width.
const CONTAINER: (f32, f32) = (200.0, 100.0);

/// Every item's height: a box's content and the image's intrinsic height.
const HEIGHT: f32 = 20.0;

/// What the item is painted with.
const INK: (u8, u8, u8) = (0xcc, 0x00, 0x00);

/// Half a pixel: a pixel count and a rounded Chrome value differ by whole
/// pixels or not at all, so this separates the two without comparing floats.
const WHOLE: f32 = 0.5;

/// How far an image's height may sit from Chrome's: one pixel, since Chrome
/// reports a stretched image's ratio-derived height in sixty-fourths (66.656)
/// and a box here covers whole pixels. Every x and width is exact.
const HEIGHT_TOLERANCE: f32 = 1.0;

/// An image `width` wide in the item's colour, drawn by this renderer so its
/// intrinsic size is one the suite can restate.
fn art(width: f32) -> Vec<u8> {
    let mut scene = Scene::new(Size::new(width, HEIGHT));
    scene.nodes[0].paint.background_color = Color::rgb(INK.0, INK.1, INK.2);
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// One row: the case, then Chrome's x, width and height.
struct Row<'table> {
    container: Display,
    rtl: bool,
    outer: Vec<Option<JustifyItems>>,
    items: Option<JustifyItems>,
    own: Option<JustifySelf>,
    image: bool,
    width: f32,
    absolute: bool,
    item_direction: Option<Direction>,
    chrome: (f32, f32, f32),
    line: &'table str,
}

fn direction(word: &str) -> Option<Direction> {
    match word {
        "-" => None,
        "ltr" => Some(Direction::Ltr),
        "rtl" => Some(Direction::Rtl),
        other => unreachable!("a direction this test does not know: {other}"),
    }
}

fn self_position(word: &str) -> Option<SelfPosition> {
    Some(match word {
        "center" => SelfPosition::Center,
        "start" => SelfPosition::Start,
        "end" => SelfPosition::End,
        "self-start" => SelfPosition::SelfStart,
        "self-end" => SelfPosition::SelfEnd,
        "flex-start" => SelfPosition::FlexStart,
        "flex-end" => SelfPosition::FlexEnd,
        "left" => SelfPosition::Left,
        "right" => SelfPosition::Right,
        _ => return None,
    })
}

fn legacy_position(word: &str) -> Option<LegacyPosition> {
    Some(match word {
        "left" => LegacyPosition::Left,
        "right" => LegacyPosition::Right,
        "center" => LegacyPosition::Center,
        _ => return None,
    })
}

/// A value both properties spell alike, as CSS writes it.
fn shared(
    value: &str,
) -> Option<(SharedKind, Option<SelfPosition>, Option<OverflowPosition>)> {
    let words: Vec<&str> = value.split(' ').collect();
    match words[..] {
        ["normal"] => Some((SharedKind::Normal, None, None)),
        ["stretch"] => Some((SharedKind::Stretch, None, None)),
        ["baseline"] | ["first", "baseline"] => {
            Some((SharedKind::Baseline, None, None))
        }
        ["last", "baseline"] => Some((SharedKind::LastBaseline, None, None)),
        [position] => {
            Some((SharedKind::Position, Some(self_position(position)?), None))
        }
        [overflow, position] => {
            let overflow = match overflow {
                "safe" => OverflowPosition::Safe,
                "unsafe" => OverflowPosition::Unsafe,
                _ => return None,
            };
            Some((
                SharedKind::Position,
                Some(self_position(position)?),
                Some(overflow),
            ))
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum SharedKind {
    Normal,
    Stretch,
    Baseline,
    LastBaseline,
    Position,
}

fn justify_self(value: &str) -> Option<JustifySelf> {
    if matches!(value, "-" | "auto") {
        return None;
    }
    let (kind, position, overflow) = shared(value).unwrap_or_else(|| {
        unreachable!("a justify-self this test does not read: {value}")
    });
    Some(match (kind, position) {
        (SharedKind::Normal, _) => JustifySelf::Normal,
        (SharedKind::Stretch, _) => JustifySelf::Stretch,
        (SharedKind::Baseline, _) => JustifySelf::Baseline,
        (SharedKind::LastBaseline, _) => JustifySelf::LastBaseline,
        (SharedKind::Position, Some(position)) => {
            JustifySelf::Position { position, overflow }
        }
        (SharedKind::Position, None) => unreachable!("{value}"),
    })
}

fn justify_items(value: &str) -> Option<JustifyItems> {
    if matches!(value, "-" | "legacy") {
        return None;
    }
    let words: Vec<&str> = value.split(' ').collect();
    if let ["legacy", position] | [position, "legacy"] = words[..] {
        return Some(JustifyItems::Legacy(
            legacy_position(position).unwrap_or_else(|| {
                unreachable!(
                    "a legacy position this test does not read: {value}"
                )
            }),
        ));
    }
    let (kind, position, overflow) = shared(value).unwrap_or_else(|| {
        unreachable!("a justify-items this test does not read: {value}")
    });
    Some(match (kind, position) {
        (SharedKind::Normal, _) => JustifyItems::Normal,
        (SharedKind::Stretch, _) => JustifyItems::Stretch,
        (SharedKind::Baseline, _) => JustifyItems::Baseline,
        (SharedKind::LastBaseline, _) => JustifyItems::LastBaseline,
        (SharedKind::Position, Some(position)) => {
            JustifyItems::Position { position, overflow }
        }
        (SharedKind::Position, None) => unreachable!("{value}"),
    })
}

fn number(text: &str) -> f32 {
    text.parse::<f32>()
        .unwrap_or_else(|error| unreachable!("`{text}`: {error}"))
}

fn parse(line: &str) -> Row<'_> {
    let columns: Vec<&str> = line.split('\t').collect();
    let [
        container,
        dir,
        outer,
        items,
        own,
        item,
        width,
        position,
        itemdir,
        x,
        w,
        h,
    ] = columns[..]
    else {
        unreachable!("a row without its twelve columns: {line}");
    };
    Row {
        container: match container {
            "grid" => Display::Grid,
            "block" => Display::Block,
            "flex" => Display::Flex,
            other => {
                unreachable!("a container this test does not know: {other}")
            }
        },
        rtl: direction(dir) == Some(Direction::Rtl),
        outer: if outer == "-" {
            Vec::new()
        } else {
            outer.split('>').map(justify_items).collect()
        },
        items: justify_items(items),
        own: justify_self(own),
        image: item == "img",
        width: number(width),
        absolute: position == "absolute",
        item_direction: direction(itemdir),
        chrome: (number(x), number(w), number(h)),
        line,
    }
}

/// The item's left edge from the container's, its width and its height.
fn placed(row: &Row<'_>) -> (f32, f32, f32) {
    let mut item: Element = if row.image {
        Image::bytes(art(row.width))
    } else {
        let content = Box::new()
            .display(Display::Block)
            .size(px(row.width), px(HEIGHT));
        let item = Box::new()
            .display(Display::Block)
            .background_color(hex_rgb(0xcc_00_00))
            .children(content);
        // A block child needs a width to have something to place, as in the
        // table; a grid, flex or absolute item is sized by its content.
        if row.container == Display::Block {
            item.width(px(row.width))
        } else {
            item
        }
    };
    if let Some(value) = row.own {
        item = item.justify_self(value);
    }
    if row.absolute {
        item = item.position_type(PositionType::Absolute);
    }
    if let Some(value) = row.item_direction {
        item = item.direction(value);
    }
    let mut container = Box::new()
        .display(row.container)
        .position_type(PositionType::Relative)
        .size(px(CONTAINER.0), px(CONTAINER.1))
        .children(item);
    if row.container == Display::Grid {
        container = container
            .grid_template_columns(vec![TrackSize::Points(CONTAINER.0)]);
    }
    if let Some(value) = row.items {
        container = container.justify_items(value);
    }
    let mut tree: Element = container;
    for value in row.outer.iter().rev() {
        let mut wrapper = Box::new()
            .display(Display::Block)
            .width(px(CONTAINER.0))
            .children(tree);
        if let Some(value) = *value {
            wrapper = wrapper.justify_items(value);
        }
        tree = wrapper;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "the page's sides are written above and small"
    )]
    let mut root = Root::new(PAGE.0 as f32)
        .height(PAGE.1 as f32)
        .display(Display::Block)
        .padding(px(GUTTER))
        .background_color(hex_rgb(0xff_ff_ff))
        .children(tree);
    if row.rtl {
        root = root.direction(Direction::Rtl);
    }
    measure(root, row.rtl)
}

/// Renders `root` and reads the item's ink: its left edge from the container's,
/// its width and its height.
fn measure(root: Root, rtl: bool) -> (f32, f32, f32) {
    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte.
    renderer.set_gpu(false);
    let pixels = root
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        })
        .to_buffer(Format::Raw)
        .unwrap_or_else(|error| {
            unreachable!("the canvas did not encode: {error}")
        });

    let inked = |x: usize, y: usize| {
        let at = (y * PAGE.0 + x) * 4;
        (pixels[at], pixels[at + 1], pixels[at + 2]) == INK
    };
    let columns: Vec<usize> = (0..PAGE.0)
        .filter(|&x| (0..PAGE.1).any(|y| inked(x, y)))
        .collect();
    let rows: Vec<usize> = (0..PAGE.1)
        .filter(|&y| (0..PAGE.0).any(|x| inked(x, y)))
        .collect();
    let span = |run: &[usize]| match (run.first(), run.last()) {
        (Some(&first), Some(&last)) => (first, last - first + 1),
        _ => (0, 0),
    };
    let ((x, w), (_, h)) = (span(&columns), span(&rows));
    // The container sits at the content box's start: its left edge under
    // `ltr`, 200 in from its right under `rtl`, and the wrappers with it.
    #[expect(
        clippy::cast_precision_loss,
        reason = "pixel counts on a 600x200 page"
    )]
    let page_width = PAGE.0 as f32;
    let left = if rtl {
        page_width - GUTTER - CONTAINER.0
    } else {
        GUTTER
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "pixel counts on a 600x200 page"
    )]
    (x as f32 - left, w as f32, h as f32)
}

#[test]
fn justify_items_and_self_place_an_item_where_chrome_does() {
    let mut wrong = Vec::new();
    let mut rows = 0;
    for line in TABLE.lines().filter(|line| !line.starts_with('#')) {
        let row = parse(line);
        rows += 1;
        let want = row.chrome;
        let got = placed(&row);
        if (got.0 - want.0.round()).abs() > WHOLE
            || (got.1 - want.1.round()).abs() > WHOLE
            || (got.2 - want.2).abs() > HEIGHT_TOLERANCE
        {
            wrong.push(format!("{}: x, w, h {got:?}, want {want:?}", row.line));
        }
    }
    // A table that lost its rows would pass.
    assert_eq!(rows, 1080, "the table has {rows} rows");
    assert!(
        wrong.is_empty(),
        "{} of {rows} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
