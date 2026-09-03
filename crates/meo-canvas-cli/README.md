# `meo-canvas-cli`

Renders an encoded scene file to an image.

## Usage

```text
meo-canvas scene.mcs --format png --output out.png
```

Install it with `cargo install meo-canvas-cli`. The binary is named
`meo-canvas`; the library crate of that name is the Rust API, and the two are
separate things that share a word.

The input is the binary wire format [`meo-canvas-scene`] encodes — the same
bytes the Node addon hands the renderer, so anything that can write a scene can
drive this.

Build with `--features net` to resolve remote image URLs through a blocking
client. Without it, a URL in a scene is an error, the same as for a Rust caller.

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
