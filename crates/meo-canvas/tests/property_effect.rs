//! Every paint property, drawn against the same scene without it, asking only
//! whether the two differ: a fixture cannot see a property that never drew,
//! since its image came from the code ignoring it. One test, so a fix that
//! moves a family reports all of it; the control is the part to get wrong.

use meo_canvas::{
    Box, Element, Format, Image, Renderer, Root, Style, Styled, Text, hex_rgb,
    px,
    scene::{
        BackgroundImage, BackgroundRepeat, BackgroundSize, BlendMode,
        BorderStyle, BoxShadow, Color, Dimension, FillRule, Gradient,
        GradientGeometry, GradientStop, ImageSource, Length, LineHeight,
        LinearDirection, Mask, MaskShape, PaintOrder, TextAlign,
        TextDecoration, TextShadow, TextStroke, Transform, VerticalAlign,
    },
    sides,
};

/// The family the text cases name, and its file: the repository's own font,
/// since a family resolved from the host's installed faces is not the same
/// family twice.
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
    Box::new()
        .size(px(72.0), px(72.0))
        .gradient(ramp())
        .children(child)
}

/// The square every compositing case puts over the gradient.
fn inner() -> Element {
    Box::new()
        .size(px(40.0), px(40.0))
        .margin(sides(px(16.0), px(16.0), px(16.0), px(16.0)))
        .background_color(hex_rgb(0x28_50_dc))
}

/// The same square, translucent, so what is behind it is still visible.
fn glass() -> Element {
    Box::new()
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
    Box::new().size(px(72.0), px(72.0)).gradient(Gradient {
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
    Box::new()
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
/// A tile the subject's box does not divide, so `Repeat`, `Round` and `Space`
/// are three pictures rather than one.
const AWKWARD_TILE: BackgroundSize =
    BackgroundSize::PerAxis(Dimension::Points(9.0), Dimension::Points(11.0));

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
    Box::new()
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

/// Every property this suite reads, and what it does today, in four lists split
/// where the subject changes: a square over a gradient, its shape, a box filled
/// edge to edge, a line of text.
fn cases() -> Vec<Case> {
    let mut all = composite_cases();
    all.extend(shape_cases());
    all.extend(background_image_cases());
    all.extend(background_tiling_cases());
    all.extend(frame_cases());
    all.extend(mask_cases());
    all.extend(text_cases());
    all.extend(font_feature_cases());
    all.extend(glyph_paint_cases());
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
        // Over a translucent square, since an opaque one covers the filtered
        // backdrop. `grayscale` rather than `blur`: blurring the linear ramp
        // behind returns the ramp, so a working blur would read as `Nothing`.
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
        // A border wide enough for a dash longer than a pixel, against `solid`
        // rather than no border, so the pair asks whether the style reaches
        // the painter.
        Case {
            property: "border_style",
            with: || {
                filled()
                    .border(sides(4.0, 4.0, 4.0, 4.0))
                    .border_color(hex_rgb(0x14_14_1e))
                    .border_style(BorderStyle::Dashed)
            },
            without: || {
                filled()
                    .border(sides(4.0, 4.0, 4.0, 4.0))
                    .border_color(hex_rgb(0x14_14_1e))
                    .border_style(BorderStyle::Solid)
            },
            effect: Effect::Draws,
        },
        // Dotted against dashed, not against solid. Both break the line, so a
        // renderer that dashed everything and ignored the keyword's *value*
        // would pass the row above and fail this one.
        Case {
            property: "border_style dotted",
            with: || {
                filled()
                    .border(sides(4.0, 4.0, 4.0, 4.0))
                    .border_color(hex_rgb(0x14_14_1e))
                    .border_style(BorderStyle::Dotted)
            },
            without: || {
                filled()
                    .border(sides(4.0, 4.0, 4.0, 4.0))
                    .border_color(hex_rgb(0x14_14_1e))
                    .border_style(BorderStyle::Dashed)
            },
            effect: Effect::Draws,
        },
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

/// A two-frame animation, frame 0 solid red and frame 1 solid blue, so a frame
/// index that reached nothing cannot pass by rounding. Written by this
/// repository's own encoder: 107 bytes, two pages at two frames a second.
const TWO_FRAMES: &[u8] = include_bytes!("assets/two-frames.gif");

/// An 8x8 image, left half opaque and right half clear: a mask is read for its
/// alpha, and `strip.png` is opaque in every pixel, so a case built on it would
/// report `Nothing`.
const MASK_IMAGE: &[u8] = include_bytes!("assets/mask-half.png");

/// The strip the background-image cases paint, and the fields that travel with
/// it: seven rows, since the source and its repeat, size and offset are
/// separate questions. Tiles are drawn one by one, since `Space` and `Round`
/// cannot be a repeating fill.
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
            effect: Effect::Draws,
        },
        // The two axes against each other rather than against `Repeat`: an
        // axis swapped for one keyword draws a picture for both, so a
        // pair against the unrepeated case would pass.
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
            effect: Effect::Draws,
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
            effect: Effect::Draws,
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
            effect: Effect::Draws,
        },
    ]
}

/// Which frame of an animated source is drawn.
///
/// Its own case rather than a row above, because the subject is an `Image`
/// node and every background row is a `Box`.
fn frame_cases() -> Vec<Case> {
    vec![Case {
        property: "image frame",
        with: || Image::bytes(TWO_FRAMES).size(px(40.0), px(40.0)).frame(1),
        without: || Image::bytes(TWO_FRAMES).size(px(40.0), px(40.0)),
        effect: Effect::Draws,
    }]
}

/// `Round` and `Space` against `Repeat`, on a tile the box does not divide: on
/// an even division all three draw one picture.
fn background_tiling_cases() -> Vec<Case> {
    vec![
        // A nine-wide tile in a box that is not a multiple of nine, so
        // `Repeat` runs a partial tile off the edge, `Round` scales to
        // fit a whole number and `Space` shares the remainder as gaps.
        Case {
            property: "background_image round",
            with: || {
                plain().background_image(tile(
                    BackgroundRepeat::Round,
                    AWKWARD_TILE,
                    (px(0.0), px(0.0)),
                ))
            },
            without: || {
                plain().background_image(tile(
                    BackgroundRepeat::Repeat,
                    AWKWARD_TILE,
                    (px(0.0), px(0.0)),
                ))
            },
            effect: Effect::Draws,
        },
        Case {
            property: "background_image space",
            with: || {
                plain().background_image(tile(
                    BackgroundRepeat::Space,
                    AWKWARD_TILE,
                    (px(0.0), px(0.0)),
                ))
            },
            without: || {
                plain().background_image(tile(
                    BackgroundRepeat::Round,
                    AWKWARD_TILE,
                    (px(0.0), px(0.0)),
                ))
            },
            effect: Effect::Draws,
        },
    ]
}

/// The five ways a mask can be written, on a box filled edge to edge: each arm
/// keeps a different part of the square.
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
        // One word split across two runs draws the same width as one run
        // carrying it: runs are styles, not words. The one row where agreement
        // is correct, so the pair's difference would be the defect.
        Case {
            property: "runs are not words",
            with: || {
                Text::rich([
                    ("Hx".to_owned(), Style::new()),
                    ("gp".to_owned(), Style::new()),
                ])
                .font_family(FONT.0)
                .font_size(14.0)
                .color(hex_rgb(0x14_14_1e))
            },
            without: || {
                Text::new("Hxgp")
                    .font_family(FONT.0)
                    .font_size(14.0)
                    .color(hex_rgb(0x14_14_1e))
            },
            effect: Effect::Nothing,
        },
    ]
}

/// What the font itself is asked to do, as against what is painted over it.
fn font_feature_cases() -> Vec<Case> {
    vec![
        // `DiagonalFractions`, not small caps: of seventeen OpenType tags
        // swept against the repository's Oswald, only `frac` moves
        // anything. The sample is `1/2`, since `frac` needs a slash to
        // act on.
        Case {
            property: "font_variant",
            with: || {
                Text::new("1/2 3/4 5/8")
                    .font_family(FONT.0)
                    .font_size(14.0)
                    .color(hex_rgb(0x14_14_1e))
                    .font_variant([
                        meo_canvas::scene::FontVariant::DiagonalFractions,
                    ])
            },
            without: || {
                Text::new("1/2 3/4 5/8")
                    .font_family(FONT.0)
                    .font_size(14.0)
                    .color(hex_rgb(0x14_14_1e))
            },
            effect: Effect::Draws,
        },
    ]
}

/// What a paint property does to glyphs, as against what the font does.
fn glyph_paint_cases() -> Vec<Case> {
    vec![
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
            effect: Effect::Draws,
        },
        // Reordering a stroke that is not drawn cannot show, so this pairs
        // with `text_stroke`.
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
            effect: Effect::Draws,
        },
        // The control carries the same `line_height`, so the pair measures
        // where the text sits and not the box's height, and a height taller
        // than the text, since alignment moves the paragraph by what the box
        // has left over.
        Case {
            property: "vertical_align",
            with: || {
                line()
                    .line_height(LineHeight::Number(2.0))
                    .height(px(60.0))
                    .vertical_align(VerticalAlign::Bottom)
            },
            without: || {
                line().line_height(LineHeight::Number(2.0)).height(px(60.0))
            },
            effect: Effect::Draws,
        },
        // Space between line boxes, so the subject has two lines, with its own
        // newline so neither side can break differently.
        Case {
            property: "line_gap",
            with: || pair().line_gap(12.0),
            without: pair,
            effect: Effect::Draws,
        },
        // A fixed box on both sides, so what moves is where the text sits:
        // text is laid out inside the border and the padding. The
        // border is transparent, since a painted one would move pixels
        // by drawing itself.
        Case {
            property: "text inside its border",
            with: || {
                line()
                    .size(px(64.0), px(48.0))
                    .border(sides(8.0, 8.0, 8.0, 8.0))
                    .border_style(BorderStyle::Solid)
                    .border_color(Color::rgba(0, 0, 0, 0))
            },
            without: || line().size(px(64.0), px(48.0)),
            effect: Effect::Draws,
        },
        Case {
            property: "text inside its padding",
            with: || {
                line().size(px(64.0), px(48.0)).padding(sides(
                    px(12.0),
                    px(0.0),
                    px(0.0),
                    px(12.0),
                ))
            },
            without: || line().size(px(64.0), px(48.0)),
            effect: Effect::Draws,
        },
    ]
}

/// Two lines, written as two lines, for the cases that need a gap between
/// them.
fn pair() -> Element {
    Text::new("Hxgp\nquick")
        .font_family(FONT.0)
        .font_size(14.0)
        .color(hex_rgb(0x14_14_1e))
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

    let mut canvas = Root::new(72.0)
        .height(72.0)
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
