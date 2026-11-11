# `meo-canvas-core`

The rendering pipeline: resolve, measure, layout, paint, encode.

Wraps taffy for layout and `meo-skia-canvas` for drawing, and exposes neither in
a public signature.

**Runtime-free always, and fetch-free by default.** Without the `net` feature an
unresolved URL is an error rather than a network call. With it, a URL is fetched
through a blocking client — no async runtime either way, which is the half of
the rule that does not bend.

## Usage

Take a [`Scene`] and get bytes back.

```rust,no_run
use meo_canvas_core::{EncodeOptions, ImageFormat, Renderer};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::Dimension,
};

let mut scene = Scene::new(Size::new(200.0, 120.0));
let box_id = scene.push(NodeId::ROOT, Node::new(NodeKind::Box))?;
if let Some(node) = scene.get_mut(box_id) {
    node.layout.size = (Dimension::Points(80.0), Dimension::Points(40.0));
}

let renderer = Renderer::new();
let png = renderer.render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())?;
std::fs::write("out.png", png)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Building

A build needs `SKIA_BINARIES_URL` in `.cargo/config.toml`'s `[env]`, the line
[`meo-canvas`]'s README gives: this crate links meo-skia-canvas's patched Skia,
downloaded from the archives it names, and a build without it stops and prints
the line.

## System libraries

This crate links Skia, so on Linux it needs fontconfig from the system — the
npm package ships a binary with it already inside, and a Rust build does not.
FreeType is inside the downloaded Skia.

```text
Debian/Ubuntu   libfontconfig1   (build: libfontconfig1-dev pkg-config)
RHEL/Alma/Rocky fontconfig       (build: fontconfig-devel pkg-config)
```

`pkg-config` is the one to get right. `rust-skia` probes it for fontconfig and
**falls back to a bare library name when the probe fails, silently** — the error
you get is `unable to find library -lfontconfig`, which points at fontconfig
rather than at the missing prober.

`cmake` is also needed at build time, for libaom, and on x86_64 an assembler:
`yasm`, or `nasm` older than 3, whose help text libaom 3.11 cannot read.

macOS and Windows need none of this: Skia uses CoreText and DirectWrite there.

[`Scene`]: https://crates.io/crates/meo-canvas-scene
[`meo-canvas`]: https://crates.io/crates/meo-canvas
