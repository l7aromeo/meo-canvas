//! A `filter` or `backdrop_filter` other than `none` makes its box the
//! containing block of absolute and fixed descendants, whose `overflow` then
//! clips them, as Chrome lays out `filter-containing-block.tsv`.

use meo_canvas::{
    Box, Display, Element, Format, Overflow, PositionType, Renderer, Root,
    Styled, hex_rgb, pct, px, sides,
};

/// The recorded Chrome rows, which `chrome_tables_are_read` requires be read.
const TABLE: &str = include_str!("assets/chrome/filter-containing-block.tsv");

/// The page, which is the initial containing block and the viewport.
const PAGE: (u32, u32) = (400, 300);

/// Where the box's top-left corner sits: the spacer's height, the left margin.
const BOX_AT: (f32, f32) = (30.0, 50.0);

const RED: u32 = 0xff_00_00;

/// Half a channel: red over white keeps red at 255 and loses green and blue
/// as its coverage rises, so both below this is more than half covered.
const HALF: u8 = 128;

struct Row {
    property: String,
    chain: String,
    position: PositionType,
    rect: [f32; 4],
    drawn: bool,
}

fn rows() -> Vec<Row> {
    let mut header = false;
    let mut rows = Vec::new();
    for line in TABLE.lines() {
        if let Some(rest) = line.strip_prefix('#') {
            header |= rest.trim()
                == "property\tchain\tposition\tleft\ttop\twidth\theight\tink";
            continue;
        }
        assert!(header, "a row before the column line: {line}");
        let cells: Vec<&str> = line.split('\t').collect();
        let [property, chain, position, left, top, width, height, ink] =
            cells.as_slice()
        else {
            unreachable!("{line} does not have eight cells")
        };
        let number = |cell: &str| {
            cell.parse::<f32>()
                .unwrap_or_else(|_| unreachable!("{cell} is not a number"))
        };
        rows.push(Row {
            property: (*property).to_owned(),
            chain: chain.trim_matches('"').to_owned(),
            position: match *position {
                "absolute" => PositionType::Absolute,
                "fixed" => PositionType::Fixed,
                other => unreachable!("no position named {other}"),
            },
            rect: [number(left), number(top), number(width), number(height)],
            drawn: match *ink {
                "drawn" => true,
                "clipped" => false,
                other => unreachable!("{other} is neither drawn nor clipped"),
            },
        });
    }
    assert_eq!(
        rows.len(),
        18,
        "filter-containing-block.tsv holds {}",
        rows.len()
    );
    rows
}

/// `clip` is the box's `overflow: hidden`; without it the child is drawn
/// whole, where its box is, which is how a clipped row's geometry is checked.
fn page(row: &Row, clip: bool) -> Root {
    let child = Box::new()
        .display(Display::Block)
        .position_type(row.position)
        .position(sides(Some(px(10.0)), None, None, Some(px(110.0))))
        .size(pct(50.0), pct(25.0))
        .background_color(hex_rgb(RED));
    let target = Box::new()
        .display(Display::Block)
        .margin(sides(px(0.0), px(0.0), px(0.0), px(BOX_AT.0)))
        .size(px(100.0), px(80.0))
        .overflow(if clip {
            Overflow::Hidden
        } else {
            Overflow::Visible
        });
    let target: Element = match row.property.as_str() {
        "none" => target,
        "filter" => target.filter(row.chain.as_str()),
        "backdrop-filter" => target.backdrop_filter(row.chain.as_str()),
        other => unreachable!("no property named {other}"),
    };
    Root::new(PAGE.0 as f32)
        .height(PAGE.1 as f32)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(vec![
            Box::new().display(Display::Block).height(px(BOX_AT.1)),
            target.children(child),
        ])
}

/// The child's red ink as a rectangle, or `None` where none was drawn: every
/// pixel more than half covered, which a blur's symmetric falloff leaves on
/// the box's own edge.
fn ink(pixels: &[u8]) -> Option<[f32; 4]> {
    let red = |x: u32, y: u32| {
        let index = ((y * PAGE.0 + x) * 4) as usize;
        let [r, g, b] = [pixels[index], pixels[index + 1], pixels[index + 2]];
        r >= HALF && g < HALF && b < HALF
    };
    let (mut low, mut high) = ((u32::MAX, u32::MAX), (0, 0));
    for y in 0..PAGE.1 {
        for x in 0..PAGE.0 {
            if red(x, y) {
                low = (low.0.min(x), low.1.min(y));
                high = (high.0.max(x), high.1.max(y));
            }
        }
    }
    (low.0 != u32::MAX).then(|| {
        [
            low.0 as f32,
            low.1 as f32,
            (high.0 - low.0 + 1) as f32,
            (high.1 - low.1 + 1) as f32,
        ]
    })
}

#[test]
fn a_filter_makes_a_containing_block_as_chrome_lays_it_out() {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let mut wrong = Vec::new();
    for row in rows() {
        let name =
            format!("{} {:?} {:?}", row.property, row.chain, row.position);
        for clip in [false, true] {
            let mut canvas = page(&row, clip)
                .render(&renderer)
                .unwrap_or_else(|error| unreachable!("{name}: {error}"));
            let pixels = canvas
                .to_buffer(Format::Raw)
                .unwrap_or_else(|error| unreachable!("{name}: {error}"));
            // A drawn child shows its whole box; a clipped one, nothing of it.
            let want = (row.drawn || !clip).then_some(row.rect);
            let got = ink(&pixels);
            if got != want {
                wrong.push(format!(
                    "{name}, clip {clip}: ink {got:?}, in Chrome {want:?}"
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
