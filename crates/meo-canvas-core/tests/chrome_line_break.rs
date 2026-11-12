//! Where a line breaks, against Chrome's HTML layout: every pair of printable
//! ASCII characters and of a set from each breaking class, sentences at 0px,
//! and strings wrapped at several widths and at `min-content`. Each row of
//! `line-break.tsv` is laid out by [`layout`] and compared on its line starts.

use meo_canvas_core::{
    lines::{Block, Metrics, TextMeasurer, layout},
    resolve::{Fonts, ResolvedText},
};
use meo_canvas_scene::style::text::{
    LineHeight, ParagraphStyle, TextSegment, TextStyle,
};
use meo_skia_canvas::{Canvas, CanvasOptions, Font, FontSlant, FontStretch};

/// The recorded Chrome rows.
const TABLE: &str =
    include_str!("../../meo-canvas/tests/assets/chrome/line-break.tsv");

/// The face the table was measured in, by the family both register it under.
const FACE: (&str, &str) =
    ("Noto Sans SC", "tests/assets/fonts/NotoSansSC-Subset.ttf");

/// One of Chrome's layout units, a 64th of a pixel: how far a `min-content`
/// width may sit from Chrome's, which rounds a text run's width to one, and an
/// ideograph's advance from its em, which Linux's font scaler puts at
/// 16.0000305 at 16px.
const SLACK: f32 = 1.0 / 64.0;

/// The rows this renderer answers differently, each with why. Asserted both
/// ways, so an entry whose row agrees fails and says to delete it.
///
/// A pair is named `first second`; a string by its text and width.
const KNOWN: &[(&str, &str)] = &[
    ("\u{3000} \u{a0}", NBSP_STARTS),
    ("\u{2013} \u{a0}", NBSP_STARTS),
    ("\u{2010} \u{a0}", NBSP_STARTS),
    ("\u{200b} \u{a0}", NBSP_STARTS),
    (
        "\u{200b} \u{3000}",
        "UAX 14 17 breaks after a zero-width space whatever follows, and \
         ICU4X does; Chrome's ICU does not before an ideographic space at the \
         start of the text",
    ),
];

/// Why a line here never begins with a no-break space.
const NBSP_STARTS: &str = "Chrome breaks before a no-break space after a \
     dash or a break-after space and draws it at the start of the next line; \
     this renderer draws one as an inter-word gap, which a line never begins \
     with, so the two stay on one line";

/// One row of the table.
#[derive(Debug)]
enum Row {
    /// Every `first` + one of `tried` at 0px; `breaks` are the seconds that
    /// began a line.
    Pairs {
        first: String,
        tried: String,
        breaks: String,
    },
    /// `text` at `width`, with the code-point index each line after the first
    /// starts at.
    Wrap {
        text: String,
        width: f32,
        starts: Vec<usize>,
    },
    /// `text` at `min-content`: Chrome's width, and where its lines start.
    MinContent {
        text: String,
        width: f32,
        starts: Vec<usize>,
    },
}

/// A JSON string cell, as the tool writes it: quoted, with `\"`, `\\` and
/// `\uXXXX` escapes and every other character literal.
fn text(cell: &str) -> String {
    let inner = cell
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| unreachable!("{cell} is not a JSON string"));
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match chars.next() {
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                let unit = u32::from_str_radix(&hex, 16)
                    .unwrap_or_else(|_| unreachable!("\\u{hex} in {cell}"));
                out.push(
                    char::from_u32(unit)
                        .unwrap_or_else(|| unreachable!("\\u{hex} in {cell}")),
                );
            }
            Some(other @ ('"' | '\\')) => out.push(other),
            other => unreachable!("\\{other:?} in {cell}"),
        }
    }
    out
}

fn number(cell: &str) -> f32 {
    cell.parse()
        .unwrap_or_else(|_| unreachable!("{cell} is not a number"))
}

fn starts(cell: &str) -> Vec<usize> {
    if cell == "-" {
        return Vec::new();
    }
    cell.split(',')
        .map(|at| {
            at.parse()
                .unwrap_or_else(|_| unreachable!("{at} is not an index"))
        })
        .collect()
}

fn rows() -> Vec<Row> {
    let mut header = false;
    let mut rows = Vec::new();
    for line in TABLE.lines() {
        if let Some(rest) = line.strip_prefix('#') {
            header |= rest.trim() == "kind\ttext\twidth\tbreaks";
            continue;
        }
        assert!(header, "a row before the column line: {line}");
        let cells: Vec<&str> = line.split('\t').collect();
        let [kind, first, second, third] = cells.as_slice() else {
            unreachable!("{line} does not have four cells")
        };
        rows.push(match *kind {
            "ascii" | "class" => Row::Pairs {
                first: text(first),
                tried: text(second),
                breaks: text(third),
            },
            "opportunity" | "wrap" => Row::Wrap {
                text: text(first),
                width: number(second),
                starts: starts(third),
            },
            "min-content" => Row::MinContent {
                text: text(first),
                width: number(second),
                starts: starts(third),
            },
            other => unreachable!("no row kind {other}"),
        });
    }
    // 94 ASCII rows, one per class character, 15 sentences, and the wrapped
    // strings that survived the trim filter.
    assert!(rows.len() > 200, "line-break.tsv holds {}", rows.len());
    rows
}

/// Lays `text` out at `width` as the table's block was: 16px, a 20px line.
fn lay(measurer: &mut TextMeasurer, text: &str, width: f32) -> Block {
    let base = ResolvedText {
        family: FACE.0.to_owned(),
        size: 16.0,
        line_height: Some(LineHeight::Length(20.0)),
        ..ResolvedText::initial()
    };
    let segments = [TextSegment {
        text: text.to_owned(),
        style: TextStyle::default(),
    }];
    layout(
        measurer,
        &base,
        &segments,
        width,
        &ParagraphStyle::default(),
        Metrics::of(&base),
    )
}

/// The code-point index each line after the first starts at: what each line
/// holds, found in `text` past the whitespace the wrap dropped before it.
fn line_starts(text: &str, block: &Block) -> Vec<usize> {
    let chars: Vec<char> = text.chars().collect();
    let mut cursor = 0;
    let mut out = Vec::new();
    for (index, line) in block.lines.iter().enumerate() {
        let body: String =
            line.runs.iter().map(|run| run.text.as_str()).collect();
        let first = body.chars().next();
        while cursor < chars.len()
            && chars[cursor].is_whitespace()
            && Some(chars[cursor]) != first
        {
            cursor += 1;
        }
        if index > 0 {
            out.push(cursor);
        }
        cursor += body.chars().count();
    }
    out
}

/// Every string wrapped at a width is drawn in [`FACE`] here, not a fallback
/// whose widths Chrome never measured; the 0px rows hold Hangul and emoji on
/// purpose, and every piece overflows 0px whatever its face. A run reports
/// the family it was drawn in, so an unregistered one is the control.
#[test]
fn every_row_is_drawn_in_the_registered_face() {
    let fonts = Fonts::new();
    fonts.register_path(FACE.0, FACE.1).unwrap_or_else(|error| {
        unreachable!("{} did not register: {error}", FACE.0)
    });
    let options = CanvasOptions {
        gpu: false,
        ..CanvasOptions::default()
    };
    let mut canvas = Canvas::with_options(1.0, 1.0, options)
        .unwrap_or_else(|error| unreachable!("no canvas: {error}"));
    let context = canvas.context();
    let mut drawn = |family: &str, text: &str| {
        context.set_font(&Font {
            families: vec![family.into()],
            size: 16.0,
            weight: 400,
            slant: FontSlant::Normal,
            stretch: FontStretch::Normal,
            line_height: None,
        });
        let metrics = context.measure_text(text, None);
        let families: Vec<String> = metrics
            .lines
            .into_iter()
            .flat_map(|line| line.runs)
            .map(|run| run.family)
            .collect();
        (metrics.width, families)
    };

    let (_, elsewhere) = drawn("No Such Family", "漢a");
    assert!(
        !elsewhere.is_empty()
            && elsewhere.iter().all(|family| family != FACE.0),
        "an unregistered family reports {elsewhere:?}"
    );
    let (em, _) = drawn(FACE.0, "漢");
    assert!((em - 16.0).abs() < SLACK, "漢 is {em} wide at 16px");

    let mut wrong = Vec::new();
    let mut widths = 0;
    for row in rows() {
        let text = match row {
            Row::Pairs { .. } | Row::Wrap { width: 0.0, .. } => continue,
            Row::Wrap { text, .. } | Row::MinContent { text, .. } => text,
        };
        widths += 1;
        let (_, families) = drawn(FACE.0, &text);
        if families.is_empty() || families.iter().any(|family| family != FACE.0)
        {
            wrong.push(format!("{text:?}: {families:?}"));
        }
    }
    // 49 wraps and 15 min-content rows: a parse that read none passes above.
    assert!(widths > 60, "{widths} rows with a width");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_line_breaks_where_chrome_breaks_it() {
    let fonts = Fonts::new();
    fonts.register_path(FACE.0, FACE.1).unwrap_or_else(|error| {
        unreachable!("{} did not register: {error}", FACE.0)
    });
    let mut measurer = TextMeasurer::new();
    let mut wrong = Vec::new();
    let mut listed = 0;
    let mut check = |name: String, ours: String, chrome: String| {
        let known = KNOWN.iter().any(|(row, _)| *row == name);
        listed += usize::from(known);
        if (ours == chrome) == known {
            wrong.push(format!(
                "{name}: ours {ours}, Chrome {chrome}{}",
                if known {
                    "; listed in KNOWN, so remove it"
                } else {
                    ""
                }
            ));
        }
    };
    let mut pairs = 0;
    for row in rows() {
        match row {
            Row::Pairs {
                first,
                tried,
                breaks,
            } => {
                for second in tried.chars() {
                    let pair = format!("{first}{second}");
                    let lines = lay(&mut measurer, &pair, 0.0).lines.len();
                    pairs += 1;
                    check(
                        format!("{first} {second}"),
                        (lines > 1).to_string(),
                        breaks.contains(second).to_string(),
                    );
                }
            }
            Row::Wrap {
                text,
                width,
                starts,
            } => {
                let block = lay(&mut measurer, &text, width);
                check(
                    format!("{text} at {width}"),
                    format!("{:?}", line_starts(&text, &block)),
                    format!("{starts:?}"),
                );
            }
            Row::MinContent {
                text,
                width,
                starts,
            } => {
                // The intrinsic question is a wrap at 0, which places every
                // piece between two opportunities on a line of its own.
                let narrowest = lay(&mut measurer, &text, 0.0).width;
                check(
                    format!("{text} at min-content"),
                    if (narrowest - width).abs() <= SLACK {
                        "the same width".to_owned()
                    } else {
                        narrowest.to_string()
                    },
                    "the same width".to_owned(),
                );
                let block = lay(&mut measurer, &text, width);
                check(
                    format!("{text} at its min-content {width}"),
                    format!("{:?}", line_starts(&text, &block)),
                    format!("{starts:?}"),
                );
            }
        }
    }
    // 94 squared, and the class characters squared less what the trim filter
    // dropped: a parse that read no pair would pass everything above.
    assert!(pairs > 8836, "{pairs} pairs");
    assert_eq!(
        listed,
        KNOWN.len(),
        "KNOWN names a row the table does not hold"
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
