//! Turns a painted surface into the bytes of an image file.
//!
//! The last pass, and the only one whose output leaves the process. It is
//! separate from [`crate::paint`] because one painted surface is often encoded
//! more than once -- a PNG for the response and a WebP for the cache -- and
//! re-painting to change container is work with no drawing in it.

use crate::Error;

/// The container an encoded image is written in.
///
/// The set here is what a caller picks between; which crate writes the bytes is
/// not part of the promise, and neither is whether Skia or a dedicated encoder
/// does it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    /// Lossless raster with an alpha channel.
    Png,
    /// Lossy raster without one.
    Jpeg,
    /// Lossy or lossless raster with an alpha channel.
    Webp,
    /// Vector output, drawn rather than rasterised.
    Svg,
    /// Vector output in a paged container.
    Pdf,
}

impl core::fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Webp => "WebP",
            Self::Svg => "SVG",
            Self::Pdf => "PDF",
        };
        f.write_str(name)
    }
}

impl ImageFormat {
    /// The extension conventionally given to this format, without a dot.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Svg => "svg",
            Self::Pdf => "pdf",
        }
    }
}

/// Finished bytes and what they are.
#[derive(Debug, Clone)]
pub struct EncodedImage {
    /// The file's contents.
    pub bytes: Vec<u8>,
    /// The container those bytes are in.
    pub format: ImageFormat,
}

/// Quality and container settings that only apply to some formats.
///
/// A single struct with `Option` fields rather than one per format: a caller
/// setting quality does not want to know which formats read it, and a format
/// that ignores a field ignores it rather than refusing the call.
#[derive(Debug, Clone, Copy, Default)]
pub struct EncodeOptions {
    /// Lossy quality from `0.0` to `1.0`, read by JPEG and WebP.
    pub quality: Option<f32>,
    /// Whether a WebP encode is lossless.
    pub lossless: Option<bool>,
}

/// Encodes a painted surface.
///
/// # Errors
///
/// Returns [`Error::Encode`] when the encoder rejects the surface, which for
/// [`ImageFormat::Jpeg`] includes a surface whose alpha channel cannot be
/// flattened without a stated background.
pub fn encode(
    _surface: &crate::paint::Surface,
    _format: ImageFormat,
    _options: EncodeOptions,
) -> Result<EncodedImage, Error> {
    unimplemented!()
}
