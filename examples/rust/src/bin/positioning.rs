//! Every `PositionType`, every kind of `z_index`, and what clipping does to
//! each.
//!
//! Overlap is the whole subject, so every cell here is a stack of boxes that
//! cover one another. A box that fails to move is a colour that fails to
//! appear.

use meo_canvas::{
    Box as BoxNode, Element, FlexDirection, Overflow, PositionType, Root,
    Styled, hex_rgb, left, px, sides, top,
};
use meo_canvas_examples::{FORMATS, draw};

/// A card the cells are built from.
fn card(colour: u32, offset: f32) -> Element {
    BoxNode::new()
        .position_type(PositionType::Absolute)
        .position(sides(Some(px(offset)), None, None, Some(px(offset))))
        .size(px(44.0), px(34.0))
        .background_color(hex_rgb(colour))
}

/// A panel the cards sit in.
fn panel(children: impl meo_canvas::IntoElements) -> Element {
    BoxNode::new()
        .position_type(PositionType::Relative)
        .size(px(86.0), px(74.0))
        .background_color(hex_rgb(0xee_ee_f2))
        .children(children)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (red, blue, green, gold) =
        (0xdc_28_28, 0x28_50_dc, 0x28_8c_3c, 0xe6_aa_1e);

    // Paint order with no z-index: later siblings cover earlier ones.
    let plain =
        panel(vec![card(red, 4.0), card(blue, 16.0), card(green, 28.0)]);

    // An explicit index overrides tree order, and a negative one sinks behind
    // the parent's background — which is what a stacking context decides.
    let indexed = panel(vec![
        card(red, 4.0).z_index(2),
        card(blue, 16.0).z_index(-1),
        card(green, 28.0),
    ]);

    // The four ways a node can be positioned. `Static` ignores its inset, which
    // is why its card sits at the origin rather than at the offset it names.
    let kinds = panel(vec![
        card(red, 10.0).position_type(PositionType::Static),
        card(blue, 24.0).position_type(PositionType::Relative),
        card(green, 38.0).position_type(PositionType::Sticky),
        card(gold, 4.0).size(px(20.0), px(20.0)),
    ]);

    // Clipping: the same overflowing child in a clipped panel and an unclipped
    // one, which is the only way to say the clip happened.
    let clipped = panel(card(red, 40.0).size(px(70.0), px(60.0)))
        .overflow(Overflow::Hidden);
    let unclipped = panel(card(red, 40.0).size(px(70.0), px(60.0)));

    // A single inset edge, so `top` and `left` are distinguishable from a
    // shorthand that sets all four.
    let edges = panel(vec![
        card(blue, 0.0).position(top(Some(px(30.0)))),
        card(green, 0.0)
            .position(left(Some(px(30.0))))
            .size(px(20.0), px(20.0)),
    ]);

    let root = Root::new(400.0, 96.0)
        .background_color(hex_rgb(0xff_ff_ff))
        .padding(px(8.0))
        .flex_direction(FlexDirection::Row)
        .gap(px(6.0))
        .children([plain, indexed, kinds, clipped, unclipped, edges]);

    draw("positioning", root, FORMATS)
}
