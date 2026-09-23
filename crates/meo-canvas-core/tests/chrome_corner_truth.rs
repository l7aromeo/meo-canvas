//! Where a border's colours meet at a rounded corner, against Chrome's painted
//! bytes. Each row reduces to a hue, since Chrome and Skia rasterise an arc to
//! different bytes; the question is which edge owns which part of the arc (CSS
//! Backgrounds 3 §4.4). Rows where the ring pinches to nothing are skipped.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Corners, Scene, Sides, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension,
        paint::{BorderStyle, Color},
    },
};

/// The page, so "outside the box" is one value and nothing else is.
const PAGE: (u8, u8, u8) = (255, 255, 255);

/// The box's own background: what shows at the boundary where no edge has
/// width, and what a gap in the ring shows where one does.
const FILL: (u8, u8, u8) = (255, 250, 240);

/// How far the two boxes sit from the page's own corner, so that a row's first
/// inked pixel is the box's and not the page's edge.
const ORIGIN: usize = 0;

/// What the outermost pixel of one row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ink {
    /// The top edge.
    Red,
    /// The left edge of the second box.
    Yellow,
    /// The left edge of the first box. Never expected: Chrome gives the whole
    /// arc to the top when the left width is zero.
    Blue,
    /// The box's own background, which below the arc is correct.
    Fill,
    /// Present but too close to the page to name, which is what a ring
    /// thinner than a pixel looks like.
    Faint,
}

/// Reduces a pixel to which edge painted it, by hue rather than distance to a
/// colour: at the pinch the ring is a pixel or two and every pixel is a blend.
fn ink(pixel: (u8, u8, u8)) -> Ink {
    if pixel == FILL {
        return Ink::Fill;
    }
    let (red, green, blue) =
        (i16::from(pixel.0), i16::from(pixel.1), i16::from(pixel.2));
    if blue > red + 10 {
        return Ink::Blue;
    }
    // Both remaining colours are warm; green above blue separates them: red is
    // (200,40,40), yellow (230,170,30). Below this the tint is noise -- at the
    // pinch (253, 240, 231) is a ring covering a fraction of a pixel, not a
    // colour.
    if red - blue < 25 {
        return Ink::Faint;
    }
    if i32::from(green - blue) * 100 > i32::from(red - blue) * 35 {
        Ink::Yellow
    } else {
        Ink::Red
    }
}

/// Renders one bordered box on a white page and returns its pixels.
fn corner(
    size: (f32, f32),
    border: Sides<f32>,
    radius: Corners<f32>,
) -> (usize, Vec<u8>) {
    let mut scene = Scene::new(Size::new(size.0 + 20.0, size.1 + 20.0));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = Color::rgb(PAGE.0, PAGE.1, PAGE.2);
    }
    let id = scene
        .push(NodeId::ROOT, Node::new(NodeKind::Box))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size =
            (Dimension::Points(size.0), Dimension::Points(size.1));
        node.layout.border = border;
        // Explicit because `BorderStyle::None` is the default and would give
        // this scene no border at all -- and no corner to measure.
        node.paint.border_style = BorderStyle::Solid;
        node.paint.background_color = Color::rgb(FILL.0, FILL.1, FILL.2);
        node.paint.border_radius = radius;
        node.paint.border_color_all = Color::rgb(120, 120, 120);
        node.paint.border_color = Sides {
            top: Some(Color::rgb(200, 40, 40)),
            right: Some(Color::rgb(40, 140, 60)),
            bottom: Some(Color::rgb(40, 60, 200)),
            left: Some(Color::rgb(230, 170, 30)),
        };
    }

    let mut renderer = Renderer::new();
    // The two rasterisers do not agree to the byte, and this reads bytes.
    renderer.set_gpu(false);
    let png = renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });

    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8()
            | png::Transformations::ALPHA,
    );
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut pixels = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut pixels)
        .unwrap_or_else(|error| unreachable!("{error}"));
    pixels.truncate(info.buffer_size());
    (info.width as usize, pixels)
}

/// How far inward to search for a nameable colour. The outermost pixel is the
/// least covered and reads near-white at a pinch; a fixed depth instead lands
/// inside one edge's colour where the ring is thick. So step in only while too
/// faint to name, at most four.
const BOUNDARY_DEPTH: usize = 4;

/// The colour of each row's outer boundary.
///
/// The outermost pixel that can be named, which is the one a browser reports.
/// The scan stops at the fill: past that there is no ring left on this row.
fn boundary(
    stride: usize,
    pixels: &[u8],
    rows: std::ops::Range<usize>,
) -> Vec<Ink> {
    rows.map(|y| {
        let mut seen = 0;
        for x in ORIGIN..40 {
            let at = (y * stride + x) * 4;
            let pixel = (pixels[at], pixels[at + 1], pixels[at + 2]);
            if pixel == PAGE {
                continue;
            }
            match ink(pixel) {
                Ink::Faint => {
                    seen += 1;
                    if seen == BOUNDARY_DEPTH {
                        return Ink::Faint;
                    }
                }
                // Reaching the fill having already passed a tint means there
                // *is* a ring on this row, thinner than a pixel -- which is
                // what the pinch beside a zero-width edge looks like. Reaching
                // it immediately means there is none.
                Ink::Fill if seen > 0 => return Ink::Faint,
                named => return named,
            }
        }
        Ink::Faint
    })
    .collect()
}

/// A zero-width edge takes none of the arc. Chrome, `border-width: 2px 8px 5px
/// 0` and `border-radius: 20px 0 10px 4px`: red on every arc row, blue on none,
/// and the fill from y=19, which a rule forbidding fill at the boundary would
/// overdraw.
#[test]
fn a_zero_width_edge_gives_up_the_whole_arc() {
    let (stride, pixels) = corner(
        (120.0, 80.0),
        Sides {
            top: 2.0,
            right: 8.0,
            bottom: 5.0,
            left: 0.0,
        },
        Corners {
            top_left: 20.0,
            top_right: 0.0,
            bottom_right: 10.0,
            bottom_left: 4.0,
        },
    );

    let arc = boundary(stride, &pixels, 0..19);
    assert!(
        arc.iter().all(|ink| matches!(ink, Ink::Red | Ink::Faint)),
        "the arc should be red at every row and reads {arc:?}"
    );
    assert!(
        !arc.contains(&Ink::Blue),
        "the left edge has no width and took part of the arc: {arc:?}"
    );
    assert!(
        arc.iter().filter(|ink| **ink == Ink::Red).count() >= 12,
        "too few rows of the arc are legibly red to call this covered: {arc:?}"
    );

    let flank = boundary(stride, &pixels, 19..24);
    assert!(
        flank.iter().all(|ink| *ink == Ink::Fill),
        "below the arc a zero-width edge should leave the fill, not a ring: \
         {flank:?}"
    );
}

/// Unequal widths hand over where CSS's division line crosses the arc. Chrome,
/// `border-width: 10px 2px`, `border-radius: 24px`: red to y=12, blended at 13,
/// yellow from 14; an angular split would hand over four rows late.
#[test]
fn unequal_widths_hand_the_arc_over_where_chrome_does() {
    let (stride, pixels) = corner(
        (120.0, 80.0),
        Sides {
            top: 10.0,
            right: 2.0,
            bottom: 10.0,
            left: 2.0,
        },
        Corners::all(24.0),
    );
    let arc = boundary(stride, &pixels, 0..20);

    assert!(
        arc[0..12]
            .iter()
            .all(|ink| matches!(ink, Ink::Red | Ink::Faint)),
        "the thick top edge should own the arc down to y=11: {arc:?}"
    );
    let handover = arc
        .iter()
        .position(|ink| *ink == Ink::Yellow)
        .unwrap_or_else(|| {
            unreachable!("the left edge painted no row: {arc:?}")
        });
    // Chrome hands over at 13. One row of slack, because the pixel there is a
    // blend of the two and which side of a half it lands on is rasteriser
    // arithmetic rather than geometry.
    assert!(
        (12..=14).contains(&handover),
        "the arc hands over at y={handover} where Chrome hands over at 13: \
         {arc:?}"
    );
    assert!(
        arc[15..20]
            .iter()
            .all(|ink| matches!(ink, Ink::Yellow | Ink::Faint)),
        "below the handover the thin left edge should own the arc: {arc:?}"
    );
}

/// One width pair, and the row Chrome hands the arc over on.
struct Pair {
    /// The top edge's width.
    top: f32,
    /// The left edge's width.
    left: f32,
    /// The first row the left edge owns, or `None` when it owns none of the
    /// arc at all.
    handover: Option<usize>,
}

/// Chrome's handover row for five width pairs: 60x60, radius 20, top `#c82828`,
/// left `#e6aa1e`. `1/20` and `20/1` hand over at 1 and 15, not both at 10 as
/// an angular split would; `0/2` fails at row 0; `6/6` is the mitre control.
/// §4.4's line solved directly gives 0.73, 14.6 and 5.86.
const CHROME_PAIRS: [Pair; 5] = [
    Pair {
        top: 2.0,
        left: 0.0,
        handover: None,
    },
    Pair {
        top: 0.0,
        left: 2.0,
        handover: Some(0),
    },
    Pair {
        top: 1.0,
        left: 20.0,
        handover: Some(1),
    },
    Pair {
        top: 20.0,
        left: 1.0,
        handover: Some(15),
    },
    Pair {
        top: 6.0,
        left: 6.0,
        handover: Some(6),
    },
];

/// Every width pair hands over on Chrome's row. The single-fill checks prove
/// the arc is covered; with one colour there is no join to see, so this is the
/// half that places it.
#[test]
fn every_width_pair_divides_its_corner_where_chrome_does() {
    for pair in CHROME_PAIRS {
        let (stride, pixels) = corner(
            (60.0, 60.0),
            Sides {
                top: pair.top,
                right: 6.0,
                bottom: 6.0,
                left: pair.left,
            },
            Corners::all(20.0),
        );
        let arc = boundary(stride, &pixels, 0..20);
        let found = arc.iter().position(|ink| *ink == Ink::Yellow);
        let Pair { top, left, .. } = pair;

        match pair.handover {
            None => {
                assert!(
                    found.is_none(),
                    "top {top}, left {left}: the left edge has no width and \
                     took the arc from row {found:?}: {arc:?}"
                );
                let flank = boundary(stride, &pixels, 19..24);
                assert!(
                    flank.iter().all(|ink| *ink == Ink::Fill),
                    "top {top}, left {left}: below the arc a zero-width edge \
                     leaves the fill, not a ring: {flank:?}"
                );
            }
            Some(expected) => {
                let found = found.unwrap_or_else(|| {
                    unreachable!(
                        "top {top}, left {left}: the left edge painted no row \
                         of the arc at all: {arc:?}"
                    )
                });
                // One row of slack: the handover pixel is a blend of the two
                // colours, and which side of a half it lands on is rasteriser
                // arithmetic rather than geometry.
                assert!(
                    found.abs_diff(expected) <= 1,
                    "top {top}, left {left}: hands over at y={found} where \
                     Chrome hands over at y={expected}: {arc:?}"
                );
                assert!(
                    arc[..found]
                        .iter()
                        .all(|ink| matches!(ink, Ink::Red | Ink::Faint)),
                    "top {top}, left {left}: the top edge does not own the \
                     arc above the handover: {arc:?}"
                );
            }
        }
    }
}
