# meo-canvas

meo-canvas turns a declarative scene description into a rendered image, laying it out with CSS flexbox, grid and block rules and drawing it on Skia.

## Two surfaces, one renderer

The same core does the work either way you reach it.

- **Rust** — `meo-canvas`, a library crate. Build a scene, render it, encode it.
- **Node.js** — `meo-canvas`, a native addon over the same core, shipped as one compiled binary.

Layout, text shaping, painting and encoding all happen in Rust. The Node surface describes a scene and asks for pixels; nothing is drawn in JavaScript.

## What it renders

- **Layout** — flexbox, CSS grid and block, with margins, padding, borders, gaps and absolute positioning.
- **Text** — shaped and broken by Skia's paragraph engine, with per-span styling, letter and word spacing, decorations, line clamping and ellipsis.
- **Images** — from a file or a buffer, with object-fit and object-position placement. The Node surface and the CLI resolve a URL to bytes before rendering; the renderer itself performs no network I/O.
- **Paths** — arbitrary shapes from SVG path data, filled and stroked.
- **Charts** — bar, line, pie and doughnut.
- **Effects** — gradients, masks, shadows, opacity groups, blend modes and CSS filters.
- **Export** — PNG, JPEG, WebP, AVIF, TIFF, BMP, ICO, SVG, PDF, GIF, APNG and raw pixels.

Multi-page renders produce frames for GIF and APNG, sheets for PDF and TIFF, and sizes for ICO.

## Installation

Rust:

```text
cargo add meo-canvas
```

Node.js:

```text
npm install meo-canvas
```

## Usage

This section holds usage examples. It is empty while the API they would show is still moving, because an example that does not compile is worse than none.

## Licence

MIT. See [LICENSE](LICENSE).
