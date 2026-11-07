//! Two colours mixed partway through an animation, against Chrome's
//! (`color-mix.tsv`): Chrome mixes with the alpha multiplied in, so a
//! transparent end adds no colour. Each row is compared as Chrome drew it, the
//! mix composited over white and over black.

use meo_canvas_core::{
    animate::color::{Rgba, mix},
    color::parse_channels,
};

/// The recorded Chrome rows, which `chrome_tables_are_read` requires be read.
const TABLE: &str =
    include_str!("../../meo-canvas/tests/assets/chrome/color-mix.tsv");

/// How far a composited channel may sit from Chrome's: one, for each side
/// rounding a half differently. Mixing straight is off by 40 or more here.
const TOLERANCE: f64 = 1.0;

fn colour(quoted: &str) -> Rgba {
    let css = quoted
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| unreachable!("`{quoted}` is not quoted"));
    let [r, g, b, a] = parse_channels(css)
        .unwrap_or_else(|| unreachable!("`{css}` is a colour"));
    Rgba { r, g, b, a }
}

fn number(text: &str) -> f64 {
    text.parse()
        .unwrap_or_else(|error| unreachable!("`{text}`: {error}"))
}

/// The colour drawn over an opaque page of `page` on every channel.
fn over(colour: Rgba, page: f64) -> [f64; 3] {
    let alpha = colour.a.clamp(0.0, 1.0);
    [colour.r, colour.g, colour.b].map(|channel| {
        channel
            .clamp(0.0, 255.0)
            .mul_add(alpha, page * (1.0 - alpha))
    })
}

#[test]
fn a_colour_animation_mixes_as_chrome_does() {
    let mut compared = 0;
    let mut wrong = Vec::new();
    for line in TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
    {
        let field: Vec<&str> = line.split('\t').collect();
        assert_eq!(field.len(), 10, "`{line}` is not ten fields");
        let mixed = mix(colour(field[1]), colour(field[2]), number(field[3]));
        let chrome = [&field[4..7], &field[7..10]]
            .map(|bytes| [bytes[0], bytes[1], bytes[2]].map(number));
        let ours = [over(mixed, 255.0), over(mixed, 0.0)];
        let close = ours
            .iter()
            .flatten()
            .zip(chrome.iter().flatten())
            .all(|(a, b)| (a - b).abs() <= TOLERANCE);
        if !close {
            wrong.push(format!(
                "{} {} -> {} at {}: ours {ours:.1?}, chrome {chrome:?}",
                field[0], field[1], field[2], field[3]
            ));
        }
        compared += 1;
    }
    assert!(compared > 0, "the table holds no rows");
    assert!(
        wrong.is_empty(),
        "{} of {compared} rows:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
