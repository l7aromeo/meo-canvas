//! Where `text-align` puts a line, and which values move with `direction`.
//!
//! # What this exists for
//!
//! `TextAlign::Start` is documented as flipping under a right-to-left
//! direction -- *"at the inline start, which flips under a right-to-left
//! direction"* -- and `TextAlign::Left` as not -- *"at the left edge
//! regardless of direction"*. The enum is the specification, and
//! `l7aromeo/meo-canvas#109` reports that the code cannot meet it.
//!
//! # Why the `ltr` rows are half the table
//!
//! Under `ltr`, `start` lands where `left` does and `end` where `right` does.
//! Those rows are the control: with only `rtl` rows, a repair to the physical
//! arms and a repair to nothing produce the same table, because the `rtl` rows
//! cannot say whether `left` stayed put because it is correct or because
//! nothing touched it.
//!
//! `center` is here and witnesses neither direction, deliberately: it is the
//! same rectangle under both. Its note says so, so the next person does not add
//! it believing it covers the centred case.
//!
//! # Why the ink and not the box
//!
//! A line's box is the container's width whatever the alignment does, so a
//! measurement of the box reports the container and agrees with itself under
//! every value. What moves is the ink, which is why both sides of this
//! comparison walk painted columns.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId},
    style::{
        Dimension,
        layout::{Direction, LayoutStyle},
        paint::Color,
        text::TextAlign,
    },
};

/// The face the fixtures register and the one Chrome was asked about.
const FONT: (&str, &str) =
    ("Fixture", "tests/assets/fonts/Oswald-VariableFont_wght.ttf");

const TABLE: &str = include_str!(
    "../../meo-canvas/tests/assets/chrome/text-align-direction.tsv"
);

/// The cell every line is placed in, wider than the line so placement shows.
const CELL: (f32, f32) = (300.0, 60.0);

/// What the cell is painted with, so the line's own extent can be found.
const PAPER: (u8, u8, u8) = (255, 255, 255);

/// How far a measured column may sit from Chrome's.
///
/// Two pixels, and stated rather than tuned. Chrome reports a fractional
/// position and this counts whole inked columns, and the two rasterisers do not
/// agree on where a glyph's antialiased edge stops being paper. The rows this
/// exists for are separated by 259 columns, not two.
const TOLERANCE: i64 = 2;

/// One row: the direction, the alignment, and where Chrome inked it.
struct Row {
    direction: Direction,
    align: TextAlign,
    left: i64,
    width: i64,
}

fn rows() -> Vec<Row> {
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split('\t');
            let direction = match columns.next() {
                Some("ltr") => Direction::Ltr,
                Some("rtl") => Direction::Rtl,
                other => unreachable!("unknown direction {other:?}"),
            };
            let align = match columns.next() {
                Some("start") => TextAlign::Start,
                Some("end") => TextAlign::End,
                Some("left") => TextAlign::Left,
                Some("right") => TextAlign::Right,
                Some("center") => TextAlign::Center,
                other => unreachable!("unknown alignment {other:?}"),
            };
            let extent = columns
                .next()
                .unwrap_or_else(|| unreachable!("a row with no extent"));
            let (left, width) = extent.split_once(',').unwrap_or_else(|| {
                unreachable!("{extent} is not `left,width`")
            });
            Row {
                direction,
                align,
                left: left.parse().unwrap_or_else(|e| unreachable!("{e}")),
                width: width.parse().unwrap_or_else(|e| unreachable!("{e}")),
            }
        })
        .collect()
}

/// The inked columns of one line, as `(left, width)`.
fn inked(direction: Direction, align: TextAlign) -> (i64, i64) {
    let mut scene = Scene::new(Size::new(CELL.0, CELL.1));
    if let Some(page) = scene.get_mut(NodeId::ROOT) {
        page.paint.background_color = Color::rgb(PAPER.0, PAPER.1, PAPER.2);
    }
    let mut node = Node::text("abc");
    node.layout = LayoutStyle {
        size: (Dimension::Points(CELL.0), Dimension::Auto),
        direction,
        ..LayoutStyle::default()
    };
    // **On the node's own text style, which is what a caller writes.** The
    // segment styles inherit from it during `resolve`, so setting it here is
    // the same scene a caller building one through either surface produces.
    node.text.text_align = Some(align);
    node.text.font_family = Some(FONT.0.to_owned());
    node.text.font_size = Some(32.0);
    node.text.color = Some(Color::rgb(0, 0, 0));
    scene
        .push(NodeId::ROOT, node)
        .unwrap_or_else(|error| unreachable!("{error}"));

    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte.
    renderer.set_gpu(false);
    renderer
        .register_font(FONT.0, FONT.1)
        .unwrap_or_else(|error| {
            unreachable!("the face did not register: {error}")
        });
    let bytes = renderer
        .render_to_buffer(&scene, ImageFormat::Raw, &EncodeOptions::default())
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a cell stated here in whole pixels"
    )]
    let (wide, tall) = (CELL.0 as usize, CELL.1 as usize);
    let mut left: Option<usize> = None;
    let mut right: Option<usize> = None;
    for x in 0..wide {
        let inked = (0..tall).any(|y| {
            let at = ((y * wide) + x) * 4;
            (bytes[at], bytes[at + 1], bytes[at + 2]) != PAPER
        });
        if inked {
            left = left.or(Some(x));
            right = Some(x);
        }
    }
    match (left, right) {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "a column index inside a 300-wide cell"
        )]
        (Some(first), Some(last)) => (first as i64, (last - first + 1) as i64),
        _ => (0, 0),
    }
}

#[test]
fn text_align_moves_with_direction_where_chrome_moves_it() {
    let rows = rows();
    assert_eq!(
        rows.len(),
        10,
        "the table changed shape; five alignments across two directions"
    );

    // **The control the table is built around, asserted rather than assumed.**
    // If `start` and `left` do not agree under `ltr` in Chrome's own numbers,
    // the rows below cannot separate a repair to the physical arms from a
    // repair to nothing, and the whole comparison is answering a different
    // question than the one it claims.
    let ltr_of = |align: TextAlign| {
        rows.iter()
            .find(|row| row.direction == Direction::Ltr && row.align == align)
            .map_or_else(
                || unreachable!("no ltr row for {align:?}"),
                |row| (row.left, row.width),
            )
    };
    assert_eq!(
        ltr_of(TextAlign::Start),
        ltr_of(TextAlign::Left),
        "under ltr Chrome puts `start` where it puts `left`; if this table \
         says otherwise it cannot act as the control for the rtl rows"
    );
    assert_eq!(
        ltr_of(TextAlign::End),
        ltr_of(TextAlign::Right),
        "and `end` where it puts `right`, for the same reason"
    );

    let mut wrong = Vec::new();
    for row in &rows {
        let (left, width) = inked(row.direction, row.align);
        if (left - row.left).abs() > TOLERANCE
            || (width - row.width).abs() > TOLERANCE
        {
            wrong.push(format!(
                "{:?} {:?}: chrome {},{} here {left},{width}",
                row.direction, row.align, row.left, row.width
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} row(s) disagree: {wrong:#?}",
        wrong.len()
    );
}
