//! Where a justified line puts its slack, as Chrome puts it under
//! `text-justify: auto`: an equal share in each gap between words and in
//! each gap beside a character of [`SHARING`], ideographs, kana and CJK
//! punctuation among them, so a line of Chinese or Japanese written without
//! spaces is spread between its characters. Letters inside a Latin word take
//! none, and a gap between a word and an ideograph takes one share, not two.

use icu_segmenter::GraphemeClusterSegmenter;

use super::Line;

/// The code points a gap beside which takes a share, as inclusive ranges,
/// swept from Chrome 158 (`justify.tsv`'s `class` rows, which
/// `the_class_is_the_one_chrome_spreads` reads): Han, kana, bopomofo, CJK
/// symbols and punctuation, most full-width forms, and symbols Chrome sets as
/// CJK, enclosed numbers and much of the emoji among them. Hangul is not.
const SHARING: [(u32, u32); 121] = [
    (0x02C7, 0x02C7),
    (0x02CA, 0x02CB),
    (0x02D9, 0x02D9),
    (0x2020, 0x2021),
    (0x2030, 0x2030),
    (0x203B, 0x203C),
    (0x2042, 0x2042),
    (0x2047, 0x2049),
    (0x2051, 0x2051),
    (0x2100, 0x2100),
    (0x2103, 0x2103),
    (0x2105, 0x2105),
    (0x2109, 0x210A),
    (0x2113, 0x2113),
    (0x2116, 0x2116),
    (0x2121, 0x2121),
    (0x212B, 0x212B),
    (0x213B, 0x213B),
    (0x2150, 0x2152),
    (0x2156, 0x215A),
    (0x2160, 0x216B),
    (0x2170, 0x217B),
    (0x217F, 0x217F),
    (0x2189, 0x2189),
    (0x2194, 0x2195),
    (0x2307, 0x2307),
    (0x2312, 0x2312),
    (0x231A, 0x231B),
    (0x23BE, 0x23CC),
    (0x23CE, 0x23CE),
    (0x23E9, 0x23EC),
    (0x23F0, 0x23F0),
    (0x23F3, 0x23F3),
    (0x2423, 0x2423),
    (0x2460, 0x2492),
    (0x249C, 0x24FF),
    (0x25A0, 0x25A2),
    (0x25AA, 0x25AB),
    (0x25B1, 0x25B3),
    (0x25B6, 0x25B7),
    (0x25BC, 0x25BD),
    (0x25C0, 0x25C1),
    (0x25C6, 0x25C7),
    (0x25C9, 0x25C9),
    (0x25CB, 0x25CC),
    (0x25CE, 0x25D3),
    (0x25E2, 0x25E6),
    (0x25EF, 0x25EF),
    (0x25FD, 0x25FE),
    (0x2600, 0x2603),
    (0x2605, 0x2606),
    (0x260E, 0x260E),
    (0x2614, 0x2617),
    (0x261D, 0x261D),
    (0x2620, 0x2620),
    (0x2640, 0x2640),
    (0x2642, 0x2642),
    (0x2648, 0x2653),
    (0x2660, 0x266F),
    (0x2672, 0x267D),
    (0x267F, 0x267F),
    (0x2693, 0x2693),
    (0x2695, 0x2696),
    (0x26A0, 0x26A1),
    (0x26A7, 0x26A7),
    (0x26AA, 0x26AB),
    (0x26BD, 0x26BE),
    (0x26C4, 0x26C5),
    (0x26CE, 0x26CE),
    (0x26D3, 0x26D4),
    (0x26EA, 0x26EA),
    (0x26F2, 0x26F3),
    (0x26F5, 0x26F5),
    (0x26F9, 0x26FA),
    (0x26FD, 0x26FD),
    (0x2705, 0x2705),
    (0x2708, 0x2708),
    (0x270A, 0x270D),
    (0x2713, 0x2713),
    (0x271A, 0x271A),
    (0x2728, 0x2728),
    (0x273F, 0x2740),
    (0x2744, 0x2744),
    (0x274C, 0x274C),
    (0x274E, 0x274E),
    (0x2753, 0x2757),
    (0x2763, 0x2764),
    (0x2776, 0x277F),
    (0x2795, 0x2797),
    (0x27A1, 0x27A1),
    (0x27B0, 0x27B0),
    (0x27BF, 0x27BF),
    (0x2B1A, 0x2B1C),
    (0x2B50, 0x2B50),
    (0x2B55, 0x2B55),
    (0x2E80, 0x3029),
    (0x3031, 0x312F),
    (0x3190, 0x31EF),
    (0x3200, 0x9FF8),
    (0xF900, 0xFAD9),
    (0xFE10, 0xFE12),
    (0xFE19, 0xFE6B),
    (0xFF01, 0xFF0C),
    (0xFF0E, 0xFF1A),
    (0xFF1D, 0xFF1D),
    (0xFF1F, 0xFF9D),
    (0xFFA1, 0xFFEE),
    (0x1F004, 0x1F004),
    (0x1F0CF, 0x1F0CF),
    (0x1F100, 0x1F100),
    (0x1F110, 0x1F129),
    (0x1F130, 0x1F149),
    (0x1F150, 0x1F169),
    (0x1F170, 0x1F189),
    (0x1F18E, 0x1F18E),
    (0x1F191, 0x1F19A),
    (0x1F1E6, 0x1F3FA),
    (0x1F400, 0x1F6FC),
    (0x1F7E0, 0x1F7F0),
    (0x1F900, 0x1F9FF),
    (0x1FA70, 0x2EB4F),
];

/// Whether a gap beside `character` takes a share of a justified line's
/// slack.
pub(crate) fn shares(character: char) -> bool {
    let point = u32::from(character);
    SHARING
        .binary_search_by(|&(from, to)| {
            if to < point {
                core::cmp::Ordering::Less
            } else if from > point {
                core::cmp::Ordering::Greater
            } else {
                core::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Per run of `line`, the byte offsets after which a share goes: the end of
/// each grapheme cluster followed on the line by another, where either one
/// [`shares`]. A cluster is never split, so a mark stays with its base; the
/// gap a space run stands for is counted apart, and so is no cut before one.
pub(crate) fn cuts(line: &Line) -> Vec<Vec<usize>> {
    let segmenter = GraphemeClusterSegmenter::new();
    // Each cluster as its run, its end and its first character; `None` for a
    // space run, which separates two clusters with a gap of its own.
    let mut clusters: Vec<Option<(usize, usize, char)>> = Vec::new();
    for (index, run) in line.runs.iter().enumerate() {
        if run.is_space() {
            clusters.push(None);
            continue;
        }
        // A run with no character that shares has no cut inside it, so it
        // stands as one cluster, its first character deciding the cut before
        // it and its end the cut after; a Latin line is not segmented.
        let Some(first) = run.text.chars().next() else {
            continue;
        };
        if !run.text.chars().any(shares) {
            clusters.push(Some((index, run.text.len(), first)));
            continue;
        }
        let bounds: Vec<usize> = segmenter.segment_str(&run.text).collect();
        for pair in bounds.windows(2) {
            if let [start, end] = *pair
                && let Some(first) = run.text[start..].chars().next()
            {
                clusters.push(Some((index, end, first)));
            }
        }
    }
    let mut out = vec![Vec::new(); line.runs.len()];
    for pair in clusters.windows(2) {
        if let [Some((index, end, first)), next] = *pair {
            let following = next.map(|(_, _, character)| character);
            if shares(first) || following.is_some_and(shares) {
                out[index].push(end);
            }
        }
    }
    out
}

/// One piece of a line as it is drawn: part of a run, from its line's left
/// edge.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Placed {
    /// The run it is part of.
    pub(crate) run: usize,
    /// Its bytes in the run's text.
    pub(crate) range: core::ops::Range<usize>,
    /// Where it starts.
    pub(crate) x: f32,
    /// What it adds after each of its characters, beyond its run's own
    /// letter-spacing: a share, where every character in it ends at a cut.
    pub(crate) spacing: f32,
}

/// Where each piece of `line` starts when the gap a space run stands for is
/// `gap` and each of `cuts` takes `share` more: a run without a cut is one
/// piece; one with cuts is a piece per cut, each placed where its text starts
/// in the run so the run's own kerning is kept, then moved on by the shares
/// before it. Characters that each end at a cut, a line of ideographs, are one
/// piece spaced by the share, which letter-spacing adds after each of them, so
/// such a line is drawn in a call per run rather than one per character. A gap
/// is added for a space run that is there, not between every pair of runs, as
/// [`super::line_width`] adds it.
pub(crate) fn place(
    measurer: &mut super::TextMeasurer,
    line: &Line,
    cuts: &[Vec<usize>],
    gap: f32,
    share: f32,
) -> Vec<Placed> {
    let mut out = Vec::new();
    let mut x = 0.0;
    let mut pending = false;
    let mut started = false;
    for (index, run) in line.runs.iter().enumerate() {
        if run.is_space() {
            pending = started;
            continue;
        }
        if pending {
            x += gap;
        }
        let at = cuts.get(index).map_or(&[][..], Vec::as_slice);
        let mut from = 0;
        let mut shared = 0.0;
        // A run of single characters each ending at a cut, from where it
        // starts, waiting for the character that ends it.
        let mut spaced: Option<(usize, f32)> = None;
        let mut spaced_to = 0;
        for &to in at.iter().chain(core::iter::once(&run.text.len())) {
            if to <= from {
                continue;
            }
            let cut = at.binary_search(&to).is_ok();
            let single = run.text[from..to].chars().nth(1).is_none();
            if cut && single && spaced.is_some() {
                spaced_to = to;
                shared += share;
                from = to;
                continue;
            }
            if let Some((start, at_x)) = spaced.take() {
                out.push(Placed {
                    run: index,
                    range: start..spaced_to,
                    x: at_x,
                    spacing: share,
                });
            }
            let offset = if from == 0 {
                0.0
            } else {
                measurer.run_width(
                    &run.style,
                    run.style.letter_spacing,
                    &run.text[..from],
                )
            };
            if cut && single {
                spaced = Some((from, x + offset + shared));
                spaced_to = to;
            } else {
                out.push(Placed {
                    run: index,
                    range: from..to,
                    x: x + offset + shared,
                    spacing: 0.0,
                });
            }
            if cut {
                shared += share;
            }
            from = to;
        }
        if let Some((start, at_x)) = spaced {
            out.push(Placed {
                run: index,
                range: start..spaced_to,
                x: at_x,
                spacing: share,
            });
        }
        x += run.width + shared;
        pending = false;
        started = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::style::text::{
        LineHeight, ParagraphStyle, Spacing, TextSegment, TextStyle,
    };

    use super::{SHARING, cuts, place, shares};
    use crate::{
        lines::{Metrics, TextMeasurer, layout, line_width},
        resolve::{ResolvedText, tests::test_fonts},
    };

    /// The recorded Chrome rows.
    const TABLE: &str =
        include_str!("../../../meo-canvas/tests/assets/chrome/justify.tsv");

    /// The faces the table names, under the families it names them by.
    const FACES: [(&str, &str); 2] = [
        ("Trim", "tests/assets/fonts/NotoSansCJKsc-Trim.otf"),
        ("Wide", "tests/assets/fonts/NotoSansSC-Subset.ttf"),
    ];

    /// How far a character's left edge may sit from Chrome's: its rect's x
    /// is floored to a 64th, up to a 64th under the glyph, and a kerned Latin
    /// pair adds what reading kerning off two widths leaves. The worst of the
    /// 243 is 0.0210, 1.34 64ths; the smallest share in the rows is 0.11px.
    const PLACE: f32 = 1.5 / 64.0;

    /// The block's width where a row states none.
    const WIDTH: f32 = 120.0;

    fn rows(kind: &str) -> impl Iterator<Item = Vec<&'static str>> {
        TABLE
            .lines()
            .filter(move |line| line.split('\t').next() == Some(kind))
            .map(|line| line.split('\t').collect())
    }

    #[test]
    fn the_class_is_the_one_chrome_spreads() {
        // Both ends of every run of swept code points, which is where a
        // range drawn from the sweep can be wrong.
        let mut read = 0;
        let mut sharing = 0;
        for cells in rows("class") {
            let [_, from, to, _, answer] = cells.as_slice() else {
                unreachable!("{cells:?} does not have five cells")
            };
            let spreads = answer.trim() == "shares";
            sharing += usize::from(spreads);
            for end in [from, to] {
                let point = u32::from_str_radix(end, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .unwrap_or_else(|| unreachable!("{end} is no code point"));
                assert_eq!(shares(point), spreads, "U+{end}");
                read += 1;
            }
        }
        assert_eq!(sharing, SHARING.len(), "a range per run that shares");
        assert!(read > 400, "only {read} ends read");
    }

    /// The text a JSON cell holds; the table's texts escape only `\n`.
    fn text(cell: &str) -> String {
        cell.trim_matches('"').replace("\\n", "\n")
    }

    /// A CSS property's value in a row's style.
    fn property<'css>(css: &'css str, name: &str) -> Option<&'css str> {
        css.split(';').find_map(|declaration| {
            declaration
                .split_once(':')
                .filter(|(key, _)| *key == name)
                .map(|(_, value)| value)
        })
    }

    fn pixels(value: &str) -> f32 {
        value
            .strip_suffix("px")
            .and_then(|number| number.parse().ok())
            .unwrap_or_else(|| unreachable!("{value} is not in pixels"))
    }

    /// Each character of `written` this renderer sets, laid out in `face`
    /// at `css`'s width and letter-spacing and justified as paint justifies:
    /// its index in the text, its line, and the left edge of the rect Chrome
    /// would give it. A glyph sits where its piece's shaping puts it, kerned
    /// with the glyph after it; a cut's share belongs to the rect of the
    /// character beside it that shares, the left one where both do, so a CJK
    /// character after one that does not share starts its rect a share
    /// before its glyph.
    fn ours(
        measurer: &mut TextMeasurer,
        face: &str,
        written: &str,
        css: &str,
    ) -> Vec<(usize, usize, f32)> {
        let width = property(css, "width").map_or(WIDTH, pixels);
        let base = ResolvedText {
            family: face.to_owned(),
            size: 16.0,
            line_height: Some(LineHeight::Length(20.0)),
            letter_spacing: property(css, "letter-spacing")
                .map_or(Spacing::Normal, |value| {
                    Spacing::Points(pixels(value))
                }),
            ..ResolvedText::initial()
        };
        let metrics = Metrics::of(&base);
        let segments = [TextSegment {
            text: written.to_owned(),
            style: TextStyle::default(),
        }];
        let block = layout(
            measurer,
            &base,
            &segments,
            width,
            &ParagraphStyle::default(),
            metrics,
        );
        let space = measurer.space_width(
            &super::super::RunStyle::base(&base),
            metrics.letter_spacing,
        );
        let chars: Vec<char> = written.chars().collect();
        let mut cursor = 0;
        let mut out = Vec::new();
        let last = block.lines.len().saturating_sub(1);
        for (number, line) in block.lines.iter().enumerate() {
            let fitted = line_width(line, space, metrics.word_spacing);
            let justified =
                number != last && !line.hard_break && fitted < width;
            let at = if justified { cuts(line) } else { Vec::new() };
            let gaps = line
                .runs
                .iter()
                .skip_while(|run| run.is_space())
                .filter(|run| run.is_space())
                .count();
            let points = gaps + at.iter().map(Vec::len).sum::<usize>();
            let share = if justified && points > 0 {
                (width - fitted) / points as f32
            } else {
                0.0
            };
            let gap = space
                + metrics.word_spacing
                + if gaps > 0 { share } else { 0.0 };
            // The character before on the line, and whether a cut follows it.
            let mut before: Option<(char, bool)> = None;
            for piece in place(measurer, line, &at, gap, share) {
                let run = &line.runs[piece.run];
                let text = &run.text[piece.range.clone()];
                // Past the spaces and newlines a wrap consumed, which no cut
                // follows.
                while cursor < chars.len() && !text.starts_with(chars[cursor]) {
                    if chars[cursor] != ' ' || before.is_some() {
                        before = Some((chars[cursor], false));
                    }
                    cursor += 1;
                }
                let width = |measurer: &mut TextMeasurer, text: &str| {
                    measurer.run_width(
                        &run.style,
                        run.style.letter_spacing,
                        text,
                    )
                };
                for (count, (offset, character)) in
                    text.char_indices().enumerate()
                {
                    let through = offset + character.len_utf8();
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "a count of a piece's characters, a few dozen"
                    )]
                    let glyph = (count as f32).mul_add(
                        piece.spacing,
                        piece.x + width(measurer, &text[..through])
                            - width(measurer, &text[offset..through]),
                    );
                    let owned = before.is_some_and(|(previous, cut)| {
                        cut && !shares(previous) && shares(character)
                    });
                    out.push((
                        cursor,
                        number,
                        glyph - if owned { share } else { 0.0 },
                    ));
                    before = Some((character, through == text.len()));
                    cursor += 1;
                }
            }
        }
        out
    }

    #[test]
    fn a_justified_line_spreads_where_chrome_spreads_it() {
        let fonts = test_fonts();
        for (family, path) in FACES {
            fonts
                .register_path(family, path)
                .unwrap_or_else(|error| unreachable!("{error}"));
        }
        let mut measurer = TextMeasurer::new();
        let mut failures = Vec::new();
        let mut read = 0;
        for cells in rows("line") {
            let [_, face, written, css, characters] = cells.as_slice() else {
                unreachable!("{cells:?} does not have five cells")
            };
            // A last line justified is `text-align-last`, which the scene has
            // no way to say; those rows are the class's evidence.
            if css.contains("text-align-last:justify") {
                continue;
            }
            let written = text(written);
            let chrome: Vec<Option<(usize, f32)>> = characters
                .split(' ')
                .map(|cell| {
                    let parts: Vec<&str> = cell.split(':').collect();
                    match parts.as_slice() {
                        [line, x, _] => line.parse().ok().zip(x.parse().ok()),
                        _ => None,
                    }
                })
                .collect();
            for (index, line, x) in ours(&mut measurer, face, &written, css) {
                match chrome.get(index).copied().flatten() {
                    Some((on, expected))
                        if on != line || (x - expected).abs() > PLACE =>
                    {
                        failures.push(format!(
                            "{written:?} {css}: character {index} on line {line} at {x}, \
                             Chrome line {on} at {expected}"
                        ));
                    }
                    Some(_) => read += 1,
                    None => failures.push(format!(
                        "{written:?}: character {index} has no rect in Chrome"
                    )),
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        // Every character but the nine spaces, which a line keeps as gaps.
        assert_eq!(read, 243, "every character the wraps place is read");
    }
}
