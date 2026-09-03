//! Turning a CSS colour string into a [`Color`].
//!
//! The scene carries colours as four bytes, so every surface that accepts the
//! string a caller wrote has to resolve it somewhere. This is that somewhere,
//! and it is one function rather than one per surface: the addon reads a
//! `matte` from an options object, [`crate::markup`] reads a `<color=…>` tag,
//! and a Rust caller writes a name -- three callers, one answer.
//!
//! # Why `csscolorparser`
//!
//! It is the crate `meo-skia-canvas` parses its own colour strings with
//! (`meo-skia-canvas-0.11.0/Cargo.toml:148`), so it is already in the graph and
//! a colour this workspace resolves agrees with one the backend resolves by
//! construction. The alternative is a second implementation of CSS Color 4
//! tracking the first, which is a class of disagreement nothing would report.
//!
//! Its `named-colors` feature is on by default, which is what makes `"black"`
//! -- v1's own default `borderColor` -- resolve rather than fail.

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

/// The same parse, **unclamped**, in the units the surfaces use.
///
/// `r`, `g` and `b` run 0 to 255 and `a` runs 0 to 1 -- v1's shape, which both
/// the TypeScript surface and [`crate::animate::color::Rgba`] carry.
///
/// # Why unclamped, and why this is not [`parse_color`]
///
/// `color(srgb 1.25 1.25 1.25)` is a real colour outside the gamut, and it is
/// **the only CSS syntax that can express one**. A scene stores four bytes, so
/// [`parse_color`] clamps -- right for a renderer. An animation mixing colours
/// needs somewhere to be outside the gamut between two of them, and clamping
/// at the parse would flatten the overshoot before the mix ever saw it. **The
/// clamp belongs where a colour becomes paint and not before.**
///
/// Both spellings come through here: the `color(srgb ...)` pre-pass and
/// everything `csscolorparser` reads. **One parser, one answer** -- which is
/// why the addon exports this rather than each surface parsing for itself.
///
/// # The number a channel reads back as
///
/// `csscolorparser` holds its channels as `f32`, so `rgba(0, 0, 0, 0.1)`
/// arrives here as `0.10000000149011612` -- the nearest `f32` to what the
/// author wrote, widened. That is an internal float width leaking through a
/// public boundary, and it is what a caller of the addon's `parseColor` saw.
///
/// **Channels are held at `f32` precision and presented as the shortest
/// decimal that identifies that value**, so an alpha written as a decimal or a
/// percentage reads back as written; an alpha written as a hex byte is
/// `byte / 255` exactly, computed where the byte is known rather than
/// recovered from the `f32`. The two rules are different because the authors
/// wrote different things: a decimal is a number, and `7f` is a byte, whose
/// value is a ratio no shortest decimal reaches from an `f32`.
///
/// Neither reference had this right. v1 quantises alpha to eight bits and
/// rounds to three decimals, answering `0.102`; this answered
/// `0.10000000149011612`; the browser answers `0.1`, and the browser is the
/// tiebreak, as it is for the mix clamp in [`crate::animate::color`].
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

/// An `f32` as the shortest decimal that identifies it.
///
/// For anything a person types -- seven significant digits or fewer -- that
/// decimal is what they typed, because the `f32` they got is the nearest one
/// to it and no shorter string names it. `Display` for `f32` is defined to
/// print exactly that string, so this is a widening rather than a rounding: it
/// returns the `f64` nearest the decimal that names the `f32`, and every
/// `f32` still maps to a distinct `f64`.
fn widen(channel: f32) -> f64 {
    channel
        .to_string()
        .parse()
        .unwrap_or_else(|_| f64::from(channel))
}

/// The alpha of a hex colour, as the byte the author wrote over 255.
///
/// `None` for every other spelling, including a hex colour with no alpha --
/// `#808080` is opaque, and `1.0` needs no recovering.
///
/// **Where the byte is known.** `#0000007f` is alpha `127/255`, which is
/// `0.4980392156862745`; the shortest decimal naming the `f32` is
/// `0.49803922`, which is neither the byte nor the ratio. `#000000cc` happens
/// to work either way because `204/255` is exactly `0.8`, which is why it
/// cannot be the only hex row a test carries.
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

/// The channels of a `color(srgb …)` string, unclamped, or `None` for
/// anything else.
///
/// **A pre-pass rather than a second parser.** `csscolorparser` dispatches on
/// the function name and implements `rgb`, `hsl`, `hwb`, `hsv`, `lab`, `lch`,
/// `oklab` and `oklch` (`csscolorparser-0.8.3/src/parser.rs:213`); `color()`
/// is not among them and 0.8.3 is the newest release. So a string **v1
/// accepts and Chrome draws** was refused here, which is a defect rather than
/// a missing luxury -- and `color(srgb …)` is additionally the only CSS
/// syntax that can name a colour outside the gamut, which is what
/// [`crate::animate`] needs to hand back an overshooting mix.
///
/// Everything else goes through unchanged, so this is one form our dependency
/// does not implement, handled in the one place that already owns colour.
///
/// # The other colour spaces are refused rather than guessed
///
/// `display-p3`, `rec2020`, `a98-rgb`, `prophoto-rgb` and `xyz` share this
/// syntax over spaces we have no conversion for. Treating their numbers as
/// sRGB would draw a wrong colour silently, so they return `None` and reach
/// the caller as the same refusal any unparseable string gets. **That is a
/// known gap: what is missing is the conversion, not the syntax.**
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

/// The colour space of a `color()` string we cannot convert, if that is why
/// it was refused.
///
/// **So a caller can say which half is missing.** `parse_color` returns
/// `None` for everything it cannot read, which tells a caller that a string
/// is not a colour -- true of `"bananas"` and misleading for
/// `color(display-p3 1 0 0)`, which is a colour, in a space we have no
/// conversion for. A surface building an error message asks this and says the
/// space is unsupported rather than the string unparseable.
///
/// Returns `None` for `srgb`, which is supported, and for anything that is
/// not a `color()` function at all.
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

/// Extended channels narrowed to the bytes a scene carries.
///
/// **This is where an out-of-gamut colour stops being one.** The scene holds
/// four bytes per colour, so a channel above 1 or below 0 has nowhere to go
/// and is clamped here -- which is the same place a browser clamps, at the
/// point of painting rather than during interpolation.
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
        // The defect this rule replaced: `csscolorparser` holds channels as
        // `f32`, so this answered `0.10000000149011612` -- an internal float
        // width reaching a caller of the addon's `parseColor`. v1 answers
        // `0.102`, quantising to eight bits. The browser answers `0.1`.
        //
        // The values a person writes are the ones that could not be seen: no
        // test anywhere parsed a string carrying an alpha, so the parameter
        // was never varied from its default of opaque.
        for (css, alpha) in [
            ("rgba(0, 0, 0, 0.1)", 0.1_f64),
            ("rgba(0, 0, 0, 0.33)", 0.33),
            ("rgba(0, 0, 0, 0.9)", 0.9),
            ("rgba(0, 0, 0, 0.005)", 0.005),
            // Exactly representable already, so these passed before and are
            // the control: the rule must not move a number that was right.
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
        // The trap in the fix rather than in the defect. Widening each channel
        // *before* scaling would multiply the shortest decimal of `128/255` by
        // 255 in `f64` and land a hair under 128, breaking rows that passed.
        // The scaling stays in `f32`, where it is exact.
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
        // `color(srgb ...)` is parsed here rather than by the dependency, and
        // now at `f64` from the text -- so an overshoot is the author's number
        // rather than the nearest `f32` to it.
        let [r, g, b, a] = parse_channels("color(srgb 1.25 -0.1 0.5 / 0.1)")
            .unwrap_or_else(|| unreachable!("a colour"));
        assert_eq!(r.to_bits(), 318.75_f64.to_bits(), "1.25 * 255");
        assert!(g < 0.0, "below the gamut rather than clamped");
        assert_eq!(b.to_bits(), 127.5_f64.to_bits());
        assert_eq!(a.to_bits(), 0.1_f64.to_bits(), "and its alpha is written");
    }

    #[test]
    fn a_named_colour_resolves() {
        // The case that made this public: v1's default `borderColor` is the
        // string `'black'`, so a surface that took only hex would break ported
        // code on a default nobody wrote.
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
        // Refused before this: `csscolorparser` has no `color()` at all, and
        // both baselines accept this string -- v1 parses it directly and
        // Chrome draws it.
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
        // `r`, `g` and `b` are 0 to 255 and `a` is 0 to 1, which is v1's shape
        // and the surface's. A parse that scaled all four alike would report
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
