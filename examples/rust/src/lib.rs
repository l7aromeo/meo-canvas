//! What every example shares: where it writes and in what formats, paths and
//! size, so the nine differ only in what they draw and the JavaScript half only
//! in syntax.

use std::path::PathBuf;

use meo_canvas::{Canvas, Format, Renderer, Root};

/// The family the text examples name, and its file: one font from this
/// repository, since both surfaces must draw the same bytes and a host's
/// installed face is not the same family twice.
pub const FONT: (&str, &str) = (
    "Showcase",
    "../../crates/meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf",
);

/// The formats every example writes: one raster family, one vector and raw
/// pixels. A refusal surfaces as an error naming the format rather than a skip.
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

/// The extension a format's file takes: the tag, not `Format::extension`'s
/// `bin` for raw, so the two surfaces' trees compare file for file.
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
/// The first failure, naming the format it was writing.
pub fn draw(
    name: &str,
    root: Root,
    formats: &[Format],
) -> Result<(), Box<dyn std::error::Error>> {
    draw_with_fonts(name, root, formats, &[])
}

/// [`draw`], with families registered first on the [`Renderer`].
///
/// # Errors
/// As [`draw`], and when a font file cannot be read.
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
