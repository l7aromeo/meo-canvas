//! A picture fitted to a box layout left fractional: fitted in that box, then
//! snapped, as Chrome does (`object-fit.tsv` pins the rectangles). Here the
//! case the table's 8x4 picture cannot show, an 840x400 picture whose aspect is
//! the box's, where `cover` and `fill` are one rectangle and so the same bytes.

use meo_canvas::{
    Box, Display, Element, FlexDirection, Format, Image, ObjectFit, Renderer,
    Root, Styled, hex_rgb, px,
};

/// The picture's size, a namecard's.
const ART: (f32, f32) = (840.0, 400.0);

/// The box: as wide as a 350px card's body and as tall as the picture's
/// aspect makes it, 166.67, which layout rounds to 167.
const BOX: (f32, f32) = (350.0, 350.0 / 2.1);

fn renderer() -> Renderer {
    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte.
    renderer.set_gpu(false);
    renderer
}

/// An 840x400 checkerboard of 60x50 cells, so a picture scaled or shifted by a
/// fraction of a pixel moves edges across the whole box.
fn art(renderer: &Renderer) -> Vec<u8> {
    let rows: Vec<Element> = (0..8)
        .map(|row| {
            Box::new()
                .display(Display::Flex)
                .flex_direction(FlexDirection::Row)
                .size(px(ART.0), px(50.0))
                .children(
                    (0..14)
                        .map(|cell| {
                            Box::new()
                                .size(px(60.0), px(50.0))
                                .background_color(hex_rgb(
                                    if (cell + row) % 2 == 0 {
                                        0x3a_6e_c8
                                    } else {
                                        0xe8_b0_40
                                    },
                                ))
                        })
                        .collect::<Vec<_>>(),
                )
        })
        .collect();
    let mut canvas = Root::new(ART.0)
        .height(ART.1)
        .display(Display::Flex)
        .flex_direction(FlexDirection::Column)
        .children(rows)
        .render(renderer)
        .unwrap_or_else(|error| {
            unreachable!("the art did not render: {error}")
        });
    canvas
        .to_buffer(Format::Png)
        .unwrap_or_else(|error| unreachable!("the art did not encode: {error}"))
}

/// The picture under `fit` in the box, with a child of its own when `child`,
/// at `scale`, as raw RGBA.
fn drawn(
    renderer: &Renderer,
    art: &[u8],
    fit: ObjectFit,
    child: bool,
    scale: f32,
) -> Vec<u8> {
    let mut image = Image::bytes(art.to_vec())
        .width(px(BOX.0))
        .height(px(BOX.1))
        .object_fit(fit);
    if child {
        image = image.children(Box::new().size(px(4.0), px(4.0)));
    }
    let mut canvas = Root::new(BOX.0 + 20.0)
        .height(200.0)
        .scale(scale)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(image)
        .render(renderer)
        .unwrap_or_else(|error| {
            unreachable!("the card did not render: {error}")
        });
    canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the card did not encode: {error}")
    })
}

#[test]
fn cover_is_fill_where_the_picture_shares_the_box_aspect() {
    let renderer = renderer();
    let art = art(&renderer);
    for scale in [1.0, 2.5] {
        let fill = drawn(&renderer, &art, ObjectFit::Fill, false, scale);
        let cover = drawn(&renderer, &art, ObjectFit::Cover, false, scale);
        let contain = drawn(&renderer, &art, ObjectFit::Contain, false, scale);
        let differing = fill.iter().zip(&cover).filter(|(a, b)| a != b).count();
        assert_eq!(differing, 0, "cover differs from fill at scale {scale}");
        assert!(
            contain == fill,
            "contain differs from fill at scale {scale}"
        );
    }
}

#[test]
fn a_picture_with_a_child_is_fitted_as_one_without() {
    // Our own consistency rather than Chrome's answer, since an `<img>` has no
    // children: a picture that has one is not a layout leaf, and its fit still
    // comes from the box as laid out.
    let renderer = renderer();
    let art = art(&renderer);
    let fill = drawn(&renderer, &art, ObjectFit::Fill, true, 1.0);
    let cover = drawn(&renderer, &art, ObjectFit::Cover, true, 1.0);
    assert!(cover == fill, "with a child, cover differs from fill");
}
