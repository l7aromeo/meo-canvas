//! Pins an inherited meo-skia-canvas 0.16.3 defect: bytes registered again
//! under the same family add another typeface, so the next draw builds glyph
//! strikes of its own. A fix fails here, and `REGISTERED_FACES` in `resolve.rs`
//! can go. `l7aromeo/meo-skia-canvas#612`, tracked as
//! `l7aromeo/meo-canvas#203`.

use std::sync::{Mutex, PoisonError};

use meo_skia_canvas::{
    Canvas, Font, FontLibrary, Pages,
    export::{EncodeOptions, ImageFormat},
};

const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/fonts/Oswald-VariableFont_wght.ttf"
);

/// Held by both tests: Skia's glyph cache is the process's, so a draw in one
/// would move the count the other reads.
static GLYPH_CACHE: Mutex<()> = Mutex::new(());

fn face() -> Vec<u8> {
    std::fs::read(FONT).unwrap_or_else(|error| unreachable!("{FONT}: {error}"))
}

/// Draws one line in `family` on the raster backend and returns its pixels.
fn draw(family: &str) -> Vec<u8> {
    let mut canvas = Canvas::new(200.0, 60.0);
    canvas.set_gpu(false);
    let context = canvas.context();
    context.set_font(&Font::new(family, 24.0));
    context.fill_text("Hamburgefonstiv", 4.0, 40.0, None);
    canvas
        .prepare_export(ImageFormat::Raw, &EncodeOptions::default())
        .and_then(Pages::encode)
        .unwrap_or_else(|error| unreachable!("{error}"))
}

#[test]
fn registering_the_same_bytes_again_adds_a_typeface() {
    let _held = GLYPH_CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    let family = "MeoReregisterStrikes";
    let library = FontLibrary::new();
    let bytes = face();
    let register = || {
        library
            .register_font_from_data(family, &bytes)
            .unwrap_or_else(|error| unreachable!("{error}"));
    };
    let strikes = || {
        let _ = draw(family);
        skia_safe::graphics::font_cache_count_used()
    };

    register();
    let first = strikes();
    // The instrument: a draw with the same typeface reuses its strikes, so a
    // count that moves below says the typeface changed.
    assert_eq!(
        strikes(),
        first,
        "a second draw of one typeface built strikes"
    );

    register();
    let after = strikes();
    assert!(
        after > first,
        "registering the same bytes again built no new strikes ({first} \
         before, {after} after), so meo-skia-canvas reuses the typeface: \
         l7aromeo/meo-skia-canvas#612 is fixed and `REGISTERED_FACES` in \
         resolve.rs can go"
    );
}

// [FOUNDATION] the property `REGISTERED_FACES` rests on: a face registered once
// draws in every later render on the thread, with no registration of its own.
// Were it false, skipping a registration would draw the fallback face.
#[test]
fn a_registered_face_draws_without_registering_again() {
    let _held = GLYPH_CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    let family = "MeoReregisterFoundation";
    {
        let library = FontLibrary::new();
        library
            .register_font_from_data(family, &face())
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    let registered = draw(family);
    // A library that registers nothing, as one whose registration was skipped.
    let _later = FontLibrary::new();
    assert_eq!(draw(family), registered, "the face drew differently later");
    assert_ne!(
        draw("MeoReregisterNoSuchFamily"),
        registered,
        "an unregistered family drew the same pixels, so this proves nothing"
    );
}
