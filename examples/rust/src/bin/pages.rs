//! Pages: a twelve-frame sequence, and the formats only a sequence exercises.
//!
//! Every page is built from the same function, so what moves between them is
//! what [`PageInfo`] reports and nothing else. The three derived numbers each
//! drive one thing: `progress` a bar that ends full, `cycle` a rotation that
//! meets itself, `index` the counter that names the page.
//!
//! It writes the still formats as well as the paged ones. What a still format
//! does with twelve pages -- write the first, refuse, or write something else
//! -- is a thing worth knowing rather than a thing to avoid asking.

use meo_canvas::{
    Box, Element, Format, PageInfo, Root, Styled, hex_rgb, pct, px,
    scene::{
        Color, Gradient, GradientGeometry, GradientStop, LinearDirection,
        Transform,
    },
    sides,
};
use meo_canvas_examples::{FONT, FORMATS, PAGED_FORMATS, draw_with_fonts};

/// The ink every page draws in.
const INK: Color = Color::rgb(0x28, 0x50, 0xdc);

fn main() -> Result<(), std::boxed::Box<dyn std::error::Error>> {
    let formats: Vec<Format> = FORMATS
        .iter()
        .copied()
        .chain(PAGED_FORMATS.iter().copied())
        .collect();

    let root = Root::new(200.0)
        .height(120.0)
        .background_color(hex_rgb(0xff_ff_ff))
        .fps(12.0)
        .pages(12)
        .page_builder(page);

    draw_with_fonts("pages", root, &formats, &[FONT])
}

/// One page of the sequence.
fn page(info: PageInfo) -> Vec<Element> {
    vec![
        Box::new()
            .size(pct(100.0), pct(100.0))
            .padding(px(12.0))
            .flex_direction(meo_canvas::FlexDirection::Column)
            .gap(px(10.0))
            .gradient(Gradient {
                geometry: GradientGeometry::Linear {
                    direction: LinearDirection::Angle(180.0),
                },
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: Color::rgb(0xf6, 0xf6, 0xfa),
                    },
                    GradientStop {
                        offset: 1.0,
                        color: Color::rgb(0xe2, 0xe2, 0xec),
                    },
                ],
            })
            .children(vec![
                // The page's own name, so a reader of one frame knows which it
                // is without counting.
                meo_canvas::Text::new(format!(
                    "{} / {}",
                    info.index + 1,
                    info.count
                ))
                .font_family(FONT.0)
                .font_size(14.0)
                .color(hex_rgb(0x14_14_1e)),
                // `progress` spans the sequence inclusively: this bar is empty
                // on the first page and exactly full on the last.
                Box::new()
                    .size(pct(100.0), px(10.0))
                    .background_color(hex_rgb(0xd0_d0_dc))
                    .children(
                        Box::new()
                            .size(pct(info.progress * 100.0), pct(100.0))
                            .background_color(INK),
                    ),
                // `cycle` goes round: the last page is one step short of the
                // first rather than a copy of it, so a loop does not stutter.
                Box::new()
                    .size(px(36.0), px(36.0))
                    .margin(sides(px(6.0), px(0.0), px(0.0), px(60.0)))
                    .background_color(INK)
                    .transform(Transform {
                        rotate_degrees: info.cycle * 360.0,
                        ..Transform::default()
                    }),
            ]),
    ]
}
