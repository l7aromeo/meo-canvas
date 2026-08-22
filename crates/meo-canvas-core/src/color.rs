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
    let parsed = csscolorparser::parse(css).ok()?;
    let [r, g, b, a] = parsed.to_rgba8();
    Some(Color::rgba(r, g, b, a))
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
