//! The values a style is written in: lengths, edges, corners and colours.
//!
//! Free functions rather than methods, because a length is written before there
//! is anything to call a method on: `padding(all(px(24.0)))` reads as the CSS
//! it came from, where `Length::points(24.0).all()` reads as arithmetic.
//!
//! Every function here is `const`, which is what lets a whole style be one:
//!
//! ```
//! use meo_canvas::{Style, all, hex_rgb, px};
//!
//! const CARD: Style = Style::new().padding(all(px(24.0))).gap(px(16.0));
//! ```
//!
//! The exception is [`hex`], which parses a string and so cannot be.
//! [`hex_rgb`] is its `const` form, taking the same digits as a number.

use meo_canvas_scene::{
    Corners, Sides,
    style::{Dimension, Length, layout::TrackSize, paint::Color},
};

/// A length in logical pixels.
///
/// The unit everything else is expressed against, and the one a bare number
/// means everywhere in the scene.
///
/// ```
/// use meo_canvas::{Style, px};
///
/// const GUTTER: Style = Style::new().gap(px(16.0));
/// ```
#[must_use]
pub const fn px(points: f32) -> Length {
    Length::Points(points)
}

/// A percentage of whatever the property resolves against.
///
/// Written as the number a stylesheet writes: `pct(50.0)` is `50%`, not `0.5`.
/// The scene stores the fraction, and converting here rather than at the call
/// site is what keeps `50` from meaning `5000%` on the way in.
///
/// ```
/// use meo_canvas::{Style, pct};
///
/// const HALF: Style = Style::new().width(pct(50.0));
/// ```
#[must_use]
pub const fn pct(percent: f32) -> Length {
    Length::Percent(percent / 100.0)
}

/// A share of a grid's leftover space, as CSS's `fr`.
///
/// Only a grid track takes one, so this returns a [`TrackSize`] rather than a
/// [`Length`] — `fr` is meaningless as a width.
#[must_use]
pub const fn fr(share: f32) -> TrackSize {
    TrackSize::Fraction(share)
}

/// A fixed or proportional grid track.
///
/// A track takes a [`TrackSize`] and a length is not one, so this is the
/// widening. `fr` and `auto` build a `TrackSize` directly, because neither is
/// expressible as a length.
#[must_use]
pub const fn track(length: Length) -> TrackSize {
    match length {
        Length::Points(points) => TrackSize::Points(points),
        Length::Percent(fraction) => TrackSize::Percent(fraction),
    }
}

/// A track sized to its content.
#[must_use]
pub const fn auto() -> TrackSize {
    TrackSize::Auto
}

/// One value on every edge, as CSS's one-value shorthand.
#[must_use]
pub const fn all<T: Copy>(value: T) -> Sides<T> {
    Sides {
        top: value,
        right: value,
        bottom: value,
        left: value,
    }
}

/// Vertical and horizontal, as CSS's two-value shorthand.
///
/// `xy(a, b)` is CSS's `margin: a b` — the first is top and bottom, the second
/// left and right. The argument order is the shorthand's, not `(x, y)`, because
/// that is what a caller copying `padding: 8px 16px` writes.
#[must_use]
pub const fn xy<T: Copy>(vertical: T, horizontal: T) -> Sides<T> {
    Sides {
        top: vertical,
        right: horizontal,
        bottom: vertical,
        left: horizontal,
    }
}

/// Each edge named, in CSS's clockwise order.
#[must_use]
pub const fn sides<T>(top: T, right: T, bottom: T, left: T) -> Sides<T> {
    Sides {
        top,
        right,
        bottom,
        left,
    }
}

/// One edge, with the rest at zero.
///
/// ```
/// use meo_canvas::{Style, px, top};
///
/// const HEADER: Style = Style::new().padding(top(px(12.0)));
/// ```
#[must_use]
pub const fn top<T: DefaultZero>(value: T) -> Sides<T> {
    let zero = T::ZERO;
    sides(value, zero, zero, zero)
}

/// The right edge alone. See [`top`].
#[must_use]
pub const fn right<T: DefaultZero>(value: T) -> Sides<T> {
    let zero = T::ZERO;
    sides(zero, value, zero, zero)
}

/// The bottom edge alone. See [`top`].
#[must_use]
pub const fn bottom<T: DefaultZero>(value: T) -> Sides<T> {
    let zero = T::ZERO;
    sides(zero, zero, value, zero)
}

/// The left edge alone. See [`top`].
#[must_use]
pub const fn left<T: DefaultZero>(value: T) -> Sides<T> {
    let zero = T::ZERO;
    sides(zero, zero, zero, value)
}

/// The value an unnamed edge of a per-edge shorthand takes.
///
/// A trait with an associated constant rather than [`Default`], because
/// `Default::default()` cannot be called from a `const fn` and these shorthands
/// are `const` so that a whole [`crate::Style`] can be.
pub trait DefaultZero: Copy {
    /// The zero of this type.
    const ZERO: Self;
}

impl DefaultZero for f32 {
    const ZERO: Self = 0.0;
}

impl DefaultZero for Length {
    const ZERO: Self = Self::Points(0.0);
}

impl DefaultZero for Dimension {
    const ZERO: Self = Self::Points(0.0);
}

/// The same value on every corner.
#[must_use]
pub const fn corners_all(border_radius: f32) -> Corners<f32> {
    Corners {
        top_left: border_radius,
        top_right: border_radius,
        bottom_right: border_radius,
        bottom_left: border_radius,
    }
}

/// Each corner named, clockwise from the top left.
#[must_use]
pub const fn corners(
    top_left: f32,
    top_right: f32,
    bottom_right: f32,
    bottom_left: f32,
) -> Corners<f32> {
    Corners {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    }
}

/// An opaque colour from packed `0xRRGGBB`.
///
/// The `const` form of [`hex`]. A string cannot be parsed in a `const fn`, so a
/// style that has to be a `const` writes its colours as numbers:
///
/// ```
/// use meo_canvas::{Style, hex_rgb};
///
/// const PANEL: Style = Style::new().background_color(hex_rgb(0x10_10_14));
/// ```
#[must_use]
pub const fn hex_rgb(packed: u32) -> Color {
    Color::rgb(
        ((packed >> 16) & 0xff) as u8,
        ((packed >> 8) & 0xff) as u8,
        (packed & 0xff) as u8,
    )
}

/// A colour from packed `0xRRGGBBAA`.
#[must_use]
pub const fn hex_rgba(packed: u32) -> Color {
    Color::rgba(
        ((packed >> 24) & 0xff) as u8,
        ((packed >> 16) & 0xff) as u8,
        ((packed >> 8) & 0xff) as u8,
        (packed & 0xff) as u8,
    )
}

/// A colour from a CSS hex string.
///
/// Takes `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa`, with or without the hash,
/// which is what a caller copying a value out of a design tool has in hand.
/// Anything else is [`Color::BLACK`] — a style is not a place to return a
/// `Result`, and a colour that silently failed to parse is visible immediately.
///
/// Not `const`: parsing a `&str` needs iteration a `const fn` cannot do. Use
/// [`hex_rgb`] where the style must be a `const`.
///
/// ```
/// use meo_canvas::{hex, scene::Color};
///
/// assert_eq!(hex("#101014"), Color::rgb(0x10, 0x10, 0x14));
/// assert_eq!(hex("f0c"), Color::rgb(0xff, 0x00, 0xcc));
/// assert_eq!(hex("#80808080").a, 0x80);
/// ```
#[must_use]
pub fn hex(value: &str) -> Color {
    parse_hex(value).unwrap_or(Color::BLACK)
}

/// [`hex`]'s parse, separated so `?` has an `Option` to return into.
fn parse_hex(value: &str) -> Option<Color> {
    let digits = value.strip_prefix('#').unwrap_or(value).as_bytes();

    let nibble = |index: usize| -> Option<u8> {
        let digit = char::from(*digits.get(index)?).to_digit(16)?;
        u8::try_from(digit).ok()
    };
    // The short forms repeat each digit, so `f` is `ff` -- CSS's rule, and the
    // reason `#f0c` and `#ff00cc` are the same colour.
    let doubled = |index: usize| nibble(index).map(|digit| digit * 17);
    let pair = |index: usize| Some(nibble(index)? * 16 + nibble(index + 1)?);

    match digits.len() {
        3 => Some(Color::rgb(doubled(0)?, doubled(1)?, doubled(2)?)),
        4 => Some(Color::rgba(
            doubled(0)?,
            doubled(1)?,
            doubled(2)?,
            doubled(3)?,
        )),
        6 => Some(Color::rgb(pair(0)?, pair(2)?, pair(4)?)),
        8 => Some(Color::rgba(pair(0)?, pair(2)?, pair(4)?, pair(6)?)),
        _ => None,
    }
}

/// A colour from channels and an alpha fraction.
///
/// Alpha is `0.0` to `1.0`, as CSS writes it, rather than the byte the scene
/// stores.
#[must_use]
pub fn rgba(red: u8, green: u8, blue: u8, alpha: f32) -> Color {
    // Rounded rather than truncated: `0.5` is 128 as a browser reports it, and
    // truncation would make every fraction one step too transparent.
    let alpha = (alpha.clamp(0.0, 1.0) * 255.0).round();
    Color::rgba(red, green, blue, alpha as u8)
}

/// An opaque colour from channels.
#[must_use]
pub const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::rgb(red, green, blue)
}

/// A width or height that may also be `auto`.
///
/// The scene distinguishes a [`Length`], which is always a measure, from a
/// [`Dimension`], which admits `auto`. A setter taking a width accepts either
/// through this conversion rather than making the caller name which one they
/// meant.
#[must_use]
pub const fn size_auto() -> Dimension {
    Dimension::Auto
}

/// What a per-edge setter accepts: one value, or the four named.
///
/// CSS's shorthand, as a trait. `padding(px(24.0))` is every edge and
/// `padding(sides(...))` is each of them, which is the same choice CSS gives
/// and the same one the JavaScript surface gives through a union type.
///
/// The node setters take this; [`Style`](crate::Style)'s take a
/// [`Sides`] directly, because they are `const fn` and a trait method cannot
/// be. So `const CARD: Style = Style::new().padding(all(px(24.0)))` spells the
/// shorthand out, and a node written in a chain does not have to.
pub trait IntoSides<T> {
    /// The four edges this names.
    fn into_sides(self) -> Sides<T>;
}

impl<T: Copy> IntoSides<T> for Sides<T> {
    fn into_sides(self) -> Self {
        self
    }
}

impl IntoSides<Self> for Length {
    fn into_sides(self) -> Sides<Self> {
        Sides::all(self)
    }
}

impl IntoSides<Self> for Dimension {
    fn into_sides(self) -> Sides<Self> {
        Sides::all(self)
    }
}

impl IntoSides<Self> for f32 {
    fn into_sides(self) -> Sides<Self> {
        Sides::all(self)
    }
}

impl IntoSides<Option<Self>> for Length {
    fn into_sides(self) -> Sides<Option<Self>> {
        Sides::all(Some(self))
    }
}

impl IntoSides<Option<Self>> for Color {
    fn into_sides(self) -> Sides<Option<Self>> {
        Sides::all(Some(self))
    }
}

/// What a per-corner setter accepts: one radius, or the four named.
pub trait IntoCorners<T> {
    /// The four corners this names.
    fn into_corners(self) -> Corners<T>;
}

impl<T: Copy> IntoCorners<T> for Corners<T> {
    fn into_corners(self) -> Self {
        self
    }
}

impl IntoCorners<Self> for f32 {
    fn into_corners(self) -> Corners<Self> {
        Corners::all(self)
    }
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::style::{
        Dimension, Length, layout::TrackSize, paint::Color,
    };

    use super::{
        all, bottom, corners, corners_all, fr, hex, hex_rgb, hex_rgba, left,
        pct, px, rgb, rgba, right, sides, size_auto, top, track, xy,
    };

    #[test]
    fn a_percentage_is_written_as_a_stylesheet_writes_it() {
        // `pct(50.0)` is `50%`. The scene stores the fraction, and converting
        // here is what keeps `50` from meaning 5000%.
        assert_eq!(pct(50.0), Length::Percent(0.5));
        assert_eq!(pct(100.0), Length::Percent(1.0));
        assert_eq!(px(16.0), Length::Points(16.0));
    }

    #[test]
    fn the_edge_shorthands_follow_css() {
        assert_eq!(all(px(4.0)), sides(px(4.0), px(4.0), px(4.0), px(4.0)));
        // CSS's two-value form is vertical then horizontal, not (x, y).
        assert_eq!(
            xy(px(8.0), px(16.0)),
            sides(px(8.0), px(16.0), px(8.0), px(16.0))
        );
    }

    #[test]
    fn a_single_edge_leaves_the_others_at_zero() {
        let zero = Length::Points(0.0);
        assert_eq!(top(px(4.0)), sides(px(4.0), zero, zero, zero));
        assert_eq!(right(px(4.0)), sides(zero, px(4.0), zero, zero));
        assert_eq!(bottom(px(4.0)), sides(zero, zero, px(4.0), zero));
        assert_eq!(left(px(4.0)), sides(zero, zero, zero, px(4.0)));
    }

    #[test]
    fn tracks_carry_the_unit_a_grid_takes() {
        assert_eq!(track(px(240.0)), TrackSize::Points(240.0));
        assert_eq!(track(pct(50.0)), TrackSize::Percent(0.5));
        assert_eq!(fr(1.0), TrackSize::Fraction(1.0));
        assert_eq!(super::auto(), TrackSize::Auto);
        assert_eq!(size_auto(), Dimension::Auto);
    }

    #[test]
    fn corner_shorthands_set_what_they_name() {
        let even = corners_all(8.0);
        assert_eq!(even.top_left.to_bits(), 8.0_f32.to_bits());
        assert_eq!(even.bottom_right.to_bits(), 8.0_f32.to_bits());

        let mixed = corners(1.0, 2.0, 3.0, 4.0);
        assert_eq!(mixed.top_left.to_bits(), 1.0_f32.to_bits());
        assert_eq!(mixed.top_right.to_bits(), 2.0_f32.to_bits());
        assert_eq!(mixed.bottom_right.to_bits(), 3.0_f32.to_bits());
        assert_eq!(mixed.bottom_left.to_bits(), 4.0_f32.to_bits());
    }

    #[test]
    fn every_css_hex_form_parses() {
        // The short forms repeat each digit, which is CSS's rule and the reason
        // `#f0c` and `#ff00cc` are the same colour.
        assert_eq!(hex("#f0c"), Color::rgb(0xff, 0x00, 0xcc));
        assert_eq!(hex("f0c"), Color::rgb(0xff, 0x00, 0xcc));
        assert_eq!(hex("#101014"), Color::rgb(0x10, 0x10, 0x14));
        assert_eq!(hex("#f0c8"), Color::rgba(0xff, 0x00, 0xcc, 0x88));
        assert_eq!(hex("#80808080"), Color::rgba(0x80, 0x80, 0x80, 0x80));
    }

    #[test]
    fn a_hex_that_does_not_parse_is_black_rather_than_a_panic() {
        // A style is not a place to return a `Result`, and a colour that failed
        // to parse is visible the moment the picture is looked at.
        for bad in ["", "#", "#12", "#12345", "zzz", "#gggggg"] {
            assert_eq!(hex(bad), Color::BLACK, "{bad:?} should read as black");
        }
    }

    #[test]
    fn the_packed_forms_match_the_string_ones() {
        assert_eq!(hex_rgb(0x10_10_14), hex("#101014"));
        assert_eq!(hex_rgba(0x80_80_80_80), hex("#80808080"));
        assert_eq!(rgb(1, 2, 3), Color::rgb(1, 2, 3));
    }

    #[test]
    fn an_alpha_fraction_rounds_the_way_a_browser_reports_it() {
        // Rounded, not truncated: `0.5` is 128, and truncation would make every
        // fraction one step too transparent.
        assert_eq!(rgba(0, 0, 0, 0.5).a, 128);
        assert_eq!(rgba(0, 0, 0, 1.0).a, 255);
        assert_eq!(rgba(0, 0, 0, 0.0).a, 0);
        // Out of range is clamped rather than wrapped.
        assert_eq!(rgba(0, 0, 0, 2.0).a, 255);
        assert_eq!(rgba(0, 0, 0, -1.0).a, 0);
    }
}
