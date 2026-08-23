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
#[must_use]
pub fn parse_channels(css: &str) -> Option<[f32; 4]> {
    let [r, g, b, a] = extended_srgb(css).or_else(|| {
        let parsed = csscolorparser::parse(css).ok()?;
        Some([parsed.r, parsed.g, parsed.b, parsed.a])
    })?;
    Some([r * 255.0, g * 255.0, b * 255.0, a])
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
pub(crate) fn extended_srgb(css: &str) -> Option<[f32; 4]> {
    let (space, values) = color_function(css)?;
    if !space.eq_ignore_ascii_case("srgb") {
        return None;
    }
    let (channels, alpha) = values
        .split_once('/')
        .map_or((values, None), |(rgb, a)| (rgb, Some(a)));
    let mut numbers = channels.split_whitespace();
    let mut next = || numbers.next()?.parse::<f32>().ok();
    let (red, green, blue) = (next()?, next()?, next()?);
    if numbers.next().is_some() {
        return None;
    }
    let alpha = match alpha {
        None => 1.0,
        Some(text) => text.trim().parse::<f32>().ok()?,
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
fn to_rgba8(channels: [f32; 4]) -> [u8; 4] {
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

    use super::parse_color;

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
        assert_eq!(
            super::parse_channels("#ff0000"),
            Some([255.0, 0.0, 0.0, 1.0])
        );
        assert_eq!(parse_color("#ff0000"), Some(Color::rgba(255, 0, 0, 255)));

        let over = super::parse_channels("color(srgb 1.2 -0.1 0.5)")
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
        let half = super::parse_channels("rgba(0, 0, 0, 0.5)")
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
