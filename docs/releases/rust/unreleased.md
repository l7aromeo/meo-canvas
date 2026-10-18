### Added

- `frame` takes a negative index, counting from the end: `.frame(-1)` draws the last frame of an animated image and `.frame(-2)` the one before it. A source with one frame ignores a negative index as it does a positive one.

### Changed

- **Breaking.** `Style::frame`, the `frame` setter and `NodeKind::Image::frame` take an `i32` rather than a `u32`, so that a negative index can count from the end. An integer literal compiles unchanged; a `u32` value needs converting, for example with `i32::try_from`.

- **Breaking.** The scene file format is version 8, and a `.mcs` file written by 0.1.x is refused with `CodecError::UnsupportedVersion` ("scene format version 7, expected 8"). Re-encode the scene with this release.

- A frame index past either end of an animated image is refused as `Error::FrameOutOfRange`, naming the index as written and the frame count ("node N asks for frame F of an image with M frames"), rather than as `Error::UndecodableImage`.

- Building from source on x86_64 needs `yasm`, or `nasm` older than 3, for libaom: libaom 3.11 refuses the nasm 3.01 that Ubuntu 26.04 ships, with "Unsupported nasm: multipass optimization not supported". `cmake` is still needed as before.

- **`rustls` is required at `^0.23.46`** under the optional `net` feature, closing four client-side advisories: an unauthenticated ServerHello downgrading later connections to classical key exchange (`RUSTSEC-2026-0349`), `Debug` output of a client session value exposing its secrets (`-0346`), buffered data sent after an Encrypted Client Hello rejection (`-0347`), and certificate decompression allocating up to 1 GiB before authentication under rustls's `brotli` feature (`-0348`). `ureq` requires `^0.23.22`, so a consumer could resolve an affected version even though this repository's lockfile could not. The requirement was `>=0.23.45`, which a 0.24 release would have satisfied without raising the copy `ureq` uses.

### Fixed

- **`--features net` builds of `meo-canvas-cli` still refused URL images.**
  The feature compiled an HTTP client the binary never called and left
  fetching off in `meo-canvas-core`, so every `ImageSource::Url` failed as
  though the feature were absent, and in exactly that build the message
  dropped its hint to build with `--features net`. The feature now enables the
  core's fetch, so a URL image is fetched. A fetch that fails, under
  `on_image_error: Throw`, exits 6, source unobtainable, and names the URL.

- With the `net` feature, an image URL whose host does not resolve is reported as `FetchFailure::HostNotFound` (do not retry) rather than `FetchFailure::Transport` (retry), in `Error::SourceFetch` and in `ImageWarning` alike. A lookup that could not finish, because the resolver was unreachable or said to try again, stays `Transport`.

- An SVG image given `frame(1)` or later now draws, as a still raster does, rather than being refused as `Error::UndecodableImage`.

- The CLI README's usage line omitted the `render` subcommand, so the command it showed was rejected with "unrecognized subcommand".

- **`meo-canvas` exited 0 with nothing on stderr when an image URL could not be
  obtained.** A dead URL in a `net` build, or a failed fetch the scene recorded,
  was drawn as a placeholder, or as nothing, and no trace of it reached the
  caller. Each such source now prints one line on stderr,
  `meo-canvas: warning: node=… nodes=… failure=… url=… detail=…`, with
  `status=…` after `failure=status`; the exit code stays 0. The CLI README
  documents the fields.
