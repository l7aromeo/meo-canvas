//! How wide a wrapping text item is in each container, against Chrome
//! (`l7aromeo/meo-canvas#162`): fit-content wherever it is not stretched, which
//! is the space on offer once the text wraps, never the widest line. Read from
//! the ink of the item's background, and of its glyphs for where lines sit.

use meo_canvas::{
    Align, Box, Display, Element, FlexDirection, Format, LineHeight,
    PositionType, Renderer, Root, Styled, Text, TextAlign, hex_rgb, px,
    scene::{Color, TrackSize},
    sides,
};

/// Chrome's answers, as measured.
const TABLE: &str = include_str!("assets/chrome/flex-column-text.tsv");

/// The face the table was measured in, and the file behind it.
const FONT: (&str, &str) = (
    "Fixture",
    "../meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf",
);

/// The words, which wrap to two lines in a container this wide.
const TEXT: &str = "one two three four five six seven eight";
const WIDTH: f32 = 200.0;

/// The page: wider and taller than the container, so nothing is clipped.
const PAGE: (usize, usize) = (300, 200);

/// The item's background; its text is black over it.
const BACKGROUND: Color = Color::rgb(0xcc, 0x00, 0x00);

/// A red channel below this is glyph, over the background or the white page:
/// the threshold `flexcolumntext.mjs` reads Chrome's shot with.
const GLYPH: u8 = 100;

/// How far a rendered edge may sit from Chrome's: every edge here is a whole
/// pixel in Chrome, so this admits only a rounding of a solved length.
const SLACK: f32 = 0.5;

/// How far a glyph's first or last inked column may sit from Chrome's: two
/// rasterisers antialias an edge differently by a pixel, where a line placed
/// in a box 13 narrower moves its right edge by 13.
const INK_SLACK: f32 = 2.0;

/// One row of the table.
struct Row {
    container: String,
    margin: String,
    rect: [f32; 4],
    widest: f32,
    ink: [f32; 2],
}

fn rows() -> Vec<Row> {
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            let number = |cell: &str| {
                cell.parse::<f32>()
                    .unwrap_or_else(|_| unreachable!("a malformed row: {line}"))
            };
            let ink: Vec<f32> = cells[8].split(',').map(number).collect();
            Row {
                container: cells[0].to_owned(),
                margin: cells[1].to_owned(),
                rect: [
                    number(cells[2]),
                    number(cells[3]),
                    number(cells[4]),
                    number(cells[5]),
                ],
                widest: number(cells[7]),
                ink: [ink[0], ink[1]],
            }
        })
        .collect()
}

/// The container a row names, as this renderer spells it, at the page origin.
fn container(name: &str) -> Element {
    let column = |align| {
        Box::new()
            .width(px(WIDTH))
            .display(Display::Flex)
            .flex_direction(FlexDirection::Column)
            .align_items(align)
    };
    match name {
        "column flex-start" | "column flex-start, text right" => {
            column(Align::FlexStart)
        }
        "column center" => column(Align::Center),
        "column stretch" => column(Align::Stretch),
        "row flex-start" => Box::new()
            .width(px(WIDTH))
            .display(Display::Flex)
            .flex_direction(FlexDirection::Row)
            .align_items(Align::FlexStart),
        "block" => Box::new().width(px(WIDTH)).display(Display::Block),
        "grid" => Box::new()
            .width(px(WIDTH))
            .display(Display::Grid)
            .grid_template_columns(vec![TrackSize::Points(WIDTH)])
            .align_items(Align::FlexStart),
        "absolute shrink-to-fit" => Box::new()
            .position_type(PositionType::Absolute)
            .max_width(px(WIDTH))
            .display(Display::Block),
        other => unreachable!("no container named {other}"),
    }
}

/// What a render shows of the item: its box, read from the background's ink,
/// and the first and last column of glyph, both relative to the page origin.
struct Drawn {
    rect: Option<[f32; 4]>,
    ink: Option<[f32; 2]>,
}

/// A CPU renderer with the face registered.
fn renderer() -> Renderer {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .register_font(FONT.0, FONT.1)
        .unwrap_or_else(|error| {
            unreachable!("the font did not register: {error}")
        });
    renderer
}

fn rendered(row: &Row) -> Drawn {
    let renderer = renderer();

    let mut item = Text::new(TEXT)
        .font_family(FONT.0)
        .font_size(20.0)
        .line_height(LineHeight::Number(1.2))
        .color(Color::rgb(0, 0, 0))
        .background_color(BACKGROUND);
    if row.margin != "none" {
        item = item.margin(sides(px(0.0), px(20.0), px(0.0), px(30.0)));
    }
    if row.container.ends_with("text right") {
        item = item.text_align(TextAlign::Right);
    }

    draw(&renderer, container(&row.container).children(item))
}

/// Renders `child` on a white page and reads it. The box is the background's
/// exact colour only, so glyphs, black and antialiased, never count as box,
/// even where they overflow it; `the_box_reading_ignores_overflowing_glyphs`.
fn draw(renderer: &Renderer, child: Element) -> Drawn {
    #[expect(
        clippy::cast_precision_loss,
        reason = "the page's sides are written above and small"
    )]
    let mut canvas = Root::new(PAGE.0 as f32)
        .height(PAGE.1 as f32)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(child)
        .render(renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    let bytes = canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    });

    let mut found: Option<[usize; 4]> = None;
    let mut glyph: Option<[usize; 2]> = None;
    for y in 0..PAGE.1 {
        for x in 0..PAGE.0 {
            let at = (y * PAGE.0 + x) * 4;
            if bytes[at] < GLYPH {
                glyph = Some(glyph.map_or([x, x], |[left, right]| {
                    [left.min(x), right.max(x)]
                }));
            }
            if bytes[at..at + 3] != [BACKGROUND.r, BACKGROUND.g, BACKGROUND.b] {
                continue;
            }
            found = Some(found.map_or([x, y, x, y], |[x0, y0, x1, y1]| {
                [x0.min(x), y0.min(y), x1.max(x), y1.max(y)]
            }));
        }
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a span of a 300-pixel page is exact in an f32"
    )]
    Drawn {
        rect: found.map(|[x0, y0, x1, y1]| {
            [
                x0 as f32,
                y0 as f32,
                (x1 - x0 + 1) as f32,
                (y1 - y0 + 1) as f32,
            ]
        }),
        ink: glyph.map(|[left, right]| [left as f32, right as f32]),
    }
}

#[test]
fn a_wrapping_text_item_is_as_wide_as_chrome_makes_it() {
    let rows = rows();
    assert_eq!(
        rows.len(),
        16,
        "the table is eight containers by two margins"
    );
    let mut wrong = Vec::new();
    for row in &rows {
        let drawn = rendered(row);
        let boxed = drawn.rect.is_some_and(|ours| {
            ours.iter()
                .zip(row.rect)
                .all(|(ours, chrome)| (ours - chrome).abs() <= SLACK)
        });
        let inked = drawn.ink.is_some_and(|ours| {
            ours.iter()
                .zip(row.ink)
                .all(|(ours, chrome)| (ours - chrome).abs() <= INK_SLACK)
        });
        if !boxed || !inked {
            wrong.push(format!(
                "{} / margin {}: Chrome box {:?} ink {:?}, here box {:?} ink \
                 {:?}",
                row.container,
                row.margin,
                row.rect,
                row.ink,
                drawn.rect,
                drawn.ink
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The instrument can see a line overflowing its box: an item 20 wide, under
/// every word, reads as 20 wide while its glyphs reach past it. Drawn in the
/// background's colour instead, the glyphs would read as box and this fails.
#[test]
fn the_box_reading_ignores_overflowing_glyphs() {
    let item = Text::new(TEXT)
        .font_family(FONT.0)
        .font_size(20.0)
        .line_height(LineHeight::Number(1.2))
        .color(Color::rgb(0, 0, 0))
        .background_color(BACKGROUND)
        .width(px(20.0));
    let drawn = draw(
        &renderer(),
        Box::new()
            .width(px(WIDTH))
            .display(Display::Block)
            .children(item),
    );
    let rect = drawn
        .rect
        .unwrap_or_else(|| unreachable!("no box was drawn"));
    let ink = drawn
        .ink
        .unwrap_or_else(|| unreachable!("no glyph was drawn"));
    assert!(
        (rect[2] - 20.0).abs() <= SLACK,
        "the box reads {rect:?}, not 20 wide"
    );
    assert!(
        ink[1] > rect[0] + rect[2],
        "the glyphs {ink:?} do not reach past the box {rect:?}, so this \
         proves nothing about reading one without the other"
    );
}

/// The table can tell fit-content from the widest line: on every row the
/// widest line Chrome broke the text into is short of the box by more than a
/// pixel, so an item sized to that line fails the row rather than passing it.
#[test]
fn every_row_separates_the_box_from_its_widest_line() {
    for row in rows() {
        assert!(
            row.rect[2] - row.widest > 1.0,
            "{} / margin {}: a box of {} and a widest line of {} cannot tell \
             the two apart",
            row.container,
            row.margin,
            row.rect[2],
            row.widest
        );
    }
}
