//! Where each `object-fit` rule puts a picture, against Chrome's own answers.
//!
//! # Why the rectangle is only half of it
//!
//! **`fill` and `cover` both fill the box.** Their rectangles are identical —
//! `0,0,72,72` in Chrome's table for both — and they differ only in what they
//! *cut*: `fill` stretches the whole picture, `cover` crops it. A test that
//! compared rectangles alone would report the two as the same rule, and a
//! renderer that implemented one for the other would pass.
//!
//! So the source carries a magenta column at its own `x = 0` and a cyan column
//! at `x = 7`, and the table records whether each survives. Under `fill` both
//! do; under `cover` neither does, because the crop takes the picture's ends.
//! **A symmetric picture would read the same stretched as cropped**, which is
//! why the marks are at the edges and in colours nothing else in the scene
//! uses.
//!
//! # Why the marks are matched with a tolerance and the rectangle is not
//!
//! Chrome measured with `image-rendering: pixelated`, so an eight-pixel source
//! scaled to 72 keeps hard columns. This renderer scales with its own filter,
//! so a mark's colour arrives blended with its neighbour at the seams. The
//! **presence** of a mark is therefore asked as *is any pixel near this
//! colour*, with a distance well inside the gap between the four colours the
//! source uses — they are far apart on purpose. The rectangle needs no
//! tolerance, because it is a bound on what is not the cell colour rather than
//! a claim about any particular pixel.

use meo_canvas::{
    Align, Box, Format, Image, ObjectFit, Overflow, PositionType, Renderer,
    Root, Styled, hex_rgb, px,
};

/// The source: eight by four, magenta at its own `x = 0`, cyan at `x = 7`.
const FIT_MARKS: &[u8] = include_bytes!("assets/fit-marks.png");

/// The colour of the cell each rule is drawn in.
///
/// **The cell's size comes from the table rather than from here.** With one
/// size, and a source that fits it, `scale-down` and `none` are the same rule
/// by definition -- CSS makes `scale-down` the smaller of `none` and `contain`
/// -- and the fixture carried two byte-identical rows for them. It could not
/// have failed for `scale-down`, and neither could this test.
const CELL_INK: (u8, u8, u8) = (0xf0, 0xf0, 0xf0);

/// The two marks, as the source spells them.
const MAGENTA: (u8, u8, u8) = (232, 40, 200);
const CYAN: (u8, u8, u8) = (40, 200, 200);

/// How far a pixel may sit from a mark and still count as it.
///
/// The source uses four colours and no two are within 150 of each other in
/// this metric, so 60 admits a blended edge and cannot admit a different mark.
/// Stated rather than tuned: a tolerance chosen by raising it until a test
/// passes is a tolerance that has stopped measuring anything.
const NEAR: u32 = 60;

/// One row of the table.
struct Row {
    fit: String,
    cell: f32,
    rect: [u32; 4],
    magenta: bool,
    cyan: bool,
}

fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let channel = |x: u8, y: u8| u32::from(x.abs_diff(y));
    channel(a.0, b.0) + channel(a.1, b.1) + channel(a.2, b.2)
}

/// Renders one cell and reports its rectangle and which marks survived.
fn drawn(fit: ObjectFit, cell: f32) -> ([u32; 4], bool, bool) {
    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte.
    renderer.set_gpu(false);

    let mut canvas = Root::new(cell)
        .height(cell)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff))
        .align_items(Align::Center)
        .children(
            Box::new()
                .position_type(PositionType::Relative)
                .size(px(cell), px(cell))
                .overflow(Overflow::Hidden)
                .background_color(hex_rgb(0xf0_f0_f0))
                .children(
                    Image::bytes(FIT_MARKS)
                        .position_type(PositionType::Relative)
                        .size(px(cell), px(cell))
                        .object_fit(fit),
                ),
        )
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    let bytes = canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    });

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a cell side the table states, four to seventy-two"
    )]
    let side = cell as usize;
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    let mut magenta = false;
    let mut cyan = false;
    for y in 0..side {
        for x in 0..side {
            let at = (y * side + x) * 4;
            let here = (bytes[at], bytes[at + 1], bytes[at + 2]);
            if distance(here, MAGENTA) <= NEAR {
                magenta = true;
            }
            if distance(here, CYAN) <= NEAR {
                cyan = true;
            }
            // The rectangle is everything that is not the cell colour, so a
            // letterboxed fit reports the picture rather than the box.
            if here == CELL_INK {
                continue;
            }
            bounds = Some(match bounds {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => {
                    (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
                }
            });
        }
    }
    let (x0, y0, x1, y1) =
        bounds.unwrap_or_else(|| unreachable!("{fit:?} drew nothing at all"));
    (
        [
            x0 as u32,
            y0 as u32,
            (x1 - x0 + 1) as u32,
            (y1 - y0 + 1) as u32,
        ],
        magenta,
        cyan,
    )
}

/// Which rules we answer differently from Chrome today.
const KNOWN_FIT: &[&str] = &[];

#[test]
fn object_fit_puts_a_picture_where_chrome_puts_it() {
    let table = include_str!("assets/chrome/object-fit.tsv");
    let rows: Vec<Row> = table
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 6 {
                return None;
            }
            let rect: Vec<u32> = fields[3]
                .split(',')
                .filter_map(|n| n.parse().ok())
                .collect();
            Some(Row {
                fit: fields[0].to_owned(),
                cell: fields[1].parse().ok()?,
                rect: [rect[0], rect[1], rect[2], rect[3]],
                magenta: fields[4] == "magenta",
                cyan: fields[5] == "cyan",
            })
        })
        .collect();
    // **The table has to be able to tell `scale-down` from `none`.**
    //
    // Not a count: a count is what the fixture already passed while carrying
    // two identical rows for two different rules. This asks the property the
    // count was standing in for -- that somewhere in the table there is a cell
    // size where the two rules give different answers. Delete the small boxes
    // for tidiness and this fails by name rather than passing quietly.
    let separates = rows.iter().any(|row| {
        row.fit == "none"
            && rows.iter().any(|other| {
                other.fit == "scale-down"
                    && (other.cell - row.cell).abs() < f32::EPSILON
                    && (other.rect != row.rect
                        || other.magenta != row.magenta
                        || other.cyan != row.cyan)
            })
    });
    assert!(
        separates,
        "no cell size in the table separates `none` from `scale-down`, so the \
         table cannot fail for `scale-down`: it is the smaller of `none` and \
         `contain`, which is `none` wherever the picture already fits"
    );

    let mut wrong = Vec::new();
    for row in &rows {
        let fit = match row.fit.as_str() {
            "fill" => ObjectFit::Fill,
            "contain" => ObjectFit::Contain,
            "cover" => ObjectFit::Cover,
            "none" => ObjectFit::None,
            "scale-down" => ObjectFit::ScaleDown,
            other => {
                unreachable!("the table names a fit we do not have: {other}")
            }
        };
        let (rect, magenta, cyan) = drawn(fit, row.cell);
        let known = KNOWN_FIT.contains(&row.fit.as_str());

        // A pixel of tolerance on the rectangle: a bounding box read from ink
        // is what the picture covers, and a layout rectangle is where it was
        // put.
        let apart = rect
            .iter()
            .zip(row.rect.iter())
            .any(|(ours, theirs)| ours.abs_diff(*theirs) > 1)
            || magenta != row.magenta
            || cyan != row.cyan;

        if apart && !known {
            wrong.push(format!(
                "{} at {}: we draw {rect:?} magenta={magenta} cyan={cyan}, Chrome {:?} magenta={} cyan={}",
                row.fit, row.cell, row.rect, row.magenta, row.cyan
            ));
        }
        if !apart && known {
            wrong.push(format!(
                "{}: now agrees with Chrome. That is a fix -- delete the row from KNOWN_FIT",
                row.fit
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "{} rules differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    eprintln!(
        "object-fit: {} rows compared across {} cell sizes, {} pinned",
        rows.len(),
        {
            let mut sizes: Vec<u32> = rows
                .iter()
                .map(|row| {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a cell side the table states"
                    )]
                    let side = row.cell as u32;
                    side
                })
                .collect();
            sizes.sort_unstable();
            sizes.dedup();
            sizes.len()
        },
        KNOWN_FIT.len()
    );
}
