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

// The CLI's whole output contract is stdout and stderr: the rendered bytes go
// to a file or to stdout, and progress goes to stderr. `print_stdout` is a
// warning aimed at libraries that log where a caller cannot intercept it, which
// does not describe a program whose stdout is the deliverable.
#![allow(
    clippy::print_stdout,
    reason = "stdout is this binary's output channel"
)]

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, ValueEnum};
use meo_canvas_core::ImageFormat;

/// Renders a `meo-canvas` scene file to an image.
#[derive(Debug, Parser)]
#[command(name = "meo-canvas", version, about)]
struct Cli {
    /// Encoded scene file to render.
    scene: PathBuf,

    /// Where to write the image. Writes to stdout when absent.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Container to encode in.
    #[arg(short, long, value_enum, default_value_t = Format::Png)]
    format: Format,

    /// Device-pixel multiplier, overriding the one the scene carries.
    #[arg(short, long)]
    scale: Option<f32>,
}

/// The formats the CLI offers, as clap spells them.
///
/// A separate enum from [`ImageFormat`] so the command-line spelling is fixed
/// here rather than derived from a library type, where renaming a variant would
/// silently rename a flag value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    /// Lossless raster with an alpha channel.
    Png,
    /// Lossy raster without one.
    Jpeg,
    /// Lossy or lossless raster with an alpha channel.
    Webp,
    /// Vector output.
    Svg,
    /// Vector output in a paged container.
    Pdf,
}

impl From<Format> for ImageFormat {
    fn from(value: Format) -> Self {
        match value {
            Format::Png => Self::Png,
            Format::Jpeg => Self::Jpeg,
            Format::Webp => Self::Webp,
            Format::Svg => Self::Svg,
            Format::Pdf => Self::Pdf,
        }
    }
}

/// Reads the scene, renders it, and writes the result.
///
/// # Errors
///
/// Returns the failure as a message already fit to print: the scene file cannot
/// be read, the buffer is not a scene this revision reads, or a render pass
/// fails.
fn run(_cli: &Cli) -> Result<(), String> {
    unimplemented!()
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("meo-canvas: {message}");
            ExitCode::FAILURE
        }
    }
}
