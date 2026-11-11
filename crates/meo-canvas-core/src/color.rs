//! Turning a CSS colour string into a [`Color`].
//!
//! The scene carries colours as four bytes, so every surface that accepts a
//! colour string resolves it here: the addon reading a `matte`,
//! [`crate::markup`] reading a `<color=…>` tag, and a Rust caller writing a
//! name. Three callers, one answer.
//!
//! # Why `meo-skia-canvas`'s reader
//!
//! [`Srgba::from_css`] is the parser behind the backend's own fills and the
//! colours inside a `filter` string, which `paint` hands it as CSS. Reading
//! here with it makes a fill and a `drop-shadow()` of one string agree by
//! construction; a second implementation of CSS Color 4 would be a
//! disagreement nothing reports. `color-strings.tsv` holds it to Chrome.

use meo_canvas_scene::style::paint::Color;
use meo_skia_canvas::color::Srgba;

/// Parses a CSS colour string.
///
/// Returns `None` for a string CSS does not name a colour. An unknown name is
/// refused rather than approximated: a name that silently became black is a
/// wrong picture, which is worse than an error saying which string was not
/// understood.
///
/// # Examples
///
/// ```
/// use meo_canvas_core::parse_color;
/// use meo_canvas_scene::style::paint::Color;
///
/// assert_eq!(parse_color("black"), Some(Color::rgba(0, 0, 0, 255)));
/// assert_eq!(parse_color("#f00"), Some(Color::rgba(255, 0, 0, 255)));
/// assert_eq!(parse_color("not a colour"), None);
/// ```
#[must_use]
pub fn parse_color(css: &str) -> Option<Color> {
    let [r, g, b, a] = to_rgba8(parse_channels(css)?);
    Some(Color::rgba(r, g, b, a))
}

/// The same parse, unclamped, in the units the surfaces use.
///
/// `r`, `g` and `b` run 0 to 255 and `a` runs 0 to 1, the shape the TypeScript
/// surface and [`crate::animate::color::Rgba`] carry.
///
/// # Why unclamped, and why this is not [`parse_color`]
///
/// `color(srgb 1.25 1.25 1.25)` and `color(display-p3 1 0 0)` are real
/// colours outside the sRGB gamut. A scene stores four bytes, so
/// [`parse_color`] clamps. An animation mixing colours needs room outside the
/// gamut between two of them, so the clamp belongs where a colour becomes paint
/// and not at the parse. The addon exports this, so both surfaces read a
/// string the same way.
///
/// # The number a channel reads back as
///
/// The one the string names, in `f64`: `rgba(0, 0, 0, 0.1)` has an alpha of
/// `0.1`, `#0000007f` one of `127/255`, and `#808080` channels of exactly 128.
/// An alpha outside 0 to 1 is clamped into it, as CSS clamps it. A colour at
/// alpha 0 keeps its hue, which a mix toward it needs. A colour converted from
/// another space can sit a few units in the last place outside the gamut:
/// `oklab(1 0 0)` has a red of 255.0000000000001.
///
/// # `currentColor`
///
/// Refused, alone or inside `color-mix()` or a relative colour. It names the
/// node's text colour, which a string read here has no node to take from, and
/// the reader answers it with opaque black.
#[must_use]
pub fn parse_channels(css: &str) -> Option<[f64; 4]> {
    if names_current_color(css) {
        return None;
    }
    let Srgba { r, g, b, a } = Srgba::from_css(css).ok()?;
    Some([r * 255.0, g * 255.0, b * 255.0, a])
}

/// The identifier CSS reads as the text colour, compared in lower case.
const CURRENT_COLOR: &str = "currentcolor";

/// Whether `css` holds `currentColor` as a whole identifier, in any case: not
/// as part of a longer name such as `--currentcolor-x`.
fn names_current_color(css: &str) -> bool {
    let lower = css.to_ascii_lowercase();
    let ident =
        |byte: u8| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_';
    lower.match_indices(CURRENT_COLOR).any(|(at, _)| {
        let before = at.checked_sub(1).map(|index| lower.as_bytes()[index]);
        let after = lower.as_bytes().get(at + CURRENT_COLOR.len()).copied();
        !before.is_some_and(ident) && !after.is_some_and(ident)
    })
}

/// The space of a well-formed `color()` string that [`parse_color`] refuses
/// for its space alone, or `None`.
///
/// So a caller can say which half is missing: [`parse_color`] returns `None`
/// for `color(srgb 1 0)` and for `color(display-p3-linear 1 0 0)` alike, and
/// only the second has well-formed channels in a space the reader does not
/// convert. Those spaces are `display-p3-linear`, which Chrome paints and
/// `meo-skia-canvas` 0.17.2 refuses (`l7aromeo/meo-skia-canvas#687`), a custom
/// `--name` profile, and an identifier CSS does not name.
///
/// Well-formed means the same channels in `srgb` parse. Returns `None` for a
/// string that parses, for a malformed `color()`, and for anything else.
#[must_use]
pub fn unsupported_space(css: &str) -> Option<&str> {
    let (space, channels) = color_function(css)?;
    if Srgba::from_css(css).is_ok() {
        return None;
    }
    Srgba::from_css(&format!("color(srgb {channels})"))
        .is_ok()
        .then_some(space)
}

/// The space and the channels of a `color(space channels)` string, its
/// function name in any case, as CSS reads it.
fn color_function(css: &str) -> Option<(&str, &str)> {
    let css = css.trim();
    let name = css.get(..6)?;
    let inner = name
        .eq_ignore_ascii_case("color(")
        .then(|| css.get(6..))??
        .strip_suffix(')')?;
    inner.trim().split_once(char::is_whitespace)
}

/// Extended channels narrowed to the bytes a scene carries. An out-of-gamut
/// colour is clamped here, where a colour becomes paint, as a browser clamps it
/// at painting rather than during interpolation.
fn to_rgba8(channels: [f64; 4]) -> [u8; 4] {
    let [r, g, b, a] = channels;
    [r, g, b, a * 255.0].map(|value| {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to the byte range on the same line as the cast"
        )]
        let byte = value.clamp(0.0, 255.0).round() as u8;
        byte
    })
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::style::paint::Color;

    use super::{parse_channels, parse_color};

    #[test]
    fn an_alpha_a_person_wrote_reads_back_as_they_wrote_it() {
        // A reader holding channels as `f32` answers `0.10000000149011612`
        // for `0.1`. The browser answers `0.1`.
        for (css, alpha) in [
            ("rgba(0, 0, 0, 0.1)", 0.1_f64),
            ("rgba(0, 0, 0, 0.33)", 0.33),
            ("rgba(0, 0, 0, 0.9)", 0.9),
            ("rgba(0, 0, 0, 0.005)", 0.005),
            // Exact in `f32` already: the control, a number the rule must not
            // move.
            ("rgba(0, 0, 0, 0.25)", 0.25),
            ("rgba(0, 0, 0, 0.5)", 0.5),
            ("rgba(0, 0, 0, 0.75)", 0.75),
            ("rgba(0, 0, 0, 0)", 0.0),
            // A percentage is a number the author wrote too, and lands on the
            // same `f32` as the decimal.
            ("rgba(0, 0, 0, 50%)", 0.5),
            ("rgba(0, 0, 0, 33%)", 0.33),
            ("rgba(0, 0, 0, 12.5%)", 0.125),
        ] {
            let [.., parsed] = parse_channels(css)
                .unwrap_or_else(|| unreachable!("{css} is a colour"));
            assert_eq!(
                parsed.to_bits(),
                alpha.to_bits(),
                "{css} read back as {parsed} rather than {alpha}"
            );
        }
    }

    #[test]
    fn an_alpha_written_as_a_byte_is_that_byte_over_255() {
        // A different rule for a different thing written. `7f` is a byte, and
        // its value is `127/255`; the shortest decimal naming the `f32` is
        // `0.49803922`, which is neither the byte nor the ratio -- so the
        // ratio is computed where the byte is known rather than recovered.
        for (css, alpha) in [
            ("#0000007f", 127.0_f64 / 255.0),
            ("#0008", 136.0 / 255.0),
            // **Not the only hex row, deliberately.** `204/255` is exactly
            // `0.8`, so this one is right under either rule and would have
            // reported a passing branch that was never taken.
            ("#000000cc", 0.8),
        ] {
            let [.., parsed] = parse_channels(css)
                .unwrap_or_else(|| unreachable!("{css} is a colour"));
            assert_eq!(
                parsed.to_bits(),
                alpha.to_bits(),
                "{css} read back as {parsed} rather than {alpha}"
            );
        }

        // A hex colour with no alpha is opaque, and takes the other path.
        for css in ["#808080", "#fff", "#f2aa4c"] {
            let [.., parsed] = parse_channels(css)
                .unwrap_or_else(|| unreachable!("{css} is a colour"));
            assert_eq!(parsed.to_bits(), 1.0_f64.to_bits(), "{css} is opaque");
        }
    }

    #[test]
    fn widening_a_channel_does_not_move_a_byte_off_its_own_value() {
        // Widening each channel *before* scaling would multiply the shortest
        // decimal of `128/255` by 255 in `f64` and land a hair under 128. The
        // scaling stays in `f32`, where it is exact.
        for (css, bytes) in [
            ("#808080", [128.0_f64, 128.0, 128.0]),
            ("#f2aa4c", [242.0, 170.0, 76.0]),
            ("rebeccapurple", [102.0, 51.0, 153.0]),
            ("rgb(255, 0, 0)", [255.0, 0.0, 0.0]),
        ] {
            let [r, g, b, _] = parse_channels(css)
                .unwrap_or_else(|| unreachable!("{css} is a colour"));
            assert!(
                [r, g, b]
                    .iter()
                    .zip(&bytes)
                    .all(|(ours, want)| ours.to_bits() == want.to_bits()),
                "{css} gave {:?} rather than {bytes:?}",
                [r, g, b]
            );
        }
    }

    #[test]
    fn an_out_of_gamut_colour_keeps_its_channels_and_its_alpha() {
        // `color(srgb ...)` is parsed here at `f64` from the text, so an
        // overshoot is the author's number rather than the nearest `f32` to it.
        let [r, g, b, a] = parse_channels("color(srgb 1.25 -0.1 0.5 / 0.1)")
            .unwrap_or_else(|| unreachable!("a colour"));
        assert_eq!(r.to_bits(), 318.75_f64.to_bits(), "1.25 * 255");
        assert!(g < 0.0, "below the gamut rather than clamped");
        assert_eq!(b.to_bits(), 127.5_f64.to_bits());
        assert_eq!(a.to_bits(), 0.1_f64.to_bits(), "and its alpha is written");
    }

    #[test]
    fn a_named_colour_resolves() {
        // A name, not only hex: `'black'` is a default a ported scene carries
        // without anyone writing it.
        assert_eq!(parse_color("black"), Some(Color::rgba(0, 0, 0, 255)));
        assert_eq!(
            parse_color("rebeccapurple"),
            Some(Color::rgba(102, 51, 153, 255))
        );
    }

    #[test]
    fn every_hex_length_resolves_and_alpha_survives() {
        let red = Some(Color::rgba(255, 0, 0, 255));
        assert_eq!(parse_color("#f00"), red);
        assert_eq!(parse_color("#ff0000"), red);
        assert_eq!(parse_color("#ff0000ff"), red);
        assert_eq!(parse_color("#ff000080"), Some(Color::rgba(255, 0, 0, 128)));
    }

    #[test]
    fn a_color_function_in_srgb_resolves() {
        // `color()` is the one syntax with no legacy form, and Chrome draws it.
        let red = Some(Color::rgba(255, 0, 0, 255));
        assert_eq!(parse_color("color(srgb 1 0 0)"), red);
        assert_eq!(parse_color("color(srgb 1.0 0.0 0.0 / 1)"), red);
        assert_eq!(
            parse_color("color(srgb 0 0 0 / 0.5)"),
            Some(Color::rgba(0, 0, 0, 128))
        );
    }

    #[test]
    fn an_out_of_gamut_channel_is_clamped_and_not_refused() {
        // The syntax exists to carry values outside the gamut, and a scene
        // holds four bytes. So it parses, and the narrowing happens here --
        // where a browser also clamps, at paint rather than in between.
        assert_eq!(
            parse_color("color(srgb 1.2 -0.1 0.5)"),
            Some(Color::rgba(255, 0, 128, 255))
        );
    }

    #[test]
    fn the_unclamped_parse_keeps_what_the_clamped_one_cannot() {
        // The two agree wherever a colour fits in a byte, and part company
        // exactly where the syntax exists to go outside it. If they ever
        // agreed everywhere, `parse_channels` would be pointless.
        assert_eq!(parse_channels("#ff0000"), Some([255.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_color("#ff0000"), Some(Color::rgba(255, 0, 0, 255)));

        let over = parse_channels("color(srgb 1.2 -0.1 0.5)")
            .unwrap_or_else(|| unreachable!("an srgb colour"));
        assert!(over[0] > 255.0, "the overshoot was flattened at the parse");
        assert!(over[1] < 0.0, "the undershoot was flattened at the parse");
        // And the clamped one puts it back in the byte range, which is what a
        // scene can hold.
        assert_eq!(
            parse_color("color(srgb 1.2 -0.1 0.5)"),
            Some(Color::rgba(255, 0, 128, 255))
        );
    }

    #[test]
    fn alpha_is_the_one_channel_that_does_not_scale() {
        // `r`, `g` and `b` are 0 to 255 and `a` is 0 to 1, the surface's
        // shape. A parse that scaled all four alike would report
        // an opaque colour as `a: 255` and every caller comparing against 1
        // would read it as transparent.
        let half = parse_channels("rgba(0, 0, 0, 0.5)")
            .unwrap_or_else(|| unreachable!("an rgba colour"));
        assert!(
            (half[3] - 0.5).abs() < 0.01,
            "alpha came back as {}",
            half[3]
        );
        assert_eq!(parse_color("rgba(0, 0, 0, 0.5)").map(|c| c.a), Some(128));
    }

    #[test]
    fn an_unsupported_space_is_named_rather_than_merely_refused() {
        use super::unsupported_space;
        // Chrome 158 paints this one; it turns `None` the day the reader
        // converts it (`l7aromeo/meo-skia-canvas#687`).
        assert_eq!(
            unsupported_space("color(display-p3-linear 1 0 0)"),
            Some("display-p3-linear")
        );
        assert_eq!(
            unsupported_space("COLOR(--profile 1 0 0 / 0.5)"),
            Some("--profile")
        );
        assert_eq!(unsupported_space("color(bananas 1 0 0)"), Some("bananas"));
        // Converted spaces, a malformed string in one, the relative form and
        // strings that are not `color()` name nothing.
        for css in [
            "color(display-p3 1 0 0)",
            "color(xyz 1 0 0)",
            "color(srgb 1 0)",
            "color(display-p3 1 0)",
            "color(from red srgb r g b)",
            "bananas",
            "#ff0000",
        ] {
            assert_eq!(unsupported_space(css), None, "{css}");
        }
    }

    #[test]
    fn a_wide_gamut_colour_converts_and_a_malformed_one_is_refused() {
        // Chrome paints both wide-gamut reds as sRGB's red, clamped at paint;
        // `color-strings.tsv` has the rows.
        let red = Some(Color::rgba(255, 0, 0, 255));
        assert_eq!(parse_color("color(display-p3 1 0 0)"), red);
        assert_eq!(parse_color("color(rec2020 1 0 0)"), red);
        let [r, ..] = parse_channels("color(display-p3 1 0 0)")
            .unwrap_or_else(|| unreachable!("a display-p3 colour"));
        assert!(r > 255.0, "display-p3's red is outside sRGB, at {r}");
        // A fourth channel needs a slash, and a space needs a name CSS knows.
        assert_eq!(parse_color("color(srgb 1 0)"), None);
        assert_eq!(parse_color("color(srgb 1 0 0 0)"), None);
        assert_eq!(parse_color("color(bananas 1 0 0)"), None);
    }

    #[test]
    fn current_color_is_refused_rather_than_read_as_black() {
        // The reader answers opaque black for it; a node's text colour is what
        // CSS means, and nothing here knows the node.
        for css in [
            "currentColor",
            " CURRENTCOLOR ",
            "currentcolor",
            "color-mix(in srgb, currentColor 50%, white)",
            "rgb(from currentColor r g b)",
        ] {
            assert_eq!(parse_channels(css), None, "{css}");
        }
        // The control: a colour beside a longer name is still read.
        assert_eq!(parse_color("black"), Some(Color::rgba(0, 0, 0, 255)));
        assert!(!super::names_current_color("--currentcolor-x"));
        assert!(!super::names_current_color("currentcolors"));
    }

    #[test]
    fn an_alpha_past_either_end_is_clamped_and_the_hue_kept() {
        // CSS clamps a declared alpha to 0..1, so an animation from this
        // colour starts opaque rather than at 2.
        assert_eq!(
            parse_channels("color(srgb 1 0 0 / 2)"),
            Some([255.0, 0.0, 0.0, 1.0])
        );
        // At alpha 0 the hue stays, which a mix toward the colour needs.
        assert_eq!(
            parse_channels("color(srgb 1 0 0 / -1)"),
            Some([255.0, 0.0, 0.0, 0.0])
        );
        assert_eq!(
            parse_channels("hsl(120 100% 50% / 0)"),
            Some([0.0, 255.0, 0.0, 0.0])
        );
    }

    #[test]
    fn the_functional_notations_resolve() {
        let red = Some(Color::rgba(255, 0, 0, 255));
        assert_eq!(parse_color("rgb(255 0 0)"), red);
        assert_eq!(parse_color("rgba(255, 0, 0, 1)"), red);
        assert_eq!(parse_color("hsl(0 100% 50%)"), red);
    }

    #[test]
    fn a_string_css_does_not_name_is_refused() {
        assert_eq!(parse_color("not a colour"), None);
        assert_eq!(parse_color(""), None);
        assert_eq!(parse_color("#ff00"), Some(Color::rgba(255, 255, 0, 0)));
    }
}
