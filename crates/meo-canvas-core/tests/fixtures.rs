//! Renders every golden fixture and compares it against its committed image.
//!
//! Executing a fill proves the line ran, not that the pixels are right. This is
//! the only check in the project that looks at the picture.
//!
//! # Why the comparison is exact
//!
//! Measured rather than assumed: one scene with text, descenders and digits,
//! rendered five times -- twice in one process with separate renderers, three
//! times through separate invocations of the CLI -- produced a single SHA-256.
//! So there is no tolerance here and no threshold to justify. A disagreement is
//! a regression until someone measures otherwise, and a tolerance argued from a
//! measured disagreement is a different object from one argued from an
//! anticipated one.
//!
//! The evidence is one architecture and one Skia build. A second machine
//! disagreeing is the moment to revisit this, with its diff in hand.
//!
//! ## The GPU is pinned off, and that is load-bearing
//!
//! A build with the Metal backend compiled does **not** produce the same bytes
//! as one without, and a single scene of text is the measurement that cannot
//! show it: text is the case where the two rasterisers agree.
//!
//! Run with `--features metal`, **eight of the ten fixtures differ**:
//! `box-shadow` by 6129 pixels, `z-order` by 7560, `gradients` by 2705,
//! and `baseline-alignment`, `borders-per-edge`, `object-fit`,
//! `overflow-clip` and `text-descenders` besides. The two that agree are
//! `block-stacking` and `block-stacking-relative`, which draw nothing but
//! axis-aligned rectangles.
//!
//! The dividing line is anti-aliased edges: a curve has them at every size, and
//! glyphs have them only at some -- text at 16, 20 and 22 is byte-identical
//! between the two, 23 and 24 differ, and 28, 32 and 48 agree again. So a
//! golden with no curve in it says nothing about the rasteriser, and one with a
//! curve says the two disagree.
//!
//! Hence [`fixture_renderer`] sets `gpu` to false rather than taking
//! `Renderer::new`'s default of true. Without it this suite passes or fails on
//! a build flag, which is exactly the kind of thing the rest of this section is
//! about not letting a host decide.
//!
//! # What the harness pins
//!
//! Everything a host could otherwise supply. The renderer registers exactly one
//! font, from this repository; the scale is fixed here; the rasteriser is the
//! CPU whatever the build compiled; and a fixture naming
//! any other family is an error rather than a resolution -- because
//! [`meo_canvas_core::resolve::Fonts`] answers `has_family` from the platform's
//! installed faces as well as the registered ones, so a fixture asking for
//! Helvetica would render on this machine and differ on any other.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{Scene, node::NodeKind};

/// The one family a fixture may name.
///
/// A fixed name rather than the face's own, so the fixture files say which font
/// they mean without depending on what the file happens to be called or on what
/// its internal name is.
const FIXTURE_FAMILY: &str = "Fixture";

/// The scale every fixture renders at.
///
/// One. A fixture is a picture of the layout, and a device scale multiplies
/// pixels without changing what is drawn, so rendering at two would quadruple
/// every committed image to prove nothing extra.
const FIXTURE_SCALE: f32 = 1.0;

/// The face registered as [`FIXTURE_FAMILY`].
fn font_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets/fonts/Oswald-VariableFont_wght.ttf")
}

/// The tracked fixture directory, two levels above this crate.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .canonicalize()
        .unwrap_or_else(|error| {
            unreachable!("the fixtures directory is missing: {error}")
        })
}

/// Where a failing fixture leaves its evidence. Untracked.
///
/// The directory is created before it is canonicalised, because a path that
/// does not exist cannot be resolved and the message names it either way -- a
/// report saying `crates/meo-canvas-core/../../target/...` is one the reader
/// has to mentally flatten before they can open it.
/// The platform whose renders `expected.png` holds.
///
/// Named rather than implied, because every other platform's golden is defined
/// relative to it. The images in this repository were rendered on an Apple
/// Silicon Mac, and that is the whole of why this constant is what it is.
const REFERENCE: (&str, &str) = ("macos", "aarch64");

/// Whether this host is the one `expected.png` was rendered on.
fn on_reference() -> bool {
    (std::env::consts::OS, std::env::consts::ARCH) == REFERENCE
}

/// This host's variant suffix, `linux-x86_64` and so on.
fn host_variant() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The image this host is checked against: its own variant where one exists,
/// and the reference image otherwise.
///
/// # Why a variant exists at all
///
/// **A different architecture rasterises anti-aliased edges differently, and
/// that is not a regression.** Measured: on `linux-x86_64`, 15 of the 23
/// fixtures are byte-identical to the reference and 8 differ -- and the 8 are
/// exactly the ones containing a curve, a gradient, a blend or a glyph, while
/// the 15 are the axis-aligned ones. That is the same dividing line the module
/// header measures for the Metal backend, arrived at independently on a second
/// axis.
///
/// The evidence that it is rasterisation rather than a fault is that **every
/// Chrome conformance suite passes on Linux** -- blend formulas, gradient
/// stops, shadow extents, corner geometry, border and dotted rhythm, text
/// truth, ellipsis truth, min-content widths. Those pin numbers rather than
/// pixels, and Linux agrees with the browser on all of them. The pixels differ;
/// what the pixels mean does not.
///
/// # Why a fallback rather than a variant per platform
///
/// Two thirds of the suite is byte-identical everywhere, and giving those a
/// file per platform would be the same picture stored three times, each able to
/// drift from the others. So a variant exists **only where a platform is
/// measurably different**, and its absence means "this platform agrees with the
/// reference" -- which is a claim the run then checks rather than assumes.
///
/// # What this deliberately does not do
///
/// It does not add a tolerance. The module header argues against one, and the
/// argument survives this change: a comparison that permits a few pixels of
/// difference cannot tell a rasteriser apart from a regression that happens to
/// be small, and this suite is the only thing in the project that looks at the
/// picture at all.
fn expected_path(name: &str) -> PathBuf {
    let dir = fixtures_dir().join(name);
    if !on_reference() {
        let variant = dir.join(format!("expected.{}.png", host_variant()));
        if variant.exists() {
            return variant;
        }
    }
    dir.join("expected.png")
}

/// Where `MEO_FIXTURE_ACCEPT` writes on this host.
///
/// The reference image on the reference platform, and this platform's variant
/// anywhere else -- so accepting on Linux can never overwrite the image macOS
/// is checked against.
fn accept_path(name: &str) -> PathBuf {
    let dir = fixtures_dir().join(name);
    if on_reference() {
        dir.join("expected.png")
    } else {
        dir.join(format!("expected.{}.png", host_variant()))
    }
}

fn report_dir(name: &str) -> PathBuf {
    let raw = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name);
    std::fs::create_dir_all(&raw).unwrap_or_else(|error| {
        unreachable!("cannot create {}: {error}", raw.display())
    });
    raw.canonicalize().unwrap_or(raw)
}

/// A renderer carrying the repository's font, on the CPU, and nothing else.
fn fixture_renderer() -> Renderer {
    let mut renderer = Renderer::new();
    // Pinned for the same reason the font and the scale are: it is something a
    // host would otherwise supply. `Renderer::new` asks for the GPU, so without
    // this the suite compares against whichever rasteriser the build happened
    // to compile -- and the two do not agree. See the module documentation.
    renderer.set_gpu(false);
    renderer
        .register_font(FIXTURE_FAMILY, font_path())
        .unwrap_or_else(|error| {
            unreachable!(
                "the repository's fixture font did not register: {error}"
            )
        });
    renderer
}

/// Every font family the scene names, at node level or within a segment.
fn families_named(scene: &Scene) -> BTreeSet<String> {
    let mut named = BTreeSet::new();
    for node in &scene.nodes {
        if let Some(family) = &node.text.font_family {
            named.insert(family.clone());
        }
        if let NodeKind::Text { segments, .. } = &node.kind {
            for segment in segments {
                if let Some(family) = &segment.style.font_family {
                    named.insert(family.clone());
                }
            }
        }
    }
    named
}

/// A decoded image, as 8-bit RGBA.
struct Decoded {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Decodes a PNG into 8-bit RGBA, whatever it was written as.
fn decode(bytes: &[u8], what: &str) -> Decoded {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8()
            | png::Transformations::ALPHA,
    );

    let mut reader = decoder.read_info().unwrap_or_else(|error| {
        unreachable!("{what} is not a PNG this can read: {error}")
    });
    let mut pixels = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut pixels)
        .unwrap_or_else(|error| unreachable!("{what} did not decode: {error}"));

    pixels.truncate(info.buffer_size());
    Decoded {
        width: info.width,
        height: info.height,
        pixels,
    }
}

/// What differs between two images.
struct Difference {
    /// How many pixels disagree.
    pixels: usize,
    /// The smallest box containing every disagreement, as `(x, y, w, h)`.
    bounds: (u32, u32, u32, u32),
    /// One RGBA image marking the disagreements.
    diff: Vec<u8>,
}

/// Compares two decoded images pixel by pixel.
///
/// `None` when they agree. The diff image dims what matches to a quarter of its
/// luminance and paints what does not in opaque red, so a glance shows both the
/// drawing and where it went wrong rather than a field of red on black.
fn compare(actual: &Decoded, expected: &Decoded) -> Option<Difference> {
    let mut count = 0;
    let (mut min_x, mut min_y, mut max_x, mut max_y) =
        (u32::MAX, u32::MAX, 0, 0);
    let mut diff = vec![0_u8; actual.pixels.len()];

    for (index, (a, e)) in actual
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.pixels.as_chunks::<4>().0)
        .enumerate()
    {
        let at = index * 4;
        if a == e {
            diff[at] = a[0] / 4;
            diff[at + 1] = a[1] / 4;
            diff[at + 2] = a[2] / 4;
            diff[at + 3] = 255;
            continue;
        }

        count += 1;
        diff[at] = 255;
        diff[at + 1] = 0;
        diff[at + 2] = 0;
        diff[at + 3] = 255;

        let index = u32::try_from(index).unwrap_or(u32::MAX);
        let (x, y) = (index % actual.width, index / actual.width);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    (count > 0).then(|| Difference {
        pixels: count,
        bounds: (min_x, min_y, max_x - min_x + 1, max_y - min_y + 1),
        diff,
    })
}

/// Writes an RGBA buffer as a PNG.
fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    let file = std::fs::File::create(path).unwrap_or_else(|error| {
        unreachable!("cannot write {}: {error}", path.display())
    });
    let mut encoder =
        png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap_or_else(|error| unreachable!("{error}"))
        .write_image_data(pixels)
        .unwrap_or_else(|error| unreachable!("{error}"));
}

/// Every fixture directory, in name order.
fn fixture_names() -> Vec<String> {
    let dir = fixtures_dir();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| {
            unreachable!("cannot read {}: {error}", dir.display())
        })
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry
                .file_type()
                .ok()?
                .is_dir()
                .then(|| entry.file_name().to_string_lossy().into_owned())
        })
        .collect();
    names.sort();
    names
}

/// Renders one fixture, returning its PNG bytes.
///
/// Fails rather than falls back when the scene names a family the harness did
/// not register: `Fonts` answers from the platform's installed faces too, so a
/// fixture asking for a system font would pass here and differ anywhere else.
fn render_fixture(name: &str) -> Vec<u8> {
    let dir = fixtures_dir().join(name);
    let bytes = std::fs::read(dir.join("scene.mcs")).unwrap_or_else(|error| {
        unreachable!("fixture `{name}` has no scene.mcs: {error}")
    });
    let mut scene =
        meo_canvas_scene::codec::decode(&bytes).unwrap_or_else(|error| {
            unreachable!("fixture `{name}` is not a scene: {error}")
        });

    let foreign: Vec<String> = families_named(&scene)
        .into_iter()
        .filter(|family| family != FIXTURE_FAMILY)
        .collect();
    assert!(
        foreign.is_empty(),
        "fixture `{name}` names families the harness does not register: {foreign:?}; \
         a fixture may only use `{FIXTURE_FAMILY}`, or it renders from this host's fonts"
    );

    // Pinned here rather than taken from the file, so a fixture cannot carry a
    // scale that makes its image disagree with every other one's resolution.
    scene.scale = FIXTURE_SCALE;

    fixture_renderer()
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| {
            unreachable!("fixture `{name}` did not render: {error}")
        })
}

// An ordinary test, so `cargo test --workspace` runs it and `cargo llvm-cov`
// counts it. That is what makes AGENTS.md's claim true -- the fixture runner is
// part of the coverage harness rather than beside it, which is how the paint
// stage earns its coverage by drawing a picture someone checked rather than by
// executing a line.
#[test]
fn every_fixture_matches_its_expected_image() {
    // `MEO_FIXTURE_ACCEPT` names exactly one fixture to rewrite. One name and
    // no bulk form: accepting every difference at once is how a regression
    // becomes a commit, and a legitimate mass change is still worth looking
    // at one image at a time.
    let accepting = std::env::var("MEO_FIXTURE_ACCEPT").ok();

    let names = fixture_names();
    assert!(
        !names.is_empty(),
        "there are no fixtures in {}; a harness over nothing passes without checking anything",
        fixtures_dir().display()
    );

    if let Some(name) = &accepting {
        assert!(
            names.contains(name),
            "no fixture named `{name}`; the fixtures are {names:?}"
        );
        let actual = render_fixture(name);
        let expected = accept_path(name);
        std::fs::write(&expected, &actual).unwrap_or_else(|error| {
            unreachable!("cannot write {}: {error}", expected.display())
        });
        // stderr, not stdout: this is progress, and `print_stdout` is denied
        // outside the binary whose stdout is its deliverable.
        eprintln!("accepted `{name}` -> {}", expected.display());
        return;
    }

    let mut failures = Vec::new();
    for name in &names {
        let actual = render_fixture(name);
        let expected_path = expected_path(name);

        let Ok(expected) = std::fs::read(&expected_path) else {
            failures.push(format!(
                "`{name}` has no expected.png; run `just fixtures-accept {name}` once its scene is right"
            ));
            continue;
        };

        if actual == expected {
            continue;
        }

        // Only decoded on failure. Equal bytes are equal pictures, and decoding
        // to prove it would be work every passing run pays for.
        let decoded_actual = decode(&actual, &format!("`{name}`'s render"));
        let decoded_expected =
            decode(&expected, &format!("`{name}`'s expected.png"));

        let report = report_dir(name);
        write_png(
            &report.join("actual.png"),
            decoded_actual.width,
            decoded_actual.height,
            &decoded_actual.pixels,
        );

        if (decoded_actual.width, decoded_actual.height)
            != (decoded_expected.width, decoded_expected.height)
        {
            failures.push(format!(
                "`{name}` rendered {}x{} against an expected {}x{}; wrote {}",
                decoded_actual.width,
                decoded_actual.height,
                decoded_expected.width,
                decoded_expected.height,
                report.display()
            ));
            continue;
        }

        let Some(difference) = compare(&decoded_actual, &decoded_expected)
        else {
            failures.push(format!(
                "`{name}`'s bytes differ but every pixel agrees -- the containers differ, not the picture; wrote {}",
                report.display()
            ));
            continue;
        };

        let (x, y, width, height) = difference.bounds;
        write_png(
            &report.join("diff.png"),
            decoded_actual.width,
            decoded_actual.height,
            &difference.diff,
        );
        failures.push(format!(
            "`{name}`: {} pixels differ in a {width}x{height} box at ({x}, {y}); wrote {}",
            difference.pixels,
            report.display()
        ));
    }

    assert!(
        failures.is_empty(),
        "{} of {} fixtures differ:\n  {}",
        failures.len(),
        names.len(),
        failures.join("\n  ")
    );
}
