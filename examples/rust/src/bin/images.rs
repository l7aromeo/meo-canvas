//! Images: every fit, both source kinds, position, borders and radius.
//!
//! One picture eight pixels wide and four tall, drawn into square boxes — so
//! every fit resolves to a visibly different rectangle rather than to a
//! difference a reader has to measure.

use meo_canvas::{
    Align, Box as BoxNode, Element, FlexDirection, Image, ObjectFit, Overflow,
    Root, Styled, corners_all, hex_rgb, pct, px, sides,
};
use meo_canvas_examples::{FORMATS, draw};

/// The picture every cell draws, beside this file rather than beside the
/// output.
const STRIP: &str = "../../crates/meo-canvas/tests/assets/strip.png";

/// A clipped cell holding one image.
fn cell(image: Element) -> Element {
    BoxNode::new()
        .size(px(64.0), px(64.0))
        .overflow(Overflow::Hidden)
        .background_color(hex_rgb(0xee_ee_f2))
        .children(image)
}

/// The picture at a fit, filling its cell.
fn fitted(fit: ObjectFit) -> Element {
    Image::path(STRIP).size(px(64.0), px(64.0)).object_fit(fit)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(STRIP)?;

    let root = Root::new(400.0, 168.0)
        .background_color(hex_rgb(0xff_ff_ff))
        .padding(px(8.0))
        .flex_direction(FlexDirection::Column)
        .gap(px(6.0))
        .children([
            // Every fit, in one row, so they are read against each other.
            BoxNode::new().gap(px(6.0)).children(vec![
                cell(fitted(ObjectFit::Fill)),
                cell(fitted(ObjectFit::Contain)),
                cell(fitted(ObjectFit::Cover)),
                cell(fitted(ObjectFit::None)),
                cell(fitted(ObjectFit::ScaleDown)),
            ]),
            BoxNode::new()
                .gap(px(6.0))
                .align_items(Align::Center)
                .children(vec![
                    // The same picture from bytes rather than from a path: two
                    // source kinds that must draw the same thing.
                    cell(
                        Image::bytes(bytes)
                            .size(px(64.0), px(64.0))
                            .object_fit(ObjectFit::Contain),
                    ),
                    // `object_position` moves the picture inside its box,
                    // which is only visible where the fit
                    // leaves room.
                    cell(
                        Image::path(STRIP)
                            .size(px(64.0), px(64.0))
                            .object_fit(ObjectFit::None)
                            .object_position((pct(0.0), pct(0.0))),
                    ),
                    cell(
                        Image::path(STRIP)
                            .size(px(64.0), px(64.0))
                            .object_fit(ObjectFit::None)
                            .object_position((pct(100.0), pct(100.0))),
                    ),
                    // An image is a box: it takes a border and a radius like
                    // one.
                    cell(
                        Image::path(STRIP)
                            .size(px(64.0), px(64.0))
                            .object_fit(ObjectFit::Cover)
                            .border(sides(4.0, 4.0, 4.0, 4.0))
                            .border_color(hex_rgb(0x28_50_dc)),
                    ),
                    cell(
                        Image::path(STRIP)
                            .size(px(64.0), px(64.0))
                            .object_fit(ObjectFit::Cover)
                            .border_radius_corners(corners_all(20.0)),
                    ),
                ]),
        ]);

    draw("images", root, FORMATS)
}
