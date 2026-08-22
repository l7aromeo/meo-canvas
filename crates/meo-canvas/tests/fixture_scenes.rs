//! The golden fixtures' scenes, as source rather than as bytes alone.
//!
//! A `scene.mcs` is opaque. A codec change makes every one of them unreadable
//! with no way to rebuild it except decoding the old bytes with the old code —
//! which is a cost already paid by hand once, for `gradients`, during the
//! gradient reshape.
//!
//! So each scene is written here in the authoring surface, and **a test asserts
//! that what this file describes encodes to exactly the committed bytes**. The
//! two cannot drift: a change here that does not match fails, and a codec
//! change is a re-run of `just fixture-scenes` rather than an archaeology
//! exercise.
//!
//! Byte equality rather than picture equality on purpose. If the bytes match,
//! the picture cannot have moved — where a picture comparison would let a
//! scene change that happens to render the same slip through.

use meo_canvas::{
    Align, Box as BoxNode, Display, FlexDirection, Image, Justify, ObjectFit,
    Overflow, PositionType, Root, Styled, Text, corners, corners_all, hex_rgb,
    px,
    scene::{
        BoxShadow, Color, Corners, FontWeight, Gradient, GradientGeometry,
        GradientStop, Length, LinearDirection, Scene, Sides, codec,
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
    ]
}

/// Two boxes stacked by CSS's block rule, the second pulled up over the first.
///
/// `z_index` on the first is what puts it on top despite coming first in paint
/// order, and the negative top margin is what makes them overlap at all.
fn block_stacking() -> Scene {
    Root::new(100.0, 60.0)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children([
            BoxNode::new()
                .size(px(80.0), px(40.0))
                .background_color(hex_rgb(0xdc_28_28))
                .z_index(5),
            BoxNode::new()
                .size(px(80.0), px(40.0))
                .margin(sides(px(-20.0), px(0.0), px(0.0), px(20.0)))
                .background_color(hex_rgb(0x28_50_dc)),
        ])
        .into_scene()
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// The same two boxes, positioned rather than static.
///
/// The pair to `block-stacking`: `Relative` is laid out identically and reads
/// `inset`, and the two fixtures together say that the difference is only in
/// what the offsets do.
fn block_stacking_relative() -> Scene {
    Root::new(100.0, 60.0)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children([
            BoxNode::new()
                .position_type(PositionType::Relative)
                .size(px(80.0), px(40.0))
                .background_color(hex_rgb(0xdc_28_28))
                .z_index(5),
            BoxNode::new()
                .position_type(PositionType::Relative)
                .size(px(80.0), px(40.0))
                .margin(sides(px(-20.0), px(0.0), px(0.0), px(20.0)))
                .background_color(hex_rgb(0x28_50_dc)),
        ])
        .into_scene()
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// Three boxes whose borders differ per edge, in width, colour and radius.
///
/// The second is the one that matters: a `None` bottom colour falls back to
/// `border_color_all`, which is the split the authoring surface hides behind
/// one `borderColor` property.
fn borders_per_edge() -> Scene {
    let row = |background: u32, border: Sides<f32>, radius: Corners<f32>| {
        BoxNode::new()
            .position_type(PositionType::Relative)
            .size(px(120.0), px(80.0))
            .border(border)
            .background_color(hex_rgb(background))
            .border_radius_corners(radius)
    };

    Root::new(440.0, 120.0)
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
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// Three shadows: an offset one, a blurred one, and a spread one.
///
/// Each isolates one term of the shorthand, so a shadow drawn with the wrong
/// term shows as one card differing rather than three.
fn box_shadow() -> Scene {
    let card = |shadow: BoxShadow| {
        BoxNode::new()
            .position_type(PositionType::Relative)
            .size(px(72.0), px(72.0))
            .background_color(hex_rgb(0xff_ff_ff))
            .border_radius_corners(corners_all(10.0))
            .box_shadow(vec![shadow])
    };
    let ink = Color::rgba(20, 20, 40, 110);

    Root::new(320.0, 140.0)
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
        ])
        .into_scene()
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// Four overlapping cards whose paint order is decided by `z_index`.
///
/// One positive, one negative and two unset, so the fixture pins the ordering
/// among the three cases rather than only that a higher index wins.
fn z_order() -> Scene {
    let card = |offset: f32, background: Color| {
        BoxNode::new()
            .position_type(PositionType::Absolute)
            .position(sides(Some(px(offset)), None, None, Some(px(offset))))
            .size(px(90.0), px(70.0))
            .background_color(background)
    };

    Root::new(220.0, 140.0)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff))
        .children([
            card(20.0, Color::rgba(220, 60, 60, 230)).z_index(2),
            card(38.0, Color::rgba(40, 140, 70, 230)).z_index(-1),
            card(56.0, Color::rgba(40, 80, 210, 230)),
            card(74.0, Color::rgba(250, 200, 40, 230)),
        ])
        .into_scene()
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// The same overflowing child in a clipping box and an unclipped one.
///
/// The pair is the check: one picture cannot say whether the clip happened or
/// the child simply fitted.
fn overflow_clip() -> Scene {
    let child = || {
        BoxNode::new()
            .position_type(PositionType::Absolute)
            .position(sides(Some(px(20.0)), None, None, Some(px(30.0))))
            .size(px(120.0), px(90.0))
            .background_color(Color::rgba(220, 60, 60, 200))
    };

    Root::new(300.0, 120.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .align_items(Align::FlexStart)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .children([
            BoxNode::new()
                .position_type(PositionType::Relative)
                .size(px(110.0), px(70.0))
                .overflow(Overflow::Hidden)
                .background_color(hex_rgb(0xe8_ec_f5))
                .border_radius_corners(corners_all(16.0))
                .children(child()),
            BoxNode::new()
                .position_type(PositionType::Relative)
                .size(px(110.0), px(70.0))
                .background_color(hex_rgb(0xe8_ec_f5))
                .children(child()),
        ])
        .into_scene()
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// The picture every `object-fit` cell draws.
///
/// Eight by four, so it is wider than it is tall and every fit resolves to a
/// visibly different rectangle in a square box. Beside this file rather than in
/// the fixture directory: it is source for the scene, not an artefact of it.
const STRIP: &[u8] = include_bytes!("assets/strip.png");

/// One clipped box per `ObjectFit`, all showing the same picture.
///
/// Five cells rather than one per fixture, because the fits are only meaningful
/// against each other — a single `Cover` says nothing that `Contain` beside it
/// does not say better.
fn object_fit() -> Scene {
    let cell = |fit: ObjectFit| {
        BoxNode::new()
            .position_type(PositionType::Relative)
            .size(px(72.0), px(72.0))
            .overflow(Overflow::Hidden)
            .background_color(hex_rgb(0xf0_f0_f0))
            .children(
                Image::bytes(STRIP)
                    .position_type(PositionType::Relative)
                    .size(px(72.0), px(72.0))
                    .object_fit(fit),
            )
    };

    Root::new(420.0, 110.0)
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
        .unwrap_or_else(|error| unreachable!("{error}"))
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
        BoxNode::new().size(px(96.0), px(96.0)).gradient(Gradient {
            geometry,
            stops: ramp(),
        })
    };

    Root::new(330.0, 120.0)
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
        .unwrap_or_else(|error| unreachable!("{error}"))
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

    Root::new(320.0, 140.0)
        .position_type(PositionType::Relative)
        .padding(px(10.0))
        .flex_direction(FlexDirection::Column)
        .gap_xy(px(6.0), px(0.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .children([line(14.0, 400), line(22.0, 400), line(34.0, 700)])
        .into_scene()
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// Two rows of mixed type sizes, one aligned on baselines and one on tops.
///
/// **This fixture pins a defect rather than correct behaviour.** The first row
/// asks for `Baseline` and the renderer aligns box bottoms, which is what the
/// name on the page root says. The control row beneath is what makes the
/// difference visible: without it, a reader cannot tell a baseline from a
/// bottom in a row where every box happens to be the same height.
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
        BoxNode::new()
            .position_type(PositionType::Relative)
            .padding(px(8.0))
            .align_items(align)
            .gap_xy(px(0.0), px(8.0))
            .background_color(hex_rgb(0xee_ee_f4))
            .name(name)
            .children([word(14.0), word(22.0), word(34.0)])
    };

    Root::new(360.0, 190.0)
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
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// A negative-`z_index` child under three parents, two of which must hoist it.
///
/// **This fixture pins a defect.** A child at `z_index: -1` belongs to the
/// nearest ancestor that establishes a stacking context, and paints there
/// *before* that ancestor's own background. A parent that establishes no
/// context does not keep the child: it hoists to the grandparent, where the
/// parent's background then covers it.
///
/// This renderer sorts children within each node and only within each node, so
/// every node behaves as though it established a context and the child is never
/// lifted out. Two of these three cells are therefore wrong today.
///
/// The third is the control, and it is why there are three. It is the cell that
/// looks right today and must **keep** looking right: its parent establishes a
/// context for real, so the child belongs to it and paints above its
/// background. Two cells flipping while one holds says the fix was the hoist
/// rather than a change of sign.
///
/// The clipping cell is here for a second reason worth keeping. `overflow:
/// hidden` is the trigger most often assumed to establish a context and does
/// not, and this renderer is on the right side of that **by construction** —
/// `enter_node` clips with `clip_to_box` rather than opening a layer. A future
/// change that reaches for `save_layer` to clip would create a context the
/// measurement says must not exist, and this cell is what would catch it.
fn stacking_hoist() -> Scene {
    let child = || {
        BoxNode::new()
            .position_type(PositionType::Absolute)
            .position(sides(Some(px(10.0)), None, None, Some(px(10.0))))
            .size(px(36.0), px(36.0))
            .background_color(hex_rgb(0x28_50_dc))
            .z_index(-1)
    };
    let cell = |name: &str| {
        BoxNode::new()
            .position_type(PositionType::Relative)
            .size(px(56.0), px(56.0))
            .background_color(hex_rgb(0xdc_28_28))
            .name(name)
            .children(child())
    };

    Root::new(200.0, 72.0)
        .position_type(PositionType::Relative)
        .padding(px(8.0))
        .align_items(Align::Center)
        .gap_xy(px(0.0), px(8.0))
        .background_color(hex_rgb(0xff_ff_ff))
        .name("PINNED DEFECT: cells 0 and 1 must hide the child and do not. See notes.json.")
        .children([
            cell("no context - DEFECTIVE, child must hoist and be covered"),
            cell("overflow: hidden - DEFECTIVE, clipping is not a context")
                .overflow(Overflow::Hidden),
            // Opacity below one establishes a context in Chrome and opens a
            // layer here, so this cell is correct today and must stay correct.
            // 0.99 rather than something lower: the cell is about the context
            // rather than about the blend, and a barely-transparent red is
            // still red to a reader.
            cell("opacity: 0.99 - CONTROL, a real context keeps its child")
                .opacity(0.99),
        ])
        .into_scene()
        .unwrap_or_else(|error| unreachable!("{error}"))
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
