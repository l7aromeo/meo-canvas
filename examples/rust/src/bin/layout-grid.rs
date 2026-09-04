//! Grid: templates, spans, auto flow, track sizing and alignment.
//!
//! Every cell is a different colour so a track in the wrong place is visible as
//! a colour in the wrong place, rather than as a size a reader has to measure.

use meo_canvas::{
    Align, Box, Element, FlexDirection, Grid, GridAutoFlow, GridPlacement,
    Justify, Root, Styled, fr, hex_rgb, px, track,
};
use meo_canvas_examples::{FORMATS, draw};

/// A filled cell.
fn cell(colour: u32) -> Element {
    Box::new().background_color(hex_rgb(colour))
}

fn main() -> Result<(), std::boxed::Box<dyn std::error::Error>> {
    let (red, blue, green, gold) =
        (0xdc_28_28, 0x28_50_dc, 0x28_8c_3c, 0xe6_aa_1e);

    // Fixed tracks, so a column in the wrong place is a colour in the wrong
    // place rather than a width to measure.
    let fixed = Grid::new()
        .size(px(180.0), px(80.0))
        .gap(px(4.0))
        .grid_template_columns(vec![track(px(40.0)), track(px(60.0)), fr(1.0)])
        .grid_template_rows(vec![track(px(30.0)), fr(1.0)])
        .children([
            cell(red),
            cell(blue),
            cell(green),
            cell(gold),
            cell(red),
            cell(blue),
        ]);

    // A span: the first cell takes two columns, so the second row's cells sit
    // under the tail of it.
    let spanning = Grid::new()
        .size(px(180.0), px(80.0))
        .gap(px(4.0))
        .grid_template_columns(vec![fr(1.0), fr(1.0), fr(1.0)])
        .grid_template_rows(vec![fr(1.0), fr(1.0)])
        .children([
            cell(red).grid_column(GridPlacement {
                start: Some(1),
                span: Some(2),
            }),
            cell(blue),
            cell(green).grid_row(GridPlacement {
                start: Some(2),
                span: Some(1),
            }),
            cell(gold),
        ]);

    // Column-major auto flow: the same six cells fill downward first.
    let flowed = Grid::new()
        .size(px(180.0), px(80.0))
        .gap(px(4.0))
        .grid_auto_flow(GridAutoFlow::Column)
        .grid_template_rows(vec![fr(1.0), fr(1.0)])
        .grid_auto_columns(track(px(52.0)))
        .children([
            cell(red),
            cell(blue),
            cell(green),
            cell(gold),
            cell(red),
            cell(blue),
        ]);

    // Alignment inside the tracks: cells smaller than their cell.
    let aligned = Grid::new()
        .size(px(180.0), px(80.0))
        .gap(px(4.0))
        .grid_template_columns(vec![fr(1.0), fr(1.0)])
        .grid_template_rows(vec![fr(1.0)])
        .justify_content(Justify::Center)
        .align_items(Align::Center)
        .children([
            cell(red).size(px(30.0), px(20.0)),
            cell(blue).size(px(30.0), px(20.0)),
        ]);

    let root = Root::new(400.0)
        .height(200.0)
        .background_color(hex_rgb(0xff_ff_ff))
        .padding(px(8.0))
        .flex_direction(FlexDirection::Column)
        .gap(px(6.0))
        .children([
            Box::new().gap(px(6.0)).children([fixed, spanning]),
            Box::new().gap(px(6.0)).children([flowed, aligned]),
        ]);

    draw("layout-grid", root, FORMATS)
}
