//! Block layout: stacking, margins and box sizing.
//!
//! Block is the display CSS starts from and the one a flex container is not, so
//! it earns an example rather than a row in the flex one: children stack down
//! whatever their width, and a margin between two of them collapses to the
//! larger rather than summing.

use meo_canvas::{
    Box, BoxSizing, Display, Element, FlexDirection, Root, Styled, hex_rgb, px,
    sides,
};
use meo_canvas_examples::{FORMATS, draw};

/// A block of a fixed height and a stated width.
fn bar(colour: u32, width: f32) -> Element {
    Box::new()
        .size(px(width), px(24.0))
        .background_color(hex_rgb(colour))
}

fn main() -> Result<(), std::boxed::Box<dyn std::error::Error>> {
    let (red, blue, green) = (0xdc_28_28, 0x28_50_dc, 0x28_8c_3c);

    // Stacking: three blocks of different widths, each on its own line.
    let stacked = Box::new()
        .display(Display::Block)
        .size(px(180.0), px(90.0))
        .padding(px(4.0))
        .background_color(hex_rgb(0xee_ee_f2))
        .children([bar(red, 60.0), bar(blue, 120.0), bar(green, 90.0)]);

    // Margins: the middle bar is pushed down and right, and the gap between it
    // and its neighbours is its own rather than the sum of two.
    let margins = Box::new()
        .display(Display::Block)
        .size(px(180.0), px(90.0))
        .padding(px(4.0))
        .background_color(hex_rgb(0xee_ee_f2))
        .children([
            bar(red, 60.0),
            bar(blue, 60.0).margin(sides(px(8.0), px(0.0), px(8.0), px(30.0))),
            bar(green, 60.0),
        ]);

    // Box sizing: the same declared width, one counting its border and one not.
    let sizing = Box::new()
        .display(Display::Block)
        .size(px(180.0), px(90.0))
        .padding(px(4.0))
        .background_color(hex_rgb(0xee_ee_f2))
        .children([
            bar(red, 100.0)
                .box_sizing(BoxSizing::BorderBox)
                .border(sides(6.0, 6.0, 6.0, 6.0))
                .border_color(hex_rgb(0x14_14_1e)),
            bar(blue, 100.0)
                .box_sizing(BoxSizing::ContentBox)
                .border(sides(6.0, 6.0, 6.0, 6.0))
                .border_color(hex_rgb(0x14_14_1e)),
        ]);

    // The three panels are `Display::Block`. Their children drew nothing when
    // this was written -- painted before their own parent, and covered by its
    // background -- and they draw now.
    let root = Root::new(400.0)
        .height(110.0)
        .background_color(hex_rgb(0xff_ff_ff))
        .padding(px(8.0))
        .flex_direction(FlexDirection::Row)
        .gap(px(6.0))
        .children([stacked, margins, sizing]);

    draw("layout-block", root, FORMATS)
}
