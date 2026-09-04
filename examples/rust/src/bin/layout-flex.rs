//! Flexbox: every direction, both wraps, and each way of distributing a line.
//!
//! Rows of coloured blocks rather than a picture, because the question each row
//! answers is *where things sit*. A block that fails to move is visible against
//! its neighbours; a prettier scene would hide it.

use meo_canvas::{
    Align, Box, Column, FlexDirection, FlexWrap, Justify, Root, Row, Styled,
    hex_rgb, px,
};
use meo_canvas_examples::{FORMATS, draw};

/// One coloured block of a fixed size.
fn block(colour: u32, width: f32) -> meo_canvas::Element {
    Box::new()
        .size(px(width), px(18.0))
        .background_color(hex_rgb(colour))
}

/// A labelled strip with a background, so an empty row is still visible.
fn strip(children: impl meo_canvas::IntoElements) -> meo_canvas::Element {
    Row::new()
        .size(px(180.0), px(26.0))
        .padding(px(4.0))
        .gap(px(4.0))
        .background_color(hex_rgb(0xee_ee_f2))
        .children(children)
}

fn main() -> Result<(), std::boxed::Box<dyn std::error::Error>> {
    let red = 0xdc_28_28;
    let blue = 0x28_50_dc;
    let green = 0x28_8c_3c;

    let root = Root::new(400.0)
        .height(372.0)
        .background_color(hex_rgb(0xff_ff_ff))
        .padding(px(8.0))
        .flex_direction(FlexDirection::Column)
        .gap(px(6.0))
        .children([
            // Direction: the same three blocks, read four ways.
            strip(vec![
                block(red, 30.0),
                block(blue, 30.0),
                block(green, 30.0),
            ]),
            strip(vec![
                block(red, 30.0),
                block(blue, 30.0),
                block(green, 30.0),
            ])
            .flex_direction(FlexDirection::RowReverse),
            // Justify: where the free space goes along the main axis.
            strip(vec![block(red, 24.0), block(blue, 24.0)])
                .justify_content(Justify::SpaceBetween),
            strip(vec![block(red, 24.0), block(blue, 24.0)])
                .justify_content(Justify::Center),
            strip(vec![block(red, 24.0), block(blue, 24.0)])
                .justify_content(Justify::SpaceEvenly),
            // Align: where a shorter item sits across the line.
            strip(vec![
                block(red, 24.0).height(px(8.0)),
                block(blue, 24.0).height(px(18.0)),
            ])
            .height(px(30.0))
            .align_items(Align::FlexEnd),
            strip(vec![
                block(red, 24.0).height(px(8.0)),
                block(blue, 24.0).height(px(18.0)),
            ])
            .height(px(30.0))
            .align_items(Align::Center),
            // Grow, shrink and basis: the three ways a length is negotiated.
            strip(vec![
                block(red, 20.0).flex_grow(1.0),
                block(blue, 20.0),
                block(green, 20.0).flex_grow(2.0),
            ]),
            strip(vec![
                block(red, 200.0).flex_shrink(1.0),
                block(blue, 200.0).flex_shrink(3.0),
            ]),
            // Wrap: a line that cannot hold its children.
            strip(vec![
                block(red, 60.0),
                block(blue, 60.0),
                block(green, 60.0),
            ])
            .height(px(52.0))
            .flex_wrap(FlexWrap::Wrap),
            // Aspect ratio: a height derived from a width.
            Column::new()
                .size(px(180.0), px(26.0))
                .background_color(hex_rgb(0xee_ee_f2))
                .children(
                    Box::new()
                        .width(px(48.0))
                        .aspect_ratio(3.0)
                        .background_color(hex_rgb(green)),
                ),
        ]);

    draw("layout-flex", root, FORMATS)
}
