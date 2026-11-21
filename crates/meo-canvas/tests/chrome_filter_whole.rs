//! A `filter` covers the node's whole rendering, children included, filtered
//! once as a group in the node's own coordinate space and before its mask,
//! opacity and blend, as Chrome paints `filter-whole.tsv`'s cases.

use meo_canvas::{
    Box, Display, Element, Format, Image, Overflow, PositionType, Renderer,
    Root, Styled, Text, hex_rgb, px,
    scene::{BlendMode, BoxShadow, FillRule, Length, Mask, Transform},
    sides,
};

const TABLE: &str = include_str!("assets/chrome/filter-whole.tsv");

/// The 80x80 opaque blue disc on transparency the table's cases draw.
const DISC: &[u8] = include_bytes!("assets/filter-disc.png");

const FONT: (&str, &str) = (
    "Fixture",
    "../meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf",
);

/// The white page each case is drawn at the top-left of.
const PAGE: u32 = 240;

/// Per-channel slack for two rasterisers compositing the same colours: a half
/// opacity is 126 in Chrome and 128 here.
const SLACK: u8 = 3;

/// Per-channel slack where a blur sets the value: this renderer's Gaussian is
/// exact where Chrome's approximates, a documented difference, so these rows
/// assert the blur reached the point rather than how far.
const BLUR_SLACK: u8 = 16;

const BLUE: u32 = 0x00_00_ff;
const GREEN: u32 = 0x00_a0_00;
const RED: u32 = 0xff_00_00;

struct Row {
    case: String,
    at: (u32, u32),
    rgb: [u8; 3],
}

fn rows() -> Vec<Row> {
    let mut header = false;
    let mut rows = Vec::new();
    for line in TABLE.lines() {
        if let Some(rest) = line.strip_prefix('#') {
            header |= rest.trim() == "case\tx\ty\tr\tg\tb\twhy";
            continue;
        }
        assert!(header, "a row before the column line: {line}");
        let cells: Vec<&str> = line.split('\t').collect();
        let [case, x, y, red, green, blue, _why] = cells.as_slice() else {
            unreachable!("{line} does not have seven cells")
        };
        let number = |cell: &str| {
            cell.parse::<u32>()
                .unwrap_or_else(|_| unreachable!("{cell} is not a number"))
        };
        let channel = |cell: &str| {
            cell.parse::<u8>()
                .unwrap_or_else(|_| unreachable!("{cell} is not a channel"))
        };
        rows.push(Row {
            case: (*case).to_owned(),
            at: (number(x), number(y)),
            rgb: [channel(red), channel(green), channel(blue)],
        });
    }
    assert_eq!(rows.len(), 49, "filter-whole.tsv holds {}", rows.len());
    rows
}

/// A block of `colour`.
fn block(width: f32, height: f32, colour: u32) -> Element {
    Box::new()
        .display(Display::Block)
        .size(px(width), px(height))
        .background_color(hex_rgb(colour))
}

/// A block container of the given size.
fn cell(width: f32, height: f32) -> Element {
    Box::new()
        .display(Display::Block)
        .size(px(width), px(height))
}

/// An absolutely positioned block.
fn placed(
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    colour: u32,
) -> Element {
    Box::new()
        .display(Display::Block)
        .position_type(PositionType::Absolute)
        .position(sides(Some(px(top)), None, None, Some(px(left))))
        .size(px(width), px(height))
        .background_color(hex_rgb(colour))
}

fn disc() -> Element {
    Image::bytes(DISC.to_vec()).size(px(80.0), px(80.0))
}

fn shadow(x: f32, y: f32, colour: &str) -> String {
    format!("drop-shadow({x}px {y}px 0px {colour})")
}

/// Turned or scaled about the box's top-left corner, or its middle.
const fn transform(rotate: f32, scale: f32, corner: bool) -> Transform {
    let origin = if corner {
        (Length::Points(0.0), Length::Points(0.0))
    } else {
        (Length::Percent(0.5), Length::Percent(0.5))
    };
    Transform {
        translate_x: Length::Points(0.0),
        translate_y: Length::Points(0.0),
        rotate_degrees: rotate,
        scale_x: scale,
        scale_y: scale,
        origin,
    }
}

/// The case `name` as the tool writes it in markup.
#[expect(
    clippy::too_many_lines,
    reason = "one arm per case, each the markup the tool writes; split, the \
              two would be read side by side across files"
)]
fn scene(name: &str) -> Element {
    let red = "red";
    match name {
        "image shadow past its box" => {
            cell(160.0, 80.0).children(disc().filter(shadow(80.0, 0.0, red)))
        }
        "image shadow at its edge" => {
            cell(160.0, 80.0).children(disc().filter(shadow(4.0, 0.0, red)))
        }
        "box shadowing an image child" => cell(160.0, 80.0).children(
            cell(80.0, 80.0)
                .filter(shadow(80.0, 0.0, red))
                .children(disc()),
        ),
        "one shadow under both children" => cell(120.0, 100.0)
            .position_type(PositionType::Relative)
            .filter(shadow(-30.0, -30.0, red))
            .children(vec![
                placed(0.0, 0.0, 60.0, 40.0, GREEN),
                placed(40.0, 40.0, 40.0, 40.0, BLUE),
            ]),
        "one opacity over both children" => cell(100.0, 60.0)
            .position_type(PositionType::Relative)
            .filter("opacity(0.5)")
            .children(vec![
                placed(0.0, 0.0, 60.0, 60.0, GREEN),
                placed(40.0, 0.0, 60.0, 60.0, BLUE),
            ]),
        "blur past an image child" => cell(80.0, 80.0)
            .padding(px(20.0))
            .box_sizing(meo_canvas::BoxSizing::ContentBox)
            .children(cell(80.0, 80.0).filter("blur(4px)").children(disc())),
        "shadow of clipped overflow" => cell(160.0, 80.0).children(
            cell(80.0, 80.0)
                .overflow(Overflow::Hidden)
                .filter(shadow(80.0, 0.0, red))
                .children(block(160.0, 80.0, BLUE)),
        ),
        "shadow of a rounded clip" => cell(160.0, 80.0).children(
            cell(80.0, 80.0)
                .overflow(Overflow::Hidden)
                .border_radius(40.0)
                .filter(shadow(80.0, 0.0, red))
                .children(block(80.0, 80.0, BLUE)),
        ),
        "opacity after the filter" => cell(160.0, 80.0).children(
            cell(80.0, 80.0)
                .opacity(0.5)
                .filter(shadow(80.0, 0.0, red))
                .children(disc()),
        ),
        "blend after the filter" => cell(160.0, 80.0)
            .background_color(hex_rgb(0xff_ff_00))
            .children(
                cell(80.0, 80.0)
                    .mix_blend_mode(BlendMode::Multiply)
                    .filter(shadow(80.0, 0.0, "rgb(0,255,255)"))
                    .children(disc()),
            ),
        "shadow scaled with its box" => cell(200.0, 100.0).children(
            cell(40.0, 40.0)
                .transform(transform(0.0, 2.0, true))
                .filter(shadow(20.0, 0.0, red))
                .children(block(40.0, 40.0, BLUE)),
        ),
        "shadow turned with its box" => cell(200.0, 200.0).children(
            cell(40.0, 40.0)
                .margin(px(60.0))
                .transform(transform(90.0, 1.0, false))
                .filter(shadow(30.0, 0.0, red))
                .children(block(40.0, 40.0, BLUE)),
        ),
        "blur scaled with its box" => cell(200.0, 100.0).children(
            cell(40.0, 40.0)
                .transform(transform(0.0, 2.0, true))
                .filter("blur(5px)")
                .children(block(40.0, 40.0, BLUE)),
        ),
        "a transformed child inside the group" => cell(240.0, 200.0).children(
            cell(40.0, 40.0)
                .margin(sides(px(80.0), px(0.0), px(0.0), px(0.0)))
                .filter(shadow(100.0, 0.0, red))
                .children(block(80.0, 20.0, BLUE).transform(Transform {
                    translate_x: Length::Points(60.0),
                    rotate_degrees: 90.0,
                    scale_x: 2.0,
                    scale_y: 2.0,
                    ..transform(0.0, 1.0, false)
                })),
        ),
        "a filter that shows nothing" => cell(240.0, 80.0)
            .position_type(PositionType::Relative)
            .children(vec![
                cell(40.0, 40.0)
                    .transform(transform(0.0, 0.0, false))
                    .filter(shadow(80.0, 0.0, red))
                    .children(block(40.0, 40.0, BLUE)),
                placed(-300.0, 0.0, 40.0, 40.0, BLUE)
                    .filter(shadow(20.0, 0.0, red)),
                placed(0.0, 40.0, 40.0, 40.0, GREEN),
            ]),
        "nested filters" => cell(200.0, 80.0).children(
            cell(40.0, 40.0).filter(shadow(50.0, 0.0, red)).children(
                cell(40.0, 40.0)
                    .filter(shadow(0.0, 30.0, "rgb(0,160,0)"))
                    .children(block(40.0, 40.0, BLUE)),
            ),
        ),
        "shadow of a box shadow" => cell(200.0, 80.0).children(
            cell(40.0, 40.0)
                .background_color(hex_rgb(BLUE))
                .box_shadow(vec![BoxShadow {
                    inset: false,
                    offset_x: 40.0,
                    offset_y: 0.0,
                    blur: 0.0,
                    spread: 0.0,
                    color: hex_rgb(GREEN).into(),
                }])
                .filter(shadow(0.0, 30.0, red)),
        ),
        "an ancestor's clip cuts the shadow" => {
            cell(100.0, 80.0).overflow(Overflow::Hidden).children(
                cell(80.0, 80.0)
                    .filter(shadow(80.0, 0.0, red))
                    .children(disc()),
            )
        }
        // A path over the gradient's opaque half: the same hard edge at 80.
        "the mask after the filter" => cell(160.0, 80.0).children(
            cell(160.0, 80.0)
                .mask(Mask::Path {
                    data: "M0 0H80V80H0Z".to_owned(),
                    fill_rule: FillRule::NonZero,
                })
                .filter(shadow(80.0, 0.0, red))
                .children(block(80.0, 80.0, BLUE)),
        ),
        "the filter as a backdrop root" | "no backdrop root" => {
            let mut layer = cell(80.0, 80.0)
                .position_type(PositionType::Absolute)
                .position(sides(Some(px(0.0)), None, None, Some(px(0.0))));
            if name == "the filter as a backdrop root" {
                layer = layer.filter("brightness(1)");
            }
            cell(80.0, 80.0)
                .position_type(PositionType::Relative)
                .background_color(hex_rgb(RED))
                .children(
                    layer.children(
                        cell(80.0, 80.0).backdrop_filter("invert(1)"),
                    ),
                )
        }
        "icon tint" => cell(80.0, 80.0).overflow(Overflow::Hidden).children(
            cell(80.0, 80.0)
                .position_type(PositionType::Relative)
                .position(sides(None, None, None, Some(px(-80.0))))
                .filter(shadow(80.0, 0.0, red))
                .children(disc()),
        ),
        "icon tint on the image" => {
            cell(80.0, 80.0).overflow(Overflow::Hidden).children(
                disc()
                    .display(Display::Block)
                    .position_type(PositionType::Relative)
                    .position(sides(None, None, None, Some(px(-80.0))))
                    .filter(shadow(80.0, 0.0, red)),
            )
        }
        "text" => Box::new()
            .display(Display::Block)
            .font_family(FONT.0)
            .font_size(80.0)
            .line_height(meo_canvas::LineHeight::Number(1.0))
            .color(hex_rgb(BLUE))
            .filter(shadow(100.0, 0.0, red))
            .children(Text::new("I")),
        "text past its line box" => Box::new()
            .display(Display::Block)
            .padding(sides(px(60.0), px(0.0), px(0.0), px(0.0)))
            .children(
                Box::new()
                    .display(Display::Block)
                    .font_family(FONT.0)
                    .font_size(80.0)
                    .line_height(meo_canvas::LineHeight::Number(0.25))
                    .color(hex_rgb(BLUE))
                    .filter(shadow(100.0, 0.0, red))
                    .children(Text::new("I")),
            ),
        other => unreachable!("no case named {other}"),
    }
}

#[test]
fn a_filter_covers_the_whole_rendering_as_chrome_paints_it() {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .register_font(FONT.0, FONT.1)
        .unwrap_or_else(|error| {
            unreachable!("the font did not register: {error}")
        });
    let rows = rows();
    let mut wrong = Vec::new();
    let mut cases: Vec<&str> =
        rows.iter().map(|row| row.case.as_str()).collect();
    cases.dedup();
    for case in cases {
        let mut canvas = Root::new(PAGE as f32)
            .height(PAGE as f32)
            .display(Display::Block)
            .background_color(hex_rgb(0xff_ff_ff))
            .children(scene(case))
            .render(&renderer)
            .unwrap_or_else(|error| unreachable!("{case}: {error}"));
        let pixels = canvas
            .to_buffer(Format::Raw)
            .unwrap_or_else(|error| unreachable!("{case}: {error}"));
        let slack = if case.contains("blur") {
            BLUR_SLACK
        } else {
            SLACK
        };
        for row in rows.iter().filter(|row| row.case == case) {
            let index = ((row.at.1 * PAGE + row.at.0) * 4) as usize;
            let got = &pixels[index..index + 3];
            if got.iter().zip(row.rgb).any(|(&g, w)| g.abs_diff(w) > slack) {
                wrong.push(format!(
                    "{case} at {:?}: {got:?}, in Chrome {:?}",
                    row.at, row.rgb
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
