# meo-canvas

meo-canvas turns a declarative scene description into a rendered image, laying it out with CSS flexbox, grid and block rules and drawing it on Skia.

## Two surfaces, one renderer

The same core does the work either way you reach it.

- **Rust** — `meo-canvas`, a library crate. Build a scene, render it, encode it.
- **Node.js** — `meo-canvas`, a native addon over the same core. The binary ships in a package of its own per platform, so an install downloads the one it can run.

Layout, text shaping, painting and encoding all happen in Rust. The Node surface describes a scene and asks for pixels; nothing is drawn in JavaScript.

## What it renders

- **Layout** — flexbox, CSS grid and block, with margins, padding, borders, gaps and absolute positioning. A canvas takes its height from its content unless one is given; a width is always stated, because text breaks its lines against it.
- **Text** — shaped by Skia and broken into lines here, with per-span styling, letter and word spacing, decorations, line clamping and ellipsis.
- **Images** — from a file or a buffer, with object-fit and object-position placement. A URL is resolved to bytes by the Node surface before rendering, and by the Rust crate or the CLI only when built with the optional `net` feature; without it a URL is an error and no HTTP stack is linked.
- **Paths** — arbitrary shapes from SVG path data, filled and stroked, with an optional `viewBox` so a path scales to the box that holds it.
- **Charts** — bar, line, pie and doughnut.
- **Effects** — gradients, masks, shadows, opacity groups, blend modes and CSS filters.
- **Export** — PNG, JPEG, WebP, AVIF, TIFF, BMP, ICO, SVG, PDF, GIF, APNG and raw pixels.

Multi-page renders produce frames for GIF, APNG, WebP and AVIF, sheets for PDF and TIFF, and sizes for ICO — and ICO is the only one whose pages may differ in size.

## What it computes

Nothing here draws. These are pure functions a caller uses to work out _what_ to
draw, and they exist on both surfaces.

- **Easing** — the CSS catalogue, `cubic-bezier` and `steps`.
- **Springs** — a damped spring solved in closed form, so any frame can be
  evaluated on its own, plus the settling time to size a render by.
- **Interpolation** — numbers, colours and keyframe tracks, and the tracks and
  sequences that drive them over time.
- **Colour** — one CSS parser behind both surfaces, so a string the renderer
  accepts is a string the animation helpers accept.

## Installation

Rust:

```text
cargo add meo-canvas
```

Node.js:

```text
npm install meo-canvas@next
```

**The `@next` is not optional while 10.x is in prerelease.** 10.x continues v1's
lineage under the same `meo-canvas` name, and npm resolves `latest` for a bare
install — so `npm install meo-canvas` gives you the 9.x line. Every 10.x
prerelease carries a hyphen, which keeps it off `latest` and out of any semver
range.

npm resolves one directory per package name, so installing both majors at once
is done with an alias, and the consumer picks the local name rather than us
picking it for them:

```text
npm install meo-canvas meo-canvas-v10@npm:meo-canvas@next
```

## Usage

The examples are the documentation, because they are the only form of it this
repository can check. `examples/bun` and `examples/rust` hold the same nine
scenes twice — block, flex and grid layout, text, images, paths, paint,
positioning and multi-page output — and `just example` renders both and
**compares the bytes**. A scene that drifts on one surface fails against the
other.

```text
just example
```

Doc comments carry runnable snippets besides: the Rust ones are doctests that
`just docs` runs, and the TypeScript ones are lifted into a generated file that
`just typecheck` covers, so a renamed property fails a gate rather than sitting
in a comment.

Nothing checks a fenced block in a README, which is why there is not one here.

## Licence

MIT. See [LICENSE](LICENSE).
