//! Which full-width punctuation is narrowed, against Chrome's default
//! `text-spacing-trim: normal`: every ordered pair of marks between
//! ideographs, sentences, lines ending on a closing mark, and min-content, in
//! a face with `halt` and the same face without it. Each row of
//! `text-spacing-trim.tsv` is laid out by [`layout`].

use meo_canvas_core::{
    lines::{Block, Metrics, RunStyle, TextMeasurer, layout, line_width},
    resolve::{Fonts, ResolvedText},
};
use meo_canvas_scene::style::text::{
    LineHeight, ParagraphStyle, TextSegment, TextStyle,
};

/// The recorded Chrome rows.
const TABLE: &str =
    include_str!("../../meo-canvas/tests/assets/chrome/text-spacing-trim.tsv");

/// The faces the table names, by the family both register them under.
const FACES: [(&str, &str); 2] = [
    ("Trim", "tests/assets/fonts/NotoSansCJKsc-Trim.otf"),
    ("NoHalt", "tests/assets/fonts/NotoSansCJKsc-NoHalt.otf"),
];

/// One of Chrome's layout units, a 64th of a pixel. Each character's client
/// rect is snapped to one, so a line's width summed from them may sit up to a
/// unit per character from this renderer's, and a min-content width a unit.
const UNIT: f32 = 1.0 / 64.0;

/// How much narrower than set whole a character must be to count as
/// narrowed: a quarter of the 16px em, where narrowing takes half of it and
/// snapping a unit.
const NARROWED: f32 = 4.0;

/// A width that wraps nothing: every row is far narrower.
const UNWRAPPED: f32 = 10_000.0;

/// One row of the table.
#[derive(Debug)]
enum Row {
    /// `漢` + `first` + each of `tried` + `漢`, unwrapped; `narrowed` says per
    /// tried mark which of the two Chrome narrowed.
    Pairs {
        face: String,
        lang: String,
        first: char,
        tried: Vec<char>,
        narrowed: Vec<char>,
    },
    /// `text` at `width`, or unwrapped, with Chrome's line starts and
    /// character widths.
    Line {
        face: String,
        text: String,
        width: Option<f32>,
        starts: Vec<usize>,
        widths: Vec<f32>,
    },
    /// `text` at min-content: Chrome's width, line starts and character
    /// widths.
    MinContent {
        face: String,
        text: String,
        width: f32,
        starts: Vec<usize>,
        widths: Vec<f32>,
    },
}

/// A JSON string cell, as the tool writes it: quoted, with `\"`, `\\`, `\n`
/// and `\uXXXX` escapes and every other character literal.
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
            Some('n') => out.push('\n'),
            other => unreachable!("\\{other:?} in {cell}"),
        }
    }
    out
}

fn number(cell: &str) -> f32 {
    cell.parse()
        .unwrap_or_else(|_| unreachable!("{cell} is not a number"))
}

fn list<T: core::str::FromStr>(cell: &str) -> Vec<T> {
    if cell == "-" {
        return Vec::new();
    }
    cell.split(',')
        .map(|value| {
            value
                .parse()
                .unwrap_or_else(|_| unreachable!("{value} in {cell}"))
        })
        .collect()
}

/// A `result` cell: line starts, `;`, character widths.
fn result(cell: &str) -> (Vec<usize>, Vec<f32>) {
    let (starts, widths) = cell
        .split_once(';')
        .unwrap_or_else(|| unreachable!("{cell} has no `;`"));
    (list(starts), list(widths))
}

fn rows() -> Vec<Row> {
    let mut header = false;
    let mut rows = Vec::new();
    for line in TABLE.lines() {
        if let Some(rest) = line.strip_prefix('#') {
            header |= rest.trim() == "kind\tface\tlang\ttext\twidth\tresult";
            continue;
        }
        assert!(header, "a row before the column line: {line}");
        let cells: Vec<&str> = line.split('\t').collect();
        let [kind, face, lang, written, width, outcome] = cells.as_slice()
        else {
            unreachable!("{line} does not have six cells")
        };
        let face = (*face).to_owned();
        rows.push(match *kind {
            "pairs" => Row::Pairs {
                face,
                lang: (*lang).to_owned(),
                first: text(written).chars().next().unwrap_or_else(|| {
                    unreachable!("{line} has no first mark")
                }),
                tried: text(width).chars().collect(),
                narrowed: outcome.chars().collect(),
            },
            "line" => {
                let (starts, widths) = result(outcome);
                Row::Line {
                    face,
                    text: text(written),
                    width: (*width != "-").then(|| number(width)),
                    starts,
                    widths,
                }
            }
            "min-content" => {
                let (starts, widths) = result(outcome);
                Row::MinContent {
                    face,
                    text: text(written),
                    width: number(width),
                    starts,
                    widths,
                }
            }
            other => unreachable!("no row kind {other}"),
        });
    }
    // Forty pairs rows per face and `lang`, and the sentences, wrapped
    // strings and min-content rows for both faces.
    assert!(
        rows.len() > 300,
        "text-spacing-trim.tsv holds {}",
        rows.len()
    );
    rows
}

/// The style the table's blocks have: 16px, a 20px line.
fn base(face: &str) -> ResolvedText {
    ResolvedText {
        family: face.to_owned(),
        size: 16.0,
        line_height: Some(LineHeight::Length(20.0)),
        ..ResolvedText::initial()
    }
}

fn lay(
    measurer: &mut TextMeasurer,
    face: &str,
    text: &str,
    width: f32,
) -> Block {
    let base = base(face);
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

/// What one laid-out block says per character: the line it is on and
/// whether it was narrowed, found in `text` past the spaces a wrap dropped.
fn read(text: &str, block: &Block) -> Vec<(usize, bool)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![(0, false); chars.len()];
    let mut cursor = 0;
    for (index, line) in block.lines.iter().enumerate() {
        let first = line.runs.iter().flat_map(|run| run.text.chars()).next();
        while cursor < chars.len()
            && matches!(chars[cursor], ' ' | '\n')
            && Some(chars[cursor]) != first
        {
            cursor += 1;
        }
        for run in &line.runs {
            for _ in run.text.chars() {
                if let Some(slot) = out.get_mut(cursor) {
                    *slot = (index, run.style.trimmed);
                }
                cursor += 1;
            }
        }
    }
    out
}

/// The code-point indices lines after the first start at.
fn starts(read: &[(usize, bool)]) -> Vec<usize> {
    (1..read.len())
        .filter(|&at| read[at].0 > read[at - 1].0)
        .collect()
}

/// Which characters Chrome narrowed: those its rect shows a quarter em or
/// more under their width set alone here.
fn chrome_narrowed(
    measurer: &mut TextMeasurer,
    face: &str,
    text: &str,
    widths: &[f32],
) -> Vec<bool> {
    let style = RunStyle::base(&base(face));
    text.chars()
        .zip(widths)
        .map(|(character, width)| {
            // A newline has no rect and is drawn as nothing.
            character != '\n' && {
                let alone =
                    measurer.run_width(&style, 0.0, &character.to_string());
                alone - width >= NARROWED
            }
        })
        .collect()
}

/// Compares one laid-out block against Chrome's starts, narrowed characters
/// and per-line widths, returning what differs.
fn compare(
    measurer: &mut TextMeasurer,
    face: &str,
    text: &str,
    block: &Block,
    chrome_starts: &[usize],
    chrome_widths: &[f32],
) -> Option<String> {
    let ours = read(text, block);
    let our_starts = starts(&ours);
    let our_narrowed: Vec<bool> = ours.iter().map(|(_, n)| *n).collect();
    let narrowed = chrome_narrowed(measurer, face, text, chrome_widths);
    let space = measurer.space_width(&RunStyle::base(&base(face)), 0.0);
    let mut lines = vec![(0.0_f32, 0_usize); block.lines.len()];
    for (at, width) in chrome_widths.iter().enumerate() {
        let line = chrome_starts.iter().filter(|&&start| start <= at).count();
        if let Some(slot) = lines.get_mut(line) {
            slot.0 += width;
            slot.1 += 1;
        }
    }
    let widths_agree = block.lines.len() == chrome_starts.len() + 1
        && block
            .lines
            .iter()
            .zip(&lines)
            .all(|(line, (chrome, count))| {
                (line_width(line, space, 0.0) - chrome).abs()
                    <= UNIT * (*count as f32 + 1.0)
            });
    (our_starts != chrome_starts || our_narrowed != narrowed || !widths_agree)
        .then(|| {
            let mark = |flags: &[bool]| -> String {
                text.chars()
                    .zip(flags)
                    .map(|(c, n)| if *n { c } else { '.' })
                    .collect()
            };
            format!(
                "starts ours {our_starts:?} Chrome {chrome_starts:?}; \
                 narrowed ours {} Chrome {}; line widths ours {:?} Chrome {:?}",
                mark(&our_narrowed),
                mark(&narrowed),
                block
                    .lines
                    .iter()
                    .map(|line| line_width(line, space, 0.0))
                    .collect::<Vec<_>>(),
                lines.iter().map(|(w, _)| *w).collect::<Vec<_>>(),
            )
        })
}

#[test]
fn punctuation_is_narrowed_where_chrome_narrows_it() {
    let fonts = Fonts::new();
    for (family, path) in FACES {
        fonts.register_path(family, path).unwrap_or_else(|error| {
            unreachable!("{family} did not register: {error}")
        });
    }
    let mut measurer = TextMeasurer::new();
    let mut wrong = Vec::new();
    let mut pairs = 0;
    for row in rows() {
        match row {
            Row::Pairs {
                face,
                lang,
                first,
                tried,
                narrowed,
            } => {
                for (second, chrome) in tried.iter().zip(&narrowed) {
                    let pair = format!("漢{first}{second}漢");
                    let block = lay(&mut measurer, &face, &pair, UNWRAPPED);
                    let ours = read(&pair, &block);
                    let code = match (ours[1].1, ours[2].1) {
                        (true, _) => '1',
                        (_, true) => '2',
                        _ => '-',
                    };
                    pairs += 1;
                    if code != *chrome {
                        wrong.push(format!(
                            "{face} lang {lang} {pair}: ours {code}, Chrome {chrome}"
                        ));
                    }
                }
            }
            Row::Line {
                face,
                text,
                width,
                starts,
                widths,
            } => {
                let block = lay(
                    &mut measurer,
                    &face,
                    &text,
                    width.unwrap_or(UNWRAPPED),
                );
                if let Some(difference) = compare(
                    &mut measurer,
                    &face,
                    &text,
                    &block,
                    &starts,
                    &widths,
                ) {
                    wrong.push(format!(
                        "{face} {text} at {width:?}: {difference}"
                    ));
                }
            }
            Row::MinContent {
                face,
                text,
                width,
                starts,
                widths,
            } => {
                // The intrinsic question is a wrap at 0, which places every
                // piece between two opportunities on a line of its own.
                let narrowest = lay(&mut measurer, &face, &text, 0.0).width;
                if (narrowest - width).abs() > UNIT {
                    wrong.push(format!(
                        "{face} {text} at min-content: ours {narrowest}, Chrome {width}"
                    ));
                }
                let block = lay(&mut measurer, &face, &text, width);
                if let Some(difference) = compare(
                    &mut measurer,
                    &face,
                    &text,
                    &block,
                    &starts,
                    &widths,
                ) {
                    wrong.push(format!(
                        "{face} {text} at its min-content {width}: {difference}"
                    ));
                }
            }
        }
    }
    // Forty by forty per face and `lang`: a parse that read no pair would pass
    // everything above.
    assert_eq!(pairs, 40 * 40 * 6, "{pairs} pairs");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
