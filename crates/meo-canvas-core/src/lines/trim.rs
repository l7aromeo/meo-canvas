//! Chrome's `text-spacing-trim: normal`: which full-width marks lose the empty
//! half of their em beside another mark, and which may lose it at a line end.
//! The parent module's documentation has the rules;
//! `chrome_text_spacing_trim.rs` measures them.

/// What a mark is to the trimming rules. The sets are the characters Chrome
/// 158 classified in Noto Sans CJK SC 2.004 by trimming them beside `「` and
/// `」`; a vertical or half-width form belongs to its class and is never
/// narrowed, since a face's `halt` leaves it alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mark {
    /// An opening bracket or quote: its empty half is before it.
    Open,
    /// A closing bracket or quote: its empty half is after it, and it may lose
    /// that half at a line end.
    Close,
    /// A comma, stop or colon: as [`Mark::Close`] beside another mark, and
    /// never at a line end.
    Dot,
    /// A middle dot, which narrows a neighbour and is never narrowed.
    Middle,
    /// The ideographic space, which narrows a neighbour and is never narrowed.
    Space,
}

/// The opening marks.
const OPEN: &str = "‘“‚„〈《「『【〔〖〘〚〝（［｛｟︗︵︷︹︻︽︿﹁﹃﹇｢";

/// The closing brackets and quotes.
const CLOSE: &str = "’”〉》」』】〕〗〙〛〞〟）］｝｠︘︶︸︺︼︾﹀﹂﹄﹈｣";

/// The commas, stops and colons.
const DOT: &str = "、。，．：；";

/// The middle dots.
const MIDDLE: &str = "・‧·";

/// The class of `character`, if it has one.
pub(super) fn mark(character: char) -> Option<Mark> {
    if OPEN.contains(character) {
        Some(Mark::Open)
    } else if CLOSE.contains(character) {
        Some(Mark::Close)
    } else if DOT.contains(character) {
        Some(Mark::Dot)
    } else if MIDDLE.contains(character) {
        Some(Mark::Middle)
    } else if character == '\u{3000}' {
        Some(Mark::Space)
    } else {
        None
    }
}

/// The byte offsets in `text` of the marks a neighbour narrows, in order. Of
/// two marks side by side the opening one is narrowed if there is one, and
/// otherwise a closing mark or dot before anything but an opening mark.
pub(super) fn adjacent(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut before: Option<(usize, Mark)> = None;
    for (at, character) in text.char_indices() {
        let this = mark(character);
        if let (Some((was, previous)), Some(current)) = (before, this) {
            if current == Mark::Open {
                out.push(at);
            } else if matches!(previous, Mark::Close | Mark::Dot) {
                out.push(was);
            }
        }
        before = this.map(|current| (at, current));
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::{Mark, adjacent, mark};

    /// The narrowed marks as characters, for readable assertions.
    fn narrowed(text: &str) -> String {
        adjacent(text)
            .into_iter()
            .filter_map(|at| text[at..].chars().next())
            .collect()
    }

    #[test]
    fn of_two_marks_the_opening_one_is_narrowed_and_otherwise_the_first() {
        // Chrome 158's rows, `text-spacing-trim.tsv` has them all.
        for (text, chrome) in [
            ("漢「「漢", "「"),
            ("漢」」漢", "」"),
            ("漢、。漢", "、"),
            ("漢」「漢", "「"),
            ("漢・「漢", "「"),
            ("漢」・漢", "」"),
            ("漢\u{3000}「漢", "「"),
            ("漢」\u{3000}漢", "」"),
            ("漢字、。」漢字", "、。"),
            ("漢」 」漢", ""),
            ("漢」漢", ""),
            ("漢・・漢", ""),
            ("「」", ""),
        ] {
            assert_eq!(narrowed(text), chrome, "{text}");
        }
    }

    #[test]
    fn a_run_of_closing_marks_narrows_all_but_the_last() {
        assert_eq!(adjacent("」」」"), vec![0, 3]);
        assert_eq!(adjacent("「「「"), vec![3, 6]);
    }

    #[test]
    fn every_class_is_one_chrome_measured() {
        assert_eq!(mark('「'), Some(Mark::Open));
        assert_eq!(mark('」'), Some(Mark::Close));
        assert_eq!(mark('：'), Some(Mark::Dot));
        assert_eq!(mark('·'), Some(Mark::Middle));
        assert_eq!(mark('\u{3000}'), Some(Mark::Space));
        assert_eq!(mark('！'), None);
        assert_eq!(mark('a'), None);
    }
}
