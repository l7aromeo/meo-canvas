//! Writes `fixtures/percentages/scene.mcs`.
//!
//! A golden fixture whose every measurement is a percentage, and the only check
//! in the project that pins what a percentage **means** rather than what it
//! encodes to.
//!
//! # Why a picture and not a comparison of bytes
//!
//! A percentage is stored as a fraction, where `1.0` is 100%. Nothing in the
//! type says so — `Length::Percent` is an `f32` — so the unit lives in prose,
//! and a surface that multiplied by a hundred where it should divide would
//! encode `'50%'` as five thousand per cent.
//!
//! **No comparison against the case artefact can catch that.** Every case
//! probes with a value drawn from one fill, and a probe and the bytes it is
//! compared against are written from the same number: they agree whether or not
//! the arithmetic is right. A fixture that shares the units error compares
//! equal too.
//!
//! Rendered pixels are the only currency here that is not downstream of this
//! project's own arithmetic. A quarter of two hundred is fifty, and Skia is the
//! one deciding it.
//!
//! # Why a quarter
//!
//! `0.25` is exact in an `f32`, and it is neither of the two values that hide a
//! hundredfold error: `1` is `Percent(1.0)` whether or not the division
//! happens, and `0` is a fixed point of it. Any other fraction would do; a
//! quarter avoids a rounding argument as well.
//!
//! # Why the scene is authored here rather than committed as bytes alone
//!
//! A `.mcs` is opaque, and a codec change makes every one of them unreadable
//! with no source to rebuild from. This is the source; the bytes are output.
//! Run through `just percentage-fixture`.

use meo_canvas::{
    Box, Root, Styled, hex_rgb, left, px,
    scene::{Length, codec},
};

/// The three shapes a percentage takes in the property tables.
///
/// A size, an offset from an edge, and a point inside a paint. Each is read by
/// a different pass — layout, layout again, and the painter — so one of the
/// three being wrong is a different defect from the other two.
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
            // Offset a quarter of the canvas from the left, and only from the
            // left: a scalar would set all four edges, which moves the box
            // down as well and makes the horizontal reading harder
            // to check. The box is forty wide, so the white runs
            // from fifty to ninety.
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
        .unwrap_or_else(|error| unreachable!("{error}"));

    let directory = std::path::Path::new(DESTINATION);
    std::fs::create_dir_all(directory)?;
    std::fs::write(directory.join("scene.mcs"), codec::encode(&scene))?;
    eprintln!("wrote {}/scene.mcs", directory.display());
    Ok(())
}
