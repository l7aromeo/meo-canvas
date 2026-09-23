//! One flat scene encoded in every format, compared by hash with the JavaScript
//! surface: agreement after the wire, not correctness, and a refusal fails. A
//! `quality` default on either side turns the same three formats red. Windows
//! writes its own `tiff`; `UPDATE_ENCODE_HASHES=1` regenerates.
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

/// FNV-1a, 64-bit, hand-written on both sides rather than a dependency bought
/// for a test; both are checked against the published vector in
/// `hashes_the_way_the_other_side_hashes`.
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

/// This host's variant suffix, `windows-x86_64` and so on, keyed on
/// architecture too, so an unmeasured cell falls back to the base and fails if
/// it differs.
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

/// The base rows with this platform's overlay applied: eight independent rows,
/// so an overlay rather than a whole file that goes stale. Each overlay row
/// must differ from the row it replaces, or it says to delete the line.
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

    // Line by line, so a failure names which of eight formats moved.
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

    // The lengths first, on their own: `zip` stops at the shorter side, so a
    // truncated asset would compare fewer rows and report nothing disagreeing.
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
