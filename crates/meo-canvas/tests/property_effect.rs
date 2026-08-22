//! Every paint property, drawn against the same scene without it.
//!
//! # Why this exists beside the golden fixtures
//!
//! A fixture asserts that a picture is *the* picture. That catches a property
//! whose drawing changes and misses one that never drew: a scene setting
//! `mask` renders, encodes and compares equal to its own committed image
//! whether or not the mask is honoured, because the image was made by the same
//! code that ignores it. The showcase found five such properties in an
//! afternoon, and no gate in this project could see any of them.
//!
//! A control pair can. Each case renders the same scene twice, once with the
//! property and once without, and asks only whether the two differ. That is a
//! comparison the renderer cannot satisfy by being consistently wrong -- a
//! property that reaches the painter and is dropped there produces two
//! identical buffers, which is exactly what [`Effect::Nothing`] records.
//!
//! # Why one test rather than one per property
//!
//! A fix usually repairs a family -- `text_stroke` and `paint_order` are one
//! defect, and the five `mask` arms are another -- so the useful report names
//! every property whose answer moved, not the first one. The whole table runs
//! and the failure lists them together.
//!
//! # What to do when this fails
//!
//! A `Nothing` case that starts drawing is a defect fixed: change its row to
//! `Draws` and the gate now guards the fix. A `Draws` case that stops drawing
//! is a regression, and the row is already correct.
//!
//! # The control is the part that is easy to get wrong
//!
//! Three of these cases reported the wrong answer first, and every time the
//! scene was right and the control was not. `backdrop_filter` drew nothing
//! under an opaque square, because a filter on what is behind a node cannot
//! show through the node that asked for it. `vertical_align` looked as though
//! it drew, against a control with no `line_height` -- so the pair measured
//! the line height and named it something else; and once the line height was
//! on both sides it drew nothing, because the subject was a text node sized
//! to its own text and a paragraph moved within a box that fits it exactly
//! does not move. `mask image` would have joined them: an opaque picture
//! masks nothing, so the asset it reads has to have an alpha channel worth
//! reading. Before trusting a row, ask what the wrong answer would have
//! looked like.

use meo_canvas::{
    Box as BoxNode, Element, Format, Renderer, Root, Styled, Text, hex_rgb, px,
    scene::{
        BackgroundImage, BackgroundRepeat, BackgroundSize, BlendMode,
        BoxShadow, Color, FillRule, Gradient, GradientGeometry, GradientStop,
        ImageSource, Length, LinearDirection, Mask, MaskShape, PaintOrder,
        TextAlign, TextDecoration, TextShadow, TextStroke, Transform,
        VerticalAlign,
    },
    sides,
};

/// The family the text cases name, and the file behind it.
///
/// The repository's own font rather than a platform face, for the reason the
/// fixture harness gives: a family resolved from whatever the host installed is
/// not the same family twice.
const FONT: (&str, &str) = (
    "Control",
    "../meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf",
);

/// Whether a property changes what is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effect {
    /// The two renders differ, which is what every property should do.
    Draws,
    /// The two renders are identical: the property reaches the painter and is
    /// dropped there. Each of these is a defect, pinned so that fixing it
    /// fails this test.
    Nothing,
}

/// One property, the scene that sets it, and the scene that does not.
struct Case {
    /// What the failure names.
    property: &'static str,
    /// The subject with the property set.
    with: fn() -> Element,
    /// The same subject without it.
    without: fn() -> Element,
    /// What the property does today.
    effect: Effect,
}

/// A square over a gradient, which is what a blend or a backdrop reads.
fn over(child: Element) -> Element {
    BoxNode::new()
        .size(px(72.0), px(72.0))
        .gradient(ramp())
        .children(child)
}

/// The square every compositing case puts over the gradient.
fn inner() -> Element {
    BoxNode::new()
        .size(px(40.0), px(40.0))
        .margin(sides(px(16.0), px(16.0), px(16.0), px(16.0)))
        .background_color(hex_rgb(0x28_50_dc))
}

/// The same square, translucent, so what is behind it is still visible.
fn glass() -> Element {
    BoxNode::new()
        .size(px(40.0), px(40.0))
        .margin(sides(px(16.0), px(16.0), px(16.0), px(16.0)))
        .background_color(Color::rgba(0xff, 0xff, 0xff, 0x40))
}

/// A ramp with a white middle, so a blend has three colours to work on.
fn ramp() -> Gradient {
    Gradient {
        geometry: GradientGeometry::Linear {
            direction: LinearDirection::Angle(135.0),
        },
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgb(0x28, 0x50, 0xdc),
            },
            GradientStop {
                offset: 0.5,
                color: Color::rgb(0xff, 0xff, 0xff),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgb(0xf2, 0xb0, 0x2c),
            },
        ],
    }
}

/// A shallow ramp across the whole box, which is where banding shows.
fn shallow() -> Element {
    BoxNode::new().size(px(72.0), px(72.0)).gradient(Gradient {
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
                color: Color::rgb(0x3a, 0x3a, 0x42),
            },
        ],
    })
}

/// A filled box, so a mask's edge is the only edge in the picture.
fn filled() -> Element {
    BoxNode::new()
        .size(px(72.0), px(72.0))
        .background_color(hex_rgb(0x28_50_dc))
}

/// One line of text in the registered family.
fn line() -> Element {
    Text::new("Hxgp quick")
        .font_family(FONT.0)
        .font_size(14.0)
        .color(hex_rgb(0x14_14_1e))
}

/// The same line in a box wider than it, so an alignment has room to move it.
fn wide() -> Element {
    line().width(px(72.0))
}

/// The picture the background-image cases paint: eight by four, so a tile is
/// small enough for a repeat to be a pattern rather than one stretched copy.
const STRIP: &[u8] = include_bytes!("assets/strip.png");

/// A background image with the three fields that travel with it.
fn tile(
    repeat: BackgroundRepeat,
    size: BackgroundSize,
    position: (Length, Length),
) -> BackgroundImage {
    BackgroundImage {
        source: ImageSource::Bytes(STRIP.to_vec()),
        repeat,
        size,
        position,
    }
}

/// A plain box, so a background image is the only thing in it.
fn plain() -> Element {
    BoxNode::new()
        .size(px(72.0), px(72.0))
        .background_color(hex_rgb(0xee_ee_f2))
}

/// A shadow that would be plainly visible if it were drawn.
fn shadow(inset: bool) -> BoxShadow {
    BoxShadow {
        inset,
        offset_x: 4.0,
        offset_y: 4.0,
        blur: 8.0,
        ..BoxShadow::default()
    }
}

/// A fade from opaque to clear, which is how a gradient mask is written.
fn fade() -> Gradient {
    Gradient {
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
    }
}

/// Every property this suite reads, and what it does today.
///
/// Four lists rather than one, split where the subject changes: a square over
/// a gradient, that square's own shape, a box filled edge to edge, a line of
/// text. They are concatenated here and read as one table.
fn cases() -> Vec<Case> {
    let mut all = composite_cases();
    all.extend(shape_cases());
    all.extend(background_image_cases());
    all.extend(mask_cases());
    all.extend(text_cases());
    all
}

/// Opacity, blending and filters, all on a square over a gradient.
fn composite_cases() -> Vec<Case> {
    vec![
        Case {
            property: "opacity",
            with: || over(inner().opacity(0.4)),
            without: || over(inner()),
            effect: Effect::Draws,
        },
        Case {
            property: "mix_blend_mode",
            with: || over(inner().mix_blend_mode(BlendMode::Multiply)),
            without: || over(inner()),
            effect: Effect::Draws,
        },
        Case {
            property: "filter",
            with: || over(inner().filter("blur(3px)")),
            without: || over(inner()),
            effect: Effect::Draws,
        },
        // Over a *translucent* square, because a backdrop filter changes what
        // is behind the node: with the opaque square of every other case, the
        // filtered backdrop is covered by the thing that asked for it and the
        // two renders agree however the property behaves.
        //
        // `grayscale` rather than `blur`, and deliberately. A blur of the
        // gradient behind this square **is that gradient again** -- blurring
        // a linear ramp returns it -- so the pair would report `Nothing` for
        // a backdrop filter that works perfectly. Every filter here has to be
        // one the backdrop is not already a fixed point of.
        Case {
            property: "backdrop_filter",
            with: || over(glass().backdrop_filter("grayscale(1)")),
            without: || over(glass()),
            effect: Effect::Draws,
        },
        Case {
            property: "transform",
            with: || {
                over(inner().transform(Transform {
                    rotate_degrees: 20.0,
                    ..Transform::default()
                }))
            },
            without: || over(inner()),
            effect: Effect::Draws,
        },
    ]
}

/// The box's own shape, the shadows it casts, and the ramp it fills with.
fn shape_cases() -> Vec<Case> {
    vec![
        Case {
            property: "border_radius",
            with: || over(inner().border_radius(16.0)),
            without: || over(inner()),
            effect: Effect::Draws,
        },
        Case {
            property: "box_shadow",
            with: || over(inner().box_shadow(vec![shadow(false)])),
            without: || over(inner()),
            effect: Effect::Draws,
        },
        // The one arm of `box_shadow` that draws nothing. Outer, spread and
        // coloured shadows all draw, so this is the inset branch rather than
        // shadows as a whole -- which is why it is a case of its own.
        Case {
            property: "box_shadow inset",
            with: || over(inner().box_shadow(vec![shadow(true)])),
            without: || over(inner()),
            effect: Effect::Draws,
        },
        Case {
            property: "dither",
            with: || shallow().dither(true),
            without: shallow,
            effect: Effect::Draws,
        },
    ]
}

/// An 8x8 image whose left half is opaque and whose right half is clear.
///
/// A mask image is read for its **alpha**, so an opaque picture masks nothing
/// and a case built on one reports `Nothing` however well the arm works. The
/// repository's other image asset, `strip.png`, is opaque in all thirty-two
/// pixels, which is exactly the control mistake this file keeps finding.
const MASK_IMAGE: &[u8] = include_bytes!("assets/mask-half.png");

/// The strip the background-image cases paint, and the fields that travel
/// with it.
///
/// Four rows rather than one: the source is a different question from the
/// repeat, the size and the offset, and a source that draws while the three
/// are ignored is exactly the shape this file exists to tell apart -- which is
/// what it found. `paint.rs:842` draws the picture stretched to the box with
/// `draw_image_sized` and says so in a comment: repetition wants a pattern
/// shader. The source row draws; the three that travel with it do not.
fn background_image_cases() -> Vec<Case> {
    vec![
        Case {
            property: "background_image",
            with: || {
                plain().background_image(tile(
                    BackgroundRepeat::Repeat,
                    BackgroundSize::AUTO,
                    (px(0.0), px(0.0)),
                ))
            },
            without: plain,
            effect: Effect::Draws,
        },
        Case {
            property: "background_image repeat",
            with: || {
                plain().background_image(tile(
                    BackgroundRepeat::NoRepeat,
                    BackgroundSize::AUTO,
                    (px(0.0), px(0.0)),
                ))
            },
            without: || {
                plain().background_image(tile(
                    BackgroundRepeat::Repeat,
                    BackgroundSize::AUTO,
                    (px(0.0), px(0.0)),
                ))
            },
            effect: Effect::Nothing,
        },
        // The two axes against each other rather than against `Repeat`. This
        // is the failure a tiling implementation is most likely to ship: an
        // axis the right way round for one keyword and swapped for the other
        // draws a picture for both, so a pair against the unrepeated case
        // would pass while the two keywords meant each other.
        Case {
            property: "background_image repeat axis",
            with: || {
                plain().background_image(tile(
                    BackgroundRepeat::RepeatX,
                    BackgroundSize::AUTO,
                    (px(0.0), px(0.0)),
                ))
            },
            without: || {
                plain().background_image(tile(
                    BackgroundRepeat::RepeatY,
                    BackgroundSize::AUTO,
                    (px(0.0), px(0.0)),
                ))
            },
            effect: Effect::Nothing,
        },
        Case {
            property: "background_image size",
            with: || {
                plain().background_image(tile(
                    BackgroundRepeat::NoRepeat,
                    BackgroundSize::Cover,
                    (px(0.0), px(0.0)),
                ))
            },
            without: || {
                plain().background_image(tile(
                    BackgroundRepeat::NoRepeat,
                    BackgroundSize::AUTO,
                    (px(0.0), px(0.0)),
                ))
            },
            effect: Effect::Nothing,
        },
        Case {
            property: "background_image position",
            with: || {
                plain().background_image(tile(
                    BackgroundRepeat::Repeat,
                    BackgroundSize::AUTO,
                    (px(6.0), px(10.0)),
                ))
            },
            without: || {
                plain().background_image(tile(
                    BackgroundRepeat::Repeat,
                    BackgroundSize::AUTO,
                    (px(0.0), px(0.0)),
                ))
            },
            effect: Effect::Nothing,
        },
    ]
}

/// The five ways a mask can be written, all on a box filled edge to edge.
///
/// Each arm keeps a different part of the same square: a circle inscribed in
/// it, an ellipse filling it, a triangle, a left-to-right fade, and the
/// image's opaque half. All five moved together when masking landed, which is
/// what "one defect, one family" means here.
fn mask_cases() -> Vec<Case> {
    vec![
        Case {
            property: "mask shape",
            with: || filled().mask(Mask::Shape(MaskShape::Circle)),
            without: filled,
            effect: Effect::Draws,
        },
        Case {
            property: "mask ellipse",
            with: || filled().mask(Mask::Shape(MaskShape::Ellipse)),
            without: filled,
            effect: Effect::Draws,
        },
        Case {
            property: "mask path",
            with: || {
                filled().mask(Mask::Path {
                    data: "M36 4 L68 68 L4 68 Z".into(),
                    fill_rule: FillRule::NonZero,
                })
            },
            without: filled,
            effect: Effect::Draws,
        },
        Case {
            property: "mask gradient",
            with: || filled().mask(Mask::Gradient(fade())),
            without: filled,
            effect: Effect::Draws,
        },
        Case {
            property: "mask image",
            with: || {
                filled()
                    .mask(Mask::Image(ImageSource::Bytes(MASK_IMAGE.to_vec())))
            },
            without: filled,
            effect: Effect::Draws,
        },
    ]
}

/// What a paint property does to glyphs.
fn text_cases() -> Vec<Case> {
    vec![
        // Both of these drew nothing earlier today -- decoration was resolved
        // and never passed to the painter, and a centred line was laid out at
        // an infinite width and placed about infinity. Nothing else guards
        // them: no golden fixture sets either.
        Case {
            property: "text_decoration",
            with: || line().text_decoration(TextDecoration::Underline),
            without: line,
            effect: Effect::Draws,
        },
        // The control is left-aligned in the *same* width, so what is compared
        // is where the glyphs sit rather than how wide the box is.
        Case {
            property: "text_align",
            with: || wide().text_align(TextAlign::Center),
            without: || wide().text_align(TextAlign::Left),
            effect: Effect::Draws,
        },
        Case {
            property: "text_shadow",
            with: || {
                line().text_shadow(vec![TextShadow {
                    offset_x: 2.0,
                    offset_y: 2.0,
                    blur: 2.0,
                    color: Color::rgba(0x14, 0x14, 0x28, 0x8c),
                }])
            },
            without: line,
            effect: Effect::Draws,
        },
        // A cannot rather than a not-yet: `meo-skia-canvas`'s text style
        // carries `foreground_color` and no stroke width, so there is no
        // glyph-stroke call to make. Pinned here so that a binding that grows
        // one is noticed rather than waited for.
        Case {
            property: "text_stroke",
            with: || {
                line().text_stroke(TextStroke {
                    width: 1.0,
                    color: hex_rgb(0xdc_28_28),
                })
            },
            without: line,
            effect: Effect::Nothing,
        },
        // Reordering a stroke that is not drawn cannot show, so this one is
        // expected to come back with `text_stroke` rather than on its own.
        Case {
            property: "paint_order",
            with: || {
                line()
                    .text_stroke(TextStroke {
                        width: 1.0,
                        color: hex_rgb(0xdc_28_28),
                    })
                    .paint_order(PaintOrder::Stroke)
            },
            without: || {
                line().text_stroke(TextStroke {
                    width: 1.0,
                    color: hex_rgb(0xdc_28_28),
                })
            },
            effect: Effect::Nothing,
        },
        // Two things this control has to carry. The same `line_height`, so
        // that what is compared is where the text sits and not how tall the
        // box is -- a control without it reports `line_height` and calls it
        // `vertical_align`. And a **height taller than the text**, because
        // the property moves the paragraph by what the box has left over and
        // an auto-sized text node has nothing left over: every alignment
        // agrees on a box that is exactly its own content.
        Case {
            property: "vertical_align",
            with: || {
                line()
                    .line_height(2.0)
                    .height(px(60.0))
                    .vertical_align(VerticalAlign::Bottom)
            },
            without: || line().line_height(2.0).height(px(60.0)),
            effect: Effect::Draws,
        },
    ]
}

/// Renders one subject and returns its pixels.
fn pixels(subject: Element) -> Vec<u8> {
    let mut renderer = Renderer::new();
    // Off for the reason the fixture harness turns it off: the two rasterisers
    // do not agree to the byte, and a control pair compares bytes.
    renderer.set_gpu(false);
    renderer
        .register_font(FONT.0, FONT.1)
        .unwrap_or_else(|error| {
            unreachable!("the font did not register: {error}")
        });

    let mut canvas = Root::new(72.0, 72.0)
        .background_color(hex_rgb(0xee_ee_f2))
        .children(subject)
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    })
}

#[test]
fn every_paint_property_draws_what_it_is_recorded_as_drawing() {
    let mut wrong = Vec::new();

    for case in cases() {
        let differs = pixels((case.with)()) != pixels((case.without)());
        let found = if differs {
            Effect::Draws
        } else {
            Effect::Nothing
        };
        if found != case.effect {
            wrong.push(match case.effect {
                Effect::Draws => format!(
                    "{}: drew nothing. It is recorded as drawing, so this is a regression",
                    case.property
                ),
                Effect::Nothing => format!(
                    "{}: now draws. It was pinned as a no-op, so this is a fix -- change its row to `Effect::Draws`",
                    case.property
                ),
            });
        }
    }

    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
