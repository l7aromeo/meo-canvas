//! Text: size, weight, style, decoration, alignment, spacing and markup.
//!
//! One string repeated with one property changed at a time. A property that
//! does nothing is a line that looks like the one above it, which is what a
//! showcase is for: `text_decoration` and a centred or right `text_align` both
//! drew exactly that, and both draw now. The two rows that still repeat their
//! neighbour are `text_stroke` and `paint_order`, which the binding underneath
//! cannot express -- its text style carries a colour and no stroke width.

use meo_canvas::{
    Box as BoxNode, Element, FlexDirection, Root, Styled, Text, TextAlign,
    TextDecoration, hex_rgb, px,
};
use meo_canvas_examples::{FONT, FORMATS, draw_with_fonts};

/// The same words every line draws.
const WORDS: &str = "Hxgp quick 0123";

/// One line at the family the example registers.
fn line(text: &str) -> Element {
    Text::new(text)
        .font_family(FONT.0)
        .font_size(16.0)
        .color(hex_rgb(0x14_14_1e))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let column = |children: Vec<Element>| {
        BoxNode::new()
            .width(px(184.0))
            .padding(px(4.0))
            .flex_direction(FlexDirection::Column)
            .gap(px(3.0))
            .background_color(hex_rgb(0xf6_f6_f8))
            .children(children)
    };

    let left = column(vec![
        line(WORDS).font_size(11.0),
        line(WORDS).font_size(22.0),
        line(WORDS).bold(),
        line(WORDS).italic(),
        line(WORDS).text_decoration(TextDecoration::Underline),
        line(WORDS).text_decoration(TextDecoration::LineThrough),
        line(WORDS).letter_spacing(px(3.0)),
        line(WORDS).word_spacing(px(12.0)),
    ]);

    let right = column(vec![
        line(WORDS).text_align(TextAlign::Center),
        line(WORDS).text_align(TextAlign::Right),
        line(WORDS).line_height(2.0),
        // Markup: the parser turns the tags into runs, so the bold word is one
        // segment and the coloured one another.
        line("plain <b>bold</b> <color=#dc2828>red</color>"),
        // Rich text built from runs rather than parsed, which is the other way
        // to reach the same shape.
        Text::rich([
            ("two ".to_owned(), meo_canvas::Style::new()),
            ("runs".to_owned(), meo_canvas::Style::new().bold()),
        ])
        .font_family(FONT.0)
        .font_size(16.0)
        .color(hex_rgb(0x14_14_1e)),
        // A paragraph clamped to one line, with what replaces the rest.
        line("this line is far too long to fit in the width it is given")
            .max_lines(1)
            .ellipsis("…"),
        // A shadow and a stroke, which are paint rather than layout.
        line(WORDS).text_shadow(vec![meo_canvas::scene::TextShadow {
            offset_x: 2.0,
            offset_y: 2.0,
            blur: 2.0,
            color: meo_canvas::scene::Color::rgba(20, 20, 40, 140),
        }]),
        line(WORDS)
            .font_size(20.0)
            .color(hex_rgb(0xff_ff_ff))
            .text_stroke(meo_canvas::scene::TextStroke {
                width: 1.0,
                color: hex_rgb(0x14_14_1e),
            }),
        // The same stroke painted over the fill rather than under it, which is
        // only legible against the line above.
        line(WORDS)
            .font_size(20.0)
            .color(hex_rgb(0xff_ff_ff))
            .text_stroke(meo_canvas::scene::TextStroke {
                width: 1.0,
                color: hex_rgb(0x14_14_1e),
            })
            .paint_order(meo_canvas::scene::PaintOrder::Stroke),
        // Where a short line sits in the space its height leaves.
        line(WORDS)
            .line_height(2.0)
            .vertical_align(meo_canvas::scene::VerticalAlign::Bottom),
    ]);

    let root = Root::new(400.0, 300.0)
        .background_color(hex_rgb(0xff_ff_ff))
        .padding(px(8.0))
        .gap(px(6.0))
        .children([left, right]);

    draw_with_fonts("text", root, FORMATS, &[FONT])
}
