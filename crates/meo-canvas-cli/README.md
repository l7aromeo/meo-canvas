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

## Encoding

`--quality` is the lossy quality from 0.0 to 1.0, read by JPEG, WebP and AVIF;
left out, each format takes its own default. A WebP at 1.0 is lossless, an AVIF
at 1.0 is its finest lossy one.

`--lossless` writes a lossless WebP, still or animated, or a lossless AVIF.
`--lossless=false` writes a lossy WebP at any quality, its finest at
`--quality 1`, and leaving the flag out leaves a WebP to the quality. The value
is taken only after `=`, so a bare `--lossless` before the scene path never
reads the path as its value. `--lossless` beside a quality below 1.0 is refused,
and so is `--lossless` for any other format.

## Warnings

An image URL the render could not obtain is reported on stderr, one line per
source, whether the scene's `on_image_error` draws a placeholder for it or
nothing:

```text
meo-canvas: warning: node=1 nodes=1 failure=status status=404 url=https://example.invalid/a.png detail=404 Not Found
```

The fields always come in that order, `status` only when `failure` is `status`.
`node` is the first node that named the source and `nodes` how many nodes in
the scene named it, since one source failing is one line however often it is
drawn. `failure` is one of `status`, `host-not-found`, `bad-url`, `transport`,
`too-large` or `other`, the words the npm package uses for
`ImageWarning.failure`.

Every field before `detail` is one token: a space, tab, carriage return or line
feed in the URL is written percent-encoded (`%20`, `%09`, `%0D`, `%0A`). `detail`
is free text and runs to the end of the line, with any carriage
return or line feed in it written as a space. So a script splits the line on
spaces up to `detail=` and takes the rest as the detail.

The exit code stays 0, since the image was written; a scene whose
`on_image_error` is `Throw` fails instead, with exit code 6.

Only URL images warn: a URL in a `net` build, or one whose failed fetch the
scene records in `image_fetch_attempts`. A missing image file is an error with
exit code 3 whatever the policy.

## Diagnostics

A value in the scene the renderer could not use, and went on without, is
reported on stderr as one line, in the shape of a warning:

```text
meo-canvas: diagnostic: path=filter detail=node 0: "blur(4 px)" is not a filter chain the renderer draws (...), so the node is drawn without it
```

Today that is a `filter` or `backdrop_filter` chain the renderer refuses, as
Chrome refuses it: the node is drawn as though the property were not set. The
exit code stays 0. Markup diagnostics are returned to whoever builds the scene,
since a scene file carries text already parsed.

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
prober.

`cmake` is also needed at build time, for libaom, and on x86_64 an assembler:
`yasm`, or `nasm` older than 3, whose help text libaom 3.11 cannot read.

macOS and Windows need none of this: Skia uses CoreText and DirectWrite there.

[`meo-canvas-scene`]: https://crates.io/crates/meo-canvas-scene
[`meo-canvas-core`]: https://crates.io/crates/meo-canvas-core
