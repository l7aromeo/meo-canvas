//! One scene, encoded in every format, compared against the JavaScript surface.
//!
//! # What this is for
//!
//! The two surfaces agree about the wire: `arena.test.ts` reads the same
//! `arena-cases.json` these tables generate, and `chart.agreement.test.ts`
//! asserts the same bytes `chart_agreement.rs` does. Nothing asserted anything
//! about what happens **after** the wire. Measured rather than assumed: a
//! JavaScript-side encode default of `quality: 0.5` changed 27 of the 76 files
//! `just example` compares -- every `jpg`, `webp` and `avif` -- and left
//! `typecheck`, `lint-check`, `arena-cases-check`, `test-js` and this whole
//! suite green.
//!
//! # What it measures, and it is not correctness
//!
//! **An encode-agreement arm measures agreement, not correctness.** Both
//! surfaces calling one encoder with one wrong option agree perfectly, and
//! this file passes. What holds correctness is `fixtures.rs`, and it holds it
//! for PNG. **Nothing in this repository establishes that the JPEG this
//! library writes is a correct JPEG**; this establishes that two callers of
//! the same encoder produce the same bytes, which is a smaller claim.
//!
//! # Why a refusal is a failure rather than a skip
//!
//! A format that will not encode a scene is a result. An arm that caught the
//! refusal and moved on would report agreement on a format neither surface
//! wrote.
//!
//! # Why the scene has no curve, gradient, blend or glyph
//!
//! `fixtures.rs` measured the dividing line: on `linux-x86_64`, 15 of 23
//! fixtures are byte-identical to the macOS reference and the 8 that are not
//! are the ones with a curve, a gradient, a blend or a glyph. A committed hash
//! is a claim about every platform that runs this suite, so this scene is flat
//! rectangles -- the half of that measurement that does not move.
//!
//!
//! # Shown to go red from both sides, not from one
//!
//! Construction says the pair is symmetric; construction is not evidence, and
//! this file is an argument against taking it as such. So the same divergence
//! was introduced on each surface in turn -- `quality: Some(0.5)` defaulted in
//! `Root::to_buffer` here, and `quality: 0.5` defaulted in `toBuffer` there --
//! and each turned **three of eight** formats red, `jpg`, `webp` and `avif`,
//! leaving `png`, `bmp`, `tiff`, `svg` and `raw` byte-identical.
//!
//! **Both provocations produced the same `jpg` hash**, `5e0afffe9d6b74ad`.
//! Two surfaces given one option arriving at one byte string is evidence they
//! reach the same encoder -- so what this arm measures is the option and not
//! the language, which is the claim it has to be able to make.
//!
//! **That number was written down before the second measurement**, in the
//! report of the JavaScript provocation, when no Rust provocation existed. It
//! is a prediction that held rather than an agreement noticed once both were
//! in hand, and the two read identically afterwards unless somebody says
//! which happened.
//!
//! # Regenerating
//!
//! `UPDATE_ENCODE_HASHES=1 npx vitest run encode.agreement` writes the asset
//! from the JavaScript side; this side only ever asserts against it, so a
//! regeneration that was not legitimate fails here.
use std::fs::read_to_string;

use meo_canvas::{Box, Format, Renderer, Root, Styled, hex_rgb, px};

/// The formats `just example` writes for every scene.
const FORMATS: &[(&str, Format)] = &[
    ("png", Format::Png),
    ("jpg", Format::Jpeg),
    ("webp", Format::Webp),
    ("avif", Format::Avif),
    ("bmp", Format::Bmp),
    ("tiff", Format::Tiff),
    ("svg", Format::Svg),
    ("raw", Format::Raw),
];

/// FNV-1a, 64-bit.
///
/// Hand-written on both sides rather than taken from a dependency, because
/// this workspace has no hasher and adding one to compare two byte strings is
/// a dependency bought for a test. Both implementations are checked against
/// the published vector in `hashes_the_way_the_other_side_hashes`, so an arm
/// that agreed because both hashers were broken the same way cannot pass.
fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The scene both surfaces build. Flat rectangles, for the reason above.
fn scene() -> Root {
    Root::new(120.0)
        .height(80.0)
        .background_color(hex_rgb(0x10_10_14))
        .children([
            Box::new()
                .size(px(40.0), px(20.0))
                .background_color(hex_rgb(0xff_00_00)),
            Box::new()
                .size(px(30.0), px(30.0))
                .background_color(hex_rgb(0x00_ff_00)),
        ])
}

#[test]
fn hashes_the_way_the_other_side_hashes() {
    // The published FNV-1a 64 vector for "a". A hasher that agrees with the
    // other side because both are wrong cannot survive this.
    assert_eq!(fnv1a(b"a"), "af63dc4c8601ec8c");
    assert_eq!(fnv1a(b""), "cbf29ce484222325");
}

#[test]
fn produces_the_hashes_the_javascript_side_wrote() {
    let renderer = Renderer::new();
    let mut canvas = scene().render(&renderer).unwrap_or_else(|error| {
        unreachable!("the scene did not render: {error}")
    });

    let measured: Vec<String> = FORMATS
        .iter()
        .map(|(name, format)| {
            // Not a skip. See the header.
            let bytes = canvas.to_buffer(*format).unwrap_or_else(|error| {
                unreachable!(
                    "{name}: the surface refused to encode this scene: {error}"
                )
            });
            format!("{name} {}", fnv1a(&bytes))
        })
        .collect();

    let expected = read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/assets/encode/flat-hashes.txt"
    ))
    .unwrap_or_else(|error| unreachable!("the asset is missing: {error}"));

    // **Line by line rather than two joined blocks.** Comparing the blocks is
    // one assertion that prints both in full, so a reader counts columns to
    // find which format moved. The provocations that checked this arm moved
    // three of eight; naming those three is the difference between a failure
    // that says what happened and one that says only that it did.
    let expected: Vec<&str> = expected.trim().lines().collect();
    let disagreed: Vec<String> = measured
        .iter()
        .zip(&expected)
        .filter(|(ours, theirs)| ours.as_str() != **theirs)
        .map(|(ours, theirs)| {
            format!("  {theirs}  <- the other surface\n  {ours}  <- this one")
        })
        .collect();

    // **The lengths first, and on their own.** `zip` stops at the shorter
    // side, so eight measured against a five-line asset compares five and
    // `disagreed` can come back empty -- and folding both facts into one
    // assertion printed "in 0 of 5 formats:" followed by nothing, a failure
    // whose own body says nothing disagreed. Measured, not reasoned: that is
    // the message a truncated asset produced. Two assertions, because they are
    // two questions and only one of them has a list to print.
    assert_eq!(
        measured.len(),
        expected.len(),
        "this test encodes {} formats and the asset names {}; regenerate it \
         with UPDATE_ENCODE_HASHES=1, or the format lists have come apart",
        measured.len(),
        expected.len()
    );

    assert!(
        disagreed.is_empty(),
        "the two surfaces disagree about encoded bytes in {} of {} formats:\n{}",
        disagreed.len(),
        expected.len(),
        disagreed.join("\n")
    );
}
