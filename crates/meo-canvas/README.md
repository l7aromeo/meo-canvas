# `meo-canvas`

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/l7aromeo/meo-canvas/main/docs/assets/brand/banner-dark.webp" />
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/l7aromeo/meo-canvas/main/docs/assets/brand/banner-light.webp" />
  <img src="https://raw.githubusercontent.com/l7aromeo/meo-canvas/main/docs/assets/brand/banner.webp" alt="meo-canvas — four easing curves animating, each drawn by the library itself" width="1280" />
</picture>

Describe a picture; get image bytes back. CSS flexbox, grid and block layout on
a Skia backend.

The public surface: struct-literal options closed with `..Default::default()`,
no Skia or taffy types anywhere, and no async runtime imposed on the caller.

```sh
cargo add meo-canvas@0.1.0-alpha.2
```

The version is named because this is a prerelease and `cargo add` will not
resolve one on its own -- without it, a crate that plainly exists on crates.io
answers that no version matches.

## A picture

```rust
use meo_canvas::{Renderer, Root, Row, Styled, Text, hex, px};

let renderer = Renderer::new();

let mut canvas = Root::new(520.0)
    .height(180.0)
    .background_color(hex("#101014"))
    .children(
        Row::new()
            .gap(px(20.0))
            .padding(px(24.0))
            .children(Text::new("Ukasyah").font_size(26.0).bold()),
    )
    .render(&renderer)?;

canvas.to_file("out.png")?;
```

Setters are flat and chained, and they come from [`Styled`], which every node
implements — so importing that trait is what makes `background_color` and
`gap` available. `Root::new` takes the width; the height is a setter, because
Rust has no optional argument and a page that sizes itself to its content is
the more common case.

**This example is compiled.** It lives at `examples/readme.rs` and is built by
the same `cargo clippy --all-targets` the rest of the tree is gated with, so
a README that stops compiling fails a run rather than a reader.

## What it writes

`to_file` takes the format from the extension; `to_file_as` takes it as an
argument, and `to_buffer` hands back bytes.

```text
raster      png  jpeg  webp  avif  bmp  ico  tiff
animated    gif  apng
vector      svg  pdf
```

## Features

All three are off by default. A build that names none of them renders on the
CPU, reads local paths and inline bytes, and links no HTTP stack.

```text
metal    GPU rasterising on Apple platforms
vulkan   GPU rasterising elsewhere
net      resolve an ImageSource::Url over HTTP
```

`metal` and `vulkan` are alternatives rather than additions: naming both builds,
and Metal wins, because the overlap is only reachable on macOS where Vulkan
runs through MoltenVK anyway. `Canvas::engine` reports which one actually drew,
since asking for the GPU is not the same as getting it.

`net` is the same shape as those two and not a smaller surface than the npm
package has. There the addon is a prebuilt binary with the HTTP client already
inside it, so the capability costs its caller nothing; here you compile it, so
it costs dependencies, build time and audit surface. Identical capability,
different price, and the flag is what lets the one who pays decide. Without it
an `ImageSource::Url` is an error naming the feature; a caller who would rather
fetch for themselves passes `ImageSource::Bytes` and needs none of this.

## System libraries

Unlike the npm package, which ships a binary with freetype and fontconfig
linked statically, a crate is built on the machine that uses it and links the
system copies. On Linux that means:

```text
Debian/Ubuntu   libfontconfig1 libfreetype6   (build: libfontconfig1-dev libfreetype-dev pkg-config)
RHEL/Alma/Rocky fontconfig freetype           (build: fontconfig-devel freetype-devel pkg-config)
```

`pkg-config` is the one to get right. Skia is built here without
`embed-freetype`, so `rust-skia` probes pkg-config for both libraries and
**falls back to bare library names when the probe fails, silently** — the error
you get is `unable to find library -lfreetype`, which points at freetype rather
than at the missing prober.

`cmake` and `nasm` are also needed at build time, for libaom.

macOS and Windows need none of this: Skia uses CoreText and DirectWrite there.

## Reference

The API documentation is on [docs.rs](https://docs.rs/meo-canvas), and the
reasoning behind the surface — why the setters are flat, why `Root` is built
rather than constructed, what each divergence from CSS costs — is in
[`AGENTS.md`](https://github.com/l7aromeo/meo-canvas/blob/main/AGENTS.md) in
the repository.

The npm package is the same renderer behind a prebuilt binary, for callers who
would rather not compile Skia: [`meo-canvas`](https://www.npmjs.com/package/meo-canvas).

## Licence

MIT.
