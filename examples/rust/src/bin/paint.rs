//! Paint: fills, gradients, borders, shadows, compositing, transforms, masks.
//!
//! Every cell is the same box with one property changed, so a cell that looks
//! like its neighbour is a property that did nothing.
//!
//! The last row is `background_image`, which this half could not spell until
//! `meo_canvas::Style` grew a setter for it: the scene carried it and only the
//! JavaScript surface could say it.

use meo_canvas::{
    Box, Element, FlexDirection, Root, Styled, corners, hex_rgb, pct, px,
    scene::{
        BackgroundImage, BackgroundRepeat, BackgroundSize, BlendMode,
        BorderStyle, BoxShadow, Color, Gradient, GradientGeometry,
        GradientStop, ImageSource, Length, LinearDirection, Mask, MaskShape,
        Transform,
    },
    sides,
};
use meo_canvas_examples::{FORMATS, draw};

/// The picture the background-image row paints, beside this file rather than
/// beside the output.
const STRIP: &str = "../../crates/meo-canvas/tests/assets/strip.png";

/// The side of every cell.
const SIDE: f32 = 72.0;

/// The two colours most cells ramp between.
const FROM: Color = Color::rgb(0x28, 0x50, 0xdc);
const TO: Color = Color::rgb(0xf2, 0xb0, 0x2c);

/// A cell of the one size every cell is.
fn cell() -> Element {
    Box::new()
        .size(px(SIDE), px(SIDE))
        .background_color(hex_rgb(0xee_ee_f2))
}

/// A ramp from one colour to another, at the ends.
fn ends() -> Vec<GradientStop> {
    vec![
        GradientStop {
            offset: 0.0,
            color: FROM,
        },
        GradientStop {
            offset: 1.0,
            color: TO,
        },
    ]
}

/// Three colours spread evenly, which is what a bare colour list means.
fn three() -> Vec<GradientStop> {
    vec![
        GradientStop {
            offset: 0.0,
            color: FROM,
        },
        GradientStop {
            offset: 0.5,
            color: Color::rgb(0xff, 0xff, 0xff),
        },
        GradientStop {
            offset: 1.0,
            color: TO,
        },
    ]
}

/// One row of cells.
fn row(children: Vec<Element>) -> Element {
    Box::new().gap(px(8.0)).children(children)
}

fn main() -> Result<(), std::boxed::Box<dyn std::error::Error>> {
    let root = Root::new(408.0)
        .height(568.0)
        .background_color(hex_rgb(0xff_ff_ff))
        .padding(px(8.0))
        .flex_direction(FlexDirection::Column)
        .gap(px(8.0))
        .children([
            // Fills: a flat colour, then the three gradient shapes, then the
            // one direction an angle cannot say.
            row(vec![
                cell().background_color(FROM),
                cell().gradient(Gradient {
                    geometry: GradientGeometry::Linear {
                        direction: LinearDirection::Angle(135.0),
                    },
                    stops: ends(),
                }),
                cell().gradient(Gradient {
                    geometry: GradientGeometry::Linear {
                        direction: LinearDirection::Between {
                            start: (pct(0.0), pct(0.0)),
                            end: (pct(50.0), pct(100.0)),
                        },
                    },
                    stops: three(),
                }),
                cell().gradient(Gradient {
                    geometry: GradientGeometry::Radial {
                        at: (pct(30.0), pct(30.0)),
                    },
                    stops: ends(),
                }),
                cell().gradient(Gradient {
                    geometry: GradientGeometry::Conic {
                        at: GradientGeometry::CENTER,
                        from: 90.0,
                    },
                    stops: three(),
                }),
            ]),
            // Borders: one colour, four colours, the two dashed styles, and a
            // radius, which is the only one that changes the box's shape.
            row(vec![
                cell().border(sides(6.0, 6.0, 6.0, 6.0)).border_color(FROM),
                cell().border(sides(6.0, 6.0, 6.0, 6.0)).border_color_sides(
                    sides(
                        Some(FROM),
                        Some(TO),
                        Some(hex_rgb(0x22_88_44)),
                        Some(hex_rgb(0xcc_44_22)),
                    ),
                ),
                cell()
                    .border(sides(4.0, 4.0, 4.0, 4.0))
                    .border_color(FROM)
                    .border_style(BorderStyle::Dashed),
                cell()
                    .border(sides(4.0, 4.0, 4.0, 4.0))
                    .border_color(FROM)
                    .border_style(BorderStyle::Dotted),
                cell()
                    .background_color(FROM)
                    .border_radius_corners(corners(4.0, 16.0, 28.0, 0.0)),
            ]),
            // Shadows: outside, inside, spread, coloured, and two at once.
            row(vec![
                cell().background_color(FROM).box_shadow(vec![BoxShadow {
                    offset_x: 4.0,
                    offset_y: 4.0,
                    blur: 6.0,
                    ..BoxShadow::default()
                }]),
                cell().background_color(FROM).box_shadow(vec![BoxShadow {
                    inset: true,
                    offset_x: 4.0,
                    offset_y: 4.0,
                    blur: 8.0,
                    ..BoxShadow::default()
                }]),
                cell().background_color(FROM).box_shadow(vec![BoxShadow {
                    blur: 2.0,
                    spread: 6.0,
                    ..BoxShadow::default()
                }]),
                cell().background_color(FROM).box_shadow(vec![BoxShadow {
                    offset_y: 6.0,
                    blur: 10.0,
                    color: TO,
                    ..BoxShadow::default()
                }]),
                cell().background_color(FROM).box_shadow(vec![
                    BoxShadow {
                        offset_x: -6.0,
                        blur: 4.0,
                        color: TO,
                        ..BoxShadow::default()
                    },
                    BoxShadow {
                        offset_x: 6.0,
                        blur: 4.0,
                        color: Color::rgb(0x22, 0x88, 0x44),
                        ..BoxShadow::default()
                    },
                ]),
            ]),
            // Compositing: each cell holds a smaller square over a gradient,
            // so there is something behind for a blend or a
            // backdrop to read.
            row(vec![
                over(inner().opacity(0.4)),
                over(inner().mix_blend_mode(BlendMode::Multiply)),
                over(inner().mix_blend_mode(BlendMode::Difference)),
                over(inner().filter("blur(3px)")),
                over(
                    inner()
                        .background_color(Color::rgba(0xff, 0xff, 0xff, 0x40))
                        .backdrop_filter("grayscale(1)"),
                ),
            ]),
            // Transforms, all about the same box: rotation, scale, movement,
            // and the same rotation about a corner rather than the centre.
            row(vec![
                over(inner().transform(Transform {
                    rotate_degrees: 20.0,
                    ..Transform::default()
                })),
                over(inner().transform(Transform {
                    scale_x: 1.6,
                    scale_y: 0.6,
                    ..Transform::default()
                })),
                over(inner().transform(Transform {
                    translate_x: px(10.0),
                    translate_y: px(-8.0),
                    ..Transform::default()
                })),
                over(inner().transform(Transform {
                    rotate_degrees: 20.0,
                    origin: (pct(0.0), pct(0.0)),
                    ..Transform::default()
                })),
                // Dithering shows on a shallow ramp rather than a steep one.
                cell().dither(true).gradient(Gradient {
                    geometry: GradientGeometry::Linear {
                        direction: LinearDirection::Angle(90.0),
                    },
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            color: Color::rgb(0x30, 0x30, 0x36),
                        },
                        GradientStop {
                            offset: 1.0,
                            color: Color::rgb(0x34, 0x34, 0x3a),
                        },
                    ],
                }),
            ]),
            // Masks: the two named shapes, a path, a gradient fade, and the
            // same gradient on a cell with a border, so a mask's effect on the
            // border is visible rather than assumed.
            row(vec![
                filled().mask(Mask::Shape(MaskShape::Circle)),
                // Not square, because an ellipse inscribed in a square box IS
                // the circle beside it: on a 72 by 72 cell the two arms draw
                // the same pixels and the picture says they are one keyword.
                cell().children(
                    Box::new()
                        .size(px(72.0), px(44.0))
                        .margin(sides(px(14.0), px(0.0), px(0.0), px(0.0)))
                        .background_color(FROM)
                        .mask(Mask::Shape(MaskShape::Ellipse)),
                ),
                filled().mask(Mask::Path {
                    data: "M36 4 L68 68 L4 68 Z".into(),
                    fill_rule: meo_canvas::FillRule::NonZero,
                }),
                filled().mask(Mask::Gradient(Gradient {
                    geometry: GradientGeometry::Linear {
                        direction: LinearDirection::Angle(90.0),
                    },
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            color: Color::rgba(0, 0, 0, 0xff),
                        },
                        GradientStop {
                            offset: 1.0,
                            color: Color::rgba(0, 0, 0, 0x00),
                        },
                    ],
                })),
                filled()
                    .border(sides(6.0, 6.0, 6.0, 6.0))
                    .border_color(TO)
                    .mask(Mask::Shape(MaskShape::Circle)),
            ]),
            // A background image, and the three things that travel with it.
            // The picture is eight by four, so a tile is small enough that the
            // repeat is a pattern rather than one stretched copy.
            //
            // All five cells draw the same thing today: the picture is
            // stretched to the box and the repeat, the size and the offset are
            // ignored. Left in rather than reduced to one cell -- five cells
            // that should differ and do not is the showcase saying which parts
            // work.
            row(vec![
                tiled(
                    BackgroundRepeat::Repeat,
                    BackgroundSize::AUTO,
                    (px(0.0), px(0.0)),
                ),
                tiled(
                    BackgroundRepeat::NoRepeat,
                    BackgroundSize::AUTO,
                    (px(0.0), px(0.0)),
                ),
                tiled(
                    BackgroundRepeat::RepeatX,
                    BackgroundSize::AUTO,
                    (px(0.0), px(0.0)),
                ),
                tiled(
                    BackgroundRepeat::NoRepeat,
                    BackgroundSize::Cover,
                    (px(0.0), px(0.0)),
                ),
                // The offset of the first tile, which only a repeat that does
                // not start at the corner can show.
                tiled(
                    BackgroundRepeat::Repeat,
                    BackgroundSize::AUTO,
                    (px(6.0), px(10.0)),
                ),
            ]),
        ]);

    draw("paint", root, FORMATS)
}

/// A cell whose gradient gives a blend or a backdrop something to read.
fn over(child: Element) -> Element {
    cell()
        .gradient(Gradient {
            geometry: GradientGeometry::Linear {
                direction: LinearDirection::Angle(135.0),
            },
            stops: three(),
        })
        .children(child)
}

/// The square every compositing cell puts over its gradient.
fn inner() -> Element {
    Box::new()
        .size(px(40.0), px(40.0))
        .margin(sides(px(16.0), px(16.0), px(16.0), px(16.0)))
        .background_color(FROM)
}

/// A cell painted with the strip, under one repeat, size and offset.
fn tiled(
    repeat: BackgroundRepeat,
    size: BackgroundSize,
    position: (Length, Length),
) -> Element {
    cell().background_image(BackgroundImage {
        source: ImageSource::Path(STRIP.into()),
        repeat,
        size,
        position,
    })
}

/// A cell filled edge to edge, so a mask's edge is the only edge in it.
fn filled() -> Element {
    cell().background_color(FROM)
}
