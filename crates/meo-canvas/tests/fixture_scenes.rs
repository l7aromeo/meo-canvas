//! The golden fixtures' scenes, as source rather than as opaque `scene.mcs`
//! bytes, and a test asserting each encodes to exactly the committed bytes. A
//! codec change is then a re-run of `just fixture-scenes`, and matching bytes
//! mean the picture cannot have moved.

use meo_canvas::{
    Align, Box, Display, Element, FlexDirection, FlexWrap, Image, Justify,
    ObjectFit, Overflow, Path, PositionType, Root, Styled, Text, corners,
    corners_all, hex_rgb, px,
    scene::{
        BackgroundImage, BackgroundRepeat, BackgroundSize, BorderStyle,
        BoxShadow, Color, Corners, Dimension, FillRule, FontWeight, Gradient,
        GradientGeometry, GradientStop, ImageSource, Length, LinearDirection,
        Mask, MaskShape, PathPaint, Scene, Sides, VerticalAlign, codec,
    },
    sides,
};

/// Every fixture whose scene is authored here.
fn scenes() -> Vec<(&'static str, Scene)> {
    vec![
        ("block-stacking", block_stacking()),
        ("block-stacking-relative", block_stacking_relative()),
        ("borders-per-edge", borders_per_edge()),
        ("box-shadow", box_shadow()),
        ("z-order", z_order()),
        ("overflow-clip", overflow_clip()),
        ("object-fit", object_fit()),
        ("gradients", gradients()),
        ("text-descenders", text_descenders()),
        ("baseline-alignment", baseline_alignment()),
        ("stacking-hoist", stacking_hoist()),
        ("mask-kinds", mask_kinds()),
        ("backdrop-filter", backdrop_filter()),
        ("vertical-align", vertical_align()),
        ("background-tiling", background_tiling()),
        ("borders-square", borders_square()),
        ("gradient-as-paint", gradient_as_paint()),
        ("gradient-linear", gradient_linear()),
        ("blend-modes", blend_modes()),
        ("flex-alignment", flex_alignment()),
        ("borders-dashed-square", borders_dashed_square()),
        ("borders-dashed-radius", borders_dashed_radius()),
    ]
}

/// Two boxes stacked by CSS's block rule, the second pulled up over the first.
///
/// `z_index` on the first is what puts it on top despite coming first in paint
/// order, and the negative top margin is what makes them overlap at all.
fn block_stacking() -> Scene {
    Root::new(100.0)
        .height(60.0)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children([
            Box::new()
                .size(px(80.0), px(40.0))
                .background_color(hex_rgb(0xdc_28_28))
                .z_index(5),
            Box::new()
                .size(px(80.0), px(40.0))
                .margin(sides(px(-20.0), px(0.0), px(0.0), px(20.0)))
                .background_color(hex_rgb(0x28_50_dc)),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// The same two boxes, positioned rather than static: with `block-stacking`, it
/// says the only difference `Relative` makes is what the offsets do.
fn block_stacking_relative() -> Scene {
    Root::new(100.0)
        .height(60.0)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children([
            Box::new()
                .position_type(PositionType::Relative)
                .size(px(80.0), px(40.0))
                .background_color(hex_rgb(0xdc_28_28))
                .z_index(5),
            Box::new()
                .position_type(PositionType::Relative)
                .size(px(80.0), px(40.0))
                .margin(sides(px(-20.0), px(0.0), px(0.0), px(20.0)))
                .background_color(hex_rgb(0x28_50_dc)),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// Three boxes whose borders differ per edge in width, colour and radius. The
/// second matters: a `None` bottom colour falls back to `border_color_all`, a
/// split one `borderColor` hides.
fn borders_per_edge() -> Scene {
    let row = |background: u32, border: Sides<f32>, radius: Corners<f32>| {
        Box::new()
            .position_type(PositionType::Relative)
            .size(px(120.0), px(80.0))
            .border(border)
            .border_style(BorderStyle::Solid)
            .background_color(hex_rgb(background))
            .border_radius_corners(radius)
    };

    Root::new(440.0)
        .height(120.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .align_items(Align::Center)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .children([
            row(0xfa_f5_e6, sides(4.0, 4.0, 4.0, 4.0), corners_all(12.0))
                .border_color(hex_rgb(0x3c_3c_5a)),
            row(
                0xeb_f5_fa,
                sides(2.0, 8.0, 5.0, 0.0),
                corners(20.0, 0.0, 10.0, 4.0),
            )
            .border_color_sides(sides(
                Some(hex_rgb(0xc8_28_28)),
                Some(hex_rgb(0x28_8c_3c)),
                None,
                Some(hex_rgb(0x28_3c_c8)),
            ))
            .border_color(hex_rgb(0x78_78_78)),
            row(0xff_fa_f0, sides(10.0, 2.0, 10.0, 2.0), corners_all(24.0))
                .border_color_sides(sides(
                    Some(hex_rgb(0xc8_28_28)),
                    Some(hex_rgb(0x28_8c_3c)),
                    Some(hex_rgb(0x28_3c_c8)),
                    Some(hex_rgb(0xe6_aa_1e)),
                ))
                .border_color(hex_rgb(0x78_78_78)),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// Four shadows, each isolating one term: offset, blur, spread and inset. The
/// inset one is the term whose ink lands inside the card, drawn after the
/// background it falls on, so no outer shadow can stand in for it.
fn box_shadow() -> Scene {
    let card = |shadow: BoxShadow| {
        Box::new()
            .position_type(PositionType::Relative)
            .size(px(72.0), px(72.0))
            .background_color(hex_rgb(0xff_ff_ff))
            .border_radius_corners(corners_all(10.0))
            .box_shadow(vec![shadow])
    };
    let ink = Color::rgba(20, 20, 40, 110);

    Root::new(410.0)
        .height(140.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .justify_content(Justify::SpaceEvenly)
        .align_items(Align::Center)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xfa_fa_fc))
        .children([
            card(BoxShadow {
                inset: false,
                offset_x: 6.0,
                offset_y: 6.0,
                blur: 0.0,
                spread: 0.0,
                color: ink,
            }),
            card(BoxShadow {
                inset: false,
                offset_x: 4.0,
                offset_y: 4.0,
                blur: 10.0,
                spread: 0.0,
                color: ink,
            }),
            card(BoxShadow {
                inset: false,
                offset_x: 0.0,
                offset_y: 0.0,
                blur: 6.0,
                spread: 6.0,
                color: ink,
            }),
            // Offset as well as blurred, so the ink is heavier along the top
            // and left edges than the bottom and right ones: an inset shadow
            // drawn without its offset is symmetric, and a symmetric ring is
            // one a reader cannot tell from a border.
            card(BoxShadow {
                inset: true,
                offset_x: 5.0,
                offset_y: 5.0,
                blur: 8.0,
                spread: 0.0,
                color: ink,
            }),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// Four overlapping cards whose paint order is decided by `z_index`.
///
/// One positive, one negative and two unset, so the fixture pins the ordering
/// among the three cases rather than only that a higher index wins.
fn z_order() -> Scene {
    let card = |offset: f32, background: Color| {
        Box::new()
            .position_type(PositionType::Absolute)
            .position(sides(Some(px(offset)), None, None, Some(px(offset))))
            .size(px(90.0), px(70.0))
            .background_color(background)
    };

    Root::new(220.0)
        .height(140.0)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff))
        .children([
            card(20.0, Color::rgba(220, 60, 60, 230)).z_index(2),
            card(38.0, Color::rgba(40, 140, 70, 230)).z_index(-1),
            card(56.0, Color::rgba(40, 80, 210, 230)),
            card(74.0, Color::rgba(250, 200, 40, 230)),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// The same overflowing child in a clipping box and an unclipped one.
///
/// The pair is the check: one picture cannot say whether the clip happened or
/// the child simply fitted.
fn overflow_clip() -> Scene {
    let child = || {
        Box::new()
            .position_type(PositionType::Absolute)
            .position(sides(Some(px(20.0)), None, None, Some(px(30.0))))
            .size(px(120.0), px(90.0))
            .background_color(Color::rgba(220, 60, 60, 200))
    };

    Root::new(300.0)
        .height(120.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .align_items(Align::FlexStart)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .children([
            Box::new()
                .position_type(PositionType::Relative)
                .size(px(110.0), px(70.0))
                .overflow(Overflow::Hidden)
                .background_color(hex_rgb(0xe8_ec_f5))
                .border_radius_corners(corners_all(16.0))
                .children(child()),
            Box::new()
                .position_type(PositionType::Relative)
                .size(px(110.0), px(70.0))
                .background_color(hex_rgb(0xe8_ec_f5))
                .children(child()),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// The picture every `object-fit` cell draws: eight by four and asymmetric,
/// with a magenta column at x=0 and a cyan one at x=7, so `Cover` on a square
/// box visibly drops both edges and `Fill` keeps them.
const FIT_MARKS: &[u8] = include_bytes!("assets/fit-marks.png");

/// The picture the other image cases draw: four quadrants of alternating
/// pixels, recognisable where the edges do not matter.
const STRIP: &[u8] = include_bytes!("assets/strip.png");

/// One clipped box per `ObjectFit`, all showing the same picture, since the
/// fits are only meaningful against each other.
fn object_fit() -> Scene {
    let cell = |fit: ObjectFit| {
        Box::new()
            .position_type(PositionType::Relative)
            .size(px(72.0), px(72.0))
            .overflow(Overflow::Hidden)
            .background_color(hex_rgb(0xf0_f0_f0))
            .children(
                Image::bytes(FIT_MARKS)
                    .position_type(PositionType::Relative)
                    .size(px(72.0), px(72.0))
                    .object_fit(fit),
            )
    };

    Root::new(420.0)
        .height(110.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .align_items(Align::Center)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .children([
            cell(ObjectFit::Fill),
            cell(ObjectFit::Contain),
            cell(ObjectFit::Cover),
            cell(ObjectFit::None),
            cell(ObjectFit::ScaleDown),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// The three gradient geometries, sharing one ramp.
///
/// One ramp across all three so a difference between the cells is the geometry
/// rather than the colours.
fn gradients() -> Scene {
    let ramp = || {
        vec![
            GradientStop {
                offset: 0.0,
                color: hex_rgb(0xf0_3c_3c),
            },
            GradientStop {
                offset: 0.5,
                color: hex_rgb(0xfa_d2_3c),
            },
            GradientStop {
                offset: 1.0,
                color: hex_rgb(0x28_5a_d2),
            },
        ]
    };
    let centre = (Length::Percent(0.5), Length::Percent(0.5));
    let cell = |geometry: GradientGeometry| {
        Box::new().size(px(96.0), px(96.0)).gradient(Gradient {
            geometry,
            stops: ramp(),
        })
    };

    Root::new(330.0)
        .height(120.0)
        .padding(px(8.0))
        .align_items(Align::Center)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .children([
            cell(GradientGeometry::Linear {
                direction: LinearDirection::Angle(45.0),
            }),
            cell(GradientGeometry::Radial { at: centre }),
            cell(GradientGeometry::Conic {
                at: centre,
                from: 45.0,
            }),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// One string at three sizes, chosen for what hangs below the baseline.
///
/// `gjpqy` are the descenders, `Hxg` gives a cap height and an x-height beside
/// one, and the digits are what a font's own metrics most often disagree about.
fn text_descenders() -> Scene {
    let line = |size: f32, weight: u16| {
        Text::new("gjpqy Hxg 0123")
            .position_type(PositionType::Relative)
            .font_family("Fixture")
            .font_size(size)
            .font_weight(FontWeight::new(weight))
            .color(hex_rgb(0x14_14_1e))
    };

    Root::new(320.0)
        .height(140.0)
        .position_type(PositionType::Relative)
        .padding(px(10.0))
        .flex_direction(FlexDirection::Column)
        .gap_xy(px(6.0), px(0.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .children([line(14.0, 400), line(22.0, 400), line(34.0, 700)])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// Two rows of mixed type sizes, one aligned on baselines and one on tops. The
/// top row is the control: a baseline and a bottom edge differ only between
/// boxes of different heights.
fn baseline_alignment() -> Scene {
    let word = |size: f32| {
        Text::new("Hxgp")
            .position_type(PositionType::Relative)
            .font_family("Fixture")
            .font_size(size)
            .font_weight(FontWeight::new(400))
            .color(hex_rgb(0x14_14_1e))
    };
    let row = |align: Align, name: &str| {
        Box::new()
            .position_type(PositionType::Relative)
            .padding(px(8.0))
            .align_items(align)
            .gap_xy(px(0.0), px(8.0))
            .background_color(hex_rgb(0xee_ee_f4))
            .name(name)
            .children([word(14.0), word(22.0), word(34.0)])
    };

    Root::new(360.0).height(190.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .flex_direction(FlexDirection::Column)
        .gap_xy(px(10.0), px(0.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .name("PINNED DEFECT: row 0 aligns on box bottoms, not baselines. See notes.json.")
        .children([
            row(Align::Baseline, "align-items: baseline - DEFECTIVE, bottoms align"),
            row(Align::FlexStart, "align-items: flex-start - control row"),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// A `z_index: -1` child under three parents. Under a parent establishing no
/// stacking context it hoists to the grandparent and the parent's background
/// covers it; the third parent's opacity makes a real context that keeps it.
/// `overflow: hidden` is the trigger wrongly assumed to make one.
fn stacking_hoist() -> Scene {
    let child = || {
        Box::new()
            .position_type(PositionType::Absolute)
            .position(sides(Some(px(10.0)), None, None, Some(px(10.0))))
            .size(px(36.0), px(36.0))
            .background_color(hex_rgb(0x28_50_dc))
            .z_index(-1)
    };
    let cell = |name: &str| {
        Box::new()
            .position_type(PositionType::Relative)
            .size(px(56.0), px(56.0))
            .background_color(hex_rgb(0xdc_28_28))
            .name(name)
            .children(child())
    };

    Root::new(200.0).height(72.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .align_items(Align::Center)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .name("cells 0 and 1 hoist the child and hide it; cell 2 keeps it. See notes.json.")
        .children([
            cell("no context - the child hoists and the parent's red covers it"),
            cell("overflow: hidden - clipping is not a context, so it hoists too")
                .overflow(Overflow::Hidden),
            // Opacity below one establishes a context in Chrome and opens a
            // layer here, so this cell must stay correct. 0.99, since the cell
            // is about the context rather than the blend.
            cell("opacity: 0.99 - CONTROL, a real context keeps its child")
                .opacity(0.99),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// The image `mask-kinds` reads for its alpha: eight by eight, left half opaque
/// and right half clear, since every pixel of `STRIP` is opaque and a mask
/// built on it keeps the whole box.
const MASK_IMAGE: &[u8] = include_bytes!("assets/mask-half.png");

/// The five ways a mask can be written, beside the unmasked control each cell
/// is read against at named points. The cells are wider than tall, since in a
/// square box the largest circle and the filling ellipse are the same shape.
fn mask_kinds() -> Scene {
    let cell = |name: &str, mask: Option<Mask>| {
        let box_node = Box::new()
            .position_type(PositionType::Relative)
            .size(px(56.0), px(40.0))
            .background_color(hex_rgb(0x28_50_dc))
            .name(name);
        match mask {
            Some(mask) => box_node.mask(mask),
            None => box_node,
        }
    };
    // Opaque to clear, left to right, so the gradient arm's alpha is read
    // along a row rather than at one point.
    let fade = Gradient {
        geometry: GradientGeometry::Linear {
            direction: LinearDirection::Angle(90.0),
        },
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgb(0xff, 0xff, 0xff),
            },
            GradientStop {
                offset: 1.0,
                color: Color::TRANSPARENT,
            },
        ],
    };

    Root::new(392.0).height(56.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .align_items(Align::Center)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .name("cell 0 carries no mask and is the control. See notes.json.")
        .children([
            cell("no mask - CONTROL, the whole box is blue", None),
            cell(
                "circle - the largest that fits, so it is as wide as the box is tall",
                Some(Mask::Shape(MaskShape::Circle)),
            ),
            cell(
                "ellipse - fills the box, so it reaches both ends of the middle row",
                Some(Mask::Shape(MaskShape::Ellipse)),
            ),
            cell(
                "path - a triangle with its apex at the top middle",
                Some(Mask::Path {
                    data: "M28 2 L54 38 L2 38 Z".to_owned(),
                    fill_rule: FillRule::NonZero,
                }),
            ),
            cell(
                "gradient - opaque at the left edge and clear at the right",
                Some(Mask::Gradient(fade)),
            ),
            cell(
                "image - the left half of an 8x8 whose right half is clear",
                Some(Mask::Image(ImageSource::Bytes(MASK_IMAGE.to_vec()))),
            ),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// Three translucent panels over stripes, two filtered: a backdrop filter is
/// invisible under an opaque box, the obvious and broken control. `grayscale`
/// and `blur` fail differently, and stripes give the blur edges to soften.
fn backdrop_filter() -> Scene {
    let stripe = |color: Color| {
        Box::new()
            .position_type(PositionType::Relative)
            .size(px(22.0), px(72.0))
            .background_color(color)
    };
    let stripes = Box::new()
        .position_type(PositionType::Absolute)
        .position(sides(Some(px(0.0)), None, None, Some(px(0.0))))
        .size(px(264.0), px(72.0))
        .children(
            (0..12)
                .map(|index| {
                    stripe(match index % 2 {
                        0 => Color::rgb(0xdc, 0x28, 0x28),
                        _ => Color::rgb(0x28, 0x50, 0xdc),
                    })
                })
                .collect::<Vec<_>>(),
        );
    let panel = |name: &str, left: f32, filter: Option<&str>| {
        let box_node = Box::new()
            .position_type(PositionType::Absolute)
            .position(sides(Some(px(8.0)), None, None, Some(px(left))))
            .size(px(72.0), px(56.0))
            .background_color(Color::rgba(0xff, 0xff, 0xff, 0x40))
            .name(name);
        match filter {
            Some(filter) => box_node.backdrop_filter(filter),
            None => box_node,
        }
    };

    Root::new(264.0)
        .height(72.0)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff))
        .name("panel 0 filters nothing and is the control. See notes.json.")
        .children([
            stripes,
            panel(
                "CONTROL - translucent white over the stripes, unfiltered",
                8.0,
                None,
            ),
            panel(
                "grayscale(1) - the stripes behind it lose their colour",
                96.0,
                Some("grayscale(1)"),
            ),
            panel(
                "blur(4px) - the stripe edges behind it soften",
                184.0,
                Some("blur(4px)"),
            ),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// One line of text in three boxes taller than it, aligned three ways: the
/// property moves the paragraph by what the box has left over, so a box sized
/// to its text shows all three agreeing. `Top` is the control.
fn vertical_align() -> Scene {
    let cell = |name: &str, align: VerticalAlign| {
        Box::new()
            .position_type(PositionType::Relative)
            .size(px(80.0), px(72.0))
            .background_color(hex_rgb(0xf0_f0_f0))
            .name(name)
            .children(
                Text::new("Hxgp")
                    .position_type(PositionType::Relative)
                    .size(px(80.0), px(72.0))
                    .font_family("Fixture")
                    .font_size(18.0)
                    .color(hex_rgb(0x14_14_1e))
                    .vertical_align(align),
            )
    };

    Root::new(280.0)
        .height(88.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .align_items(Align::Center)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .name("cell 0 is Top, the default, and is the control. See notes.json.")
        .children([
            cell(
                "top - CONTROL, the default every other text fixture draws",
                VerticalAlign::Top,
            ),
            cell(
                "middle - the paragraph centred in what the box has left over",
                VerticalAlign::Middle,
            ),
            cell(
                "bottom - the paragraph against the bottom of the box",
                VerticalAlign::Bottom,
            ),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// The six ways a background picture tiles, beside the one that does not. Cells
/// 4 and 5 use a tile the cell does not divide, since on an even division
/// `Repeat`, `Space` and `Round` draw one picture.
fn background_tiling() -> Scene {
    let tile =
        |repeat: BackgroundRepeat, size: BackgroundSize| BackgroundImage {
            source: ImageSource::Bytes(STRIP.to_vec()),
            repeat,
            size,
            position: (Length::ZERO, Length::ZERO),
        };
    let cell = |name: &str, background: BackgroundImage| {
        Box::new()
            .position_type(PositionType::Relative)
            .size(px(56.0), px(40.0))
            .background_color(hex_rgb(0xff_ff_ff))
            .background_image(background)
            .name(name)
    };
    // Twenty-eight by twenty: two tiles across and two down in a 56 by 40
    // cell, so a single tile is a quarter of the cell and every mode is
    // legible at a glance rather than at the pixel.
    let plain = BackgroundSize::PerAxis(
        Dimension::Points(28.0),
        Dimension::Points(20.0),
    );
    // Fifteen by thirteen against the same cell: three across with eleven
    // left over, and three down with one. That remainder is what separates
    // `Space` from `Round` from `Repeat`.
    let awkward = BackgroundSize::PerAxis(
        Dimension::Points(15.0),
        Dimension::Points(13.0),
    );

    Root::new(392.0).height(56.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .align_items(Align::Center)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .name("cell 0 draws one tile and is the control. See notes.json.")
        .children([
            cell(
                "no-repeat - CONTROL, one tile at the corner and white after it",
                tile(BackgroundRepeat::NoRepeat, plain),
            ),
            cell(
                "repeat - tiled on both axes",
                tile(BackgroundRepeat::Repeat, plain),
            ),
            cell(
                "repeat-x - one row of tiles, white beneath it",
                tile(BackgroundRepeat::RepeatX, plain),
            ),
            cell(
                "repeat-y - one column of tiles, white beside it",
                tile(BackgroundRepeat::RepeatY, plain),
            ),
            cell(
                "space - whole tiles with the remainder shared out as gaps",
                tile(BackgroundRepeat::Space, awkward),
            ),
            cell(
                "round - the tile scaled until a whole number fits",
                tile(BackgroundRepeat::Round, awkward),
            ),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// A bordered box with square corners beside two that are not: `box_path`
/// builds square and rounded boxes by different mechanisms, and a mismatched
/// ring left a diagonal wedge. `notes.json` names a row per edge, since the
/// middle row never crosses the top or bottom.
fn borders_square() -> Scene {
    let cell = |radius: Corners<f32>, border: Sides<f32>| {
        Box::new()
            .position_type(PositionType::Relative)
            .size(px(56.0), px(56.0))
            .border(border)
            .border_style(BorderStyle::Solid)
            .border_radius_corners(radius)
            .border_color(hex_rgb(0x28_50_dc))
            .background_color(hex_rgb(0xdc_28_28))
    };

    Root::new(200.0)
        .height(72.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .align_items(Align::Center)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .name("read a row per edge, not the middle one alone. See notes.json.")
        .children([
            cell(corners_all(0.0), sides(6.0, 6.0, 6.0, 6.0)).name(
                "square corners - the branch that mixed two contour mechanisms",
            ),
            cell(corners_all(12.0), sides(6.0, 6.0, 6.0, 6.0))
                .name("rounded - CONTROL, the branch that was always right"),
            cell(corners_all(0.0), sides(2.0, 10.0, 6.0, 14.0))
                .name("square, per-edge widths - the third branch"),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// One gradient in each of its four roles -- background, path fill, path
/// stroke, mask -- over their flat controls. Cells are 72 by 48 with a
/// left-to-right ramp, since a square cell with a diagonal ramp hides an axis
/// swap.
fn gradient_as_paint() -> Scene {
    let ramp = || {
        vec![
            GradientStop {
                offset: 0.0,
                color: hex_rgb(0xf0_3c_3c),
            },
            GradientStop {
                offset: 0.5,
                color: hex_rgb(0xfa_d2_3c),
            },
            GradientStop {
                offset: 1.0,
                color: hex_rgb(0x28_5a_d2),
            },
        ]
    };
    // Left to right, so the ramp crosses the long axis of the cell and a
    // direction read off the wrong axis is a different picture rather than a
    // rotated one.
    let across = || Gradient {
        geometry: GradientGeometry::Linear {
            direction: LinearDirection::Angle(90.0),
        },
        stops: ramp(),
    };
    // The same geometry as an alpha ramp, which is what a mask reads.
    let fade = || Gradient {
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
    };

    let cell = || {
        Box::new()
            .position_type(PositionType::Relative)
            .size(px(72.0), px(48.0))
            .background_color(hex_rgb(0xf4_f4_f6))
    };
    // A triangle rather than a rectangle: a path filled with a gradient and a
    // box painted with one would be the same picture in a rectangle, and the
    // point of these two cells is that they are different code.
    let triangle = "M6 42 L36 6 L66 42 Z";
    let path = || {
        Path::d(triangle)
            .position_type(PositionType::Relative)
            .size(px(72.0), px(48.0))
    };

    Root::new(328.0)
        .height(128.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .flex_direction(FlexDirection::Column)
        .gap_xy(px(8.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .name("one gradient in four roles, over four controls. See notes.json.")
        .children([
            gradient_roles(&cell, &path, &across, &fade),
            flat_roles(&cell, &path),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// The row where every role takes the gradient.
fn gradient_roles(
    cell: &dyn Fn() -> Element,
    path: &dyn Fn() -> Element,
    across: &dyn Fn() -> Gradient,
    fade: &dyn Fn() -> Gradient,
) -> Element {
    Box::new()
        .position_type(PositionType::Relative)
        .gap_xy(px(0.0), px(8.0))
        .children([
            cell().gradient(across()).name("background"),
            cell().children(
                path()
                    .fill(Some(PathPaint::Gradient(across())))
                    .name("path fill"),
            ),
            cell().children(
                path()
                    .fill(None)
                    .stroke(Some(PathPaint::Gradient(across())))
                    .line_width(8.0)
                    .name("path stroke"),
            ),
            cell()
                .background_color(hex_rgb(0x28_5a_d2))
                .mask(Mask::Gradient(fade()))
                .name("mask"),
        ])
}

/// The control row: the same four roles in one flat colour, and the mask cell
/// unmasked -- a mask has no flat equivalent, so what its cell is read against
/// is the shape it was applied to.
fn flat_roles(
    cell: &dyn Fn() -> Element,
    path: &dyn Fn() -> Element,
) -> Element {
    let flat = hex_rgb(0xfa_d2_3c);
    Box::new()
        .position_type(PositionType::Relative)
        .gap_xy(px(0.0), px(8.0))
        .children([
            cell()
                .background_color(flat)
                .name("CONTROL flat background"),
            cell().children(
                path()
                    .fill(Some(PathPaint::Solid(flat)))
                    .name("CONTROL flat path fill"),
            ),
            cell().children(
                path()
                    .fill(None)
                    .stroke(Some(PathPaint::Solid(flat)))
                    .line_width(8.0)
                    .name("CONTROL flat path stroke"),
            ),
            cell()
                .background_color(hex_rgb(0x28_5a_d2))
                .name("CONTROL the same box unmasked"),
        ])
}

/// A linear gradient in each direction, plus both forms. Axis-aligned cells
/// catch an axis swap a diagonal hides, and `Angle(0.0)` points up, as CSS
/// measures. The last two take `LinearDirection::Between`, one stopping short
/// so the remainder takes the last stop.
fn gradient_linear() -> Scene {
    // Two stops rather than three: a direction is read from where the ends
    // are, and a midpoint would only add a colour to describe.
    let ends = || {
        vec![
            GradientStop {
                offset: 0.0,
                color: hex_rgb(0xf0_3c_3c),
            },
            GradientStop {
                offset: 1.0,
                color: hex_rgb(0x28_5a_d2),
            },
        ]
    };
    let cell = |geometry: GradientGeometry| {
        Box::new()
            .position_type(PositionType::Relative)
            // 88 by 56: a direction read off the wrong axis lands somewhere a
            // square cell would have hidden.
            .size(px(88.0), px(56.0))
            .gradient(Gradient {
                geometry,
                stops: ends(),
            })
    };
    let angle = |degrees: f32| {
        cell(GradientGeometry::Linear {
            direction: LinearDirection::Angle(degrees),
        })
    };
    let between = |start, end| {
        cell(GradientGeometry::Linear {
            direction: LinearDirection::Between { start, end },
        })
    };
    let row = |children: Vec<Element>| {
        Box::new()
            .position_type(PositionType::Relative)
            .gap_xy(px(0.0), px(8.0))
            .children(children)
    };

    Root::new(392.0).height(136.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .flex_direction(FlexDirection::Column)
        .gap_xy(px(8.0), px(0.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .name("linear gradients: four axes, an odd angle, and both forms. See notes.json.")
        .children([
            row(vec![
                angle(0.0).name("0deg - red at the BOTTOM, this cell pins the sign"),
                angle(90.0).name("90deg - to the right"),
                angle(180.0).name("180deg - to the bottom"),
                angle(270.0).name("270deg - to the left"),
            ]),
            row(vec![
                angle(30.0).name("30deg - not symmetric, unlike 45"),
                between(
                    (Length::Percent(0.0), Length::Percent(0.0)),
                    (Length::Percent(1.0), Length::Percent(0.5)),
                )
                .name("between: corner to mid-right"),
                between(
                    (Length::Percent(0.0), Length::Percent(0.0)),
                    (Length::Percent(0.5), Length::Percent(0.5)),
                )
                .name("between: stops short, the rest takes the last stop"),
                // Two stops of one colour: whatever the geometry does, this
                // cell is flat. It is what says a difference between the other
                // seven is the direction and not the ramp.
                cell(GradientGeometry::Linear {
                    direction: LinearDirection::Angle(30.0),
                })
                .gradient(Gradient {
                    geometry: GradientGeometry::Linear {
                        direction: LinearDirection::Angle(30.0),
                    },
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            color: hex_rgb(0xf0_3c_3c),
                        },
                        GradientStop {
                            offset: 1.0,
                            color: hex_rgb(0xf0_3c_3c),
                        },
                    ],
                })
                .name("CONTROL one colour twice - flat whatever the angle"),
            ]),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// Every `BlendMode` over the same backdrop, with the backdrop alone. The
/// backdrop is a ramp, since on a flat colour `Multiply` agrees with `Darken`
/// and `Screen` with `Lighten`; the empty cell is what `Normal` is read
/// against.
fn blend_modes() -> Scene {
    use meo_canvas::scene::BlendMode;

    let backdrop = || Gradient {
        geometry: GradientGeometry::Linear {
            direction: LinearDirection::Angle(90.0),
        },
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: hex_rgb(0x18_18_38),
            },
            GradientStop {
                offset: 1.0,
                color: hex_rgb(0xf0_e0_a0),
            },
        ],
    };
    let cell = |mode: Option<BlendMode>| {
        let mut source = Box::new()
            .position_type(PositionType::Relative)
            .size(px(36.0), px(24.0))
            .margin(sides(px(8.0), px(0.0), px(0.0), px(10.0)))
            .background_color(hex_rgb(0x40_90_c0));
        if let Some(mode) = mode {
            source = source.mix_blend_mode(mode);
        }
        let cell = Box::new()
            .position_type(PositionType::Relative)
            .size(px(56.0), px(40.0))
            .gradient(backdrop());
        match mode {
            None => cell,
            Some(_) => cell.children(source),
        }
    };
    let row = |children: Vec<Element>| {
        Box::new()
            .position_type(PositionType::Relative)
            .gap_xy(px(0.0), px(6.0))
            .children(children)
    };

    Root::new(382.0)
        .height(148.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .flex_direction(FlexDirection::Column)
        .gap_xy(px(6.0), px(0.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .name("every blend mode over one ramp. See notes.json.")
        .children([
            row(vec![
                cell(Some(BlendMode::Normal)).name("normal"),
                cell(Some(BlendMode::Multiply)).name("multiply"),
                cell(Some(BlendMode::Screen)).name("screen"),
                cell(Some(BlendMode::Overlay)).name("overlay"),
                cell(Some(BlendMode::Darken)).name("darken"),
                cell(Some(BlendMode::Lighten)).name("lighten"),
            ]),
            row(vec![
                cell(Some(BlendMode::ColorDodge)).name("color-dodge"),
                cell(Some(BlendMode::ColorBurn)).name("color-burn"),
                cell(Some(BlendMode::HardLight)).name("hard-light"),
                cell(Some(BlendMode::SoftLight)).name("soft-light"),
                cell(Some(BlendMode::Difference)).name("difference"),
                cell(Some(BlendMode::Exclusion)).name("exclusion"),
            ]),
            row(vec![
                cell(Some(BlendMode::Hue)).name("hue"),
                cell(Some(BlendMode::Saturation)).name("saturation"),
                cell(Some(BlendMode::Color)).name("color"),
                cell(Some(BlendMode::Luminosity)).name("luminosity"),
                cell(None).name("CONTROL the backdrop with no source"),
            ]),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// Five cross-axis alignments and the wrapping the Chrome matrix cannot show:
/// `Stretch` and `Wrap` change a child's own size. Children are sized by a
/// spacer inside, or `Stretch` draws as `FlexStart`.
fn flex_alignment() -> Scene {
    let child = |width: f32, content: f32, ink: Color| {
        Box::new()
            .position_type(PositionType::Relative)
            .width(px(width))
            .background_color(ink)
            .children(
                Box::new()
                    .position_type(PositionType::Relative)
                    .height(px(content)),
            )
    };
    let children = || {
        vec![
            child(24.0, 20.0, hex_rgb(0xdc_28_28)),
            child(30.0, 32.0, hex_rgb(0x28_50_dc)),
            child(20.0, 44.0, hex_rgb(0x28_8c_3c)),
        ]
    };
    // Six children in six distinct colours: identical colours in identical
    // columns make wrap and wrap-reverse one picture, and a per-colour bounding
    // box merges the two lines.
    let six = || {
        vec![
            child(24.0, 20.0, hex_rgb(0xdc_28_28)),
            child(30.0, 32.0, hex_rgb(0x28_50_dc)),
            child(20.0, 44.0, hex_rgb(0x28_8c_3c)),
            child(24.0, 20.0, hex_rgb(0xe6_a0_1e)),
            child(30.0, 32.0, hex_rgb(0x96_3c_be)),
            child(20.0, 44.0, hex_rgb(0x1e_aa_b4)),
        ]
    };
    let cell = |align: Align| {
        Box::new()
            .position_type(PositionType::Relative)
            .size(px(88.0), px(56.0))
            .background_color(hex_rgb(0xf0_f0_f4))
            .align_items(align)
            .justify_content(Justify::SpaceBetween)
            .children(children())
    };

    Root::new(392.0).height(136.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .flex_direction(FlexDirection::Column)
        .gap_xy(px(8.0), px(0.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .name("the five cross-axis alignments, and wrapping. See notes.json.")
        .children([
            Box::new()
                .position_type(PositionType::Relative)
                .gap_xy(px(0.0), px(8.0))
                .children([
                    cell(Align::FlexStart).name("flex-start"),
                    cell(Align::Center).name("center"),
                    cell(Align::FlexEnd).name("flex-end"),
                    cell(Align::Stretch).name("stretch - the one that resizes"),
                ]),
            Box::new()
                .position_type(PositionType::Relative)
                .gap_xy(px(0.0), px(8.0))
                .children([
                    cell(Align::Baseline).name("baseline - on boxes, the bottom edge"),
                    // Wrap needs more children than fit, so this cell has six.
                    Box::new()
                        .position_type(PositionType::Relative)
                        .size(px(88.0), px(56.0))
                        .background_color(hex_rgb(0xf0_f0_f4))
                        .flex_wrap(FlexWrap::Wrap)
                        .children(six())
                        .name("wrap - six children in a box that fits three"),
                    Box::new()
                        .position_type(PositionType::Relative)
                        .size(px(88.0), px(56.0))
                        .background_color(hex_rgb(0xf0_f0_f4))
                        .flex_wrap(FlexWrap::WrapReverse)
                        .children(six())
                        .name("wrap-reverse - the same six, lines the other way up"),
                    Box::new()
                        .position_type(PositionType::Relative)
                        .size(px(88.0), px(56.0))
                        .background_color(hex_rgb(0xf0_f0_f4))
                        .flex_direction(FlexDirection::ColumnReverse)
                        .children(children())
                        .name("column-reverse - the axis and the order both flip"),
                ]),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// A dashed border with square corners, at the width the defect lived at:
/// Chrome fits dashes to the outer edge, `8, 5, 8, 6, 8, 5, 8` on a 48-pixel
/// side. 240 by 48 with a 4-wide border, the box the Chrome table measured, so
/// `notes.json` names Chrome's own runs.
fn borders_dashed_square() -> Scene {
    Root::new(240.0)
        .height(48.0)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff))
        .name("a dashed square border. Chrome's runs are in notes.json.")
        .children(
            Box::new()
                .position_type(PositionType::Relative)
                .size(px(240.0), px(48.0))
                .border(sides(4.0, 4.0, 4.0, 4.0))
                .border_style(BorderStyle::Dashed)
                .border_color(hex_rgb(0x00_00_00))
                .background_color(hex_rgb(0xff_ff_ff))
                .name(
                    "border: 4px dashed -- per-side fitting, both ends flush",
                ),
        )
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// The same border above the threshold: at radius 8 and width 4 the inner
/// corner is round, and Chrome runs one dash pattern around the whole path. The
/// square fixture is the control; the pair is the point.
fn borders_dashed_radius() -> Scene {
    Root::new(240.0)
        .height(48.0)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff))
        .name("a dashed border above the radius threshold. See notes.json.")
        .children(
            Box::new()
                .position_type(PositionType::Relative)
                .size(px(240.0), px(48.0))
                .border(sides(4.0, 4.0, 4.0, 4.0))
                .border_style(BorderStyle::Dashed)
                .border_color(hex_rgb(0x00_00_00))
                .border_radius(8.0)
                .background_color(hex_rgb(0xff_ff_ff))
                .name("radius 8 at width 4 -- one run around the path"),
        )
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene)
}

/// Where the fixtures live, relative to this crate.
const FIXTURES: &str = "../../fixtures";

#[test]
fn each_authored_scene_encodes_to_its_committed_bytes() {
    for (name, scene) in scenes() {
        let path = std::path::Path::new(FIXTURES).join(name).join("scene.mcs");
        let committed = std::fs::read(&path)
            .unwrap_or_else(|error| unreachable!("{name}: {error}"));

        assert_eq!(
            codec::encode(&scene),
            committed,
            "the source for `{name}` no longer encodes to its committed scene"
        );
    }
}

#[test]
#[ignore = "writes checked-in files; run through `just fixture-scenes`"]
fn emit_fixture_scenes() -> Result<(), std::io::Error> {
    for (name, scene) in scenes() {
        let path = std::path::Path::new(FIXTURES).join(name).join("scene.mcs");
        std::fs::write(&path, codec::encode(&scene))?;
        eprintln!("wrote {}", path.display());
    }
    Ok(())
}
