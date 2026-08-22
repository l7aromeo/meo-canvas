//! Renders a scene file to an image from the command line.
//!
//! The surface that exists to make the pipeline usable without writing a
//! program: read an encoded scene, render it, write the bytes. It is also the
//! only place in the workspace that touches the network, and only when built
//! with `--features net`.
//!
//! # What this crate deliberately excludes
//!
//! No scene authoring. The CLI reads the binary format
//! [`meo_canvas_scene::codec`] defines; it does not parse a text or JSON
//! description into one. A second authoring syntax is a second thing that can
//! disagree with the scene types, and the Node addon and the Rust API already
//! cover authoring.
//!
//! No async runtime, with or without `net`. Fetching goes through a blocking
//! client, because a command-line renderer runs one job and exits -- there is
//! nothing for an executor to overlap it with.
//!
//! # Exit codes
//!
//! Distinct per failure class, so a script can branch on what went wrong
//! without parsing the message. `2` belongs to clap and is what a misspelled
//! flag produces.
//!
//! | code | meaning |
//! | ---- | ------- |
//! | 0 | the image was written |
//! | 2 | the command line was not understood |
//! | 3 | an input or output file could not be read or written |
//! | 4 | the scene file is not a scene this revision reads |
//! | 5 | a font could not be registered |
//! | 6 | the scene names a source this build cannot obtain |
//! | 7 | a render pass failed |

// The CLI's whole output contract is stdout and stderr: the rendered bytes go
// to a file or to stdout, and progress goes to stderr. `print_stdout` is a
// warning aimed at libraries that log where a caller cannot intercept it, which
// does not describe a program whose stdout is the deliverable.
#![allow(
    clippy::print_stdout,
    reason = "stdout is this binary's output channel"
)]

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use meo_canvas_core::{Error, ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::Scene;

/// An input or output file could not be read or written.
const EXIT_IO: u8 = 3;
/// The bytes are not a scene this revision reads.
const EXIT_MALFORMED_SCENE: u8 = 4;
/// A font file could not be registered.
const EXIT_FONT: u8 = 5;
/// The scene names an image this build cannot obtain by itself.
const EXIT_UNRESOLVED_SOURCE: u8 = 6;
/// Resolve, measure, layout, paint or encode failed.
const EXIT_RENDER: u8 = 7;

/// Renders a `meo-canvas` scene file to an image.
#[derive(Debug, Parser)]
#[command(name = "meo-canvas", version, about)]
struct Cli {
    /// What to do.
    #[command(subcommand)]
    command: Command,
}

/// The verbs the binary offers.
///
/// A subcommand rather than a bare set of flags, so that a second verb -- an
/// inspector, a fixture recorder -- is an addition rather than a break in the
/// command line that already shipped.
#[derive(Debug, Subcommand)]
enum Command {
    /// Renders a scene file to an image.
    Render(RenderArgs),
}

/// Everything `render` takes.
#[derive(Debug, Parser)]
struct RenderArgs {
    /// Encoded scene file to render.
    scene: PathBuf,

    /// Where to write the image. Writes to stdout when absent.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Container to encode in. Taken from the output file's extension when
    /// absent.
    #[arg(short, long)]
    format: Option<String>,

    /// A font face, as `family=path`. Repeat it to give one family several
    /// weights, or to register several families.
    #[arg(long = "font", value_name = "FAMILY=PATH")]
    fonts: Vec<String>,

    /// Lossy quality from 0.0 to 1.0, read by JPEG, WebP and AVIF.
    #[arg(long)]
    quality: Option<f32>,

    /// Encode WebP without loss.
    #[arg(long)]
    lossless: bool,

    /// Which page a single-page format writes, counting from zero.
    #[arg(long)]
    page: Option<usize>,

    /// Frames per second for an animated format.
    #[arg(long)]
    fps: Option<f32>,

    /// How many times an animation plays. Absent plays it forever.
    #[arg(long)]
    loops: Option<u32>,
}

/// A failure with the exit code that names its class.
#[derive(Debug)]
struct Failure {
    /// What to print on stderr.
    message: String,
    /// What to exit with.
    code: u8,
}

impl Failure {
    /// Builds a failure from anything printable.
    fn new(message: impl Into<String>, code: u8) -> Self {
        Self {
            message: message.into(),
            code,
        }
    }
}

/// The exit code a core failure belongs to.
///
/// Mapped per variant rather than collapsed to one code, because the three a
/// caller can act on differ: a missing font is fixed by passing `--font`, an
/// unresolved source by building with `net`, and a malformed scene by
/// re-encoding it.
const fn exit_code_for(error: &Error) -> u8 {
    match error {
        Error::UnresolvedSource(_) => EXIT_UNRESOLVED_SOURCE,
        Error::UnknownFont(_) | Error::FontRegister { .. } => EXIT_FONT,
        Error::ImageRead { .. } => EXIT_IO,
        _ => EXIT_RENDER,
    }
}

/// The message a failure prints, with the part the caller can act on.
///
/// A core error says what went wrong; only the CLI knows that a URL source is
/// obtainable by a different build of itself. Naming the feature turns "this
/// crate does not fetch" into an instruction.
fn explain(error: &Error) -> String {
    match error {
        Error::UnresolvedSource(_) if cfg!(not(feature = "net")) => {
            format!("{error}; build with `--features net` to fetch it")
        }
        other => other.to_string(),
    }
}

/// Splits a `family=path` pair.
///
/// The family is named rather than read from the file because that is the name
/// a scene's `fontFamily` has to match, and a caller who wants their file
/// called something else should not have to rename the file. `canvas.type.ts`
/// settled the same shape as `{ family, paths[] }`.
fn parse_font(pair: &str) -> Result<(&str, &Path), Failure> {
    let (family, path) = pair.split_once('=').ok_or_else(|| {
        Failure::new(
            format!("--font expects `family=path`, not {pair:?}"),
            EXIT_FONT,
        )
    })?;

    if family.is_empty() {
        return Err(Failure::new(
            format!("--font {pair:?} names no family"),
            EXIT_FONT,
        ));
    }

    Ok((family, Path::new(path)))
}

/// Works out which container to write.
///
/// A named format wins; otherwise the output file's extension names one. There
/// is no default: writing a PNG because nothing said otherwise turns a
/// misspelled `--format` into a silently wrong file.
fn resolve_format(args: &RenderArgs) -> Result<ImageFormat, Failure> {
    if let Some(name) = &args.format {
        return ImageFormat::from_extension(name).ok_or_else(|| {
            Failure::new(format!("{name:?} names no format"), EXIT_IO)
        });
    }

    let extension = args
        .output
        .as_ref()
        .and_then(|path| path.extension())
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| {
            Failure::new(
                "no --format, and the output names no extension to infer one from",
                EXIT_IO,
            )
        })?;

    ImageFormat::from_extension(extension).ok_or_else(|| {
        Failure::new(
            format!("the output extension {extension:?} names no format"),
            EXIT_IO,
        )
    })
}

/// Reads and decodes the scene file.
fn read_scene(path: &Path) -> Result<Scene, Failure> {
    let bytes = std::fs::read(path).map_err(|source| {
        Failure::new(
            format!("cannot read {}: {source}", path.display()),
            EXIT_IO,
        )
    })?;

    meo_canvas_scene::codec::decode(&bytes).map_err(|source| {
        Failure::new(
            format!(
                "{} is not a scene this build reads: {source}",
                path.display()
            ),
            EXIT_MALFORMED_SCENE,
        )
    })
}

/// Builds the renderer every `--font` pair is registered into.
///
/// The renderer owns the fonts, so registering them is building it: a caller
/// rendering a thousand scenes registers once and the faces outlive any one
/// scene.
fn build_renderer(pairs: &[String]) -> Result<Renderer, Failure> {
    let mut renderer = Renderer::new();
    for pair in pairs {
        let (family, path) = parse_font(pair)?;
        renderer
            .register_font(family, path)
            .map_err(|source| Failure::new(explain(&source), EXIT_FONT))?;
    }
    Ok(renderer)
}

/// Turns the flags into the encoder's options.
fn encode_options(args: &RenderArgs) -> EncodeOptions {
    EncodeOptions {
        quality: args.quality,
        // Only sent when asked for: a `false` here would override the
        // renderer's own default rather than leave it alone.
        lossless: args.lossless.then_some(true),
        matte: None,
        page: args.page,
        fps: args.fps,
        frame_delays: Vec::new(),
        loops: args.loops,
    }
}

/// Writes the encoded bytes where the caller asked for them.
fn write_output(bytes: &[u8], output: Option<&Path>) -> Result<(), Failure> {
    output.map_or_else(
        || {
            std::io::stdout().write_all(bytes).map_err(|source| {
                Failure::new(
                    format!("cannot write to stdout: {source}"),
                    EXIT_IO,
                )
            })
        },
        |path| {
            std::fs::write(path, bytes).map_err(|source| {
                Failure::new(
                    format!("cannot write {}: {source}", path.display()),
                    EXIT_IO,
                )
            })
        },
    )
}

/// Reads the scene, renders it, and writes the result.
fn render(args: &RenderArgs) -> Result<(), Failure> {
    let format = resolve_format(args)?;
    let scene = read_scene(&args.scene)?;
    let renderer = build_renderer(&args.fonts)?;
    let options = encode_options(args);

    let image = renderer
        .render_to_buffer(&scene, format, &options)
        .map_err(|error| {
            Failure::new(explain(&error), exit_code_for(&error))
        })?;

    write_output(&image, args.output.as_deref())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let Command::Render(args) = &cli.command;

    match render(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("meo-canvas: {}", failure.message);
            ExitCode::from(failure.code)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use meo_canvas_core::Error;

    use super::{
        EXIT_FONT, EXIT_IO, EXIT_UNRESOLVED_SOURCE, ImageFormat, RenderArgs,
        encode_options, exit_code_for, parse_font, resolve_format,
    };

    /// The arguments a caller who named nothing optional would produce.
    fn bare(scene: &str, output: Option<&str>) -> RenderArgs {
        RenderArgs {
            scene: PathBuf::from(scene),
            output: output.map(PathBuf::from),
            format: None,
            fonts: Vec::new(),
            quality: None,
            lossless: false,
            page: None,
            fps: None,
            loops: None,
        }
    }

    #[test]
    fn a_font_pair_splits_on_the_first_equals() {
        // A path may contain `=`, so only the first one separates. Splitting on
        // the last would take a family called `Inter` and a file called
        // `a=b.ttf` and produce a family called `Inter=a`.
        let (family, path) = parse_font("Inter=/fonts/a=b.ttf")
            .unwrap_or_else(|failure| unreachable!("{}", failure.message));

        assert_eq!(family, "Inter");
        assert_eq!(path.to_string_lossy(), "/fonts/a=b.ttf");
    }

    #[test]
    fn a_font_without_a_family_is_refused() {
        for pair in ["/fonts/Inter.ttf", "=/fonts/Inter.ttf"] {
            let failure = parse_font(pair)
                .err()
                .unwrap_or_else(|| unreachable!("{pair} names no family"));
            assert_eq!(failure.code, EXIT_FONT);
        }
    }

    #[test]
    fn the_output_extension_names_the_format() {
        let args = bare("scene.mcs", Some("out.webp"));
        let format = resolve_format(&args)
            .unwrap_or_else(|failure| unreachable!("{}", failure.message));

        assert_eq!(format, ImageFormat::Webp);
    }

    #[test]
    fn a_named_format_wins_over_the_extension() {
        let mut args = bare("scene.mcs", Some("out.webp"));
        args.format = Some("png".to_owned());

        let format = resolve_format(&args)
            .unwrap_or_else(|failure| unreachable!("{}", failure.message));

        assert_eq!(format, ImageFormat::Png);
    }

    #[test]
    fn a_name_and_an_extension_that_name_no_format_are_both_refused() {
        // Two paths to the same refusal, and both carry the offending spelling
        // so the caller sees what they typed rather than only that it failed.
        let mut named = bare("scene.mcs", Some("out.png"));
        named.format = Some("nonsense".to_owned());
        let failure = resolve_format(&named)
            .err()
            .unwrap_or_else(|| unreachable!("nonsense names no format"));
        assert_eq!(failure.code, EXIT_IO);
        assert!(failure.message.contains("nonsense"), "{}", failure.message);

        let inferred = bare("scene.mcs", Some("out.xyz"));
        let failure = resolve_format(&inferred)
            .err()
            .unwrap_or_else(|| unreachable!("xyz names no format"));
        assert_eq!(failure.code, EXIT_IO);
        assert!(failure.message.contains("xyz"), "{}", failure.message);
    }

    #[test]
    fn an_unreadable_image_is_an_io_class_rather_than_a_render_one() {
        // The caller's fix is a filesystem one -- the path in the scene is
        // wrong or unreadable -- so it reads as I/O rather than sending them to
        // look at the renderer.
        let error = Error::ImageRead {
            path: "/no-such-directory/a.png".to_owned(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        };

        assert_eq!(exit_code_for(&error), EXIT_IO);
    }

    #[test]
    fn nothing_to_infer_from_is_refused_rather_than_defaulted() {
        // Defaulting to PNG would turn a misspelled `--format` into a silently
        // wrong file rather than a message.
        for output in [None, Some("out")] {
            let failure = resolve_format(&bare("scene.mcs", output))
                .err()
                .unwrap_or_else(|| unreachable!("nothing names a format"));
            assert_eq!(failure.code, EXIT_IO);
        }
    }

    #[test]
    fn an_unset_flag_leaves_the_renderers_default() {
        // `--lossless` absent sends `None`, not `Some(false)`: the second would
        // override a default this crate has no opinion about.
        let options = encode_options(&bare("scene.mcs", Some("out.webp")));

        assert_eq!(options.lossless, None);
        assert_eq!(options.fps, None);
        assert_eq!(options.quality, None);
    }

    /// A temporary path this process alone writes to.
    ///
    /// The process id is in it because these tests write a fixed name into a
    /// shared temporary directory, and cargo runs a crate's test binaries
    /// concurrently -- the unit tests here and the integration tests beside
    /// them are separate processes. Two of them sharing a path is a write, a
    /// delete and a read racing, which fails as a missing file in whichever
    /// one read last.
    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("meo-canvas-cli-{}-{name}", std::process::id()))
    }

    #[test]
    fn a_named_output_receives_the_bytes_and_an_unwritable_path_is_an_io_failure()
     {
        let path = scratch("write-output.bin");

        super::write_output(b"pixels", Some(&path))
            .unwrap_or_else(|failure| unreachable!("{}", failure.message));
        let written = std::fs::read(&path)
            .unwrap_or_else(|source| unreachable!("{source}"));
        assert_eq!(written, b"pixels");
        drop(std::fs::remove_file(&path));

        // A directory that does not exist is the ordinary way this fails, and
        // it is an I/O class rather than a render one.
        let missing = path.join("no-such-directory").join("out.png");
        let failure = super::write_output(b"pixels", Some(&missing))
            .err()
            .unwrap_or_else(|| {
                unreachable!("a missing directory cannot be written to")
            });
        assert_eq!(failure.code, EXIT_IO);
    }

    #[test]
    fn bytes_that_are_not_a_scene_are_a_distinct_failure_from_a_missing_file() {
        let path = scratch("not-a-scene.mcs");
        std::fs::write(&path, b"not a scene at all")
            .unwrap_or_else(|source| unreachable!("{source}"));

        let malformed = super::read_scene(&path)
            .err()
            .unwrap_or_else(|| unreachable!("those bytes are not a scene"));
        drop(std::fs::remove_file(&path));

        let missing = super::read_scene(std::path::Path::new(
            "/no-such-directory/no-such-scene.mcs",
        ))
        .err()
        .unwrap_or_else(|| unreachable!("that file does not exist"));

        assert_eq!(malformed.code, super::EXIT_MALFORMED_SCENE);
        assert_eq!(missing.code, EXIT_IO);
        assert_ne!(malformed.code, missing.code);
    }

    #[test]
    fn a_font_file_that_is_not_there_fails_as_a_font_rather_than_as_io() {
        // The caller's fix is the same either way -- pass a `--font` that
        // exists -- so it reads as a font failure rather than sending them to
        // look at the scene file.
        let failure = super::build_renderer(&[
            "Inter=/no-such-directory/Inter.ttf".to_owned(),
        ])
        .err()
        .unwrap_or_else(|| unreachable!("that font is not there"));

        assert_eq!(failure.code, EXIT_FONT);
    }

    #[test]
    fn registering_nothing_succeeds_and_leaves_the_platforms_faces() {
        assert!(super::build_renderer(&[]).is_ok());
    }

    #[test]
    fn a_url_source_names_the_feature_that_would_fetch_it() {
        let message = super::explain(&Error::UnresolvedSource(
            meo_canvas_scene::NodeId::ROOT,
        ));

        if cfg!(feature = "net") {
            assert!(!message.contains("--features net"));
        } else {
            assert!(
                message.contains("--features net"),
                "the message should say how to obtain it: {message}"
            );
        }
    }

    #[test]
    fn each_failure_class_exits_differently() {
        // A script branches on these, so two classes sharing a code would make
        // "add a font" and "build with net" indistinguishable.
        let unresolved = exit_code_for(&Error::UnresolvedSource(
            meo_canvas_scene::NodeId::ROOT,
        ));
        let font = exit_code_for(&Error::UnknownFont("Inter".to_owned()));
        let layout = exit_code_for(&Error::Layout("no".to_owned()));

        assert_eq!(unresolved, EXIT_UNRESOLVED_SOURCE);
        assert_eq!(font, EXIT_FONT);
        assert_ne!(layout, unresolved);
        assert_ne!(layout, font);
    }
}
