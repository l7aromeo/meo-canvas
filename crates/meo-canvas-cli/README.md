# `meo-canvas-cli`

Renders an encoded scene file to an image.

## Usage

```text
meo-canvas render scene.mcs --format png --output out.png
```

Install it with `cargo install meo-canvas-cli`. The binary is named
`meo-canvas`; the library crate of that name is the Rust API, and the two are
separate things that share a word.

The input is the binary wire format [`meo-canvas-scene`] encodes — the same
bytes the Node addon hands the renderer, so anything that can write a scene can
drive this.

Build with `--features net` to resolve remote image URLs through a blocking
client. Without it, a URL in a scene is an error, the same as for a Rust caller.

## Warnings

An image URL the render could not obtain is reported on stderr, one line per
source, whether the scene's `on_image_error` draws a placeholder for it or
nothing:

```text
meo-canvas: warning: node=1 nodes=1 failure=status status=404 url=https://example.invalid/a.png detail=404 Not Found
```

The fields always come in that order, `status` only when `failure` is `status`,
and `detail` runs to the end of the line. `failure` is one of `status`,
`host-not-found`, `bad-url`, `transport`, `too-large` or `other`, the words the
npm package uses for `ImageWarning.failure`. The exit code stays 0, since the
image was written; a scene whose `on_image_error` is `Throw` fails instead, with
exit code 6.

Only URL images warn: a URL in a `net` build, or one whose failed fetch the
scene records in `image_fetch_attempts`. A missing image file is an error with
exit code 3 whatever the policy. Markup diagnostics are returned to whoever
builds the scene; a scene file carries none.

## System libraries

This binary links Skia through [`meo-canvas-core`], so on Linux it needs
freetype and fontconfig from the system — at build time to link, and at run
time to load.

```text
Debian/Ubuntu   libfontconfig1 libfreetype6   (build: libfontconfig1-dev libfreetype-dev pkg-config)
RHEL/Alma/Rocky fontconfig freetype           (build: fontconfig-devel freetype-devel pkg-config)
```

`cargo install` needs the `-dev`/`-devel` packages; running what it produces
needs the runtime ones. Installing only the first set builds successfully and
then fails at load, which reads as a broken crate rather than a missing library.

`pkg-config` is the one to get right: Skia is built without `embed-freetype`, so
`rust-skia` probes pkg-config and **falls back to bare library names when the
probe fails, silently** — the error names freetype rather than the missing
prober. `cmake` and `nasm` are also needed at build time, for libaom.

macOS and Windows need none of this: Skia uses CoreText and DirectWrite there.

[`meo-canvas-scene`]: https://crates.io/crates/meo-canvas-scene
[`meo-canvas-core`]: https://crates.io/crates/meo-canvas-core
