//! A text box breaks into the lines layout measured, not the lines its rounded
//! box would give: taffy rounds a 137.2-wide box to 137, and a paragraph whose
//! widest line is just over 137 painted a third line below its two-line box.
//! Against Chrome's `wrap-width.tsv`, the box read from its background alone
//! and the lines from the text's own ink, so text outside the box is not box.

use meo_canvas::{
    Align, Box, Display, Element, Format, LineHeight, Renderer, Root, Styled,
    Text, hex_rgb, left, px,
};

/// Chrome's answers, which `chrome_tables_are_read` requires be read.
const TABLE: &str = include_str!("assets/chrome/wrap-width.tsv");

/// The face the table was measured in, and the file behind it.
const FONT: (&str, &str) = (
    "Fixture",
    "../meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf",
);

/// The text the table calls `paragraph`.
const PARAGRAPH: &str = "one two three four five six seven eight";

/// The page: room below the box for a line that should not be there.
const PAGE: (usize, usize) = (300, 120);

/// The box's background, which no glyph is drawn in.
const PAPER: [u8; 3] = [0xcc, 0x00, 0x00];

/// One row of the table.
struct Row {
    name: String,
    text: String,
    left: f32,
    stated: Option<f32>,
    chrome: (f32, f32, usize),
}

fn rows() -> Vec<Row> {
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            let [name, text, left, stated, width, height, lines, ..] =
                cells.as_slice()
            else {
                unreachable!("a malformed row: {line}")
            };
            let number = |cell: &str| {
                cell.parse::<f32>()
                    .unwrap_or_else(|_| unreachable!("{cell} is not a number"))
            };
            Row {
                name: name.trim_matches('"').to_owned(),
                text: if *text == "paragraph" {
                    PARAGRAPH.to_owned()
                } else {
                    text.trim_matches('"').to_owned()
                },
                left: number(left),
                stated: stated.strip_suffix("px").map(number),
                chrome: (
                    number(width),
                    number(height),
                    lines.parse().unwrap_or_else(|_| {
                        unreachable!("{lines} is not a count")
                    }),
                ),
            }
        })
        .collect()
}

/// Our box as `(width, height)` from its background, and the bands of rows the
/// black glyphs ink, one per line.
fn rendered(row: &Row) -> ((usize, usize), usize) {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .register_font(FONT.0, FONT.1)
        .unwrap_or_else(|error| {
            unreachable!("the font did not register: {error}")
        });
    let text = Text::new(&row.text)
        .font_family(FONT.0)
        .font_size(20.0)
        .line_height(LineHeight::Number(1.2))
        .color(hex_rgb(0x00_00_00))
        .background_color(hex_rgb(0xcc_00_00));
    // A stated width sits in a block; its own width is a flex row's item.
    let (holder, text): (Element, Element) = match row.stated {
        Some(width) => {
            (Box::new().display(Display::Block), text.width(px(width)))
        }
        None => (
            Box::new()
                .display(Display::Flex)
                .align_items(Align::FlexStart),
            text,
        ),
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "the page's sides are written above and small"
    )]
    let pixels = Root::new(PAGE.0 as f32)
        .height(PAGE.1 as f32)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(holder.margin(left(px(row.left))).children(text))
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        })
        .to_buffer(Format::Raw)
        .unwrap_or_else(|error| {
            unreachable!("the canvas did not encode: {error}")
        });

    let at = |x: usize, y: usize| &pixels[(y * PAGE.0 + x) * 4..][..3];
    let mut found: Option<[usize; 4]> = None;
    let mut inked = vec![false; PAGE.1];
    for (y, inked) in inked.iter_mut().enumerate() {
        for x in 0..PAGE.0 {
            let rgb = at(x, y);
            if rgb == PAPER {
                found = Some(found.map_or([x, y, x, y], |[x0, y0, x1, y1]| {
                    [x0.min(x), y0.min(y), x1.max(x), y1.max(y)]
                }));
            }
            *inked |= rgb.iter().all(|&channel| channel < 100);
        }
    }
    let box_size =
        found.map_or((0, 0), |[x0, y0, x1, y1]| (x1 - x0 + 1, y1 - y0 + 1));
    let bands = inked.windows(2).filter(|pair| pair[1] && !pair[0]).count()
        + usize::from(inked[0]);
    (box_size, bands)
}

#[test]
fn a_text_box_breaks_where_its_layout_broke_it() {
    let rows = rows();
    assert_eq!(rows.len(), 6, "wrap-width.tsv holds {} rows", rows.len());
    let mut wrong = Vec::new();
    for row in &rows {
        let ((width, height), lines) = rendered(row);
        let (chrome_width, chrome_height, chrome_lines) = row.chrome;
        #[expect(
            clippy::cast_precision_loss,
            reason = "a box on a 300-pixel page is exact in an f32"
        )]
        let agrees = (width as f32 - chrome_width).abs() < 1.0
            && (height as f32 - chrome_height).abs() < 0.5
            && lines == chrome_lines;
        if !agrees {
            wrong.push(format!(
                "{}: a {width}x{height} box drawing {lines} lines, Chrome \
                 {chrome_width}x{chrome_height} with {chrome_lines}",
                row.name
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
