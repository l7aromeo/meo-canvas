//! Where a transformed box inks, against Chrome (`l7aromeo/meo-canvas#166`):
//! CSS translates last, so `translate` is neither rotated nor scaled with the
//! box. Each row of `transform-order.tsv` is built from its own columns and
//! read from ink as an exact rectangle.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Sides, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension, Length, effect::Transform, layout::Display, paint::Color,
    },
};

/// The recorded Chrome rows, which `chrome_tables_are_read` requires be read.
const TABLE: &str = include_str!("assets/chrome/transform-order.tsv");

/// The page, the box's side and where the page's padding puts it, as the
/// table's header states them.
const PAGE: u32 = 100;
const SIDE: f32 = 20.0;
const AT: f32 = 20.0;

const INK: [u8; 4] = [0x28, 0x50, 0xdc, 0xff];

/// One row: its name, the transform its columns spell, and Chrome's inclusive
/// `(left, top, right, bottom)`.
type Row = (String, Transform, (u32, u32, u32, u32));

/// A cell written `20px` or `50%`.
fn length(cell: &str) -> Length {
    match (cell.strip_suffix("px"), cell.strip_suffix('%')) {
        (Some(points), _) => Length::Points(number(points)),
        (_, Some(percent)) => Length::Percent(number(percent) / 100.0),
        (None, None) => unreachable!("{cell} is neither px nor %"),
    }
}

fn number(cell: &str) -> f32 {
    cell.parse()
        .unwrap_or_else(|_| unreachable!("{cell} is not a number"))
}

fn rows() -> Vec<Row> {
    let rows: Vec<Row> = TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            let [name, tx, ty, rotate, sx, sy, ox, oy, l, t, r, b] =
                cells.as_slice()
            else {
                unreachable!("{line} does not have twelve cells")
            };
            let pixel = |cell: &str| {
                cell.parse::<u32>()
                    .unwrap_or_else(|_| unreachable!("{cell} is not a pixel"))
            };
            let transform = Transform {
                translate_x: length(tx),
                translate_y: length(ty),
                rotate_degrees: number(rotate),
                scale_x: number(sx),
                scale_y: number(sy),
                origin: (length(ox), length(oy)),
            };
            (
                name.trim_matches('"').to_owned(),
                transform,
                (pixel(l), pixel(t), pixel(r), pixel(b)),
            )
        })
        .collect();
    assert_eq!(
        rows.len(),
        7,
        "transform-order.tsv holds {} rows",
        rows.len()
    );
    rows
}

/// A 100x100 white page with the box at its 20px padding, as Chrome's `body`.
fn scene(transform: Transform) -> Scene {
    let mut scene = Scene::new(Size::new(PAGE as f32, PAGE as f32));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.layout.display = Display::Block;
        root.layout.padding = Sides::all(Length::Points(AT));
        root.paint.background_color = Color::rgb(255, 255, 255);
    }
    let mut child = Node::new(NodeKind::Box);
    child.layout.display = Display::Block;
    child.layout.size = (Dimension::Points(SIDE), Dimension::Points(SIDE));
    child.paint.background_color = Color::rgb(INK[0], INK[1], INK[2]);
    child.effects.transform = Some(transform);
    scene
        .push(NodeId::ROOT, child)
        .unwrap_or_else(|error| unreachable!("{error}"));
    scene
}

/// The box's ink as an inclusive rectangle, or a sentence saying why it is not
/// one: no ink, or a rectangle with pixels of another colour inside it.
fn ink(raw: &[u8]) -> Result<(u32, u32, u32, u32), String> {
    let at = |x: u32, y: u32| {
        let index = ((y * PAGE + x) * 4) as usize;
        raw[index..index + 4] == INK
    };
    let (mut l, mut t, mut r, mut b) = (PAGE, PAGE, 0, 0);
    let mut count = 0;
    for y in 0..PAGE {
        for x in 0..PAGE {
            if at(x, y) {
                count += 1;
                (l, t, r, b) = (l.min(x), t.min(y), r.max(x), b.max(y));
            }
        }
    }
    if count == 0 {
        return Err("no ink on the page".to_owned());
    }
    let area = (r - l + 1) * (b - t + 1);
    if count == area {
        Ok((l, t, r, b))
    } else {
        Err(format!("{count} pixels inked across {:?}", (l, t, r, b)))
    }
}

#[test]
fn a_transform_inks_where_chrome_puts_it() {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let mut wrong = Vec::new();
    for (name, transform, chrome) in rows() {
        let raw = renderer
            .render_to_buffer(
                &scene(transform),
                ImageFormat::Raw,
                &EncodeOptions::default(),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        match ink(&raw) {
            Ok(ours) if ours == chrome => {}
            Ok(ours) => wrong
                .push(format!("{name}: we ink {ours:?}, Chrome {chrome:?}")),
            Err(why) => wrong.push(format!("{name}: {why}")),
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
