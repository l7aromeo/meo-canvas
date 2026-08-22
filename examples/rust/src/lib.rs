//! What every example in this directory shares: where it writes and in what.
//!
//! Each example is a scene and nothing else. This decides the formats, the
//! paths and the size, so that the nine of them differ only in what they draw —
//! and so that the JavaScript half beside them can differ only in syntax.

use std::path::PathBuf;

use meo_canvas::{Canvas, Format, Renderer, Root};

/// The family the text examples name, and the file behind it.
///
/// One font from this repository rather than a platform face: the two surfaces
/// must draw the same bytes, and a family resolved from whatever the host has
/// installed is not the same family twice.
pub const FONT: (&str, &str) = (
    "Showcase",
    "../../crates/meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf",
);

/// The formats every example writes.
///
/// One raster family, one vector, and the raw pixels. A format that refuses a
/// scene is a finding rather than something to skip, so this list is the same
/// for every example and a refusal surfaces as an error naming the format.
pub const FORMATS: &[Format] = &[
    Format::Png,
    Format::Jpeg,
    Format::Webp,
    Format::Avif,
    Format::Bmp,
    Format::Tiff,
    Format::Svg,
    Format::Raw,
];

/// The formats only a multi-page scene has anything to say in.
///
/// A single-page example writing a GIF would write a one-frame animation, which
/// says nothing the PNG does not. These are exercised by `pages` alone.
pub const PAGED_FORMATS: &[Format] =
    &[Format::Pdf, Format::Gif, Format::Apng, Format::Ico];

/// The extension a format's file takes.
///
/// `Format::extension` answers `bin` for raw, which is the filename convention
/// rather than the tag a caller writes. The directory is named by tag on both
/// surfaces so the two trees compare file for file.
#[must_use]
pub fn tag(format: Format) -> &'static str {
    match format {
        Format::Raw => "raw",
        other => other.extension(),
    }
}

/// Renders `root` and writes it in every format `formats` names.
///
/// # Errors
///
/// Returns the first failure, naming the format it was writing. A format that
/// cannot encode a scene is a result worth stopping on rather than skipping:
/// the whole point of the directory is to say which parts work.
pub fn draw(
    name: &str,
    root: Root,
    formats: &[Format],
) -> Result<(), Box<dyn std::error::Error>> {
    draw_with_fonts(name, root, formats, &[])
}

/// The same, with families registered before anything is measured.
///
/// Fonts live on the [`Renderer`] here and on `Root` in the JavaScript surface,
/// because a renderer outlives any one scene and JavaScript exposes no such
/// object. It is the one place the two examples differ in where a thing is
/// written rather than in how.
///
/// # Errors
///
/// As [`draw`], and additionally when a font file cannot be read.
pub fn draw_with_fonts(
    name: &str,
    root: Root,
    formats: &[Format],
    fonts: &[(&str, &str)],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = Renderer::new();
    for (family, path) in fonts {
        renderer.register_font(family, path)?;
    }
    let mut canvas: Canvas = root.render(&renderer)?;

    let directory = PathBuf::from("out").join(name);
    std::fs::create_dir_all(&directory)?;

    for format in formats {
        let path = directory.join(format!("{name}.{}", tag(*format)));
        canvas.to_file(&path).map_err(|error| {
            format!("{name}: writing {} failed: {error}", tag(*format))
        })?;
    }

    println!("{name}: {} formats", formats.len());
    Ok(())
}
