//! Colour as an animatable value.
//!
//! `f64` channels, unclamped, so an interpolation can sit between two colours
//! and an overshoot outside both; narrowing to bytes happens once, where the
//! value becomes a scene colour. `r`, `g` and `b` run 0 to 255 and `a` 0 to 1,
//! the scale both surfaces use and the one `in_gamut` needs to send an
//! out-of-range colour to `color(srgb ...)`. [`mix`] does not clamp `t`: CSS
//! interpolates through an overshooting timing function and clamps at paint,
//! which `outBack` and `outElastic` rely on.

#![expect(
    clippy::suboptimal_flops,
    reason = "compared bit-for-bit against v9's own numbers; see \
              `animate::easing` for the rule and where it does not apply."
)]

use meo_canvas_scene::style::paint::Color;

/// The top of a colour channel: 255, the scale both surfaces use.
pub const CHANNEL_MAX: f64 = 255.0;

/// A colour with room to be between two others, and outside both.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rgba {
    /// Red, 0 to 255, and not clamped to it.
    pub r: f64,
    /// Green, likewise.
    pub g: f64,
    /// Blue, likewise.
    pub b: f64,
    /// Alpha, 0 to 1, and not clamped to it either.
    pub a: f64,
}

impl Rgba {
    /// Whether every channel sits inside what sRGB can express.
    ///
    /// Alpha is not part of it: alpha outside 0..1 is meaningless rather than
    /// out of gamut, and is clamped wherever it is used.
    #[must_use]
    pub fn in_gamut(self) -> bool {
        [self.r, self.g, self.b]
            .iter()
            .all(|channel| (0.0..=CHANNEL_MAX).contains(channel))
    }

    /// The scene colour this becomes when drawn: the only clamp, since a
    /// channel outside the gamut has nowhere to go in a byte.
    #[must_use]
    pub fn to_color(self) -> Color {
        let byte =
            |channel: f64| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color::rgba(byte(self.r), byte(self.g), byte(self.b), byte(self.a))
    }
}

/// Blends two colours, straight alpha, in sRGB. `t` is not clamped, so an
/// overshooting curve carries the colour out of the gamut until paint.
#[must_use]
pub fn mix(from: Rgba, to: Rgba, t: f64) -> Rgba {
    let between = |a: f64, b: f64| a + (b - a) * t;
    Rgba {
        r: between(from.r, to.r),
        g: between(from.g, to.g),
        b: between(from.b, to.b),
        a: between(from.a, to.a),
    }
}

#[cfg(test)]
mod tests {
    use super::{Rgba, mix};

    /// Mid grey, opaque, as channels rather than bytes.
    const GREY: Rgba = Rgba {
        r: 0.5,
        g: 0.5,
        b: 0.5,
        a: 1.0,
    };

    #[test]
    fn a_mix_past_the_end_leaves_the_gamut_rather_than_stopping() {
        // The whole reason this type is not four bytes: an overshooting curve
        // hands `t` past 1, and `mix` carries it out of the gamut.
        let white = Rgba {
            r: 255.0,
            g: 255.0,
            b: 255.0,
            a: 1.0,
        };
        let over = mix(GREY, white, 1.5);
        assert!(over.r > 255.0, "the mix stopped at the endpoint");
        assert!(!over.in_gamut());
        // And it comes back at the byte boundary rather than in between.
        assert_eq!(over.to_color().r, 255);
    }

    #[test]
    fn a_mix_inside_the_range_is_ordinary() {
        let black = Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let white = Rgba {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        };
        let half = mix(black, white, 0.5);
        assert_eq!(half, GREY);
        assert!(half.in_gamut());
        assert_eq!(half.to_color().r, 128);
    }

    #[test]
    fn alpha_is_not_part_of_the_gamut_but_is_part_of_the_clamp() {
        // Alpha outside 0..1 is meaningless rather than out of gamut: the
        // colour is still sRGB, so `in_gamut` ignores it and `to_color`
        // clamps it like any other channel.
        let loud = Rgba { a: 1.4, ..GREY };
        assert!(loud.in_gamut());
        assert_eq!(loud.to_color().a, 255);
        let quiet = Rgba { a: -0.2, ..GREY };
        assert!(quiet.in_gamut());
        assert_eq!(quiet.to_color().a, 0);
    }

    #[test]
    fn a_negative_channel_is_out_of_gamut_at_both_ends() {
        assert!(!Rgba { r: -0.01, ..GREY }.in_gamut());
        assert!(!Rgba { b: 255.01, ..GREY }.in_gamut());
        assert_eq!(Rgba { r: -20.0, ..GREY }.to_color().r, 0);
    }
}
