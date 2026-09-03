# meo-canvas

Server-side image generation for Node. Describe a layout the way you would describe a page — boxes, rows, text, images, paths, charts — and get back encoded bytes.

## The canvas is as tall as what is in it

`height` is optional. Leave it out and the page is the height of its content,
the way it is in 9.x; `minHeight` is the floor when you want "at least this
tall". A width is always stated, and cannot be otherwise: text breaks into lines
against a width, so a width has to be known before anything can be measured.

```js
Root({ width: 520, children }) // as tall as the content
Root({ width: 520, minHeight: 200, children }) // ...and at least 200
Root({ width: 520, height: 180, children }) // exactly 180
```

## Nothing is drawn in JavaScript

This package is a thin surface over a native addon. Your calls describe a scene; layout, text shaping, painting and encoding all happen in Rust, and the whole description crosses into it once per render rather than once per drawing call.

What that buys you is that a scene of any size costs one crossing, and that the drawing itself runs at native speed with no per-call boundary tax.

## Installation

```text
npm install meo-canvas@next
```

**The `@next` is not optional.** `meo-canvas@latest` is the 9.x line, and npm
resolves `latest` for a bare install — so `npm install meo-canvas` gives you 9.x
today and will keep doing so until 10 is out of prerelease. Every 10.x
prerelease carries a hyphen in its version, which is what keeps it off `latest`
and out of any semver range: nothing reaches it without asking for it by name.

To run 9.x and 10.x side by side, alias one of them. npm resolves one directory
per package name, so two names is what it takes:

```text
npm install meo-canvas meo-canvas-v10@npm:meo-canvas@next
```

Requires Node 22 or newer. The package is ESM only.

### Platforms

The renderer is a native addon of about 51 MB, and it is **not** in this
package. One package per platform carries one binary, named in
`optionalDependencies` with its own `os`, `cpu` and `libc`, so an install
downloads the one it can run and skips the rest. Nothing is fetched by a
postinstall script, which is what keeps offline and `--ignore-scripts` installs
working.

| Platform      | Architecture | Requires                                      |
| ------------- | ------------ | --------------------------------------------- |
| Linux (glibc) | x64, arm64   | glibc 2.28 or newer                           |
| Linux (musl)  | x64, arm64   | Alpine 3.x, or any musl host with `libstdc++` |
| macOS         | arm64        | macOS 11 or newer                             |
| Windows       | x64          | —                                             |

**The glibc floor is 2.28, and it is measured rather than declared.** That is
AlmaLinux 8 and RHEL 8, which is older than every currently supported
distribution: Ubuntu 20.04 is 2.31, RHEL 9 and Amazon Linux 2023 are 2.34,
Ubuntu 22.04 is 2.35, Debian 12 is 2.36. What it excludes has already reached
end of life — RHEL 7, Ubuntu 18.04, Debian 9. Each release asserts the built
binary demands no more than this and loads on Alma 8, Rocky 9, Amazon Linux
2023, Debian 12, `node:22-slim` and `node:22-alpine` with no font packages
installed.

**No system libraries are needed.** freetype and fontconfig are linked
statically, so a slim container image needs nothing added to it. The musl
binaries do need `libstdc++`, which a bare Alpine image lacks — but any image
that can run Node already has it, since Node links it too.

macOS x64 and 32-bit targets are not built. Apple has ended support for Intel
Macs, and no 32-bit target has been asked for.

## Usage

This section holds usage examples. It is empty while the API they would show is still moving, because an example that does not run is worse than none.

## What runs in JavaScript

Two things, and both compute rather than draw.

**The animation helpers**: easings, springs, interpolation and colour. The value
they produce goes into a scene like any other number. **The colour parser is the
renderer's own**, reached through the addon, so a string that renders is a
string that animates.

**`Chart`**, which turns data into boxes and paths — bar widths, slice angles,
gridline positions, the line series' own path data. It emits a scene node and
nothing else; the layout and the drawing of what it emits happen in Rust like
everything else. The Rust crate has its own chart rather than a call into this
one, and the two are checked against each other by encoding the same chart and
comparing bytes.

## Types

Every enumerated value is a string-literal union, so an editor completes `'flex-start'` as you type it and the compiler rejects `'flexstart'` before anything renders.

## Licence

MIT. See [LICENSE](LICENSE).
