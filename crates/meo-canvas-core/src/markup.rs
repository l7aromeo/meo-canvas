//! Turns a marked-up string into the segments a `Text` node holds.
//!
//! [`meo_canvas_scene::style::text::TextSegment`] is the scene's representation
//! of styled text, and something has to produce it from the string a caller
//! actually writes. In v1 that something was TypeScript -- `TextNode`'s
//! constructor parsed the markup on its way into the node -- which meant only
//! JavaScript callers got rich text. A Rust caller writing
//! `Text::new("hello <b>world</b>")` would have got the angle brackets
//! literally.
//!
//! So the parser lives here, above the scene crate and below every surface, for
//! the same reason text measurement does: one implementation serves the addon,
//! the CLI and a Rust caller alike, and none of them can disagree with the
//! others about what `<b>` means.
//!
//! # The grammar
//!
//! Ported from `text.canvas.ts` -- `processEscapeSequences` at `:250` and
//! `parseRichText` at `:298` -- because the markup is a published surface and a
//! string that rendered one way in v1 has to render the same way here.
//!
//! Escapes are processed first, over the whole string, and then tags are
//! scanned in the result. There is no escape for a `<`: `\<` is not a sequence
//! v1 knows, so it keeps both characters, and the backslash it leaves is then
//! what stops the tag scanner matching. A backslash therefore does suppress a
//! tag -- and prints itself doing it. Writing a literal `<b>` is not something
//! this markup can express, which is v1's position too.
//!
//! Five tags carry meaning -- `color`, `weight`, `size`, `b` and `i` -- and
//! their values may be double-quoted, single-quoted or bare. Any other tag name
//! is consumed and styles nothing, and a closing tag closes whatever is open
//! regardless of the name it gives. Both are v1's behaviour rather than
//! decisions taken here; see [`parse`].
//!
//! # What a segment inherits
//!
//! v1 seeded its parse with the node's own colour, weight and size
//! (`text.canvas.ts:58-64`) and wrote them into every segment, because a v1
//! segment carried a resolved style rather than an override.
//!
//! Here the base is [`TextStyle::default`], whose every field is `None`
//! meaning "inherit". A segment records only what a tag actually set, and
//! everything else resolves from the node and its ancestors at paint. The
//! observable behaviour is v1's -- a tag inherits from the node it sits in --
//! without freezing the node's style into the scene at parse time, where a
//! later change to the node would no longer reach the text.

use meo_canvas_scene::style::text::{
    FontStyle, FontWeight, TextSegment, TextStyle,
};

use crate::{color::parse_color, diagnostic::Diagnostic};

/// The spaces a `\t` becomes.
///
/// Four, which is v1's (`text.canvas.ts:256`). A real tab stop depends on the
/// column the tab lands in, and neither renderer has ever had columns.
const TAB: &str = "    ";

/// Parses markup into styled segments.
///
/// Escape sequences are resolved first, then tags. Text outside any tag comes
/// back as a segment with an empty [`TextStyle`], and an empty run of text
/// produces no segment at all -- so an input of `""`, or of nothing but tags,
/// returns an empty vector. A caller that needs a paragraph to exist supplies
/// the empty segment itself; this function does not invent one.
///
/// # Tags
///
/// | Tag | Effect |
/// |---|---|
/// | `<color=v>` | Sets the colour, from any string CSS names |
/// | `<weight=v>` | Sets the weight, from `normal`, `bold` or a number |
/// | `<size=v>` | Sets the size in pixels, from a number |
/// | `<b>` | Sets the weight to bold |
/// | `<i>` | Sets the style to italic |
///
/// A value may be written `<color="red">`, `<color='red'>` or `<color=red>`.
/// Tag names are matched case-insensitively.
///
/// # Where a value is not understood
///
/// The property is **cleared** rather than left as it was, so the span falls
/// back to what it inherits. `<size=wide>` does not keep an enclosing
/// `<size=20>`. This is v1's behaviour at `text.canvas.ts:346-351`, where an
/// unparseable size is set to `undefined`, and the other properties follow it
/// so that one bad value behaves the same whichever tag carried it.
///
/// # Tags this does not know
///
/// An unknown tag name is consumed and styles nothing, and its closing tag
/// closes the span it opened: `<foo>x</foo>` renders `x` unstyled, not
/// `<foo>x</foo>`. A closing tag ignores the name it gives, so `</b>` closes an
/// open `<i>`. Both follow v1 (`text.canvas.ts:332,365`) -- the switch there
/// has no default arm and the closing branch never reads the name.
///
/// A `<` that does not begin a well-formed tag is ordinary text.
///
/// # Examples
///
/// ```
/// use meo_canvas_core::markup;
///
/// let segments = markup::parse("plain <b>bold</b>");
/// assert_eq!(segments.len(), 2);
/// assert_eq!(segments[0].text, "plain ");
/// assert_eq!(segments[1].text, "bold");
/// assert!(segments[1].style.font_weight.is_some());
/// ```
#[must_use]
pub fn parse(input: &str) -> Vec<TextSegment> {
    parse_reporting(input).0
}

/// [`parse`], and what it could not use.
///
/// **Added beside the total form rather than replacing it**, because
/// `parse` and [`parse_paragraph`] are this crate's public API and a caller
/// who does not want diagnostics should not have to say so at every call. The
/// two share one implementation, so they cannot disagree about what a tag
/// means -- the total one calls this and drops the second half.
#[must_use]
pub fn parse_reporting(input: &str) -> (Vec<TextSegment>, Vec<Diagnostic>) {
    let mut found = Vec::new();
    let segments = walk(input, &mut found);
    (segments, found)
}

/// The parse both entry points run.
fn walk(input: &str, found: &mut Vec<Diagnostic>) -> Vec<TextSegment> {
    let text = unescape(input);
    let mut segments = Vec::new();
    let mut stack: Vec<TextStyle> = Vec::new();
    let mut style = TextStyle::default();
    let mut run_start = 0;
    let mut cursor = 0;

    let bytes = text.as_bytes();
    while cursor < bytes.len() {
        if bytes[cursor] != b'<' {
            cursor += 1;
            continue;
        }
        let Some((tag, end)) = scan_tag(&text, cursor) else {
            cursor += 1;
            continue;
        };

        push_run(&mut segments, &text[run_start..cursor], &style);
        run_start = end;
        cursor = end;

        if tag.closing {
            // The name is not read, exactly as v1 does not read it.
            style = stack.pop().unwrap_or_else(|| {
                // Nothing was open. **Nobody writes this on purpose**, so a
                // diagnostic here cannot become the noise a caller learns to
                // ignore -- which is the argument that keeps the deliberate
                // spellings quiet.
                found.push(Diagnostic::new(
                    format!("</{}>", tag.name),
                    "closes a span that was never opened; it was ignored"
                        .to_owned(),
                ));
                TextStyle::default()
            });
        } else {
            stack.push(style.clone());
            apply(&mut style, &tag, found);
        }
    }
    push_run(&mut segments, &text[run_start..], &style);
    segments
}

/// Parses markup into the segments of a paragraph, which is never empty.
///
/// [`parse`] reports what the markup said, and a string of nothing but tags --
/// or nothing at all -- says nothing. A `Text` node is still a paragraph
/// though, so every surface that turns a string into one needs the same empty
/// run to stand in for it. That is this function rather than each surface's
/// own `if`: the addon's arena decoder and the Rust facade's `Text::new` both
/// call it, and neither can disagree with the other about what `Text("")` is.
///
/// # Examples
///
/// ```
/// use meo_canvas_core::markup;
///
/// assert!(markup::parse("").is_empty());
/// assert_eq!(markup::parse_paragraph("").len(), 1);
/// assert_eq!(markup::parse_paragraph("")[0].text, "");
/// ```
#[must_use]
pub fn parse_paragraph_reporting(
    input: &str,
) -> (Vec<TextSegment>, Vec<Diagnostic>) {
    let (mut segments, found) = parse_reporting(input);
    if segments.is_empty() {
        segments.push(TextSegment {
            text: String::new(),
            style: TextStyle::default(),
        });
    }
    (segments, found)
}

/// [`parse_paragraph_reporting`], with what it could not use discarded.
#[must_use]
pub fn parse_paragraph(input: &str) -> Vec<TextSegment> {
    let mut segments = parse(input);
    if segments.is_empty() {
        segments.push(TextSegment {
            text: String::new(),
            style: TextStyle::default(),
        });
    }
    segments
}

/// Records a run of text under the style in force, unless the run is empty.
fn push_run(segments: &mut Vec<TextSegment>, text: &str, style: &TextStyle) {
    if text.is_empty() {
        return;
    }
    segments.push(TextSegment {
        text: text.to_owned(),
        style: style.clone(),
    });
}

/// One tag, as scanned.
struct Tag<'a> {
    /// Whether it opened with `</`.
    closing: bool,
    /// The name, lowercased.
    name: String,
    /// The value, absent when the tag carried none or carried an empty one.
    ///
    /// Empty and absent are the same thing because v1 selects among its three
    /// capture groups with `||` (`text.canvas.ts:323`), and an empty string is
    /// falsy there -- so `<color="">` reaches the switch with no value at all.
    value: Option<&'a str>,
}

/// Applies an opening tag to the style in force.
fn apply(style: &mut TextStyle, tag: &Tag<'_>, found: &mut Vec<Diagnostic>) {
    /// What a value tag carried, once read.
    ///
    /// Three outcomes and not two, because **an absent value and an unusable
    /// one are different in v1** and the difference is observable. A tag with
    /// no value assigns `undefined` there and clears; a tag whose value will
    /// not parse assigns the raw string, which the canvas then ignores.
    enum Read<T> {
        /// The tag carried no value. Clears, as `<color>` does in v1.
        Absent,
        /// The value parsed.
        Good(T),
        /// The value did not parse. A diagnostic was raised for it.
        Unusable,
    }

    /// One value tag: parse it, and say so when it will not parse.
    ///
    /// **The `None` a bad value produces and the `None` an absent property
    /// produces are the same value one layer down**, so this is the last place
    /// the two can be told apart. A diagnostic raised later would be guessing.
    fn read<T>(
        tag: &Tag<'_>,
        parse: impl Fn(&str) -> Option<T>,
        takes: &str,
        found: &mut Vec<Diagnostic>,
    ) -> Read<T> {
        let Some(written) = tag.value else {
            // **Reported although the clearing is deliberate and v1's.** The
            // grammar this surface documents says a tag's value "may be
            // double-quoted, single-quoted or bare" and never describes a tag
            // without one, so a caller who reaches this is more likely to have
            // lost a value than to be using an idiom nothing offers them. One
            // who meant it loses nothing by being told.
            found.push(Diagnostic::new(
                format!("<{}>", tag.name),
                "carries no value, so the property was cleared; write \
                 `</...>` to close a span"
                    .to_owned(),
            ));
            return Read::Absent;
        };
        if let Some(parsed) = parse(written) {
            return Read::Good(parsed);
        }
        found.push(Diagnostic::new(
            format!("<{}={written}>", tag.name),
            format!("{takes}; the tag was ignored"),
        ));
        Read::Unusable
    }

    match tag.name.as_str() {
        // Colour and weight leave the enclosing value standing when the value
        // is unusable, because that is what v1 does -- measured by running it,
        // not inferred: `<color=red>a<color=notacolour>b` draws `b` red there.
        "color" => match read(
            tag,
            parse_color,
            "not a colour any CSS syntax spells",
            found,
        ) {
            Read::Absent => style.color = None,
            Read::Good(color) => style.color = Some(color),
            Read::Unusable => {}
        },
        "weight" => match read(
            tag,
            parse_weight,
            "not a weight; it takes 1 to 1000, or normal or bold",
            found,
        ) {
            Read::Absent => style.font_weight = None,
            Read::Good(weight) => style.font_weight = Some(weight),
            Read::Unusable => {}
        },
        // Size clears, and **that is the one arm v1 validates**: it runs
        // `Number(value)` and assigns `undefined` on `NaN`. Measured the same
        // way -- `<size=30><size=wide>MMM` renders at the node's own size.
        "size" => match read(
            tag,
            parse_size,
            "not a size; it takes a positive number of pixels",
            found,
        ) {
            Read::Absent | Read::Unusable => style.font_size = None,
            Read::Good(size) => style.font_size = Some(size),
        },
        "b" => style.font_weight = Some(FontWeight::BOLD),
        "i" => style.font_style = Some(FontStyle::Italic),
        // No default arm in v1's switch either: the tag is consumed, the stack
        // still carries it, and its closing tag still pops. What is new is that
        // the caller is told -- the text renders identically to writing no tag
        // at all, so nothing in the output could have told them.
        other => found.push(Diagnostic::new(
            format!("<{other}>"),
            "not a tag this parser knows; its text is kept and the tag ignored"
                .to_owned(),
        )),
    }
}

/// Parses a weight keyword or number.
///
/// `normal` and `bold` are the two keywords `canvas.type.ts:884` names beside
/// the numbers. CSS's relative `lighter` and `bolder` are absent there and
/// absent here: both are defined against the parent's computed weight, which is
/// a resolution step this parser does not have and v1 never had either.
fn parse_weight(value: &str) -> Option<FontWeight> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("normal") {
        return Some(FontWeight::NORMAL);
    }
    if trimmed.eq_ignore_ascii_case("bold") {
        return Some(FontWeight::BOLD);
    }
    trimmed.parse::<u16>().ok().map(FontWeight::new)
}

/// Parses a size in pixels.
///
/// A negative or non-finite size is refused rather than clamped, and so is a
/// zero: all three name no text, and clearing the property leaves the span the
/// size it inherits, which is the reading that puts something on the page.
fn parse_size(value: &str) -> Option<f32> {
    let size = value.trim().parse::<f32>().ok()?;
    (size.is_finite() && size > 0.0).then_some(size)
}

/// Scans a tag beginning at `at`, which must index a `<`.
///
/// Returns the tag and the byte index just past its `>`, or `None` when what
/// follows is not a well-formed tag -- in which case the `<` is ordinary text.
///
/// The grammar is v1's regular expression (`text.canvas.ts:301`) read left to
/// right: `<`, an optional `/`, one or more word characters, optionally `=`
/// and a value, then `>`.
fn scan_tag(text: &str, at: usize) -> Option<(Tag<'_>, usize)> {
    let rest = text.get(at + 1..)?;
    let mut offset = 0;

    let closing = rest.starts_with('/');
    if closing {
        offset += 1;
    }

    let name_start = offset;
    // `\w` is ASCII alphanumerics and the underscore, and nothing else: a tag
    // named in another script does not match in v1 and does not match here.
    offset += rest[name_start..]
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        .count();
    if offset == name_start {
        return None;
    }
    let name = rest[name_start..offset].to_ascii_lowercase();

    let value = if let Some(after) = rest[offset..].strip_prefix('=') {
        let (raw, consumed) = scan_value(after)?;
        offset += 1 + consumed;
        (!raw.is_empty()).then_some(raw)
    } else {
        None
    };

    if !rest[offset..].starts_with('>') {
        return None;
    }
    Some((
        Tag {
            closing,
            name,
            value,
        },
        at + 1 + offset + 1,
    ))
}

/// Scans a tag's value, returning it and how many bytes it occupied.
///
/// Three forms, in v1's order: double-quoted, single-quoted, or bare. A quoted
/// form runs to its closing quote and may be empty; a bare one runs to the
/// first whitespace or `>` and may not.
fn scan_value(rest: &str) -> Option<(&str, usize)> {
    for quote in ['"', '\''] {
        if let Some(body) = rest.strip_prefix(quote) {
            let end = body.find(quote)?;
            return Some((&body[..end], end + 2));
        }
    }
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '>' || c == '\u{feff}')
        .unwrap_or(rest.len());
    (end > 0).then(|| (&rest[..end], end))
}

/// Resolves backslash escapes, over the whole string, before any tag is read.
///
/// v1's table at `text.canvas.ts:250-277`. The three that surprise: `\t` is
/// four spaces rather than a tab, `\r`, `\f` and `\v` are all newlines, and
/// `\0` and `\b` delete themselves.
///
/// An escape v1 does not know keeps both characters, and so does a backslash
/// with nothing after it. A backslash before a line terminator keeps both too,
/// because v1's `\\(.)` cannot match one: JavaScript's `.` excludes `\n`, `\r`,
/// `\u{2028}` and `\u{2029}`, so `\` followed by a real newline is left alone
/// rather than swallowing it.
fn unescape(input: &str) -> String {
    if !input.contains('\\') {
        // The overwhelmingly common case, and the only reason this is worth
        // checking: it turns a string with no escapes into one allocation
        // instead of a character-by-character rebuild.
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        // A backslash with nothing after it is the end of the string, and v1's
        // `\\(.)` needs a character to match, so it stands for itself.
        let Some(escaped) = chars.next() else {
            out.push('\\');
            break;
        };
        match escaped {
            'n' | 'r' | 'f' | 'v' => out.push('\n'),
            't' => out.push_str(TAB),
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            '0' | 'b' => {}
            // Everything else keeps both characters, the line terminators
            // included -- JavaScript's `.` matches none of `\n`, `\r`,
            // `\u{2028}` or `\u{2029}`, so v1's regular expression never
            // reaches them and the pair survives for that reason rather than
            // for the reason an unknown letter does.
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

#[cfg(test)]
mod diagnostic_tests {
    use super::{parse_paragraph, parse_paragraph_reporting};

    /// An unknown tag renders as if it were not written, and now says so.
    ///
    /// The ink is the point: `<nope>abc</nope> def` and `abc def` produce the
    /// same segments, so **nothing in the output could tell a caller their tag
    /// did nothing**. The diagnostic is the only thing that can.
    #[test]
    fn an_unknown_tag_is_reported_though_the_text_is_unchanged() {
        let (with_tag, found) = parse_paragraph_reporting("<nope>abc</nope> d");
        let plain = parse_paragraph("abc d");

        let texts: Vec<&str> =
            with_tag.iter().map(|run| run.text.as_str()).collect();
        let same: Vec<&str> =
            plain.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(texts.concat(), same.concat());

        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].path, "<nope>");
        assert!(found[0].detail.contains("not a tag this parser knows"));
    }

    /// A value that will not parse is reported, and a good one is not.
    ///
    /// The pair is the check. Asserting only the bad case would pass on a
    /// parser that reported every tag, which would be noise a caller learns
    /// to ignore -- and an ignored channel is the silence this exists to end.
    #[test]
    fn a_bad_value_is_reported_and_a_good_one_is_not() {
        for (markup, path) in [
            ("<color=zzz>a</color>", "<color=zzz>"),
            ("<weight=heavy>a</weight>", "<weight=heavy>"),
            ("<size=-4>a</size>", "<size=-4>"),
        ] {
            let (_, found) = parse_paragraph_reporting(markup);
            assert_eq!(found.len(), 1, "{markup}: {found:?}");
            assert_eq!(found[0].path, path, "{markup}");
        }

        for good in [
            "<color=#ff0000>a</color>",
            "<weight=700>a</weight>",
            "<size=12>a</size>",
            "<b>a</b>",
            "plain text",
        ] {
            let (_, found) = parse_paragraph_reporting(good);
            assert!(found.is_empty(), "{good} reported {found:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::style::{
        paint::Color,
        text::{FontStyle, FontWeight},
    };

    use super::{TAB, parse, parse_paragraph, unescape};

    #[test]
    fn text_with_no_markup_is_one_unstyled_segment() {
        let segments = parse("hello");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "hello");
        assert_eq!(segments[0].style, super::TextStyle::default());
    }

    #[test]
    fn a_paragraph_is_never_empty_even_when_the_markup_says_nothing() {
        for input in ["", "<b></b>"] {
            let segments = parse_paragraph(input);
            assert_eq!(segments.len(), 1, "{input}");
            assert!(segments[0].text.is_empty(), "{input}");
            assert_eq!(segments[0].style, super::TextStyle::default());
        }
        // And it does not add a run where there already is one.
        assert_eq!(parse_paragraph("hello").len(), 1);
        assert_eq!(parse_paragraph("a<b>b</b>").len(), 2);
    }

    #[test]
    fn an_empty_input_produces_no_segments() {
        // And so does a string that is nothing but tags. A paragraph that must
        // exist is the caller's to supply; inventing an empty segment here
        // would put a run in the scene that nothing asked for.
        assert!(parse("").is_empty());
        assert!(parse("<b></b>").is_empty());
    }

    #[test]
    fn a_tag_splits_the_text_and_styles_only_its_span() {
        let segments = parse("plain <b>bold</b> plain");
        let texts: Vec<&str> =
            segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["plain ", "bold", " plain"]);
        assert_eq!(segments[0].style.font_weight, None);
        assert_eq!(segments[1].style.font_weight, Some(FontWeight::BOLD));
        assert_eq!(segments[2].style.font_weight, None);
    }

    #[test]
    fn tags_nest_and_the_inner_one_restores_the_outer() {
        let segments = parse("<color=red>a<i>b</i>c</color>d");
        let red = Some(Color::rgba(255, 0, 0, 255));
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0].style.color, red);
        assert_eq!(segments[0].style.font_style, None);
        assert_eq!(segments[1].style.color, red);
        assert_eq!(segments[1].style.font_style, Some(FontStyle::Italic));
        assert_eq!(segments[2].style.color, red);
        assert_eq!(segments[2].style.font_style, None);
        assert_eq!(segments[3].style.color, None);
    }

    #[test]
    fn a_value_may_be_double_quoted_single_quoted_or_bare() {
        let red = Some(Color::rgba(255, 0, 0, 255));
        for input in ["<color=\"red\">x", "<color='red'>x", "<color=red>x"] {
            let segments = parse(input);
            assert_eq!(segments[0].style.color, red, "{input}");
        }
    }

    #[test]
    fn an_empty_quoted_value_is_the_same_as_no_value() {
        // v1 picks among its capture groups with `||`, and "" is falsy there.
        for input in ["<color=\"\">x", "<color>x"] {
            assert_eq!(parse(input)[0].style.color, None, "{input}");
        }
    }

    #[test]
    fn a_tag_name_is_matched_without_regard_to_case() {
        assert_eq!(parse("<B>x")[0].style.font_weight, Some(FontWeight::BOLD));
        assert_eq!(
            parse("<SIZE=20>x")[0].style.font_size,
            Some(20.0),
            "an upper-case tag name is the same tag"
        );
    }

    #[test]
    fn a_weight_is_a_number_or_one_of_two_keywords() {
        assert_eq!(
            parse("<weight=250>x")[0].style.font_weight,
            Some(FontWeight::new(250))
        );
        assert_eq!(
            parse("<weight=bold>x")[0].style.font_weight,
            Some(FontWeight::BOLD)
        );
        assert_eq!(
            parse("<weight=normal>x")[0].style.font_weight,
            Some(FontWeight::NORMAL)
        );
    }

    /// Only `size` clears on a value it cannot read. The other two do not.
    ///
    /// **v1 validates one of the three.** `text.canvas.ts:351-357` runs
    /// `Number(value)` for `size` and assigns `undefined` when it is `NaN`;
    /// `color` and `weight` at `:341-349` assign the raw string with no
    /// validation at all. So the enclosing value surviving those two is **a
    /// fact about the Canvas API rather than about v1's intent** -- an invalid
    /// assignment to `fillStyle` or `font` is ignored and the previous value
    /// stands, which is why reading v1's source suggests the opposite of what
    /// running it shows.
    ///
    /// Measured by running v1, each row against a control that binds:
    /// `<color=red>a<color=notacolour>b` draws `b` red where a valid inner
    /// green draws it green; a bad weight leaves ink at the enclosing 900's
    /// 410 where an explicit 400 gives 166; a bad size renders at the node's
    /// own 7px where `<size=30>` gives 22px.
    #[test]
    fn only_a_size_clears_when_its_value_is_not_understood() {
        assert_eq!(
            parse("<size=20>a<size=wide>b</size>")[1].style.font_size,
            None
        );
        assert_eq!(
            parse("<weight=bold>a<weight=heavy>b")[1].style.font_weight,
            Some(FontWeight::BOLD)
        );
        assert_eq!(
            parse("<color=red>a<color=notacolour>b")[1].style.color,
            parse("<color=red>a")[0].style.color
        );
    }

    /// Every site that discards input reports, and the deliberate ones stay
    /// quiet.
    ///
    /// **Enumerated from the walk rather than from a list of inputs**, because
    /// a list can miss a site. The four value cases were found by a survey
    /// organised around *what a bad value does*; these last two are about the
    /// **absence** of a value or an opener, which is a different axis and is
    /// why that frame could not see them.
    ///
    /// The quiet rows are the ones a caller writes on purpose: a literal `<`,
    /// an unclosed span, and a tag name nothing matches are all visible in the
    /// output, so the render itself is the report.
    #[test]
    fn every_discard_reports_and_nothing_else_does() {
        let count =
            |markup: &str| super::parse_paragraph_reporting(markup).1.len();

        // Reported: something the caller wrote could not be used.
        assert_eq!(count("<color=zzz>a</color>"), 1, "bad value");
        assert_eq!(count("<nope>a</nope>"), 1, "unknown name");
        assert_eq!(count("<color=red>a<color>b</color>"), 1, "no value");
        assert_eq!(count("a</color>b"), 1, "close with no opener");

        // Quiet: deliberate, and visible in the output either way.
        assert_eq!(count("a < b"), 0, "a bare less-than renders literally");
        assert_eq!(count("<>a"), 0, "an empty tag renders literally");
        assert_eq!(count("<color=red>a"), 0, "an unclosed span runs on");
        assert_eq!(count("<color=red>a</color>"), 0, "valid");
        assert_eq!(count("plain"), 0, "no markup");
    }

    /// A tag carrying no value clears, which is not the same as one carrying
    /// an unusable value.
    ///
    /// v1 assigns `undefined` for the first and the raw string for the second,
    /// so the two part company on `color` and `weight`. Keeping them apart is
    /// the whole reason `apply` reads three outcomes rather than two.
    #[test]
    fn a_tag_with_no_value_clears_where_an_unusable_one_does_not() {
        assert_eq!(parse("<color=red>a<color>b")[1].style.color, None);
        assert_eq!(parse("<weight=bold>a<weight>b")[1].style.font_weight, None);
    }

    #[test]
    fn a_size_that_names_no_text_is_refused() {
        for input in ["<size=0>x", "<size=-4>x", "<size=inf>x"] {
            assert_eq!(parse(input)[0].style.font_size, None, "{input}");
        }
    }

    #[test]
    fn an_unknown_tag_is_consumed_and_styles_nothing() {
        let segments = parse("<foo>x</foo>y");
        let texts: Vec<&str> =
            segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["x", "y"]);
        assert_eq!(segments[0].style, super::TextStyle::default());
    }

    #[test]
    fn a_closing_tag_ignores_the_name_it_gives() {
        // v1's closing branch never reads the name, so this closes the italic.
        let segments = parse("<i>a</b>b");
        assert_eq!(segments[0].style.font_style, Some(FontStyle::Italic));
        assert_eq!(segments[1].style.font_style, None);
    }

    #[test]
    fn a_closing_tag_with_nothing_open_resets_to_the_base() {
        let segments = parse("</b>x");
        assert_eq!(segments[0].style, super::TextStyle::default());
        assert_eq!(segments[0].text, "x");
    }

    #[test]
    fn a_span_left_open_runs_to_the_end() {
        let segments = parse("a<b>b");
        assert_eq!(segments[1].style.font_weight, Some(FontWeight::BOLD));
    }

    #[test]
    fn text_that_is_not_a_well_formed_tag_is_ordinary_text() {
        for input in ["a < b", "<b x>y", "<=v>y", "</>y", "<b"] {
            let joined: String =
                parse(input).iter().map(|s| s.text.as_str()).collect();
            assert_eq!(joined, input, "{input} is not a tag");
        }
    }

    #[test]
    fn an_equals_with_no_value_after_it_leaves_the_tag_unformed() {
        // A bare value needs at least one character, so `<color=>` never
        // reaches the `>` the pattern wants and the whole thing is text. The
        // no-value form is `<color>`, which is a different string.
        let input = "<color=>x";
        let joined: String =
            parse(input).iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, input);
    }

    #[test]
    fn whitespace_ends_a_bare_value_and_so_ends_the_tag() {
        // The bare form stops at the first space, and the pattern then wants a
        // `>` where the space is. So a bare value cannot contain one, and a
        // tag written as though it could is text. `<color="a b">` is the way
        // to say it.
        // U+FEFF counts as whitespace to JavaScript's `\s` and to nothing
        // else, so it ends a bare value here for v1's sake and for no other
        // reason.
        let bom = "<color=red\u{feff}>x";
        let joined: String =
            parse(bom).iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, bom);

        let input = "<color=red blue>x";
        let joined: String =
            parse(input).iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, input);
        assert_eq!(
            parse("<color=\"a red\">x")[0].style.color,
            None,
            "a quoted value may contain a space, and is still refused when it \
             names no colour"
        );
    }

    #[test]
    fn an_unterminated_quote_leaves_the_tag_unformed() {
        // The quote has to close before the `>` can be found, so the whole
        // thing is text rather than a tag with a truncated value.
        let input = "<color=\"red>x";
        let joined: String =
            parse(input).iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, input);
    }

    #[test]
    fn escapes_resolve_to_v1_s_table() {
        assert_eq!(unescape(r"a\nb"), "a\nb");
        assert_eq!(unescape(r"a\rb"), "a\nb");
        assert_eq!(unescape(r"a\fb"), "a\nb");
        assert_eq!(unescape(r"a\vb"), "a\nb");
        assert_eq!(unescape(r"a\tb"), format!("a{TAB}b"));
        assert_eq!(unescape(r"a\\b"), r"a\b");
        assert_eq!(unescape("a\\'b"), "a'b");
        assert_eq!(unescape(r#"a\"b"#), "a\"b");
        assert_eq!(unescape(r"a\0b"), "ab");
        assert_eq!(unescape(r"a\bb"), "ab");
    }

    #[test]
    fn an_escape_v1_does_not_know_keeps_both_characters() {
        assert_eq!(unescape(r"a\qb"), r"a\qb");
        assert_eq!(unescape(r"a\"), r"a\");
        // JavaScript's `.` matches no line terminator, so the pair survives.
        assert_eq!(unescape("a\\\nb"), "a\\\nb");
        assert_eq!(unescape("a\\\u{2028}b"), "a\\\u{2028}b");
    }

    #[test]
    fn a_string_with_no_backslash_is_returned_as_it_stands() {
        assert_eq!(unescape("nothing to do"), "nothing to do");
    }

    #[test]
    fn a_digit_is_a_word_character_so_it_can_name_a_tag() {
        // `\w` is alphanumeric, so `<2>` matches v1's tag pattern, is an
        // unknown name, and is consumed. Surprising, and v1's.
        let joined: String =
            parse("1<2>3").iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "13");
    }

    #[test]
    fn a_backslash_suppresses_a_tag_and_prints_itself_doing_it() {
        // `\<` is not an escape v1 knows, so the pair survives; the surviving
        // backslash is then what stops `<b\>` matching the tag pattern. There
        // is no way to write a literal `<b>` and no way to hide the backslash.
        let input = r"a\<b\>c";
        let joined: String =
            parse(input).iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, input);
    }

    #[test]
    fn a_multi_byte_character_does_not_split_a_run() {
        let segments = parse("héllo <b>wörld</b> 😀");
        let texts: Vec<&str> =
            segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["héllo ", "wörld", " 😀"]);
    }

    #[test]
    fn a_colour_may_be_written_any_way_css_names_one() {
        let opaque_red = Some(Color::rgba(255, 0, 0, 255));
        for input in ["<color=#f00>x", "<color=\"rgb(255 0 0)\">x"] {
            assert_eq!(parse(input)[0].style.color, opaque_red, "{input}");
        }
        assert_eq!(
            parse("<color=#ff000080>x")[0].style.color,
            Some(Color::rgba(255, 0, 0, 128))
        );
    }
}
