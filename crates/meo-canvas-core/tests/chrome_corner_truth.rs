//! Where a border's colours meet at a rounded corner, against Chrome.
//!
//! # Why this exists beside the golden fixtures
//!
//! `fixtures/borders-per-edge` draws these same two boxes and could not see
//! either of the defects they carried: it was accepted from our own render, so
//! it certified a ring with a hole in it, twice. A fixture says "this is the
//! picture"; only a browser can say "this is the *right* picture".
//!
//! # Why a hue and not a byte
//!
//! Chrome and Skia do not rasterise an arc to the same bytes, and the question
//! here is not the bytes. It is **which edge owns which part of the arc** --
//! the handover row, which is where CSS Backgrounds 3 §4.4's line from the
//! outer corner point to the inner one crosses the outer contour. So each row
//! is reduced to a hue: red, yellow, blue, the box's own fill, or too faint to
//! call.
//!
//! Where the ring pinches to nothing the outermost pixel is almost white, and
//! that is not a defect -- it is what a sub-pixel-thin ring looks like. Those
//! rows are skipped rather than classified, which is why every assertion here
//! is about the rows that *are* legible.
//!
//! # The numbers
//!
//! Measured in Chrome by MC Main, rasterising the identical CSS through an SVG
//! `foreignObject` into a canvas and reading it back with `getImageData` --
//! Chrome's own painted bytes rather than a screenshot. Recorded in
//! `scratchpad/chrome/corner-truth.tsv`.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Corners, Scene, Sides, Size,
    node::{Node, NodeId, NodeKind},
    style::{Dimension, paint::Color},
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

/// Reduces a pixel to which edge painted it.
///
/// By hue rather than by distance to a colour, because every one of these
/// pixels is a blend: the ring is one or two pixels thick at the pinch and
/// antialiasing takes the rest.
fn ink(pixel: (u8, u8, u8)) -> Ink {
    if pixel == FILL {
        return Ink::Fill;
    }
    let (red, green, blue) =
        (i16::from(pixel.0), i16::from(pixel.1), i16::from(pixel.2));
    if blue > red + 10 {
        return Ink::Blue;
    }
    // Both remaining colours are warm; what separates them is how much green
    // survives above the blue. Red is (200,40,40) -- green and blue together;
    // yellow is (230,170,30) -- green far above blue.
    // Below this the tint is a hair off the page and the hue is noise: at the
    // pinch the ring covers a fraction of a pixel, and (253, 240, 231) is a
    // ring, not a colour anyone could name.
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

/// How far inward the search for a nameable colour may go.
///
/// Four. The outermost inked pixel is the *least* covered one -- an
/// antialiased edge spans a pixel or two -- so where the ring pinches to
/// nothing, reading only it reports the page's white with a trace of border in
/// it and calls the arc faint. Reading a fixed depth instead is the opposite
/// mistake, and cost this file a wrong answer: where the ring is twenty pixels
/// thick, three pixels in is deep inside one edge's own colour and reports it
/// for a row the *other* edge owns at the boundary. So: outermost first, step
/// inward only while the pixel is too faint to name, and stop after four.
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

/// A zero-width edge takes none of the arc, and the arc is still covered.
///
/// Chrome, `border-width: 2px 8px 5px 0` with `border-radius: 20px 0 10px 4px`:
/// red at the outer boundary for every row of the arc, the fill from y=19
/// where the radius ends and a zero-width left edge means there is no ring to
/// paint, and **blue at no row at all**.
///
/// The fill from y=19 is the part that makes this a conformance test rather
/// than a gap detector: a rule forbidding fill at the boundary everywhere
/// would pass the arc and overdraw the flank.
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

/// Two unequal widths hand the arc over where CSS's division line crosses it.
///
/// Chrome, `border-width: 10px 2px` with `border-radius: 24px`: red to y=12,
/// a red-and-yellow blend at y=13, yellow from y=14. The handover row is the
/// discriminator -- closing a gap does not put the join in the right place,
/// and an angular split would hand over four rows late.
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

/// Chrome's answer for five width pairs on one geometry.
///
/// 60x60, `border-radius: 20px`, top `#c82828` and left `#e6aa1e`, the other
/// two 6px. Read from Chrome's own painted bytes;
/// `scratchpad/chrome/pair-truth.tsv`.
///
/// # Why these five
///
/// **`1/20` and `20/1` are not mirror images** -- one hands over at y=1 and
/// the other at y=15 on a twenty-row arc, and a rule that split the corner by
/// angle would put both at y=10. That pair alone rejects the two wrong answers
/// this code has already had: an angular split, and a split that leans towards
/// the thicker edge.
///
/// **`0/2` fails at the first row rather than in the middle of an arc.** It is
/// the only case where the edge that gets nothing is the *thicker* one, so a
/// division leaning the wrong way is yellow-less at y=0 where every other case
/// would still look plausible for a dozen rows.
///
/// `6/6` is the control: equal widths, the 45-degree mitre, which every
/// bordered fixture in the suite already draws.
///
/// # The arithmetic, solved independently
///
/// CSS Backgrounds 3 §4.4's line runs from the outer corner `(0, 0)` to the
/// inner one `(left, top)`; the outer contour is the circle of radius 20
/// centred at `(20, 20)`. Solving the two gives 14.6 for `20/1`, 0.73 for
/// `1/20` and 5.86 for `6/6` -- against Chrome's 15, 1 and 6. A measurement
/// and an arithmetic that agree are worth more than either alone.
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

/// Every width pair hands the arc over on the row Chrome hands it over on.
///
/// The five pairs were already checked against the ring drawn as a single
/// fill, which proves the arc is *covered*. It cannot prove the join is in the
/// right place: with one colour on every edge there is no join to see. This is
/// the other half, and it is the half with the handover row in it.
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
