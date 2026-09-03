# `meo-canvas-node`

The Node.js addon. One `.node` binary, one Neon module entry point.

**Not published to crates.io** — this crate carries `publish = false`, and this
file is for someone reading the repository rather than a registry front page.
The published surface is the npm package `meo-canvas`, which ships the compiled
binary this crate produces; the Rust equivalent is [`meo-canvas`] on crates.io.

JavaScript encodes a scene into a single buffer; this crate decodes it, renders,
and returns the bytes. It holds the only `#[neon::main]` in the binary, which is
why `meo-skia-canvas` is built with its `node-addon` feature off.

## Usage

Nothing depends on this crate from Rust, and nothing can: it is `cdylib` only,
with no `rlib`, so that a Rust-only build cannot pull Neon in through it. Build
it with `just addon`, which writes the `.node` beside the npm package for the
JavaScript surface to load.

```text
just addon
```

## System libraries

This crate links Skia, so building it on Linux needs freetype and fontconfig
development packages plus `pkg-config`, and `cmake` and `nasm` for libaom — the
same set [`meo-canvas-core`] documents.

The binary the npm package _ships_ needs none of them: it is built in a
container that links the font libraries statically, so an installed
`meo-canvas` addon loads on a stock `node:22-slim` with no font packages at all.
That is a property of the release image rather than of this crate, and building
here yourself does not reproduce it.

[`meo-canvas`]: https://crates.io/crates/meo-canvas
[`meo-canvas-core`]: https://crates.io/crates/meo-canvas-core
