//! Turns a painted surface into the bytes of an image file.
//!
//! The last pass, and the only one whose output leaves the process. It is
//! separate from [`crate::paint`] because one painted surface is often encoded
//! more than once -- a PNG for the response and a WebP for the cache -- and
//! re-painting to change container is work with no drawing in it.
//!
//! # Pages
//!
//! A [`Scene`] carries several page roots, and what a page means is the
//! format's answer rather than the scene's: a frame for GIF and APNG, a sheet
//! for PDF and TIFF, one size of the same icon for ICO. Every other format
//! writes a single page and [`EncodeOptions::page`] chooses which.
//!
//! That rule is the renderer's, and it is asked rather than restated:
//! [`ImageFormat::spans_pages`] and [`ImageFormat::is_animated`] forward to
//! `meo-skia-canvas`, so there is one table and this module reads it. The
//! conformance test walks the renderer's own format list rather than ours, so a
//! format it gains that this crate has not mapped fails a test instead of
//! passing unnoticed.
//!
//! # Timing
//!
//! [`Scene`] carries no frame rate, no duration and no per-page delay, and this
//! module does not invent one. Timing reaches the encoder through
//! [`EncodeOptions::fps`] or [`EncodeOptions::frame_delays`], which is a
//! property of the encode rather than of the scene -- the same pages played at
//! two rates are two files from one scene, not two scenes.
//!
//! [`Scene`]: meo_canvas_scene::Scene

use crate::Error;

/// The container an encoded image is written in.
///
/// The set here is what a caller picks between; which crate writes the bytes is
/// not part of the promise, and neither is whether Skia or a dedicated encoder
/// does it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImageFormat {
    /// Lossless raster with an alpha channel.
    Png,
    /// Lossy raster without one. Transparency is flattened against
    /// [`EncodeOptions::matte`], or black when none is given.
    Jpeg,
    /// Lossy or lossless raster with an alpha channel, and animated when the
    /// scene carries more than one page.
    Webp,
    /// Raster as AV1 intra frames, animated when the scene carries more than
    /// one page. The smallest files here and by far the slowest to write.
    Avif,
    /// Uncompressed raster. The format some Windows tooling reads and nothing
    /// else does.
    Bmp,
    /// Raster, one page per directory entry, and the only format whose pages
    /// may differ in size -- an icon at 16, 32, 48 and 256 pixels is one file.
    Ico,
    /// Raster, one page per directory. Paged but not animated: the pages are
    /// sheets and carry no timing.
    Tiff,
    /// Animated raster, one page per frame, from a palette of 256 colours per
    /// frame.
    Gif,
    /// Animated raster, one page per frame, in full colour with alpha.
    Apng,
    /// Vector markup.
    Svg,
    /// Vector document, one page per sheet.
    Pdf,
    /// Unencoded pixel bytes in the surface's own layout, with no container
    /// around them.
    Raw,
}

impl core::fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

impl ImageFormat {
    /// Every format, in declaration order.
    ///
    /// Exists so a test can exercise the whole set without listing it a second
    /// time, which is what keeps a format added later from being untested by
    /// omission.
    pub const ALL: &'static [Self] = &[
        Self::Png,
        Self::Jpeg,
        Self::Webp,
        Self::Avif,
        Self::Bmp,
        Self::Ico,
        Self::Tiff,
        Self::Gif,
        Self::Apng,
        Self::Svg,
        Self::Pdf,
        Self::Raw,
    ];

    /// The format's name as a reader would write it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Webp => "WebP",
            Self::Avif => "AVIF",
            Self::Bmp => "BMP",
            Self::Ico => "ICO",
            Self::Tiff => "TIFF",
            Self::Gif => "GIF",
            Self::Apng => "APNG",
            Self::Svg => "SVG",
            Self::Pdf => "PDF",
            Self::Raw => "raw",
        }
    }

    /// The extension conventionally given to this format, without a dot.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            // `apng`, not `png`: upstream registers the two separately
            // (`export.rs:426` against `:362`), and a filename that says which
            // it is spares a decoder guessing from the chunks.
            Self::Apng => "apng",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Avif => "avif",
            Self::Bmp => "bmp",
            Self::Ico => "ico",
            Self::Tiff => "tiff",
            Self::Gif => "gif",
            Self::Svg => "svg",
            Self::Pdf => "pdf",
            Self::Raw => "bin",
        }
    }

    /// The IANA media type this format is served as.
    ///
    /// Delegated to the renderer rather than matched here, for the reason
    /// [`ImageFormat::from_extension`] is: upstream's `mime_type` reads the
    /// same trait table its `extension` and `is_animated` read
    /// (`meo-skia-canvas-0.11.0/src/export.rs:576`), so a format whose type
    /// changes there changes here, and there is one table rather than a second
    /// one restating it.
    ///
    /// [`Raw`](ImageFormat::Raw) has no registered type and reports
    /// `application/octet-stream`.
    #[must_use]
    pub fn media_type(self) -> &'static str {
        to_skia_format(self).mime_type()
    }

    /// The format a **caller** named, by extension or by name.
    ///
    /// [`ImageFormat::from_extension`] answers the other question: what format
    /// a filename *found on disk* implies. It refuses `raw` deliberately, and
    /// rightly — upstream calls that container `.bin`, and a `.bin` of pixel
    /// bytes is not something any format may be inferred from.
    ///
    /// This is for a path the caller has just typed, where `.raw` is as plain a
    /// statement of intent as `.png`. A caller who writes `to_file("x.raw")`
    /// has named the format; nothing is being inferred.
    ///
    /// The two questions were being answered in three places — here, the
    /// addon's format tag, and the JavaScript surface's `formatForPath` — with
    /// the Rust half of the pair disagreeing with the other two. `to_file`
    /// refused a `.raw` path that `toFile` accepted.
    #[must_use]
    pub fn from_named(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("raw") {
            return Some(Self::Raw);
        }
        Self::from_extension(name)
    }

    /// The format a filename extension names, if any.
    ///
    /// Delegates to the renderer rather than matching here, so the aliases it
    /// accepts -- `jpeg` for `jpg`, and whichever others it registers -- are
    /// the same set on both sides and there is one table rather than a third
    /// copy. A format the renderer marks as not inferable, `raw` among them,
    /// answers `None`: asking for pixel bytes is something a caller says by
    /// name, and a `.bin` that wrote them would be a file nothing reads back.
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        use meo_skia_canvas::export::ImageFormat as Skia;

        Some(match Skia::from_extension(extension)? {
            Skia::Png => Self::Png,
            Skia::Jpeg => Self::Jpeg,
            Skia::Webp => Self::Webp,
            Skia::Avif => Self::Avif,
            Skia::Bmp => Self::Bmp,
            Skia::Ico => Self::Ico,
            Skia::Tiff => Self::Tiff,
            Skia::Gif => Self::Gif,
            Skia::Apng => Self::Apng,
            Skia::Svg => Self::Svg,
            Skia::Pdf => Self::Pdf,
            Skia::Raw => Self::Raw,
        })
    }

    /// Whether one file of this format carries every page.
    ///
    /// The renderer's answer, not a copy of it. PDF, TIFF, ICO and the animated
    /// formats span; the rest write the page [`EncodeOptions::page`] names.
    #[must_use]
    pub fn spans_pages(self) -> bool {
        to_skia_format(self).spans_pages()
    }

    /// Whether this format's pages are frames with durations.
    ///
    /// Distinct from [`spans_pages`](Self::spans_pages): PDF, TIFF and ICO
    /// gather every page and carry no clock, so a frame rate is meaningless to
    /// them rather than merely unused. WebP and AVIF animate as well as GIF and
    /// APNG, which is what `canvas.type.ts:1353` spells as
    /// `AnimatedFormat = 'gif' | 'apng' | 'webp' | 'avif'`.
    #[must_use]
    pub fn is_animated(self) -> bool {
        to_skia_format(self).is_animated()
    }

    /// Whether the drawing is written as vectors rather than pixels.
    #[must_use]
    pub fn is_vector(self) -> bool {
        to_skia_format(self).is_vector()
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
///
/// The timing fields are the exception, and deliberately: naming a frame rate
/// for a format with no clock is [`Error::Encode`] rather than something
/// quietly dropped, because a caller who wrote `fps` and got a still image back
/// asked for something that did not happen.
#[derive(Debug, Clone, Default)]
pub struct EncodeOptions {
    /// Lossy quality from `0.0` to `1.0`, read by JPEG, WebP and AVIF.
    pub quality: Option<f32>,
    /// Whether a WebP encode is lossless.
    pub lossless: Option<bool>,
    /// The colour transparency is flattened against by a format with no alpha
    /// channel, as a packed `0xRRGGBB`.
    pub matte: Option<u32>,
    /// Which page a single-page format writes, counting from zero.
    ///
    /// `None` writes the last page, which is the one a caller who drew a
    /// sequence and asked for a PNG almost always means.
    pub page: Option<usize>,
    /// Frames per second for an animated format.
    ///
    /// `None` leaves the encoder's own default, which is 30 -- the same rate
    /// v1 assumed when a caller gave a duration and no rate.
    pub fps: Option<f32>,
    /// Per-frame durations in milliseconds, overriding [`fps`](Self::fps).
    ///
    /// Read only when it holds one entry per written page. A shorter or longer
    /// list is [`Error::Encode`] rather than a partial application, because a
    /// list that silently retimed some frames and not others is worse than a
    /// refusal.
    pub frame_delays: Vec<u32>,
    /// How many times an animation plays. `None` plays it forever.
    pub loops: Option<u32>,
}

impl EncodeOptions {
    /// Rejects a combination the format cannot honour.
    ///
    /// Checked before anything is drawn or encoded, so a caller learns that
    /// `fps` means nothing to a PNG without waiting for a surface to be
    /// rasterised first.
    ///
    /// `pages` is the number the scene carries, which for a single-page format
    /// is the number [`EncodeOptions::page`] indexes into.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Encode`] when a timing field is set for a format with
    /// no clock, when `frame_delays` does not hold one entry per written page,
    /// or when `page` is past the last one.
    pub fn validate(
        &self,
        format: ImageFormat,
        pages: usize,
    ) -> Result<(), Error> {
        let timed = self.fps.is_some()
            || !self.frame_delays.is_empty()
            || self.loops.is_some();
        if timed && !format.is_animated() {
            return Err(Error::Encode {
                format,
                detail: format!(
                    "{format} has no frame timing, so fps, frame_delays and loops name nothing"
                ),
            });
        }

        if let Some(page) = self.page
            && page >= pages
        {
            return Err(Error::Encode {
                format,
                detail: format!(
                    "page {page} is out of range; the scene has {pages}"
                ),
            });
        }

        let written = self.written_pages(format, pages);
        if !self.frame_delays.is_empty() && self.frame_delays.len() != written {
            return Err(Error::Encode {
                format,
                detail: format!(
                    "frame_delays holds {} entries for {written} written pages",
                    self.frame_delays.len()
                ),
            });
        }

        Ok(())
    }

    /// How many pages this encode writes.
    ///
    /// A named page wins over the format spanning them: asking for one page of
    /// a PDF writes a one-sheet PDF rather than the whole document. That is the
    /// order `meo-skia-canvas` resolves the pair in, and the two must agree or
    /// a `frame_delays` list sized against one is refused by the other.
    #[must_use]
    pub fn written_pages(&self, format: ImageFormat, pages: usize) -> usize {
        if self.page.is_some() || !format.spans_pages() {
            1
        } else {
            pages
        }
    }
}

/// The renderer's own format for one of ours.
const fn to_skia_format(
    format: ImageFormat,
) -> meo_skia_canvas::export::ImageFormat {
    use meo_skia_canvas::export::ImageFormat as Skia;
    match format {
        ImageFormat::Png => Skia::Png,
        ImageFormat::Jpeg => Skia::Jpeg,
        ImageFormat::Webp => Skia::Webp,
        ImageFormat::Avif => Skia::Avif,
        ImageFormat::Bmp => Skia::Bmp,
        ImageFormat::Ico => Skia::Ico,
        ImageFormat::Tiff => Skia::Tiff,
        ImageFormat::Gif => Skia::Gif,
        ImageFormat::Apng => Skia::Apng,
        ImageFormat::Svg => Skia::Svg,
        ImageFormat::Pdf => Skia::Pdf,
        ImageFormat::Raw => Skia::Raw,
    }
}

/// Lowers this crate's options onto the renderer's.
///
/// Only the fields a caller set are written; the rest keep the renderer's own
/// defaults, which is what makes `fps: None` mean 30 rather than zero. A field
/// this crate does not expose -- `density`, `msaa`, `bit_depth`, `chroma`,
/// `color_space`, `page_range` -- is left alone rather than restated, so the
/// default that applies is the renderer's and there is one copy of it.
fn to_skia_options(
    options: &EncodeOptions,
) -> meo_skia_canvas::export::EncodeOptions {
    let mut lowered = meo_skia_canvas::export::EncodeOptions::default();

    if let Some(quality) = options.quality {
        lowered.quality = quality;
    }
    if let Some(lossless) = options.lossless {
        lowered.lossless = lossless;
    }
    if let Some(matte) = options.matte {
        // Packed `0xRRGGBB`, opaque: a matte is what transparency is flattened
        // against, so a translucent one would leave some of it unflattened.
        lowered.matte = Some(meo_skia_canvas::color::RgbaLinear::from_srgb8(
            ((matte >> 16) & 0xff) as u8,
            ((matte >> 8) & 0xff) as u8,
            (matte & 0xff) as u8,
            1.0,
        ));
    }
    lowered.page = options.page;
    lowered.fps = options.fps;
    lowered.frame_delays.clone_from(&options.frame_delays);
    lowered.loops = options.loops;

    lowered
}

/// Encodes a painted surface.
///
/// Every page the surface holds is already drawn; which of them reach the file
/// is the format's rule, and [`EncodeOptions::validate`] has already refused
/// the combinations that cannot be honoured.
///
/// # Errors
///
/// Returns [`Error::Encode`] when the options do not suit the format -- see
/// [`EncodeOptions::validate`] -- or when the encoder rejects the surface,
/// which for [`ImageFormat::Jpeg`] includes a surface whose alpha channel
/// cannot be flattened without a stated background.
pub fn encode(
    surface: &mut crate::paint::Surface,
    format: ImageFormat,
    options: &EncodeOptions,
) -> Result<EncodedImage, Error> {
    // Counted through the surface rather than the canvas, so the count is read
    // without a Skia type entering this function's reasoning, and read before
    // the mutable borrow the encode itself needs.
    //
    // The surface's pages rather than the scene's: a page that failed to begin
    // is not one the encoder can write, and `frame_delays` is checked against
    // what is written.
    options.validate(format, surface.page_count())?;

    // `&mut`, because every encode entry point upstream takes `&mut self`
    // (`canvas.rs:551`, `:624`, `:656`) -- `to_buffer` prepares the page
    // sequence before writing it, which is a mutation of the canvas.
    let bytes = surface
        .canvas_mut()
        .to_buffer(to_skia_format(format), &to_skia_options(options))
        .map_err(|error| Error::Encode {
            format,
            detail: error.to_string(),
        })?;

    Ok(EncodedImage { bytes, format })
}

#[cfg(test)]
mod tests {
    use super::{EncodeOptions, ImageFormat};
    use crate::Error;

    #[test]
    fn every_format_names_an_extension_a_name_and_a_media_type() {
        for format in ImageFormat::ALL {
            assert!(!format.extension().is_empty());
            assert!(!format.name().is_empty());
            assert!(
                format.media_type().contains('/'),
                "{format} has no media type"
            );
        }
        // The one a caller is most likely to read, spelled out rather than
        // only asserted structurally.
        assert_eq!(ImageFormat::Png.media_type(), "image/png");
        assert_eq!(ImageFormat::Jpeg.media_type(), "image/jpeg");
        assert_eq!(ImageFormat::Raw.media_type(), "application/octet-stream");
    }

    #[test]
    fn a_caller_may_name_raw_where_a_filename_may_not_imply_it() {
        // The two questions this crate answers about a string, and the one
        // place they differ. Inference from a filename found on disk refuses
        // `raw`; a caller typing `.raw` has named it.
        assert_eq!(ImageFormat::from_extension("raw"), None);
        assert_eq!(ImageFormat::from_named("raw"), Some(ImageFormat::Raw));
        assert_eq!(ImageFormat::from_named("RAW"), Some(ImageFormat::Raw));

        // Everything else answers the same to both, so the pair cannot drift
        // into two different tables.
        for format in ImageFormat::ALL {
            if *format == ImageFormat::Raw {
                continue;
            }
            let extension = format.extension();
            assert_eq!(
                ImageFormat::from_named(extension),
                ImageFormat::from_extension(extension),
                "{format} answers differently to the two questions"
            );
        }
        assert_eq!(ImageFormat::from_named("nonsense"), None);
    }

    #[test]
    fn every_extension_this_crate_names_round_trips_through_the_renderer() {
        // Each format's own extension resolves back to it, so the two tables
        // agree on every entry rather than only on the ones a test happened to
        // name. `raw` is the exception the renderer states: it is asked for by
        // name and its `.bin` is deliberately not inferable.
        for format in ImageFormat::ALL {
            let resolved = ImageFormat::from_extension(format.extension());
            if *format == ImageFormat::Raw {
                assert_eq!(resolved, None, "raw should not be inferable");
            } else {
                assert_eq!(
                    resolved,
                    Some(*format),
                    "{format} did not round-trip"
                );
            }
        }
    }

    #[test]
    fn an_unknown_extension_names_no_format() {
        assert_eq!(ImageFormat::from_extension("docx"), None);
        assert_eq!(ImageFormat::from_extension(""), None);
    }

    #[test]
    fn no_two_formats_claim_the_same_extension() {
        // Including APNG and PNG, which upstream registers separately. A
        // collision here would make a filename ambiguous about what wrote it.
        let mut extensions: Vec<&str> = ImageFormat::ALL
            .iter()
            .map(|format| format.extension())
            .collect();
        extensions.sort_unstable();
        let total = extensions.len();
        extensions.dedup();

        assert_eq!(total, extensions.len());
    }

    #[test]
    fn animated_formats_span_pages_but_not_every_spanning_format_is_animated() {
        for format in ImageFormat::ALL {
            assert!(
                !format.is_animated() || format.spans_pages(),
                "{format} is animated but writes one page"
            );
        }

        // The pair that makes the two axes distinct: sheets, not frames.
        for format in [ImageFormat::Pdf, ImageFormat::Tiff] {
            assert!(format.spans_pages(), "{format} gathers every page");
            assert!(!format.is_animated(), "{format} has no clock");
        }
    }

    #[test]
    fn timing_named_for_a_format_with_no_clock_is_refused() {
        let options = EncodeOptions {
            fps: Some(24.0),
            ..EncodeOptions::default()
        };

        for format in ImageFormat::ALL {
            let checked = options.validate(*format, 2);
            if format.is_animated() {
                assert!(checked.is_ok(), "{format} takes a frame rate");
            } else {
                assert!(
                    matches!(checked, Err(Error::Encode { .. })),
                    "{format} has no clock and should refuse one"
                );
            }
        }
    }

    #[test]
    fn this_crates_format_table_conforms_to_the_renderers() {
        use meo_skia_canvas::export::ImageFormat as Skia;

        /// The local format that lowers onto `upstream`, if this crate has one.
        fn local(upstream: Skia) -> Option<ImageFormat> {
            ImageFormat::ALL
                .iter()
                .copied()
                .find(|format| super::to_skia_format(*format) == upstream)
        }

        // Driven from the renderer's list rather than ours, which is the whole
        // point: a format it gains that this crate has not mapped has no entry
        // to disagree with, so a loop over `ImageFormat::ALL` would pass while
        // the gap widened. Driven from `Skia::all()`, the missing entry is the
        // failure.
        //
        // Collected rather than asserted inside the loop, so a release adding
        // three formats reports three rather than the first of them.
        let unmapped: Vec<Skia> = Skia::all()
            .filter(|upstream| local(*upstream).is_none())
            .collect();
        assert!(
            unmapped.is_empty(),
            "the renderer has formats this crate does not map: {unmapped:?}"
        );

        let disagreements: Vec<String> = Skia::all()
            .filter_map(|upstream| {
                let ours = local(upstream)?;
                let mut wrong = Vec::new();
                if ours.extension() != upstream.extension() {
                    wrong.push("extension");
                }
                if ours.is_animated() != upstream.is_animated() {
                    wrong.push("is_animated");
                }
                if ours.spans_pages() != upstream.spans_pages() {
                    wrong.push("spans_pages");
                }
                if ours.is_vector() != upstream.is_vector() {
                    wrong.push("is_vector");
                }
                if ours.media_type() != upstream.mime_type() {
                    wrong.push("media_type");
                }
                (!wrong.is_empty()).then(|| {
                    format!("{ours} disagrees on {}", wrong.join(", "))
                })
            })
            .collect();
        assert!(
            disagreements.is_empty(),
            "this crate's table disagrees with the renderer's: {disagreements:?}"
        );

        // The other direction: a variant of ours lowering onto a format
        // `all()` never yields would be one the renderer does not have.
        assert_eq!(Skia::all().count(), ImageFormat::ALL.len());
    }

    #[test]
    fn every_format_lowers_onto_the_renderers_own() {
        use meo_skia_canvas::export::ImageFormat as Skia;

        // A mapping that is total and injective is one that lost nothing. The
        // two enums carry the same twelve formats, so anything else here is a
        // format silently encoded as another.
        let lowered: Vec<Skia> = ImageFormat::ALL
            .iter()
            .copied()
            .map(super::to_skia_format)
            .collect();

        for (index, format) in lowered.iter().enumerate() {
            assert!(
                !lowered[..index].contains(format),
                "two formats lower onto {format:?}"
            );
        }
        assert_eq!(lowered.len(), ImageFormat::ALL.len());
    }

    #[test]
    fn an_unset_option_keeps_the_renderers_default() {
        // `fps: None` meaning 30 is the renderer's default, not one restated
        // here -- which is what makes it agree with v1's `DEFAULT_FPS` without
        // this crate holding a copy of the number.
        let lowered = super::to_skia_options(&EncodeOptions::default());
        let untouched = meo_skia_canvas::export::EncodeOptions::default();

        assert_eq!(lowered.fps, untouched.fps);
        assert_eq!(lowered.quality.to_bits(), untouched.quality.to_bits());
        assert_eq!(lowered.lossless, untouched.lossless);
        assert!(lowered.matte.is_none());
    }

    #[test]
    fn the_timing_fields_reach_the_renderer() {
        let options = EncodeOptions {
            fps: Some(12.0),
            frame_delays: vec![80, 80],
            loops: Some(3),
            page: Some(1),
            ..EncodeOptions::default()
        };

        let lowered = super::to_skia_options(&options);

        assert_eq!(lowered.fps, Some(12.0));
        assert_eq!(lowered.frame_delays, vec![80, 80]);
        assert_eq!(lowered.loops, Some(3));
        assert_eq!(lowered.page, Some(1));
    }

    #[test]
    fn a_matte_unpacks_to_an_opaque_colour() {
        // A matte is what transparency is flattened against, so a translucent
        // one would leave some of it unflattened.
        let options = EncodeOptions {
            matte: Some(0x00_40_80),
            ..EncodeOptions::default()
        };

        let lowered = super::to_skia_options(&options);
        let matte = lowered
            .matte
            .unwrap_or_else(|| unreachable!("a matte was set"));

        assert_eq!(matte.a.to_bits(), 1.0_f32.to_bits());
        assert!(matte.r < matte.g && matte.g < matte.b);
    }

    #[test]
    fn the_animated_set_is_the_one_v1_names() {
        // `canvas.type.ts:1353` spells it
        // `AnimatedFormat = 'gif' | 'apng' | 'webp' | 'avif'`, and
        // `meo-skia-canvas`'s own traits table agrees -- WebP and AVIF are
        // `animated: true, pages: All` there. Both animate over a multi-page
        // scene, which is easy to miss because each is best known as a still
        // format.
        let animated: Vec<ImageFormat> = ImageFormat::ALL
            .iter()
            .copied()
            .filter(|format| format.is_animated())
            .collect();

        assert_eq!(
            animated,
            vec![
                ImageFormat::Webp,
                ImageFormat::Avif,
                ImageFormat::Gif,
                ImageFormat::Apng
            ]
        );
    }

    #[test]
    fn a_page_past_the_last_is_refused() {
        let options = EncodeOptions {
            page: Some(3),
            ..EncodeOptions::default()
        };

        assert!(matches!(
            options.validate(ImageFormat::Png, 3),
            Err(Error::Encode { .. })
        ));
        assert!(options.validate(ImageFormat::Png, 4).is_ok());
    }

    #[test]
    fn a_named_page_wins_over_the_format_spanning_them() {
        // Asking for one page of a PDF writes a one-sheet PDF. The
        // `frame_delays` length is checked against what is written, so
        // the two rules have to agree or a correctly sized list is
        // refused.
        let all = EncodeOptions::default();
        assert_eq!(all.written_pages(ImageFormat::Pdf, 5), 5);

        let one = EncodeOptions {
            page: Some(1),
            ..EncodeOptions::default()
        };
        assert_eq!(one.written_pages(ImageFormat::Pdf, 5), 1);
        assert_eq!(one.written_pages(ImageFormat::Png, 5), 1);
    }

    #[test]
    fn frame_delays_must_hold_one_entry_per_written_page() {
        let short = EncodeOptions {
            frame_delays: vec![100, 100],
            ..EncodeOptions::default()
        };
        assert!(matches!(
            short.validate(ImageFormat::Gif, 3),
            Err(Error::Encode { .. })
        ));

        let exact = EncodeOptions {
            frame_delays: vec![100, 100, 100],
            ..EncodeOptions::default()
        };
        assert!(exact.validate(ImageFormat::Gif, 3).is_ok());

        // One named page writes one frame, so one delay is the right count
        // even though the scene carries three pages.
        let single = EncodeOptions {
            page: Some(0),
            frame_delays: vec![100],
            ..EncodeOptions::default()
        };
        assert!(single.validate(ImageFormat::Gif, 3).is_ok());
    }

    #[test]
    fn a_scene_of_one_page_encodes_to_a_still_image_in_every_format() {
        let options = EncodeOptions::default();
        for format in ImageFormat::ALL {
            assert!(
                options.validate(*format, 1).is_ok(),
                "{format} refused one page"
            );
            assert_eq!(options.written_pages(*format, 1), 1);
        }
    }

    #[test]
    fn only_the_two_vector_formats_are_vector() {
        let vector: Vec<ImageFormat> = ImageFormat::ALL
            .iter()
            .copied()
            .filter(|format| format.is_vector())
            .collect();

        assert_eq!(vector, vec![ImageFormat::Svg, ImageFormat::Pdf]);
    }
}

#[cfg(test)]
mod ico_pages {
    use meo_canvas_scene::Size;

    use super::{ImageFormat, encode};
    use crate::paint::{Surface, SurfaceOptions};

    /// One directory entry of an ICO, as the file writes it.
    #[derive(Debug, PartialEq, Eq)]
    struct Entry {
        width: u32,
        height: u32,
    }

    /// Reads an ICO's directory, which is the only part this asserts on.
    ///
    /// **Decoded from the bytes we wrote rather than inferred from the call we
    /// made.** A test that checks the encoder was invoked with four pages
    /// passes whether or not the file has four entries, and the promise in
    /// `ImageFormat::Ico` is about the file.
    ///
    /// Layout: a six-byte header of `reserved:u16 = 0`, `type:u16 = 1`,
    /// `count:u16`, then `count` sixteen-byte entries whose first two bytes are
    /// the width and height. **A zero means 256** -- one byte cannot hold it,
    /// which is why an icon's largest conventional size is the one that reads
    /// as nothing.
    fn directory(bytes: &[u8]) -> Vec<Entry> {
        assert!(bytes.len() >= 6, "an ICO has at least a header");
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]),
            0,
            "the reserved field is not zero, so this is not an ICO"
        );
        assert_eq!(
            u16::from_le_bytes([bytes[2], bytes[3]]),
            1,
            "the type field does not say icon"
        );
        let count = usize::from(u16::from_le_bytes([bytes[4], bytes[5]]));
        (0..count)
            .map(|index| {
                let at = 6 + index * 16;
                let dimension = |byte: u8| {
                    if byte == 0 { 256 } else { u32::from(byte) }
                };
                Entry {
                    width: dimension(bytes[at]),
                    height: dimension(bytes[at + 1]),
                }
            })
            .collect()
    }

    #[test]
    fn an_ico_carries_one_directory_entry_per_page_at_that_page_s_own_size() {
        let sizes = [16.0_f32, 32.0, 48.0, 256.0];
        let mut surface = Surface::new(
            Size {
                width: sizes[0],
                height: sizes[0],
            },
            1.0,
            SurfaceOptions::default(),
        )
        .unwrap_or_else(|error| {
            unreachable!("the surface did not allocate: {error}")
        });

        for &side in &sizes[1..] {
            surface
                .begin_page(Size {
                    width: side,
                    height: side,
                })
                .unwrap_or_else(|error| {
                    unreachable!("a page did not begin: {error}")
                });
        }

        let written = encode(
            &mut surface,
            ImageFormat::Ico,
            &super::EncodeOptions::default(),
        )
        .unwrap_or_else(|error| {
            unreachable!("the ICO did not encode: {error}")
        });

        let entries = directory(&written.bytes);
        assert_eq!(
            entries,
            sizes
                .iter()
                .map(|&side| Entry {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "every side here is a small whole number"
                    )]
                    width: side as u32,
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "every side here is a small whole number"
                    )]
                    height: side as u32,
                })
                .collect::<Vec<_>>(),
            "the pages went in at four sizes and the directory should say so"
        );
    }
}
