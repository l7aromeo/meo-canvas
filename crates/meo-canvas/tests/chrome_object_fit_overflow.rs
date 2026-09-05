//! That a picture stays inside the element it was placed in.
//!
//! `fit_image` returns a destination that may be larger than the box — its own
//! documentation says so, and says the caller crops. The caller did not, so
//! `objectFit: 'cover'` painted outside the element wherever the source's
//! aspect did not match its box (`l7aromeo/meo-canvas#36`, a 152x186 avatar in
//! a 26x26 frame painting 26x32).
//!
//! **Two fits can exceed, and only one was reported.** `Cover` scales by
//! `max(sx, sy)`; `None` draws at intrinsic size, so any source larger than its
//! box overflows on both axes. `Contain` cannot exceed, and is here as the
//! control: it must paint *smaller* than the box, which is what shows these
//! assertions are reading the picture rather than echoing the rectangle they
//! compare against.
//!
//! Chrome's behaviour is the reason this is a defect rather than a preference,
//! and the rows are read from its table rather than restated here. Measured
//! with no `overflow` declared on the element or any ancestor and a page larger
//! than the box: `cover` and `none` paint inside, the computed `overflow` is
//! `clip`, and forcing `overflow: visible` makes the same picture spill.
//!
//! **The comparison is of rectangles relative to each box**, not absolute ones:
//! the two harnesses put their element in different places, and what is being
//! compared is where the picture sits within it.
//!
//! `object-fit.tsv` cannot answer this and never could — its cell is
//! `overflow:hidden`, its viewport is the box and its screenshot is clipped to
//! the box. It measures placement *given* a clip and is silent on whether
//! Chrome applies one.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Sides, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        layout::LayoutStyle,
        paint::{Color, ObjectFit},
    },
};

/// The page, larger than the box, so spill has somewhere to land.
const PAGE: f32 = 120.0;

/// Where the element sits, with room on every side.
const INSET: f32 = 40.0;

/// The picture's colour. Distinct from the page so ink is unambiguous.
const INK: (u8, u8, u8) = (232, 40, 200);

/// The page's colour.
const PAPER: (u8, u8, u8) = (0, 255, 0);

/// A flat RGBA PNG of the given size, written out rather than read from disk.
///
/// Generated because the two cases need different intrinsic sizes: `Cover`
/// needs an aspect that does not match its box and `None` needs a source larger
/// than its box, and one asset cannot be both without boxes so small the
/// assertion becomes a rounding argument.
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

/// Renders one image node and reports the bounding box of its ink.
///
/// `None` when nothing was drawn, which is a failure to distinguish from a
/// picture that stayed inside its box: an absent picture satisfies "does not
/// paint outside" and proves nothing.
fn painted(
    fit: ObjectFit,
    box_size: (f32, f32),
    source: (u32, u32),
) -> Option<(u32, u32, u32, u32)> {
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
    }

    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let bytes = renderer
        .render_to_buffer(&scene, ImageFormat::Raw, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let side = PAGE as u32;
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in 0..side {
        for x in 0..side {
            let at = ((y * side + x) * 4) as usize;
            let pixel = (bytes[at], bytes[at + 1], bytes[at + 2]);
            if pixel != INK {
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
    found
}

/// Chrome's answers, as measured.
const TABLE: &str = include_str!("assets/chrome/object-fit-overflow.tsv");

/// One row: the fit, its box, its source, Chrome's picture relative to the box,
/// its verdict, and the `overflow` the element computed to.
struct Row {
    fit: ObjectFit,
    box_size: (f32, f32),
    source: (u32, u32),
    relative: (i64, i64, u32, u32),
    verdict: String,
    overflow: String,
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
            assert_eq!(cells.len(), 7, "unexpected columns in {line:?}");
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

        let Some((x0, y0, x1, y1)) = painted(row.fit, row.box_size, row.source)
        else {
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
        checked += 1;
    }
    assert!(checked >= 2, "only {checked} rows were compared");
}

#[test]
fn the_table_still_carries_a_row_that_spills() {
    // The control lives in the table rather than here. Three rows reading
    // `inside` are also what a harness blind to everything outside the box
    // would print, so one case forces `overflow: visible` and must spill. If a
    // regeneration ever loses that row, the other rows stop meaning anything
    // and this says so rather than passing quietly.
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
