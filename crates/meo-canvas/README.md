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
