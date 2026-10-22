//! `direction` reaches a box that does not state one, as it does in Chrome 153
//! (`l7aromeo/meo-canvas#165`): a grid's tracks, a flex row's items, a block's
//! child and a line's `text-align: start` all follow the nearest ancestor that
//! states a direction, and an explicit value still overrides it.

use meo_canvas::{
    Box, Direction, Display, Element, Format, LineHeight, PositionType,
    Renderer, Root, Styled, Text, TextAlign, TrackSize, hex_rgb, left, px,
    sides,
};

const TABLE: &str = include_str!("assets/chrome/direction-inherit.tsv");

/// The face every text measurement uses.
const FONT: (&str, &str) = (
    "Fixture",
    "../meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf",
);

/// The page the box cases sit on, and the box: 20 in from the left, two 30-wide
/// slots, as the table's header states.
const PAGE: (f32, f32) = (100.0, 20.0);
const MARGIN: f32 = 20.0;
const BOX: (f32, f32) = (60.0, 20.0);
const SLOT: f32 = 30.0;

/// The cell the text cases sit on.
const CELL: (f32, f32) = (300.0, 60.0);

const RED: u32 = 0xdc_28_28;
const BLUE: u32 = 0x28_50_dc;
const PAPER: (u8, u8, u8) = (255, 255, 255);

/// How far an inked column may sit from Chrome's: two, since Chrome's
/// anti-aliasing and ours disagree at a glyph's edge. The rows it separates
/// are about 260 columns apart. Box edges are exact.
const INK_TOLERANCE: i64 = 2;

/// A block between the page and the box, and what it states.
#[derive(Default, Clone, Copy)]
enum Wrapper {
    /// The box is the page's child.
    #[default]
    Absent,
    /// A block stating no direction.
    Plain,
    /// A block stating this direction.
    Stating(Direction),
}

/// Where the direction is stated in one box case, and how the box is placed.
#[derive(Default)]
struct Placement {
    page: Option<Direction>,
    container: Option<Direction>,
    wrapper: Wrapper,
    /// Placed against the page rather than in flow.
    absolute: bool,
    flex: bool,
}

fn placement(case: &str) -> Placement {
    let rtl = Some(Direction::Rtl);
    match case {
        "grid under an rtl page" => Placement {
            page: rtl,
            ..Placement::default()
        },
        "grid under an rtl grandparent" => Placement {
            page: rtl,
            wrapper: Wrapper::Plain,
            ..Placement::default()
        },
        "grid stating rtl" => Placement {
            container: rtl,
            ..Placement::default()
        },
        "grid and page both rtl" => Placement {
            page: rtl,
            container: rtl,
            ..Placement::default()
        },
        "grid with no direction anywhere" => Placement::default(),
        "grid stating ltr under an rtl page" => Placement {
            page: rtl,
            container: Some(Direction::Ltr),
            ..Placement::default()
        },
        "absolute grid in an rtl block on an ltr page" => Placement {
            wrapper: Wrapper::Stating(Direction::Rtl),
            absolute: true,
            ..Placement::default()
        },
        "flex row under an rtl page" => Placement {
            page: rtl,
            flex: true,
            ..Placement::default()
        },
        "flex row stating rtl" => Placement {
            container: rtl,
            flex: true,
            ..Placement::default()
        },
        "flex row with no direction anywhere" => Placement {
            flex: true,
            ..Placement::default()
        },
        other => unreachable!(
            "the table has a box case this test cannot build: {other}"
        ),
    }
}

fn render(root: Root, register_font: bool) -> Vec<u8> {
    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte.
    renderer.set_gpu(false);
    if register_font {
        renderer
            .register_font(FONT.0, FONT.1)
            .unwrap_or_else(|error| {
                unreachable!("the font did not register: {error}")
            });
    }
    let mut canvas = root.render(&renderer).unwrap_or_else(|error| {
        unreachable!("the scene did not render: {error}")
    });
    canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    })
}

/// Each slot child's left edge, red then blue, read along the box's middle row.
fn slots(case: &str) -> [i64; 2] {
    let Placement {
        page,
        container,
        wrapper,
        absolute,
        flex,
    } = placement(case);
    let slot = |colour: u32| {
        let child = Box::new()
            .display(Display::Block)
            .height(px(BOX.1))
            .background_color(hex_rgb(colour));
        if flex {
            child.width(px(SLOT)).flex_shrink(0.0)
        } else {
            child
        }
    };
    let mut boxed = Box::new()
        .display(if flex { Display::Flex } else { Display::Grid })
        .size(px(BOX.0), px(BOX.1))
        .children([slot(RED), slot(BLUE)]);
    boxed = if absolute {
        boxed.position_type(PositionType::Absolute).position(sides(
            Some(px(0.0)),
            None,
            None,
            Some(px(MARGIN)),
        ))
    } else {
        boxed.margin(left(px(MARGIN)))
    };
    if !flex {
        boxed = boxed.grid_template_columns(vec![
            TrackSize::Points(SLOT),
            TrackSize::Points(SLOT),
        ]);
    }
    if let Some(direction) = container {
        boxed = boxed.direction(direction);
    }
    let block = || Box::new().display(Display::Block);
    let inner: Element = match wrapper {
        Wrapper::Absent => boxed,
        Wrapper::Plain => block().children(boxed),
        Wrapper::Stating(direction) => {
            block().direction(direction).children(boxed)
        }
    };
    let mut root = Root::new(PAGE.0)
        .height(PAGE.1)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(inner);
    if let Some(direction) = page {
        root = root.direction(direction);
    }
    let pixels = render(root, false);

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a page stated here in whole pixels"
    )]
    let (wide, middle) = (PAGE.0 as usize, (PAGE.1 / 2.0) as usize);
    let first = |colour: u32| -> i64 {
        let want = colour.to_be_bytes();
        (0..wide)
            .find(|x| {
                let at = (middle * wide + x) * 4;
                pixels[at..at + 3] == want[1..]
            })
            .map_or(-1, |x| i64::try_from(x).unwrap_or(-1))
    };
    [first(RED), first(BLUE)]
}

/// A block child's size, as the table's header states.
const BLOCK: (f32, f32) = (30.0, 20.0);

/// A block child's left edge from the page, read along its middle row.
fn block(case: &str) -> i64 {
    let (page, wrapped) = match case {
        "block child under an rtl page" => (Some(Direction::Rtl), false),
        "block child under an rtl grandparent" => (Some(Direction::Rtl), true),
        "block child with no direction anywhere" => (None, true),
        other => unreachable!(
            "the table has a block case this test cannot build: {other}"
        ),
    };
    let child = Box::new()
        .display(Display::Block)
        .size(px(BLOCK.0), px(BLOCK.1))
        .background_color(hex_rgb(RED));
    let inner: Element = if wrapped {
        Box::new().display(Display::Block).children(child)
    } else {
        child
    };
    let mut root = Root::new(PAGE.0)
        .height(PAGE.1)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(inner);
    if let Some(direction) = page {
        root = root.direction(direction);
    }
    let pixels = render(root, false);

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a page stated here in whole pixels"
    )]
    let (wide, middle) = (PAGE.0 as usize, (PAGE.1 / 2.0) as usize);
    let want = RED.to_be_bytes();
    (0..wide)
        .find(|x| {
            let at = (middle * wide + x) * 4;
            pixels[at..at + 3] == want[1..]
        })
        .map_or(-1, |x| i64::try_from(x).unwrap_or(-1))
}

/// The inked columns of one line, as `[left, width]`.
fn ink(case: &str) -> [i64; 2] {
    let (page, line, align) = match case {
        "start under an rtl page" => {
            (Some(Direction::Rtl), None, TextAlign::Start)
        }
        "end under an rtl page" => (Some(Direction::Rtl), None, TextAlign::End),
        "start with no direction anywhere" => (None, None, TextAlign::Start),
        "start stating ltr under an rtl page" => {
            (Some(Direction::Rtl), Some(Direction::Ltr), TextAlign::Start)
        }
        other => {
            unreachable!("the table has a line this test cannot build: {other}")
        }
    };
    let mut text = Text::new("abc")
        .font_size(32.0)
        .line_height(LineHeight::Length(40.0))
        .color(hex_rgb(0x00_00_00))
        .text_align(align)
        .width(px(CELL.0));
    if let Some(direction) = line {
        text = text.direction(direction);
    }
    let mut root = Root::new(CELL.0)
        .height(CELL.1)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .font_family(FONT.0)
        .children(text);
    if let Some(direction) = page {
        root = root.direction(direction);
    }
    let pixels = render(root, true);

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a cell stated here in whole pixels"
    )]
    let (wide, tall) = (CELL.0 as usize, CELL.1 as usize);
    let inked: Vec<usize> = (0..wide)
        .filter(|x| {
            (0..tall).any(|y| {
                let at = (y * wide + x) * 4;
                (pixels[at], pixels[at + 1], pixels[at + 2]) != PAPER
            })
        })
        .collect();
    match (inked.first(), inked.last()) {
        (Some(&first), Some(&last)) => [
            i64::try_from(first).unwrap_or(-1),
            i64::try_from(last - first + 1).unwrap_or(-1),
        ],
        _ => [-1, -1],
    }
}

fn pair(value: &str) -> [i64; 2] {
    let mut parts = value.split(',').map(|part| {
        part.parse::<i64>().unwrap_or_else(|error| {
            unreachable!("`{value}` is not two numbers: {error}")
        })
    });
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), None) => [a, b],
        _ => unreachable!("`{value}` is not two numbers"),
    }
}

#[test]
fn an_unstated_direction_follows_the_nearest_ancestor_that_states_one() {
    let mut wrong = Vec::new();
    let mut rows = 0;
    for line in TABLE.lines().filter(|line| !line.starts_with('#')) {
        let columns: Vec<&str> = line.split('\t').collect();
        let [case, measure, value, ..] = columns[..] else {
            unreachable!("a row without its three columns: {line}");
        };
        rows += 1;
        match measure {
            "first,second" => {
                let want = pair(value);
                let got = slots(case);
                if got != want {
                    wrong.push(format!(
                        "{case}: red and blue start at {got:?}, Chrome {want:?}"
                    ));
                }
            }
            "left" => {
                let want = value.parse::<i64>().unwrap_or_else(|error| {
                    unreachable!("`{value}` is not a number: {error}")
                });
                let got = block(case);
                if got != want {
                    wrong.push(format!("{case}: at {got}, Chrome {want}"));
                }
            }
            "left,width" => {
                let want = pair(value);
                let got = ink(case);
                if (got[0] - want[0]).abs() > INK_TOLERANCE
                    || (got[1] - want[1]).abs() > INK_TOLERANCE
                {
                    wrong.push(format!(
                        "{case}: inked {got:?}, Chrome {want:?}"
                    ));
                }
            }
            other => unreachable!("a measure this test does not read: {other}"),
        }
    }
    // Ten box cases, three blocks and four lines: a table that lost its rows
    // would pass.
    assert_eq!(rows, 17, "the table has {rows} rows");
    assert!(
        wrong.is_empty(),
        "{} of {rows} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
