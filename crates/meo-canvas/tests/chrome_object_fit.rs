//! Where each `object-fit` rule puts a picture, against Chrome's answers.
//! `fill` and `cover` share a rectangle and differ in what they cut, so the
//! source's magenta and cyan edge columns are checked for survival too -- by
//! colour distance, since this renderer's filter blends a mark's seams.

use meo_canvas::{
    Align, Box, Display, Format, Image, ObjectFit, Overflow, PositionType,
    Renderer, Root, Styled, hex_rgb, px,
};

/// The source: eight by four, magenta at its own `x = 0`, cyan at `x = 7`.
const FIT_MARKS: &[u8] = include_bytes!("assets/fit-marks.png");

/// The same picture as a document, byte-identical to the bitmap at its own 8x4,
/// so the `svg` rows are about placement only. Why the workspace requires
/// `meo-skia-canvas` 0.16.1 (`l7aromeo/meo-canvas#95`,
/// `l7aromeo/meo-skia-canvas#212`).
const FIT_MARKS_SVG: &[u8] = include_bytes!("assets/fit-marks.svg");

/// The colour of the cell each rule is drawn in; the cell's size comes from the
/// table, since with one size a fitting source makes `scale-down` and `none`
/// the same rule.
const CELL_INK: (u8, u8, u8) = (0xf0, 0xf0, 0xf0);

/// The two marks, as the source spells them.
const MAGENTA: (u8, u8, u8) = (232, 40, 200);
const CYAN: (u8, u8, u8) = (40, 200, 200);

/// How far a pixel may sit from a mark and still count: no two of the source's
/// four colours are within 150 in this metric, so 60 admits a blended edge and
/// no other mark.
const NEAR: u32 = 60;

/// One row of the table.
struct Row {
    fit: String,
    cell: (f32, f32),
    rect: [u32; 4],
    magenta: bool,
    cyan: bool,
    source: String,
}

/// Whether two rows describe the same cell, by both extents, since the boxes
/// are not all square.
fn same_box(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < f32::EPSILON && (a.1 - b.1).abs() < f32::EPSILON
}

fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let channel = |x: u8, y: u8| u32::from(x.abs_diff(y));
    channel(a.0, b.0) + channel(a.1, b.1) + channel(a.2, b.2)
}

/// Renders one cell and reports its rectangle and which marks survived.
fn drawn(
    fit: ObjectFit,
    (width, height): (f32, f32),
    source: &[u8],
) -> ([u32; 4], bool, bool) {
    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte.
    renderer.set_gpu(false);

    let mut canvas = Root::new(width)
        .height(height)
        .position_type(PositionType::Relative)
        .background_color(hex_rgb(0xff_ff_ff))
        .align_items(Align::Center)
        .children(
            Box::new()
                .display(Display::Block)
                .position_type(PositionType::Relative)
                .size(px(width), px(height))
                .overflow(Overflow::Hidden)
                .background_color(hex_rgb(0xf0_f0_f0))
                .children(
                    Image::bytes(source)
                        .position_type(PositionType::Relative)
                        .size(px(width), px(height))
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
        reason = "a cell side the table states, four to two hundred"
    )]
    let (side, down) = (width as usize, height as usize);
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    let mut magenta = false;
    let mut cyan = false;
    for y in 0..down {
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

/// Refuses a table that could not fail, before anything is compared against it:
/// four checks on whether it can report a defect.
fn assert_the_table_can_fail(rows: &[Row]) {
    // Counted first: every guard below is false of an empty table, so an
    // unreadable one would blame `scale-down` for a missing column.
    assert!(
        !rows.is_empty(),
        "no row of the table parsed: it has fewer fields than this reads, so \
     the fixture predates the `source` column and `just conformance` has \
     not been run since"
    );

    // The table has to be able to tell `scale-down` from `none`: a cell size
    // where the two differ, which a row count cannot stand in for.
    let separates = rows.iter().any(|row| {
        row.fit == "none"
            && rows.iter().any(|other| {
                other.fit == "scale-down"
                    && same_box(other.cell, row.cell)
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

    // Both source kinds, since walking only the PNG let three rules place the
    // vector source at the wrong rectangle.
    for kind in ["raster", "svg"] {
        assert!(
            rows.iter().any(|row| row.source == kind),
            "the table has no `{kind}` rows, so it cannot say whether the two \
         source kinds are placed alike -- which is the whole finding"
        );
    }

    // The box has to disagree with the picture about aspect: a 200x100 box
    // against this 2:1 picture agrees on all five rules with the defect present
    // or fixed.
    let discriminates = rows.iter().any(|row| {
        row.fit == "fill"
            && rows.iter().any(|other| {
                other.fit == "contain"
                    && other.source == row.source
                    && same_box(other.cell, row.cell)
                    && other.rect != row.rect
            })
    });
    assert!(
        discriminates,
        "no box in the table gives `fill` and `contain` different rectangles, \
     so every box shares the picture's aspect and the table cannot fail \
     for a renderer that letterboxes what it should stretch"
    );
}

#[test]
fn object_fit_puts_a_picture_where_chrome_puts_it() {
    let table = include_str!("assets/chrome/object-fit.tsv");
    let rows: Vec<Row> = table
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 7 {
                return None;
            }
            let rect: Vec<u32> = fields[3]
                .split(',')
                .filter_map(|n| n.parse().ok())
                .collect();
            Some(Row {
                fit: fields[0].to_owned(),
                cell: (fields[1].parse().ok()?, fields[2].parse().ok()?),
                rect: [rect[0], rect[1], rect[2], rect[3]],
                magenta: fields[4] == "magenta",
                cyan: fields[5] == "cyan",
                source: fields[6].to_owned(),
            })
        })
        .collect();
    assert_the_table_can_fail(&rows);

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
        let source = match row.source.as_str() {
            "raster" => FIT_MARKS,
            "svg" => FIT_MARKS_SVG,
            other => {
                unreachable!("the table names a source we do not have: {other}")
            }
        };
        let (rect, magenta, cyan) = drawn(fit, row.cell, source);
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
                "{} at {:?} as {}: we draw {rect:?} magenta={magenta} cyan={cyan}, Chrome {:?} magenta={} cyan={}",
                row.fit, row.cell, row.source, row.rect, row.magenta, row.cyan
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
            let mut sizes: Vec<(u32, u32)> = rows
                .iter()
                .map(|row| {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a cell the table states in whole pixels"
                    )]
                    let box_size = (row.cell.0 as u32, row.cell.1 as u32);
                    box_size
                })
                .collect();
            sizes.sort_unstable();
            sizes.dedup();
            sizes.len()
        },
        KNOWN_FIT.len()
    );
}
