//! Turning a CSS colour string into a [`Color`].
//!
//! The scene carries colours as four bytes, so every surface that accepts a
//! colour string resolves it here: the addon reading a `matte`,
//! [`crate::markup`] reading a `<color=…>` tag, and a Rust caller writing a
//! name. Three callers, one answer.
//!
//! # Why `csscolorparser`
//!
//! `meo-skia-canvas` parses its own colour strings with it, so it is already in
//! the graph, and a colour resolved here agrees with one the backend resolves
//! by construction. A second implementation of CSS Color 4 would be a
//! disagreement nothing reports.
//!
//! Its default `named-colors` feature is what makes `"black"` resolve rather
//! than fail.

use meo_canvas_scene::style::paint::Color;

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
/// `color(srgb 1.25 1.25 1.25)` is a real colour outside the gamut, and the
/// only CSS syntax that can express one. A scene stores four bytes, so
/// [`parse_color`] clamps. An animation mixing colours needs room outside the
/// gamut between two of them, so the clamp belongs where a colour becomes paint
/// and not at the parse.
///
/// Both spellings come through here, the `color(srgb ...)` pre-pass and
/// everything `csscolorparser` reads, which is why the addon exports this
/// rather than each surface parsing for itself.
///
/// # The number a channel reads back as
///
/// `csscolorparser` holds channels as `f32`, so `rgba(0, 0, 0, 0.1)` arrives as
/// the nearest `f32` to `0.1`. An alpha written as a decimal or a percentage is
/// presented as the shortest decimal naming that `f32`, which reads back as
/// written. An alpha written as a hex byte is `byte / 255`, computed where the
/// byte is known: `7f` is a byte, and no shortest decimal reaches `127/255`
/// from an `f32`. The browser answers `0.1` here, and is the tiebreak, as it is
/// for the mix clamp in [`crate::animate::color`].
#[must_use]
pub fn parse_channels(css: &str) -> Option<[f64; 4]> {
    if let Some([r, g, b, a]) = extended_srgb(css) {
        // Already parsed at `f64` from the text, so there is nothing to
        // recover: these are the author's numbers.
        return Some([r * 255.0, g * 255.0, b * 255.0, a]);
    }

    let parsed = csscolorparser::parse(css).ok()?;
    // The scaling stays in `f32`, where `#808080` gives exactly `128.0`.
    // Widening first and multiplying in `f64` would land a byte a hair below
    // itself, which is a worse answer than the one being fixed.
    let [r, g, b] =
        [parsed.r, parsed.g, parsed.b].map(|channel| widen(channel * 255.0));
    Some([r, g, b, hex_alpha(css).unwrap_or_else(|| widen(parsed.a))])
}

/// An `f32` as the shortest decimal that identifies it. For anything a person
/// types, seven significant digits or fewer, that is what they typed; `Display`
/// for `f32` prints exactly that string, and distinct `f32`s stay distinct.
fn widen(channel: f32) -> f64 {
    channel
        .to_string()
        .parse()
        .unwrap_or_else(|_| f64::from(channel))
}

/// The alpha of a hex colour, as the author's byte over 255, or `None` for any
/// other spelling, including hex with no alpha. `#0000007f` is `127/255`, where
/// the shortest decimal naming the `f32` is `0.49803922`, neither of the two.
fn hex_alpha(css: &str) -> Option<f64> {
    let digits = css.trim().strip_prefix('#')?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let byte = match digits.len() {
        // `#rgba`, where each digit is a doubled nibble: `8` is `0x88`.
        4 => {
            let nibble = u8::from_str_radix(&digits[3..], 16).ok()?;
            nibble * 17
        }
        8 => u8::from_str_radix(&digits[6..], 16).ok()?,
        _ => return None,
    };
    Some(f64::from(byte) / 255.0)
}

/// The channels of a `color(srgb …)` string, unclamped, or `None`. A pre-pass:
/// `csscolorparser`'s `parse_abs` has no `color()`. Other spaces return `None`,
/// since there is no conversion for them and their numbers read as sRGB would
/// draw a wrong colour silently.
pub(crate) fn extended_srgb(css: &str) -> Option<[f64; 4]> {
    let (space, values) = color_function(css)?;
    if !space.eq_ignore_ascii_case("srgb") {
        return None;
    }
    let (channels, alpha) = values
        .split_once('/')
        .map_or((values, None), |(rgb, a)| (rgb, Some(a)));
    let mut numbers = channels.split_whitespace();
    let mut next = || numbers.next()?.parse::<f64>().ok();
    let (red, green, blue) = (next()?, next()?, next()?);
    if numbers.next().is_some() {
        return None;
    }
    let alpha = match alpha {
        None => 1.0,
        Some(text) => text.trim().parse::<f64>().ok()?,
    };
    Some([red, green, blue, alpha])
}

/// The colour space of a `color()` string we cannot convert, if that is why it
/// was refused.
///
/// So a caller can say which half is missing: [`parse_color`] returns `None`
/// for `"bananas"` and for `color(display-p3 1 0 0)` alike, and only the second
/// is a colour, in a space with no conversion here.
///
/// Returns `None` for `srgb`, which is supported, and for anything that is not
/// a `color()` function.
#[must_use]
pub fn unsupported_space(css: &str) -> Option<&str> {
    let (space, _) = color_function(css)?;
    (!space.eq_ignore_ascii_case("srgb")).then_some(space)
}

/// The space and the values of a `color(space values)` string.
fn color_function(css: &str) -> Option<(&str, &str)> {
    let inner = css.trim().strip_prefix("color(")?.strip_suffix(')')?;
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
        // `csscolorparser` holds channels as `f32`, so a plain widening answers
        // `0.10000000149011612` for `0.1`. The browser answers `0.1`.
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
        // `csscolorparser` has no `color()` at all, and Chrome draws this.
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
        // The difference a caller can act on: `"bananas"` is not a colour and
        // `color(display-p3 1 0 0)` is one we cannot convert. Both are `None`
        // from `parse_color`, and only one has a space to report.
        assert_eq!(
            super::unsupported_space("color(display-p3 1 0 0)"),
            Some("display-p3")
        );
        assert_eq!(super::unsupported_space("color(xyz 1 0 0)"), Some("xyz"));
        assert_eq!(super::unsupported_space("color(srgb 1 0 0)"), None);
        assert_eq!(super::unsupported_space("bananas"), None);
        assert_eq!(super::unsupported_space("#ff0000"), None);
    }

    #[test]
    fn a_colour_space_we_cannot_convert_is_refused() {
        // Not a syntax we fail to read: a space we have no conversion for.
        // Reading its numbers as sRGB would draw a wrong colour in silence.
        assert_eq!(parse_color("color(display-p3 1 0 0)"), None);
        assert_eq!(parse_color("color(rec2020 1 0 0)"), None);
        assert_eq!(parse_color("color(srgb 1 0)"), None);
        assert_eq!(parse_color("color(srgb 1 0 0 0)"), None);
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
