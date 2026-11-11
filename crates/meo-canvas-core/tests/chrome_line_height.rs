//! Line boxes against Chrome's HTML layout, at `line-height: normal` and two
//! multiples, on a face with no line gap and one with a gap of a tenth of its
//! em. Every row of `line-height-normal.tsv` is built from its own columns and
//! compared on the box and the first baseline.

use meo_canvas_core::{
    lines::{Metrics, TextMeasurer, layout},
    resolve::{Fonts, ResolvedText},
};
use meo_canvas_scene::style::text::{
    LineHeight, ParagraphStyle, TextSegment, TextStyle,
};

/// The recorded Chrome rows, which `chrome_tables_are_read` requires be read.
const TABLE: &str =
    include_str!("../../meo-canvas/tests/assets/chrome/line-height-normal.tsv");

/// Each family the table names, and the face registered under it.
const FACES: [(&str, &str); 2] = [
    ("Fixture", "tests/assets/fonts/Oswald-VariableFont_wght.ttf"),
    ("Poppins", "tests/assets/fonts/Poppins-Regular.ttf"),
];

/// The block's width in Chrome, wide enough that `Ag` never wraps.
const WIDTH: f32 = 400.0;

/// How far a box or a baseline may sit from Chrome's: none, beyond the float
/// noise of summing whole pixels.
const SLACK: f32 = 0.01;

/// The rows this renderer answers differently, by family, line height, size,
/// line count and span: `normal` leaves the line gap of every face on the line
/// out of the box (`l7aromeo/meo-canvas#232`), as meo-skia-canvas exposes no
/// line gap to add (`l7aromeo/meo-skia-canvas#689`). Asserted both ways, so the
/// fix fails this test until the list is emptied.
const KNOWN: &[&str] = &[
    "Poppins normal 12 1",
    "Poppins normal 12 2",
    "Poppins normal 13 1",
    "Poppins normal 13 2",
    "Poppins normal 15 1",
    "Poppins normal 15 2",
    "Poppins normal 16 1",
    "Poppins normal 16 2",
    "Poppins normal 17 1",
    "Poppins normal 17 2",
    "Poppins normal 20 1",
    "Poppins normal 20 2",
    "Poppins normal 25 1",
    "Poppins normal 25 2",
    "Poppins normal 40 1",
    "Poppins normal 40 2",
    "Fixture normal 13 1 + Poppins 13",
    "Poppins normal 13 1 + Fixture 13",
    "Fixture normal 15 1 + Poppins 15",
    "Poppins normal 15 1 + Fixture 15",
    "Fixture normal 25 1 + Poppins 25",
    "Poppins normal 25 1 + Fixture 25",
    "Poppins normal 13 1 + Poppins 25",
    "Fixture normal 13 1 + Poppins 25",
];

/// One row: family, line height as written, size, lines, the span's family
/// and size where the line holds one, Chrome's box and first baseline.
struct Row<'a> {
    family: &'a str,
    line_height: &'a str,
    size: f32,
    lines: usize,
    span: Option<(&'a str, f32)>,
    chrome_box: f32,
    chrome_baseline: f32,
}

impl Row<'_> {
    fn name(&self) -> String {
        let span = self.span.map_or(String::new(), |(family, size)| {
            format!(" + {family} {size}")
        });
        format!(
            "{} {} {} {}{span}",
            self.family, self.line_height, self.size, self.lines
        )
    }
}

fn number(cell: &str) -> f32 {
    cell.parse()
        .unwrap_or_else(|_| unreachable!("{cell} is not a number"))
}

fn rows() -> Vec<Row<'static>> {
    let mut header = false;
    let mut rows = Vec::new();
    for line in TABLE.lines() {
        if let Some(rest) = line.strip_prefix('#') {
            header |= rest.trim()
                == "family\tline_height\tsize\tlines\tspan_family\tspan_size\tbox\tbaseline";
            continue;
        }
        assert!(header, "a row before the column line: {line}");
        let cells: Vec<&str> = line.split('\t').collect();
        let [
            family,
            line_height,
            size,
            lines,
            span_family,
            span_size,
            chrome_box,
            chrome_baseline,
        ] = cells.as_slice()
        else {
            unreachable!("{line} does not have eight cells")
        };
        rows.push(Row {
            family,
            line_height,
            size: number(size),
            lines: lines
                .parse()
                .unwrap_or_else(|_| unreachable!("{lines} is not a count")),
            span: match *span_family {
                "-" => None,
                family => Some((family, number(span_size))),
            },
            chrome_box: number(chrome_box),
            chrome_baseline: number(chrome_baseline),
        });
    }
    assert_eq!(
        rows.len(),
        90,
        "line-height-normal.tsv holds {}",
        rows.len()
    );
    rows
}

/// This renderer's box and first baseline for a row.
fn ours(
    fonts: &Fonts,
    measurer: &mut TextMeasurer,
    row: &Row<'_>,
) -> (f32, f32) {
    assert!(fonts.has(row.family), "{} is not registered", row.family);
    let base = ResolvedText {
        family: row.family.to_owned(),
        size: row.size,
        line_height: match row.line_height {
            "normal" => None,
            multiple => Some(LineHeight::Number(number(multiple))),
        },
        ..ResolvedText::initial()
    };
    let mut segments = vec![TextSegment {
        text: vec!["Ag"; row.lines].join("\n"),
        style: TextStyle::default(),
    }];
    if let Some((family, size)) = row.span {
        assert!(fonts.has(family), "{family} is not registered");
        segments.push(TextSegment {
            text: "Ag".to_owned(),
            style: TextStyle {
                font_family: Some(family.to_owned()),
                font_size: Some(size),
                ..TextStyle::default()
            },
        });
    }
    let block = layout(
        measurer,
        &base,
        &segments,
        WIDTH,
        &ParagraphStyle::default(),
        Metrics::of(&base),
    );
    assert_eq!(block.lines.len(), row.lines, "{}", row.name());
    (block.height, block.lines[0].baseline_from_top())
}

#[test]
fn a_line_box_is_the_one_chrome_builds() {
    let fonts = Fonts::new();
    for (family, path) in FACES {
        fonts.register_path(family, path).unwrap_or_else(|error| {
            unreachable!("{family} did not register: {error}")
        });
    }
    let mut measurer = TextMeasurer::new();
    let mut wrong = Vec::new();
    let mut listed = 0;
    for row in rows() {
        let name = row.name();
        let (height, baseline) = ours(&fonts, &mut measurer, &row);
        let agrees = (height - row.chrome_box).abs() < SLACK
            && (baseline - row.chrome_baseline).abs() < SLACK;
        let known = KNOWN.contains(&name.as_str());
        if agrees == known {
            wrong.push(format!(
                "{name}: box {height} baseline {baseline}, Chrome {} and {}{}",
                row.chrome_box,
                row.chrome_baseline,
                if known {
                    "; listed in KNOWN, so remove it"
                } else {
                    ""
                }
            ));
        }
        listed += usize::from(known);
    }
    assert_eq!(
        listed,
        KNOWN.len(),
        "KNOWN names a row the table does not hold"
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
