//! Marked-up strings through the parser, in bulk.
//!
//! **This one takes a `&str` from a caller and indexes it by byte.** `parse`
//! walks `text.as_bytes()` and then slices `&text[run_start..cursor]`, which
//! panics if either end is not a character boundary. It is safe because `<` is
//! `0x3C` and UTF-8 never puts a byte below `0x80` inside a multi-byte
//! sequence, so a match can only land on a boundary -- an argument worth
//! having and worth not relying on alone, which is what this is for.
//!
//! **The alphabet is tokens rather than characters, and that was measured
//! rather than assumed.** A per-character generator over the same symbols
//! produced a styled run in 125 inputs out of 100,000 -- the floor below
//! caught it -- because a random walk almost never spells `<b>`. Whole tags,
//! whole closers and whole words reach the parser's interior; the loose
//! characters stay in the mix so a half-formed tag is still generated.
use meo_canvas_core::markup;
use meo_canvas_scene::style::text::TextStyle;

/// Inputs per run. Two hundred thousand take about a second.
const ITERATIONS: usize = 100_000;

/// Fixed, so a failure is reproducible.
const SEED: u64 = 0xA5A5_1234_5678_9ABC;

/// **The floor that stops this measuring the absence of tags.** A run in which
/// nothing parses to a styled run is a run that never built a tag, and it
/// would be green while testing only the "no `<` here" branch.
const MIN_STYLED: usize = ITERATIONS / 100;

struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        usize::try_from(self.next() % bound as u64).unwrap_or(0)
    }
}

#[test]
fn markup_parses_anything_without_panicking() {
    let tokens = [
        "<b>",
        "</b>",
        "<i>",
        "<u>",
        "<s>",
        "<color=red>",
        "<color=\"#fff\">",
        "<size=12>",
        "<size='9'>",
        "<weight=700>",
        "</>",
        "<",
        ">",
        "/",
        "=",
        "\"",
        "'",
        " ",
        "hello",
        "\u{1F600}",
        "\u{0301}",
        "\u{202E}",
        "&amp;",
        "\\n",
        "<color=",
        "<size",
        "0123",
    ];
    let mut rng = Rng(SEED);
    let mut styled = 0_usize;

    for _ in 0..ITERATIONS {
        let length = rng.below(12);
        let input: String = (0..length)
            .map(|_| tokens[rng.below(tokens.len())])
            .collect();

        // Both entry points: one reports what the markup said and may be
        // empty, the other stands an empty run in for a paragraph that said
        // nothing, and the surfaces call different ones.
        let segments = markup::parse(&input);
        assert!(
            !markup::parse_paragraph(&input).is_empty(),
            "a paragraph came back with no runs for {input:?}"
        );

        if segments
            .iter()
            .any(|segment| segment.style != TextStyle::default())
        {
            styled += 1;
        }
    }

    assert!(
        styled >= MIN_STYLED,
        "only {styled} of {ITERATIONS} inputs produced a styled run, which is \
         below the {MIN_STYLED} that says these strings contain tags at all"
    );
}
