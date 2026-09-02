# meo-canvas

meo-canvas turns a declarative scene description into a rendered image, laying it out with CSS flexbox, grid and block rules and drawing it on Skia.

## Two surfaces, one renderer

The same core does the work either way you reach it.

- **Rust** — `meo-canvas`, a library crate. Build a scene, render it, encode it.
- **Node.js** — `@l7aromeo/meo-canvas`, a native addon over the same core. The binary ships in a package of its own per platform, so an install downloads the one it can run.

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
npm install @l7aromeo/meo-canvas
```

The scope is what lets this be installed beside the 9.x line, which holds the
unscoped `meo-canvas` name: npm resolves one directory per package name, so two
majors can only coexist under two names.

## Usage

This section holds usage examples. It is empty while the API they would show is still moving, because an example that does not compile is worse than none.

## Licence

MIT. See [LICENSE](LICENSE).
