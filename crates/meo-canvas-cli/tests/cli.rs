//! Runs the binary and checks what a script would see: `exit_code_for`'s unit
//! test covers the mapping, and only spawning the process checks the path from
//! argv to status. `CARGO_BIN_EXE_meo-canvas` is the binary this test run
//! compiled.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use meo_canvas_scene::{
    Length, OnImageError, Scene, Size,
    node::{ImageSource, Node, NodeKind},
    style::paint::ObjectFit,
};

/// The exit codes `main.rs` documents.
const EXIT_IO: i32 = 3;
const EXIT_MALFORMED_SCENE: i32 = 4;
const EXIT_FONT: i32 = 5;
const EXIT_SOURCE_UNOBTAINABLE: i32 = 6;

/// A directory of this test run's own, so two tests cannot collide on a name.
fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("meo-canvas-cli-{name}"));
    drop(std::fs::remove_dir_all(&path));
    std::fs::create_dir_all(&path)
        .unwrap_or_else(|error| unreachable!("{error}"));
    path
}

/// Writes an encoded one-page scene and returns its path.
fn write_scene(dir: &Path) -> PathBuf {
    let scene = Scene::new(Size::new(6.0, 3.0));
    let path = dir.join("scene.mcs");
    std::fs::write(&path, meo_canvas_scene::codec::encode(&scene))
        .unwrap_or_else(|error| unreachable!("{error}"));
    path
}

/// Runs the binary and returns its status code and stderr.
fn run(args: &[&str]) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_meo-canvas"))
        .args(args)
        .output()
        .unwrap_or_else(|error| unreachable!("cannot run the binary: {error}"));

    (
        output.status.code().unwrap_or_else(|| {
            unreachable!("the process was killed by a signal")
        }),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn a_scene_renders_to_the_named_file_and_exits_zero() {
    let dir = scratch("renders");
    let scene = write_scene(&dir);
    let out = dir.join("out.png");

    let (code, stderr) = run(&[
        "render",
        &scene.to_string_lossy(),
        "-o",
        &out.to_string_lossy(),
    ]);

    assert_eq!(code, 0, "{stderr}");

    // Decoded rather than merely present: a zero-byte file also exists.
    let bytes =
        std::fs::read(&out).unwrap_or_else(|error| unreachable!("{error}"));
    let reader = png::Decoder::new(std::io::Cursor::new(&bytes))
        .read_info()
        .unwrap_or_else(|error| {
            unreachable!("the output is not a PNG: {error}")
        });
    assert_eq!((reader.info().width, reader.info().height), (6, 3));
}

#[test]
fn the_format_comes_from_the_output_extension() {
    let dir = scratch("infers");
    let scene = write_scene(&dir);
    let out = dir.join("out.webp");

    let (code, stderr) = run(&[
        "render",
        &scene.to_string_lossy(),
        "-o",
        &out.to_string_lossy(),
    ]);

    assert_eq!(code, 0, "{stderr}");
    // WebP's container magic, which a PNG written under a `.webp` name lacks.
    let bytes =
        std::fs::read(&out).unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
}

#[test]
fn bytes_that_are_not_a_scene_exit_four() {
    let dir = scratch("malformed");
    let scene = dir.join("scene.mcs");
    std::fs::write(&scene, b"not a scene at all")
        .unwrap_or_else(|error| unreachable!("{error}"));

    let (code, stderr) = run(&[
        "render",
        &scene.to_string_lossy(),
        "-o",
        &dir.join("out.png").to_string_lossy(),
    ]);

    assert_eq!(code, EXIT_MALFORMED_SCENE, "{stderr}");
}

#[test]
fn a_font_that_is_not_there_exits_five() {
    let dir = scratch("font");
    let scene = write_scene(&dir);

    let (code, stderr) = run(&[
        "render",
        &scene.to_string_lossy(),
        "-o",
        &dir.join("out.png").to_string_lossy(),
        "--font",
        "Inter=/no-such-directory/Inter.ttf",
    ]);

    assert_eq!(code, EXIT_FONT, "{stderr}");
}

#[test]
fn a_font_without_a_family_exits_five_and_says_the_shape() {
    let dir = scratch("font-shape");
    let scene = write_scene(&dir);

    let (code, stderr) = run(&[
        "render",
        &scene.to_string_lossy(),
        "-o",
        &dir.join("out.png").to_string_lossy(),
        "--font",
        "/fonts/Inter.ttf",
    ]);

    assert_eq!(code, EXIT_FONT);
    assert!(stderr.contains("family=path"), "{stderr}");
}

#[test]
fn nothing_naming_a_format_exits_three() {
    let dir = scratch("no-format");
    let scene = write_scene(&dir);

    let (code, stderr) = run(&[
        "render",
        &scene.to_string_lossy(),
        "-o",
        &dir.join("out").to_string_lossy(),
    ]);

    assert_eq!(code, EXIT_IO, "{stderr}");
}

#[test]
fn a_frame_rate_on_a_still_format_is_refused() {
    let dir = scratch("fps");
    let scene = write_scene(&dir);

    let (code, stderr) = run(&[
        "render",
        &scene.to_string_lossy(),
        "-o",
        &dir.join("out.png").to_string_lossy(),
        "--fps",
        "24",
    ]);

    assert_ne!(code, 0);
    assert!(stderr.contains("frame timing"), "{stderr}");
}

#[test]
fn a_misspelled_flag_is_claps_exit_two() {
    let dir = scratch("usage");
    let scene = write_scene(&dir);

    let (code, _stderr) =
        run(&["render", &scene.to_string_lossy(), "--nonsense"]);

    assert_eq!(code, 2);
}

#[test]
fn the_output_goes_to_stdout_when_no_file_is_named() {
    let dir = scratch("stdout");
    let scene = write_scene(&dir);

    let output = Command::new(env!("CARGO_BIN_EXE_meo-canvas"))
        .args(["render", &scene.to_string_lossy(), "--format", "png"])
        .output()
        .unwrap_or_else(|error| unreachable!("{error}"));

    assert!(output.status.success());
    let reader = png::Decoder::new(std::io::Cursor::new(&output.stdout))
        .read_info()
        .unwrap_or_else(|error| unreachable!("stdout is not a PNG: {error}"));
    assert_eq!((reader.info().width, reader.info().height), (6, 3));
}

/// Writes a scene whose one image names `url` and whose policy is `Throw`,
/// so a failed source ends the render rather than drawing a placeholder.
fn write_url_scene(dir: &Path, url: &str) -> PathBuf {
    let mut scene = Scene::new(Size::new(4.0, 4.0));
    scene.on_image_error = OnImageError::Throw;
    let page = scene
        .root()
        .unwrap_or_else(|| unreachable!("a fresh scene has a page"));
    scene
        .push(
            page,
            Node::new(NodeKind::Image {
                source: ImageSource::url(url.to_owned()),
                fit: ObjectFit::Contain,
                position: (Length::Percent(0.5), Length::Percent(0.5)),
                frame: None,
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));

    let path = dir.join("scene.mcs");
    std::fs::write(&path, meo_canvas_scene::codec::encode(&scene))
        .unwrap_or_else(|error| unreachable!("{error}"));
    path
}

#[cfg(not(feature = "net"))]
#[test]
fn without_net_a_url_source_exits_six_and_names_the_feature() {
    // An exit code alone leaves the caller guessing between a bad URL and the
    // wrong build, so the message says which build would fetch it.
    let dir = scratch("url");
    let path = write_url_scene(&dir, "https://example.invalid/a.png");

    let (code, stderr) = run(&[
        "render",
        &path.to_string_lossy(),
        "-o",
        &dir.join("out.png").to_string_lossy(),
    ]);

    assert_eq!(code, EXIT_SOURCE_UNOBTAINABLE, "{stderr}");
    assert!(stderr.contains("--features net"), "{stderr}");
}

#[cfg(feature = "net")]
#[test]
fn with_net_a_url_source_is_fetched_rather_than_refused() {
    // Port 1 refuses a connection at once, with no DNS involved, so this is a
    // fetch that failed rather than one never attempted. A build whose `net`
    // does not reach the core refuses with "this build cannot fetch" instead.
    let dir = scratch("url-net");
    let path = write_url_scene(&dir, "http://127.0.0.1:1/never.png");

    let (code, stderr) = run(&[
        "render",
        &path.to_string_lossy(),
        "-o",
        &dir.join("out.png").to_string_lossy(),
    ]);

    assert!(
        !stderr.contains("this build cannot fetch"),
        "a `net` build refused to try: {stderr}"
    );
    assert!(
        stderr.contains("127.0.0.1:1"),
        "the failure does not name the URL it tried: {stderr}"
    );
    assert_eq!(code, EXIT_SOURCE_UNOBTAINABLE, "{stderr}");
}
