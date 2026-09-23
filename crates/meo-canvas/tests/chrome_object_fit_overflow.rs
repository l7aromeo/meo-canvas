//! A picture stays inside its element (`l7aromeo/meo-canvas#36`): `Cover` and
//! `None` can exceed the box and Chrome clips both, with `Contain` the control
//! that paints smaller. Rectangles are relative to each box, since the two
//! harnesses place it differently.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Corners, Scene, Sides, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        layout::LayoutStyle,
        paint::{BorderStyle, Color, ObjectFit},
    },
};

/// The page, larger than the box, so spill has somewhere to land.
// Large enough that the biggest box plus its margins fits: 60 + 80 + 60. At
// 120 the 80x80 cases had no room on the far side, taffy shrank them, and the
// element measured was not the element the table describes.
const PAGE: f32 = 240.0;

/// Where the element sits, with room on every side.
const INSET: f32 = 60.0;

/// The picture's colour. Distinct from the page so ink is unambiguous.
const INK: (u8, u8, u8) = (232, 40, 200);

/// The page's colour.
const PAPER: (u8, u8, u8) = (0, 255, 0);

/// A flat RGBA PNG of the given size, generated since `Cover` needs a
/// mismatched aspect and `None` a source larger than its box.
fn picture(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut data, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .unwrap_or_else(|error| unreachable!("{error}"));
        let pixels: Vec<u8> = (0..width * height)
            .flat_map(|_| [INK.0, INK.1, INK.2, 255])
            .collect();
        writer
            .write_image_data(&pixels)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    data
}

/// Renders one image node and reports its ink's bounding box, `None` when
/// nothing was drawn: an absent picture trivially stays inside and proves
/// nothing.
fn painted(fit: ObjectFit, box_size: (f32, f32), source: (u32, u32)) -> Extent {
    render(fit, box_size, source, 0.0, 0.0, 0.0).0
}

/// The ink's bounding box: left, top, right, bottom, all inclusive.
type Extent = Option<(u32, u32, u32, u32)>;

/// One pixel, as red, green and blue.
type Pixel = (u8, u8, u8);

/// The ink's bounding box, the pixel at the box's own corner, and how many
/// pixels the picture covers: only the count reads a curve or tells a
/// content-box placement from a border-box one.
fn render(
    fit: ObjectFit,
    box_size: (f32, f32),
    source: (u32, u32),
    radius: f32,
    border: f32,
    padding: f32,
) -> (Extent, Pixel, u32) {
    let mut scene = Scene::new(Size::new(PAGE, PAGE));
    scene.nodes[0].paint.background_color =
        Color::rgb(PAPER.0, PAPER.1, PAPER.2);

    let node = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Image {
                source: ImageSource::Bytes(picture(source.0, source.1)),
                fit,
                // A fraction rather than a percentage: `Length::Percent(0.5)`
                // is the centre. `50.0` places the picture fifty times the
                // leftover away and off the page entirely, which reads as "the
                // renderer drew nothing".
                position: (Length::Percent(0.5), Length::Percent(0.5)),
                frame: None,
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(node) {
        node.layout = LayoutStyle {
            size: (
                Dimension::Points(box_size.0),
                Dimension::Points(box_size.1),
            ),
            margin: Sides::all(Dimension::Points(INSET)),
            ..LayoutStyle::default()
        };
        node.layout.border = Sides::all(border);
        // The Chrome side writes `border:...px solid`, because in CSS a width
        // paints nothing without a style. Naming it here says the same thing,
        // and keeps every row of the fixture describing a bordered element now
        // that `BorderStyle::None` is the default.
        node.paint.border_style = BorderStyle::Solid;
        node.layout.padding = Sides::all(Length::Points(padding));
        // `solid` named, as `objectfit-overflow.mjs` writes it, so these rows
        // keep measuring the border they were generated with whatever the
        // default becomes.
        node.paint.border_style = BorderStyle::Solid;
        node.paint.border_radius = Corners::all(radius);
        node.paint.border_color_all = Color::rgb(0, 0, 255);
        // The harness paints the padding band with the element's background, so
        // this does too, or the two sides measure different pictures of one
        // element.
        if padding > 0.0 {
            node.paint.background_color = Color::rgb(0, 0, 255);
        }
    }

    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let bytes = renderer
        .render_to_buffer(&scene, ImageFormat::Raw, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let side = PAGE as u32;
    let mut found: Extent = None;
    let mut pixels = 0;
    for y in 0..side {
        for x in 0..side {
            let at = ((y * side + x) * 4) as usize;
            let pixel = (bytes[at], bytes[at + 1], bytes[at + 2]);
            // Counted apart from the extent: the extent is everything not the
            // page, a border included, and the count is the picture alone.
            if pixel == INK {
                pixels += 1;
            }
            if pixel == PAPER {
                continue;
            }
            found = Some(match found {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => {
                    (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
                }
            });
        }
    }
    let inset = INSET as u32;
    let at = ((inset * side + inset) * 4) as usize;
    (found, (bytes[at], bytes[at + 1], bytes[at + 2]), pixels)
}

/// Chrome's answers, as measured.
const TABLE: &str = include_str!("assets/chrome/object-fit-overflow.tsv");

/// One row: the fit, its box, its source, Chrome's picture relative to the box,
/// its verdict, and the `overflow` the element computed to.
struct Row {
    fit: ObjectFit,
    box_size: (f32, f32),
    source: (u32, u32),
    border: f32,
    padding: f32,
    /// How many pixels Chrome's picture covers.
    pixels: u32,
    relative: (i64, i64, u32, u32),
    verdict: String,
    overflow: String,
    radius: f32,
    /// Chrome's pixel at the box's own rectangular corner: `paper` where a
    /// radius cut it and nothing painted back over it.
    corner: String,
}

fn pair(text: &str, separator: char) -> (u32, u32) {
    let (a, b) = text
        .split_once(separator)
        .unwrap_or_else(|| unreachable!("{text} is not a pair"));
    (
        a.parse().unwrap_or_else(|error| unreachable!("{error}")),
        b.parse().unwrap_or_else(|error| unreachable!("{error}")),
    )
}

fn rows() -> Vec<Row> {
    let rows: Vec<Row> = TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').map(str::trim).collect();
            assert_eq!(cells.len(), 12, "unexpected columns in {line:?}");
            let fit = match cells[0] {
                "cover" => ObjectFit::Cover,
                "none" => ObjectFit::None,
                "contain" => ObjectFit::Contain,
                other => unreachable!("no fit named {other}"),
            };
            let (bw, bh) = pair(cells[1], 'x');
            let box_rect: Vec<i64> = cells[3]
                .split(',')
                .map(|n| n.parse().unwrap_or_else(|e| unreachable!("{e}")))
                .collect();
            let painted: Vec<i64> = cells[4]
                .split(',')
                .map(|n| n.parse().unwrap_or_else(|e| unreachable!("{e}")))
                .collect();
            Row {
                fit,
                box_size: (bw as f32, bh as f32),
                source: pair(cells[2], 'x'),
                relative: (
                    painted[0] - box_rect[0],
                    painted[1] - box_rect[1],
                    painted[2] as u32,
                    painted[3] as u32,
                ),
                verdict: cells[5].to_owned(),
                overflow: cells[6].to_owned(),
                radius: cells[7]
                    .parse()
                    .unwrap_or_else(|error| unreachable!("{error}")),
                corner: cells[8].to_owned(),
                border: cells[9]
                    .parse()
                    .unwrap_or_else(|error| unreachable!("{error}")),
                padding: cells[10]
                    .parse()
                    .unwrap_or_else(|error| unreachable!("{error}")),
                pixels: cells[11]
                    .parse()
                    .unwrap_or_else(|error| unreachable!("{error}")),
            }
        })
        .collect();

    // A walker over nothing agrees with everything.
    assert!(!rows.is_empty(), "no rows were read from the Chrome table");
    rows
}

#[test]
fn every_row_chrome_clipped_is_a_row_this_renderer_clips() {
    let inset = INSET as i64;
    let mut checked = 0;
    for row in rows() {
        if row.overflow != "clip" {
            continue;
        }
        assert_eq!(
            row.verdict, "inside",
            "{:?}: Chrome computed `overflow: clip` and still spilled",
            row.fit
        );

        let (found, corner, pixels) = render(
            row.fit,
            row.box_size,
            row.source,
            row.radius,
            row.border,
            row.padding,
        );
        let Some((x0, y0, x1, y1)) = found else {
            unreachable!(
                "{:?} drew nothing; an absent picture is not a picture that \
                 stayed inside its box",
                row.fit
            );
        };
        assert_eq!(
            (
                i64::from(x0) - inset,
                i64::from(y0) - inset,
                x1 - x0 + 1,
                y1 - y0 + 1
            ),
            row.relative,
            "{:?} does not sit where Chrome puts it within the box",
            row.fit
        );

        // Only where a radius could have cut it. The `contain` row with a
        // radius is the one that separates clipping to the radius from clipping
        // to the box: unclipped it paints the full 6400-pixel square, clipped
        // 5976.
        if row.radius > 0.0 {
            let ours = if corner == (PAPER.0, PAPER.1, PAPER.2) {
                "paper"
            } else {
                "ink"
            };
            assert_eq!(
                ours, row.corner,
                "{:?} with a {}px radius: Chrome's corner is {} and ours is \
                 {ours}, so a rounded picture reads square on one of them",
                row.fit, row.radius, row.corner
            );
        }
        // The picture's own area separates content-box from border-box
        // placement, 4096 for an 80x80 element with an 8px border and 2304 with
        // 8px of padding too. Exact on straight edges, within two per cent on a
        // curve, which rasterisers differ on.
        let allowed = if row.radius > 0.0 {
            (f64::from(row.pixels) * 0.02).ceil() as i64
        } else {
            0
        };
        let difference = (i64::from(pixels) - i64::from(row.pixels)).abs();
        assert!(
            difference <= allowed,
            "{:?} with border {} padding {} radius {}: Chrome paints {} \
             pixels and we paint {pixels}, off by {difference} against an \
             allowance of {allowed}",
            row.fit,
            row.border,
            row.padding,
            row.radius,
            row.pixels
        );
        checked += 1;
    }
    assert!(checked >= 2, "only {checked} rows were compared");
}

#[test]
fn the_table_still_carries_a_row_that_spills() {
    // The control is in the table: one case forces `overflow: visible` and must
    // spill, since rows reading `inside` are also what a harness blind outside
    // the box prints.
    let spilling: Vec<Row> = rows()
        .into_iter()
        .filter(|row| row.overflow == "visible")
        .collect();
    assert!(
        !spilling.is_empty(),
        "the table has no `overflow: visible` row, so nothing in it \
         demonstrates that the harness can see outside the box"
    );
    for row in spilling {
        assert_eq!(
            row.verdict, "spills",
            "{:?} with `overflow: visible` did not spill, so the harness \
             cannot distinguish clipped from unclipped",
            row.fit
        );
    }
}

#[test]
fn the_control_paints_smaller_than_its_box() {
    // Without this, the assertion above passes on a renderer that draws
    // nothing but the box: every extent it compares is the box's own. `contain`
    // scales by `min(sx, sy)`, so a 40x10 source in a 40x30 box is 40x10
    // centred -- the same rectangle horizontally and a third of it vertically.
    let Some((x0, y0, x1, y1)) =
        painted(ObjectFit::Contain, (40.0, 30.0), (40, 10))
    else {
        unreachable!("contain drew nothing");
    };
    let inset = INSET as u32;
    assert_eq!((x0, x1 - x0 + 1), (inset, 40), "contain moved horizontally");
    assert_eq!(
        (y0, y1 - y0 + 1),
        (inset + 10, 10),
        "contain filled its box vertically, so this test cannot tell a \
         cropped picture from an uncropped one"
    );
}
