//! The probe for `lines.rs`'s `FREETYPE_SCALE`: on Linux, meo-skia-canvas
//! measures a 16px ideograph of a 1000-unit face as 16.0000305, the 16.16
//! scale of `FreeType` rounded up from 1024 * 65536 / 1000, where macOS and
//! Windows measure 16, as Chrome on macOS does.
//! `l7aromeo/meo-skia-canvas#695`.

use meo_canvas_core::{
    lines::{RunStyle, TextMeasurer},
    resolve::{Fonts, ResolvedText},
};

/// A 1000-unit face holding the ideograph, under the family the line-break
/// table registers it as.
const FACE: (&str, &str) =
    ("Noto Sans SC", "tests/assets/fonts/NotoSansSC-Subset.ttf");

/// The em the probe measures at.
const SIZE: f32 = 16.0;

/// The margin `FREETYPE_SCALE` widens a room by, relative.
const MARGIN: f32 = 1.0 / 65536.0;

/// The width of `text` in the face at 16px.
fn width(text: &str) -> f32 {
    let fonts = Fonts::new();
    fonts
        .register_path(FACE.0, FACE.1)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let base = ResolvedText {
        family: FACE.0.to_owned(),
        size: SIZE,
        ..ResolvedText::initial()
    };
    TextMeasurer::new().run_width(&RunStyle::base(&base), 0.0, text)
}

/// The defect: an ideograph a 16.16 step of `FreeType`'s scale over its em.
/// Fails the day meo-skia-canvas measures 16.0 on Linux, as it does on macOS,
/// and `FREETYPE_SCALE` in `lines.rs` can then be one everywhere and go.
#[cfg(target_os = "linux")]
#[test]
fn linux_still_measures_an_em_of_1000_units_over_16px() {
    let one = width("漢");
    assert!(
        one > SIZE && one - SIZE < 4.0 / 65536.0,
        "a 16px ideograph measures {one}; at 16.0 l7aromeo/meo-skia-canvas#695 \
         is fixed, and FREETYPE_SCALE in lines.rs should be removed"
    );
}

// [FOUNDATION] the property `FREETYPE_SCALE` rests on: the error is a scale,
// so it grows with the width in proportion and stays inside the margin's
// fraction of it. An error that added a fixed amount per glyph or per run would
// outgrow the margin on a long line, and lines would break early again.
#[test]
fn the_error_is_a_scale_within_the_margin() {
    let one = width("漢");
    let twelve = width(&"漢".repeat(12));
    let error = one / SIZE - 1.0;
    assert!(
        (0.0..=MARGIN).contains(&error),
        "a 16px ideograph is {one}, {error} of an em over"
    );
    assert!(
        (twelve / one - 12.0).abs() <= 12.0 * f32::EPSILON,
        "twelve ideographs are {twelve}, not twelve of {one}"
    );
}
