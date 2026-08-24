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
//! A fix usually repairs a family -- `text_stroke` and `paint_order` were one
//! defect, and the five `mask` arms another -- so the useful report names
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
    Box as BoxNode, Element, Format, Image, Renderer, Root, Style, Styled,
    Text, hex_rgb, px,
    scene::{
        BackgroundImage, BackgroundRepeat, BackgroundSize, BlendMode,
        BorderStyle, BoxShadow, Color, Dimension, FillRule, Gradient,
        GradientGeometry, GradientStop, ImageSource, Length, LineHeight,
        LinearDirection, Mask, MaskShape, PaintOrder, TextAlign,
        TextDecoration, TextShadow, TextStroke, Transform, VerticalAlign,
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
        // A border wide enough for a dash to be longer than a pixel, on a box
        // long enough to hold several. Against `solid` rather than against no
        // border at all: the question is whether the *style* reaches the
        // painter, and a pair against an unbordered box would report the
        // border.
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

/// A two-frame animation: frame 0 solid red, frame 1 solid blue.
///
/// **Solid colours a channel apart, and deliberately.** A frame index that
/// reached nothing would draw frame 0 whatever the scene asked for, so the two
/// frames have to differ in a way no rounding could produce -- an animation
/// whose frames looked alike would pin "the property did nothing" exactly as
/// an opaque mask asset nearly did.
///
/// Written by this repository's own encoder rather than by hand: 107 bytes,
/// two pages at two frames a second.
const TWO_FRAMES: &[u8] = include_bytes!("assets/two-frames.gif");

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
/// Seven rows rather than one: the source is a different question from the
/// repeat, the size and the offset, and a source that draws while the three
/// are ignored is exactly the shape this file exists to tell apart -- which is
/// what it found. The painter drew the picture stretched to the box with
/// `draw_image_sized` and said so in a comment: repetition wants a pattern
/// shader.
///
/// It now tiles, the way v1 does -- by drawing the tiles rather than through a
/// pattern, because `Space` shares the leftover out between whole tiles and
/// `Round` scales them so a whole number fits, and a repeating fill can
/// express neither. Those two have rows of their own for that reason: they are
/// the pair a pattern-shader implementation would quietly collapse into
/// `Repeat`, and on a box the tile divides evenly all three are the same
/// picture -- so both rows use a tile the box does **not** divide.
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

/// `Round` and `Space` against `Repeat`, on a tile the box does not divide.
///
/// Their own function rather than two more rows above, because the tile has to
/// be the awkward one: on a box the tile divides evenly all three modes draw
/// the same picture, and a pair written with the ordinary tile would report
/// two working keywords as dead.
fn background_tiling_cases() -> Vec<Case> {
    vec![
        // A nine-wide tile in a box that is not a multiple of nine, so the
        // three modes are three pictures: `Repeat` runs a partial tile off
        // the far edge, `Round` scales the tile until a whole number fits,
        // and `Space` keeps the tile and shares the remainder out as gaps.
        // With a tile the box divides evenly all three agree, which is the
        // control mistake this pair of rows is written to avoid.
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
        // One word split across two runs must draw the same width as one run
        // carrying it whole: runs are styles, not words, and a painter that
        // puts an inter-word gap between every pair draws a space the text
        // does not contain. `Nothing` here means the two agree, which is the
        // one row in this file where agreement is the correct answer -- so it
        // is written as a pair whose *difference* would be the defect.
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
        // **`DiagonalFractions`, not small caps.** Seventeen OpenType tags
        // swept against this repository's Oswald move exactly one of them:
        // `frac`. The face has no small-caps glyphs and nothing synthesises
        // them, so a control written with `SmallCaps` would report a working
        // property as dead — the opaque mask asset, one layer in.
        //
        // The sample is a fraction for the same reason: `1/2` is what `frac`
        // acts on, and a string without a slash gives the feature nothing to
        // do however well it is plumbed.
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
        // Reordering a stroke that is not drawn cannot show, so this one came
        // back with `text_stroke` rather than on its own -- both landed with
        // the text port, and neither needed anything from the binding that
        // was not already public. `stroke_text` is what v1 calls, and moving
        // off the paragraph is what made it reachable.
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
                    .line_height(LineHeight::Number(2.0))
                    .height(px(60.0))
                    .vertical_align(VerticalAlign::Bottom)
            },
            without: || {
                line().line_height(LineHeight::Number(2.0)).height(px(60.0))
            },
            effect: Effect::Draws,
        },
        // Space **between** line boxes, so the subject needs two lines: on one
        // line there is no gap to add and the pair measures nothing. The text
        // carries its own newline rather than relying on a wrap, so the two
        // sides cannot differ in how they broke.
        //
        // Resolved, inherited, and read by nothing -- the same shape
        // `vertical_align` had. Pinned before the text port rather than after
        // it, so the port has to make it draw instead of being credited with
        // it afterwards.
        Case {
            property: "line_gap",
            with: || pair().line_gap(12.0),
            without: pair,
            effect: Effect::Draws,
        },
        // A fixed box on both sides, so what moves is where the text sits
        // inside it and not how big it is. Text is drawn from the border box
        // today: neither the padding nor the border is taken off before the
        // first glyph, so a padded text node's ink starts at the same pixel as
        // an unpadded one's. v1 lays text out inside border and padding, so
        // the text port closes this by construction.
        // The border half of the same question, with the same fixed box on
        // both sides. The border is transparent on purpose: a painted one
        // would move pixels by drawing itself, and the row would pass while
        // the text stayed exactly where it was.
        Case {
            property: "text inside its border",
            with: || {
                line()
                    .size(px(64.0), px(48.0))
                    .border(sides(8.0, 8.0, 8.0, 8.0))
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
