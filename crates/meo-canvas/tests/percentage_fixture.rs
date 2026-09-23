//! Writes `fixtures/percentages/scene.mcs`, a golden whose every measurement is
//! a percentage: the one check on what a percentage means, since a probe and
//! its expected bytes come from one number and agree even with a hundredfold
//! units error. A quarter, exact in an `f32` and neither `1` nor `0`.

use meo_canvas::{
    Box, Root, Styled, hex_rgb, left, px,
    scene::{Length, codec},
};

/// The three shapes a percentage takes -- a size, an edge offset, a point
/// inside a paint -- each read by a different pass, so each is a different
/// defect.
const QUARTER: Length = Length::Percent(0.25);

/// Where the fixture lives, relative to this crate.
const DESTINATION: &str = "../../fixtures/percentages";

#[test]
#[ignore = "writes a checked-in file; run through `just percentage-fixture`"]
fn emit_percentage_scene() -> Result<(), std::io::Error> {
    // 200 x 120, so a quarter of the width is fifty pixels and a quarter of the
    // height is thirty. Both are whole numbers of pixels, so a disagreement is
    // never a rounding argument.
    let scene = Root::new(200.0)
        .height(120.0)
        .background_color(hex_rgb(0x10_10_14))
        // Stacked, so each bar has the full width to be a percentage of. A row
        // would make every measurement a share of what the siblings left over,
        // which is a different question from the one this fixture asks.
        .flex_direction(meo_canvas::FlexDirection::Column)
        .children([
            // A quarter of the canvas wide: fifty pixels of white against a
            // hundred and fifty of ground.
            Box::new()
                .width(QUARTER)
                .height(px(40.0))
                .background_color(hex_rgb(0xff_ff_ff)),
            // Offset a quarter of the canvas from the left only, since a
            // scalar sets all four edges. The box is forty wide,
            // so the white runs from fifty to ninety.
            Box::new()
                .position_type(meo_canvas::PositionType::Relative)
                .position(left(Some(QUARTER)))
                .width(px(40.0))
                .height(px(40.0))
                .background_color(hex_rgb(0xff_ff_ff)),
            // A gradient reaching white a quarter of the way across and
            // holding it: the edge of the ramp is at fifty pixels.
            Box::new()
                .width(Length::Percent(1.0))
                .height(px(40.0))
                .gradient(meo_canvas::scene::Gradient {
                    geometry: meo_canvas::scene::GradientGeometry::Linear {
                        direction:
                            meo_canvas::scene::LinearDirection::Between {
                                start: (
                                    Length::Points(0.0),
                                    Length::Points(0.0),
                                ),
                                end: (QUARTER, Length::Points(0.0)),
                            },
                    },
                    stops: vec![
                        meo_canvas::scene::GradientStop {
                            offset: 0.0,
                            color: hex_rgb(0x10_10_14),
                        },
                        meo_canvas::scene::GradientStop {
                            offset: 1.0,
                            color: hex_rgb(0xff_ff_ff),
                        },
                    ],
                }),
        ])
        .into_scene()
        .map_or_else(|error| unreachable!("{error}"), |(scene, _)| scene);

    let directory = std::path::Path::new(DESTINATION);
    std::fs::create_dir_all(directory)?;
    std::fs::write(directory.join("scene.mcs"), codec::encode(&scene))?;
    eprintln!("wrote {}/scene.mcs", directory.display());
    Ok(())
}
