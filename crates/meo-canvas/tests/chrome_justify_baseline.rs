//! Where grid items sharing a column sit under `justify-self: baseline` and
//! `last baseline`, as Chrome 153 places them (`l7aromeo/meo-canvas#171`): on
//! the left edges of their border boxes, as a group at the column's start or
//! its end.

use meo_canvas::{
    BorderStyle, Box, BoxSizing, Direction, Display, Element, Format,
    GridPlacement, Image, JustifyItems, JustifySelf, PositionType, Renderer,
    Root, SelfPosition, Styled, TrackSize, hex_rgb, px, sides,
};
use meo_canvas_core::{ImageFormat, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    style::{Dimension, paint::Color},
};

const TABLE: &str = include_str!("assets/chrome/justify-baseline.tsv");

/// The page, with room either side of the grid for a group that overflows it.
const PAGE: (usize, usize) = (400, 100);

/// Where the grid's left edge sits on the page.
const GRID_LEFT: f32 = 100.0;

/// The grid's width.
const GRID_WIDTH: f32 = 200.0;

/// Every child's height.
const HEIGHT: f32 = 10.0;

/// How far down an absolutely positioned child is moved, clear of the rows
/// the grid's in-flow children fill.
const BELOW: f32 = 50.0;

/// One colour per child, in document order, so that children sharing a row
/// can be told apart; none is the page's white.
const PALETTE: [u32; 3] = [0xcc_00_00, 0x00_88_00, 0x00_00_cc];

/// An image `width` wide in `rgb`, drawn by this renderer so its intrinsic size
/// is one the suite can restate.
fn art(width: f32, rgb: u32) -> Vec<u8> {
    let mut scene = Scene::new(Size::new(width, HEIGHT));
    let [_, r, g, b] = rgb.to_be_bytes();
    scene.nodes[0].paint.background_color = Color::rgb(r, g, b);
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// One child, from the table's spelling of it: `box:60 ml=10 self=last
/// baseline`.
#[derive(Default)]
struct Child {
    image: bool,
    width: f32,
    margin: (f32, f32),
    border: (f32, f32),
    column: Option<GridPlacement>,
    row: Option<GridPlacement>,
    absolute: bool,
    own: Option<JustifySelf>,
}

fn number(text: &str) -> f32 {
    text.parse::<f32>()
        .unwrap_or_else(|error| unreachable!("`{text}`: {error}"))
}

/// A line or a span of lines, as `1` or `1 / 3`.
fn lines(text: &str) -> GridPlacement {
    let ends: Vec<i16> = text
        .split(" / ")
        .map(|line| {
            line.parse::<i16>()
                .unwrap_or_else(|error| unreachable!("`{line}`: {error}"))
        })
        .collect();
    match ends[..] {
        [start] => GridPlacement::spanning(start, 1),
        [start, end] => GridPlacement::spanning(
            start,
            u16::try_from(end - start)
                .unwrap_or_else(|error| unreachable!("{text}: {error}")),
        ),
        _ => unreachable!("grid lines this test does not read: {text}"),
    }
}

fn baseline(value: &str) -> JustifySelf {
    match value {
        "baseline" | "first baseline" => JustifySelf::Baseline,
        "last baseline" => JustifySelf::LastBaseline,
        "normal" => JustifySelf::Normal,
        "start" => JustifySelf::Position {
            position: SelfPosition::Start,
            overflow: None,
        },
        other => {
            unreachable!("a justify-self this test does not read: {other}")
        }
    }
}

fn child(spelling: &str) -> Child {
    let (head, rest) = spelling.split_once(' ').unwrap_or((spelling, ""));
    let (kind, width) = head
        .split_once(':')
        .unwrap_or_else(|| unreachable!("a child without a width: {spelling}"));
    let mut child = Child {
        image: kind == "img",
        width: number(width),
        ..Child::default()
    };
    // `self=` comes last and its value may hold a space.
    let (fields, own) = match rest.split_once("self=") {
        Some((fields, own)) => (fields, Some(own)),
        None => (rest, None),
    };
    child.own = own.map(baseline);
    // `col=1 / 3` holds spaces too, so fields are found by their keys.
    let mut words = fields.split_whitespace().peekable();
    while let Some(word) = words.next() {
        let value =
            |words: &mut std::iter::Peekable<std::str::SplitWhitespace<'_>>,
             first: &str| {
                let mut value = String::from(first);
                while words.peek() == Some(&"/") {
                    words.next();
                    let end = words.next().unwrap_or_default();
                    value = format!("{value} / {end}");
                }
                value
            };
        match word.split_once('=') {
            Some(("ml", v)) => child.margin.0 = number(v),
            Some(("mr", v)) => child.margin.1 = number(v),
            Some(("bl", v)) => child.border.0 = number(v),
            Some(("br", v)) => child.border.1 = number(v),
            Some(("col", v)) => {
                child.column = Some(lines(&value(&mut words, v)));
            }
            Some(("row", v)) => child.row = Some(lines(&value(&mut words, v))),
            None if word == "abs" => child.absolute = true,
            _ => unreachable!("a field this test does not read: {word}"),
        }
    }
    child
}

/// Each child's left edge from the grid's, and its width.
fn placed(
    rtl: bool,
    tracks: &[f32],
    items: Option<JustifyItems>,
    children: &[Child],
) -> Vec<(f32, f32)> {
    let elements: Vec<Element> = children
        .iter()
        .zip(PALETTE)
        .map(|(child, rgb)| {
            let mut element: Element = if child.image {
                Image::bytes(art(child.width, rgb))
            } else {
                // `content-box`, a `div`'s, so a border widens the box as in
                // the table, where the scene's own default is `border-box`.
                Box::new()
                    .display(Display::Block)
                    .box_sizing(BoxSizing::ContentBox)
                    .size(px(child.width), px(HEIGHT))
                    .background_color(hex_rgb(rgb))
                    .border(sides(0.0, child.border.1, 0.0, child.border.0))
                    .border_style(BorderStyle::Solid)
                    .border_color(hex_rgb(rgb))
            };
            element = element.margin(sides(
                Dimension::Points(0.0),
                Dimension::Points(child.margin.1),
                Dimension::Points(0.0),
                Dimension::Points(child.margin.0),
            ));
            if let Some(column) = child.column {
                element = element.grid_column(column);
            }
            if let Some(row) = child.row {
                element = element.grid_row(row);
            }
            if child.absolute {
                // Below the grid, so it does not cover the in-flow children
                // it would otherwise share the first row with: a block-axis
                // inset moves nothing on the inline axis.
                element = element
                    .position_type(PositionType::Absolute)
                    .position(sides(Some(px(BELOW)), None, None, None));
            }
            if let Some(own) = child.own {
                element = element.justify_self(own);
            }
            element
        })
        .collect();
    let mut grid = Box::new()
        .display(Display::Grid)
        .position_type(PositionType::Relative)
        .width(px(GRID_WIDTH))
        .grid_template_columns(
            tracks
                .iter()
                .map(|&track| TrackSize::Points(track))
                .collect::<Vec<_>>(),
        )
        .children(elements);
    if rtl {
        grid = grid.direction(Direction::Rtl);
    }
    if let Some(items) = items {
        grid = grid.justify_items(items);
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "the page's sides are written above and small"
    )]
    let root = Root::new(PAGE.0 as f32)
        .height(PAGE.1 as f32)
        .display(Display::Block)
        .padding(sides(px(0.0), px(0.0), px(0.0), px(GRID_LEFT)))
        .background_color(hex_rgb(0xff_ff_ff))
        .children(grid);
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

    ink(&pixels, children.len())
}

/// Each of the first `count` palette colours' left edge from the grid's, and
/// its width.
fn ink(pixels: &[u8], count: usize) -> Vec<(f32, f32)> {
    PALETTE[..count]
        .iter()
        .map(|&rgb| {
            let [_, red, green, blue] = rgb.to_be_bytes();
            let columns: Vec<usize> = (0..PAGE.0)
                .filter(|&x| {
                    (0..PAGE.1).any(|y| {
                        let at = (y * PAGE.0 + x) * 4;
                        (pixels[at], pixels[at + 1], pixels[at + 2])
                            == (red, green, blue)
                    })
                })
                .collect();
            let (left, width) = match (columns.first(), columns.last()) {
                (Some(&first), Some(&last)) => (first, last - first + 1),
                _ => (0, 0),
            };
            #[expect(
                clippy::cast_precision_loss,
                reason = "pixel counts on a 400x100 page"
            )]
            (left as f32 - GRID_LEFT, width as f32)
        })
        .collect()
}

#[test]
fn a_column_s_baseline_items_align_where_chrome_aligns_them() {
    let mut wrong = Vec::new();
    let mut rows = 0;
    for line in TABLE.lines().filter(|line| !line.starts_with('#')) {
        let columns: Vec<&str> = line.split('\t').collect();
        let [dir, tracks, items, children, xs, ws] = columns[..] else {
            unreachable!("a row without its six columns: {line}");
        };
        rows += 1;
        let tracks: Vec<f32> = tracks.split(' ').map(number).collect();
        let items = match items {
            "-" => None,
            "baseline" => Some(JustifyItems::Baseline),
            "last baseline" => Some(JustifyItems::LastBaseline),
            other => {
                unreachable!("a justify-items this test does not read: {other}")
            }
        };
        let children: Vec<Child> = children.split("; ").map(child).collect();
        let want: Vec<(f32, f32)> = xs
            .split(' ')
            .zip(ws.split(' '))
            .map(|(x, w)| (number(x).round(), number(w).round()))
            .collect();
        let got = placed(dir == "rtl", &tracks, items, &children);
        if got != want {
            wrong.push(format!("{line}\n    got {got:?}, Chrome {want:?}"));
        }
    }
    // Thirteen scenes, each under both baselines and both directions.
    assert_eq!(rows, 52, "the table has {rows} rows");
    assert!(
        wrong.is_empty(),
        "{} of {rows} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
