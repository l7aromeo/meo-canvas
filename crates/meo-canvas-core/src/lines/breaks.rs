//! Chrome's break opportunities for `white-space: normal`, `line-break: auto`
//! and `word-break: normal`: Blink's space rule and ASCII table, then UAX 14
//! 17 at `line-break: normal`. The parent module's documentation has the rules;
//! `chrome_line_break.rs` measures them.

use icu_segmenter::{
    LineSegmenter,
    options::{LineBreakOptions, LineBreakStrictness, LineBreakWordOption},
};

/// The soft hyphen, never an opportunity: Chrome breaks there and draws a
/// hyphen, and this renderer, which draws none, breaks as `hyphens: none` does.
const SOFT_HYPHEN: char = '\u{ad}';

/// What Blink's table breaks before after `-`, besides letters; a digit is
/// decided by what precedes the hyphen.
const AFTER_HYPHEN: &str = "\"#%&'(*+-<=>@[\\^_`{|~";

/// What Blink's table breaks before after `?`, besides letters and digits.
const AFTER_QUESTION: &str = "#$%&(*+-<=>@[\\^_`{|~";

/// The opening brackets Blink's table breaks before, after [`CLOSING`].
const OPENING: &str = "([{<";

/// What a line may break after when an [`OPENING`] bracket follows: closing
/// brackets and most punctuation; not letters, digits, opening brackets, or
/// any of `$'/@^_` and the backtick. `-` and `?` have rows of their own.
const CLOSING: &str = "!\"#%&)*+,.:;=>\\]|}~";

/// The characters Blink's breaker treats as spaces: a break follows a run of
/// them. Not every `char::is_whitespace`: a no-break space joins what is
/// either side of it, and an ideographic space breaks as UAX 14 says.
const fn is_breakable_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n')
}

/// Whether Blink's table decides the pair `before`, `after`: both printable
/// ASCII, `!` to `~`.
const fn is_table_pair(before: char, after: char) -> bool {
    matches!(before, '!'..='~') && matches!(after, '!'..='~')
}

/// Blink's table for a pair of printable ASCII characters: whether a line may
/// break between `before` and `after`. A hyphen before a digit is not decided
/// here; see [`opportunities`].
fn table_breaks(before: char, after: char) -> bool {
    match before {
        '-' => after.is_ascii_alphabetic() || AFTER_HYPHEN.contains(after),
        '?' => after.is_ascii_alphanumeric() || AFTER_QUESTION.contains(after),
        _ => OPENING.contains(after) && CLOSING.contains(before),
    }
}

/// The byte offsets in `text` before which a line may break, ascending, never
/// 0 or `text.len()`. A newline is followed by one like any space; that the
/// break there is mandatory is the wrap's business.
pub(crate) fn opportunities(text: &str) -> Vec<usize> {
    let mut options = LineBreakOptions::default();
    options.strictness = Some(LineBreakStrictness::Normal);
    options.word_option = Some(LineBreakWordOption::Normal);
    let mut segmenter = LineSegmenter::new_17_for_non_complex_scripts(options);
    // The dictionary, not the LSTM: ICU, and so Chrome, breaks Thai, Lao,
    // Khmer and Burmese by dictionary, and the two disagree on Thai words.
    segmenter.load_dictionary();
    let unicode: Vec<usize> = segmenter.segment_str(text).collect();

    let mut out = Vec::new();
    let mut next_unicode = unicode.iter().copied().peekable();
    let mut before_before: Option<char> = None;
    let mut before: Option<char> = None;
    for (at, after) in text.char_indices() {
        while next_unicode.next_if(|&offset| offset < at).is_some() {}
        if let Some(previous) = before {
            let breaks = if is_breakable_space(after) {
                false
            } else if is_breakable_space(previous) {
                true
            } else if previous == '-' && after.is_ascii_digit() {
                before_before.is_some_and(|c| c.is_ascii_alphanumeric())
            } else if is_table_pair(previous, after) {
                table_breaks(previous, after)
            } else if !previous.is_ascii() || !after.is_ascii() {
                previous != SOFT_HYPHEN && next_unicode.peek() == Some(&at)
            } else {
                // Two ASCII characters outside the table: a control character
                // beside anything. Blink keeps them together.
                false
            };
            if breaks {
                out.push(at);
            }
        }
        before_before = before;
        before = Some(after);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::opportunities;

    /// The opportunities as the text either side of each, for readable
    /// assertions: `"a b"` reads `["a ", "b"]`.
    fn split(text: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut from = 0;
        for at in opportunities(text) {
            out.push(&text[from..at]);
            from = at;
        }
        out.push(&text[from..]);
        out
    }

    #[test]
    fn ideographs_and_kana_break_between_every_character() {
        assert_eq!(split("漢字かな"), ["漢", "字", "か", "な"]);
    }

    #[test]
    fn closing_punctuation_stays_with_what_precedes_it() {
        assert_eq!(split("漢、。」漢"), ["漢、。」", "漢"]);
        assert_eq!(split("漢「漢"), ["漢", "「漢"]);
    }

    #[test]
    fn small_kana_and_the_prolonged_sound_mark_may_begin_a_line() {
        // `line-break: normal`, which Chrome applies at `auto`; `strict` would
        // keep each with the character before it.
        assert_eq!(split("ッァー"), ["ッ", "ァ", "ー"]);
        // An iteration mark never begins one, at any strictness.
        assert_eq!(split("漢々"), ["漢々"]);
    }

    #[test]
    fn a_space_hangs_and_the_line_breaks_after_it() {
        assert_eq!(split("a  b"), ["a  ", "b"]);
        // UAX 14 would keep `(` with `a`, and `)` with `a`.
        assert_eq!(split("( a )"), ["( ", "a ", ")"]);
    }

    #[test]
    fn a_no_break_space_joins_its_neighbours() {
        assert_eq!(split("a\u{a0}b"), ["a\u{a0}b"]);
    }

    #[test]
    fn ascii_pairs_follow_blinks_table() {
        assert_eq!(split("well-known"), ["well-", "known"]);
        assert_eq!(split("what?yes"), ["what?", "yes"]);
        assert_eq!(split("a/b C++ a!b"), ["a/b ", "C++ ", "a!b"]);
        assert_eq!(split("f(x) a.(b)"), ["f(x) ", "a.", "(b)"]);
    }

    #[test]
    fn a_hyphen_before_a_digit_breaks_only_after_a_letter_or_digit() {
        assert_eq!(split("x86-64"), ["x86-", "64"]);
        assert_eq!(split("-5 (-5"), ["-5 ", "(-5"]);
    }

    #[test]
    fn a_word_beside_an_ideograph_breaks_from_it_but_not_inside() {
        assert_eq!(split("在Tokyo见"), ["在", "Tokyo", "见"]);
    }

    #[test]
    fn a_soft_hyphen_is_not_an_opportunity() {
        assert_eq!(split("a\u{ad}b"), ["a\u{ad}b"]);
    }

    #[test]
    fn neither_end_of_the_text_is_an_opportunity() {
        assert!(opportunities("").is_empty());
        assert!(opportunities("漢").is_empty());
        assert_eq!(opportunities("a "), Vec::<usize>::new());
    }
}
