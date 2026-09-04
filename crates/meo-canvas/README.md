# `meo-canvas`

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/l7aromeo/meo-canvas/v10/docs/assets/brand/banner-dark.webp" />
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/l7aromeo/meo-canvas/v10/docs/assets/brand/banner-light.webp" />
  <img src="https://raw.githubusercontent.com/l7aromeo/meo-canvas/v10/docs/assets/brand/banner.webp" alt="meo-canvas — four easing curves animating, each drawn by the library itself" width="1280" />
</picture>

Describe a picture; get image bytes back. CSS flexbox, grid and block layout on
a Skia backend.

The public surface: struct-literal options closed with `..Default::default()`,
no Skia or taffy types anywhere, and no async runtime imposed on the caller.

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
