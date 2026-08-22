//! Paths: fill rules, caps, joins, dashes, and a path drawn by stroke alone.
//!
//! Every cell is the same size and its path is written in the cell's own
//! coordinates, so a difference on the page is a difference in one property
//! rather than in where the shape was put.

use meo_canvas::{
    Box as BoxNode, Element, FillRule, FlexDirection, Path, Root, Styled,
    hex_rgb, px,
    scene::{
        Gradient, GradientGeometry, GradientStop, LineCap, LineJoin,
        LinearDirection, PathPaint,
    },
};
use meo_canvas_examples::{FORMATS, draw};

/// A five-pointed star, whose arms overlap in the middle.
///
/// The overlap is the whole point: it is the one region the two fill rules
/// disagree about, so a reader sees the rule rather than reads about it.
const STAR: &str =
    "M32 4 L40 24 L60 24 L44 36 L50 56 L32 44 L14 56 L20 36 L4 24 L24 24 Z";

/// A stroke long enough that its ends are a small part of it.
const SEGMENT: &str = "M8 32 L56 32";

/// One corner, so a join has something to be drawn at.
const CHEVRON: &str = "M10 52 L32 12 L54 52";

/// A cell of the one size every cell is.
fn cell(path: Element) -> Element {
    BoxNode::new()
        .size(px(64.0), px(64.0))
        .background_color(hex_rgb(0xee_ee_f2))
        .children(path)
}

/// The star, filled by one rule.
fn star(rule: FillRule) -> Element {
    Path::d(STAR)
        .size(px(64.0), px(64.0))
        .fill(Some(PathPaint::Solid(hex_rgb(0x28_50_dc))))
        .fill_rule(rule)
}

/// A thick horizontal segment, so a cap is a visible fraction of it.
fn capped(cap: LineCap) -> Element {
    Path::d(SEGMENT)
        .size(px(64.0), px(64.0))
        .fill(None)
        .stroke(Some(PathPaint::Solid(hex_rgb(0x22_88_44))))
        .line_width(14.0)
        .line_cap(cap)
}

/// A corner, so a join is a visible fraction of it.
fn joined(join: LineJoin) -> Element {
    Path::d(CHEVRON)
        .size(px(64.0), px(64.0))
        .fill(None)
        .stroke(Some(PathPaint::Solid(hex_rgb(0xcc_44_22))))
        .line_width(12.0)
        .line_join(join)
}

/// A segment under a dash pattern.
fn dashed(pattern: &[f32], offset: f32) -> Element {
    Path::d(SEGMENT)
        .size(px(64.0), px(64.0))
        .fill(None)
        .stroke(Some(PathPaint::Solid(hex_rgb(0x11_11_18))))
        .line_width(6.0)
        .line_dash(pattern.iter().copied())
        .line_dash_offset(offset)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Root::new(360.0, 300.0)
        .background_color(hex_rgb(0xff_ff_ff))
        .padding(px(8.0))
        .flex_direction(FlexDirection::Column)
        .gap(px(6.0))
        .children([
            // The two fill rules on the one shape they disagree about, then
            // the same shape with no fill at all -- which is the
            // only way to see that `None` means unpainted rather
            // than black.
            BoxNode::new().gap(px(6.0)).children(vec![
                cell(star(FillRule::NonZero)),
                cell(star(FillRule::EvenOdd)),
                cell(
                    Path::d(STAR)
                        .size(px(64.0), px(64.0))
                        .fill(None)
                        .stroke(Some(PathPaint::Solid(hex_rgb(0x28_50_dc))))
                        .line_width(2.0),
                ),
                // A path with neither fill nor stroke set draws its default,
                // which SVG says is black.
                cell(Path::d(STAR).size(px(64.0), px(64.0))),
                // A path paint is a colour or a gradient, and until today the
                // JavaScript surface could spell only the first.
                cell(Path::d(STAR).size(px(64.0), px(64.0)).fill(Some(
                    PathPaint::Gradient(Gradient {
                        geometry: GradientGeometry::Linear {
                            direction: LinearDirection::Angle(135.0),
                        },
                        stops: vec![
                            GradientStop {
                                offset: 0.0,
                                color: hex_rgb(0x28_50_dc),
                            },
                            GradientStop {
                                offset: 1.0,
                                color: hex_rgb(0xf2_b0_2c),
                            },
                        ],
                    }),
                ))),
            ]),
            BoxNode::new().gap(px(6.0)).children(vec![
                cell(capped(LineCap::Butt)),
                cell(capped(LineCap::Round)),
                cell(capped(LineCap::Square)),
            ]),
            BoxNode::new().gap(px(6.0)).children(vec![
                cell(joined(LineJoin::Bevel)),
                cell(joined(LineJoin::Round)),
                cell(joined(LineJoin::Miter)),
            ]),
            BoxNode::new().gap(px(6.0)).children(vec![
                // Solid, the same pattern, and the same pattern begun part-way
                // through -- so the offset is read against the dash it moves.
                cell(dashed(&[], 0.0)),
                cell(dashed(&[10.0, 6.0], 0.0)),
                cell(dashed(&[10.0, 6.0], 8.0)),
            ]),
        ]);

    draw("paths", root, FORMATS)
}
