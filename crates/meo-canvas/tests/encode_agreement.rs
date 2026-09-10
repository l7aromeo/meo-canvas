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
//!
//! # One platform moves, and which
//!
//! `windows-x86_64` writes a different `tiff` and nothing else. Both surfaces
//! there produced `2d53a12a013360c6` -- the Rust half in one CI run and the
//! TypeScript half in another -- so they agree with each other and differ from
//! the macOS reference, which makes it a platform row rather than the
//! cross-surface disagreement this arm exists to catch. `ubuntu-latest` moves
//! nothing, so the axis is Windows and not "every platform except the
//! reference". The evidence that it is the container rather than the picture,
//! and the limit of the claim, are in
//! `tests/assets/encode/flat-hashes.windows-x86_64.txt`.
//!
//! **To measure a platform this machine is not**, dispatch
//! `.github/workflows/encode-hashes.yml`. It prints the rows that differ and
//! uploads them; it writes nothing to the branch, because a row is a claim
//! about a platform and a bare hash is unreviewable.
//!
//! # Regenerating
//!
//! `UPDATE_ENCODE_HASHES=1 npx vitest run encode.agreement` writes the asset
//! from the JavaScript side; this side only ever asserts against it, so a
//! regeneration that was not legitimate fails here.
use std::{fs::read_to_string, path::PathBuf};

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

/// Where the assets live.
fn assets() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/encode"))
}

/// This host's variant suffix, `windows-x86_64` and so on.
///
/// The same shape `fixtures.rs` uses, and deliberately not a looser one: the
/// axis measured so far is Windows against a macOS reference with Linux
/// agreeing, and `win32-arm64` is a published target CI does not gate. Keyed
/// on the architecture as well, an unmeasured cell falls back to the base and
/// fails if it differs -- which is a finding rather than a silent pass.
fn host_variant() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The `<format> <hash>` rows of an asset, with comments and blanks dropped.
fn rows(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (name, hash) = line.split_once(' ').unwrap_or_else(|| {
                unreachable!("`{line}` is not `<format> <hash>`")
            });
            (name.to_owned(), hash.to_owned())
        })
        .collect()
}

/// The base rows with this platform's overlay applied.
///
/// # Why an overlay rather than a whole file per platform
///
/// `fixtures.rs` keeps a whole `expected.<os>-<arch>.png` because a PNG is one
/// indivisible artefact -- there is no way to say "this image, but one pixel
/// differs". Eight independent rows are not like that, and a whole-file
/// variant would store seven values twice. **That duplication goes stale in
/// one direction and says nothing about it**: regenerate the base, forget the
/// variant, and seven rows disagree for a reason nobody intended while the one
/// row that was supposed to differ looks untouched.
///
/// # A row that has stopped differing is an error
///
/// `fixtures.rs` states the rule -- a variant exists **only where a platform
/// is measurably different** -- and checks half of it: an absent variant means
/// this platform agrees, and the run finds out. A *present* variant that has
/// become identical to what it replaces is checked nowhere, and is
/// indistinguishable from one still doing work. So each overlay row is
/// asserted to differ from the row it replaces, and an equal one says to
/// delete the line.
fn expected_rows() -> Vec<(String, String)> {
    let base = read_to_string(assets().join("flat-hashes.txt"))
        .unwrap_or_else(|error| unreachable!("the asset is missing: {error}"));
    let mut expected = rows(&base);

    let overlay = assets().join(format!("flat-hashes.{}.txt", host_variant()));
    let Ok(text) = read_to_string(&overlay) else {
        return expected;
    };

    for (name, hash) in rows(&text) {
        let row = expected
            .iter_mut()
            .find(|(format, _)| *format == name)
            .unwrap_or_else(|| {
                unreachable!(
                    "{}: names `{name}`, which the base asset does not",
                    overlay.display()
                )
            });
        assert_ne!(
            row.1,
            hash,
            "{}: `{name}` no longer differs from the base asset. Delete the \
             line -- an override that agrees with what it overrides cannot be \
             told from one still doing work.",
            overlay.display()
        );
        row.1 = hash;
    }

    expected
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

    // **Line by line rather than two joined blocks.** Comparing the blocks is
    // one assertion that prints both in full, so a reader counts columns to
    // find which format moved. The provocations that checked this arm moved
    // three of eight; naming those three is the difference between a failure
    // that says what happened and one that says only that it did.
    let expected: Vec<String> = expected_rows()
        .into_iter()
        .map(|(name, hash)| format!("{name} {hash}"))
        .collect();
    let disagreed: Vec<String> = measured
        .iter()
        .zip(&expected)
        .filter(|(ours, theirs)| ours != theirs)
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
