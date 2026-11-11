//! What a colour string resolves to, against what Chrome paints for it as a
//! background (`color-strings.tsv`): the bytes where Chrome paints it, and a
//! refusal where Chrome drops the declaration. The `unread` group is where this
//! renderer still differs, and must keep differing until a change moves it.

use meo_canvas_core::parse_color;
use meo_canvas_scene::style::paint::Color;

/// The recorded Chrome rows, which `chrome_tables_are_read` requires be read.
const TABLE: &str =
    include_str!("../../meo-canvas/tests/assets/chrome/color-strings.tsv");

/// How far a translucent row's channel or alpha may sit from Chrome's: one,
/// since its colour and alpha are recovered from two composited bytes, each
/// rounded once. An opaque row is read exactly.
const TOLERANCE: i32 = 1;

/// The groups this renderer still answers differently from Chrome, each with
/// why: a pinned list, so a row that starts agreeing fails and is moved.
const KNOWN_APART: &[(&str, &str)] = &[(
    "unread",
    "`meo-skia-canvas` 0.17.2 refuses `color(display-p3-linear ...)`, which \
     Chrome paints (l7aromeo/meo-skia-canvas#687)",
)];

/// One row: the string, its group, and what Chrome did with it.
struct Row<'a> {
    css: String,
    group: &'a str,
    chrome: Option<Color>,
    translucent: bool,
}

fn number(text: &str) -> i32 {
    text.parse()
        .unwrap_or_else(|error| unreachable!("`{text}`: {error}"))
}

/// The colour Chrome's two composited pixels name: the same over white and
/// black is opaque; otherwise alpha is one minus their difference, and the
/// colour is the black reading over that alpha.
fn painted(white: [i32; 3], black: [i32; 3]) -> (Color, bool) {
    if white == black {
        let [r, g, b] = black.map(byte);
        return (Color::rgba(r, g, b, 255), false);
    }
    let alpha = 255 - (white[0] - black[0]);
    let unmultiplied = |value: i32| {
        if alpha == 0 {
            0
        } else {
            byte((value * 255 + alpha / 2) / alpha)
        }
    };
    let [r, g, b] = black.map(unmultiplied);
    (Color::rgba(r, g, b, byte(alpha)), true)
}

fn byte(value: i32) -> u8 {
    u8::try_from(value.clamp(0, 255))
        .unwrap_or_else(|error| unreachable!("clamped to a byte: {error}"))
}

fn rows() -> Vec<Row<'static>> {
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let field: Vec<&str> = line.split('\t').collect();
            assert_eq!(field.len(), 9, "`{line}` is not nine fields");
            let css: String = unquoted(field[0]);
            let white = [field[3], field[4], field[5]].map(number);
            let black = [field[6], field[7], field[8]].map(number);
            let (chrome, translucent) = match field[2] {
                "refused" => (None, false),
                "painted" => {
                    let (color, translucent) = painted(white, black);
                    (Some(color), translucent)
                }
                other => unreachable!("`{other}` is not a verdict"),
            };
            Row {
                css,
                group: field[1],
                chrome,
                translucent,
            }
        })
        .collect()
}

/// A JSON string literal as the tool writes it: quoted, with no escapes but
/// `\"`, since no row holds a backslash.
fn unquoted(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| unreachable!("`{text}` is not quoted"))
        .replace("\\\"", "\"")
}

/// Whether ours is Chrome's, to the tolerance its row allows.
fn agrees(row: &Row<'_>, ours: Option<Color>) -> bool {
    match (ours, row.chrome) {
        (None, None) => true,
        (Some(ours), Some(chrome)) => {
            let tolerance = if row.translucent { TOLERANCE } else { 0 };
            let close =
                |a: u8, b: u8| (i32::from(a) - i32::from(b)).abs() <= tolerance;
            // At alpha 0 Chrome's pixels say nothing about the colour.
            (chrome.a == 0
                || (close(ours.r, chrome.r)
                    && close(ours.g, chrome.g)
                    && close(ours.b, chrome.b)))
                && close(ours.a, chrome.a)
        }
        _ => false,
    }
}

#[test]
fn a_colour_string_resolves_to_what_chrome_paints() {
    let rows = rows();
    assert!(rows.len() > 50, "the table holds {} rows", rows.len());
    // The table can fail only if it holds both verdicts.
    assert!(rows.iter().any(|row| row.chrome.is_none()));
    assert!(rows.iter().any(|row| row.translucent));

    let mut wrong = Vec::new();
    for row in &rows {
        let ours = parse_color(&row.css);
        let apart = KNOWN_APART.iter().any(|(group, _)| *group == row.group);
        match (agrees(row, ours), apart) {
            (true, false) | (false, true) => {}
            (false, false) => wrong.push(format!(
                "{}: ours {ours:?}, chrome {:?}",
                row.css, row.chrome
            )),
            (true, true) => wrong.push(format!(
                "{}: agrees with Chrome now, in the `{}` group -- move it out \
                 of KNOWN_APART's group",
                row.css, row.group
            )),
        }
    }
    for (group, _) in KNOWN_APART {
        assert!(
            rows.iter().any(|row| row.group == *group),
            "`{group}` is pinned and the table has no row in it"
        );
    }
    assert!(
        wrong.is_empty(),
        "{} rows:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
