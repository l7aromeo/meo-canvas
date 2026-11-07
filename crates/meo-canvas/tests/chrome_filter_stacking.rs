//! Whether a `filter` or `backdrop_filter` chain makes a stacking context, and
//! whether it is refused, against Chrome: a chain Chrome computes to `none`
//! stacks nothing, so dropping one has to hoist a `z_index: -1` child beneath
//! its parent, as `filter-stacking.tsv` records Chrome doing.

use meo_canvas::{
    Box, Display, Element, Format, PositionType, Renderer, Root, Styled,
    hex_rgb, px, sides,
};

/// The recorded Chrome rows, which `chrome_tables_are_read` requires be read.
const TABLE: &str = include_str!("assets/chrome/filter-stacking.tsv");

/// The page, wide enough to hold the parent at its origin.
const PAGE: (u32, u32) = (60, 40);

/// The child's centre, where Chrome's `elementFromPoint` read.
const CHILD_CENTRE: (u32, u32) = (15, 15);

/// A point of the parent clear of the child by 7px and of its own edge by 8,
/// beyond a 2px blur's reach, so it shows the parent alone however it stacks.
const PARENT_ALONE: (u32, u32) = (32, 15);

/// The rows whose stacking differs from Chrome's, all spellings of `none`:
/// `meo-skia-canvas` takes `none` as an empty chain, so it is not dropped, and
/// a chain that is not dropped stacks (`l7aromeo/meo-canvas#210`). Asserted
/// both ways, so the fix fails this test until the list is emptied.
const KNOWN_NONE_STACKING: [&str; 8] = [
    "filter \"none\"",
    "filter \"NONE\"",
    "filter \" none \"",
    "filter \"/* c */ none\"",
    "backdrop-filter \"none\"",
    "backdrop-filter \"NONE\"",
    "backdrop-filter \" none \"",
    "backdrop-filter \"/* c */ none\"",
];

/// One row: the property, the chain, whether Chrome refused the declaration,
/// and whether the child shows at its centre.
struct Row {
    property: String,
    chain: String,
    refused: bool,
    child_on_top: bool,
}

/// A cell written as a JSON string. The tool writes only the escapes these
/// chains need, which is none, so a backslash is refused rather than decoded.
fn json(cell: &str) -> String {
    let inner = cell
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| unreachable!("{cell} is not a JSON string"));
    assert!(!inner.contains('\\'), "{cell} carries an escape");
    inner.to_owned()
}

fn rows() -> Vec<Row> {
    let mut header = false;
    let mut rows = Vec::new();
    for line in TABLE.lines() {
        if let Some(rest) = line.strip_prefix('#') {
            header |= rest.trim() == "property\tchain\tdeclared\tcomputed\ttop";
            continue;
        }
        assert!(header, "a row before the column line: {line}");
        let cells: Vec<&str> = line.split('\t').collect();
        let [property, chain, declared, _computed, top] = cells.as_slice()
        else {
            unreachable!("{line} does not have five cells")
        };
        rows.push(Row {
            property: (*property).to_owned(),
            chain: json(chain),
            refused: json(declared).is_empty(),
            child_on_top: match *top {
                "child" => true,
                "parent" => false,
                other => unreachable!("{other} is neither child nor parent"),
            },
        });
    }
    assert_eq!(rows.len(), 36, "filter-stacking.tsv holds {}", rows.len());
    rows
}

/// A red parent carrying the chain, with a blue child at `z_index: -1`.
fn page(row: &Row) -> Root {
    let parent = Box::new()
        .position_type(PositionType::Relative)
        .size(px(40.0), px(30.0))
        .background_color(hex_rgb(0xdc_28_28))
        .children(
            Box::new()
                .position_type(PositionType::Absolute)
                .position(sides(Some(px(5.0)), None, None, Some(px(5.0))))
                .size(px(20.0), px(20.0))
                .background_color(hex_rgb(0x28_50_dc))
                .z_index(-1),
        );
    let parent: Element = match row.property.as_str() {
        "filter" => parent.filter(row.chain.as_str()),
        "backdrop-filter" => parent.backdrop_filter(row.chain.as_str()),
        other => unreachable!("no property named {other}"),
    };
    Root::new(PAGE.0 as f32)
        .height(PAGE.1 as f32)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(parent)
}

#[test]
fn a_filter_chain_stacks_and_is_refused_as_chrome_decides() {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let mut wrong = Vec::new();
    let mut listed = 0;
    for row in rows() {
        let name = format!("{} {:?}", row.property, row.chain);
        let mut canvas = page(&row)
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{name}: {error}"));
        let refused = !canvas.diagnostics().is_empty();
        let pixels = canvas
            .to_buffer(Format::Raw)
            .unwrap_or_else(|error| unreachable!("{name}: {error}"));
        let at = |(x, y): (u32, u32)| {
            let index = ((y * PAGE.0 + x) * 4) as usize;
            pixels[index..index + 4].to_vec()
        };
        let child_on_top = at(CHILD_CENTRE) != at(PARENT_ALONE);
        if refused != row.refused {
            wrong.push(format!(
                "{name}: refused {refused}, in Chrome {}",
                row.refused
            ));
        }
        let known = KNOWN_NONE_STACKING.contains(&name.as_str());
        if (child_on_top == row.child_on_top) == known {
            wrong.push(format!(
                "{name}: child on top {child_on_top}, in Chrome {}{}",
                row.child_on_top,
                if known {
                    "; listed in KNOWN_NONE_STACKING, so remove it"
                } else {
                    ""
                }
            ));
        }
        listed += usize::from(known);
    }
    assert_eq!(
        listed,
        KNOWN_NONE_STACKING.len(),
        "KNOWN_NONE_STACKING names a row the table does not hold"
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
