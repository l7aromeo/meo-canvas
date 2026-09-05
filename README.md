# meo-canvas

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/l7aromeo/meo-canvas/v10/docs/assets/brand/banner-dark.webp" />
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/l7aromeo/meo-canvas/v10/docs/assets/brand/banner-light.webp" />
  <img src="https://raw.githubusercontent.com/l7aromeo/meo-canvas/v10/docs/assets/brand/banner.webp" alt="meo-canvas — four easing curves animating, each drawn by the library itself" width="1280" />
</picture>

meo-canvas turns a declarative scene description into a rendered image, laying it out with CSS flexbox, grid and block rules and drawing it on Skia.

## Two surfaces, one renderer

The same core does the work either way you reach it.

- **Rust** — `meo-canvas`, a library crate. Build a scene, render it, encode it.
- **Node.js** — `meo-canvas`, a native addon over the same core. The binary ships in a package of its own per platform, so an install downloads the one it can run.

Layout, text shaping, painting and encoding all happen in Rust. The Node surface describes a scene and asks for pixels; nothing is drawn in JavaScript.

## Documentation

The JavaScript API reference is at **<https://l7aromeo.github.io/meo-canvas/>**,
one directory per published version, generated from the type declarations each
release ships. `latest/` follows the newest **stable** release the way npm's
`latest` dist-tag does — so while 10.x is in prerelease there is no `latest/`
and the index says so.

The Rust reference will be on docs.rs once the crate is published; it is not
yet, so there is deliberately no link here rather than a dead one.

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

```ts
import { writeFileSync } from 'node:fs'
import { Box, Column, Root, Text } from 'meo-canvas'

const canvas = await Root({
  width: 320,
  padding: 20,
  backgroundColor: '#101820',
  children: Column({
    gap: 10,
    children: [
      Text('meo-canvas', { fontSize: 22, fontWeight: 600, color: '#f2aa4c' }),
      Text('Describe a layout; get image bytes back.', { fontSize: 13, color: '#c8ccd4' }),
      Box({ height: 4, width: 64, backgroundColor: '#f2aa4c', borderRadius: 2 }),
    ],
  }),
})

writeFileSync('card.png', await canvas.toBuffer('png'))
canvas.release()
```

The same picture from Rust. Properties are named on the node, flat, exactly as
they are above; a new property is a new entry in one table rather than a method
on seven node types:

```rust,no_run
use meo_canvas::{
    Box, Column, Format, Renderer, Root, Styled, Text, all, hex_rgb, px,
};

let card = Column::new().gap(px(10.0)).children([
    Text::new("meo-canvas")
        .font_size(22.0)
        .bold()
        .color(hex_rgb(0xf2_aa_4c)),
    Text::new("Describe a layout; get image bytes back.")
        .font_size(13.0)
        .color(hex_rgb(0xc8_cc_d4)),
    Box::new()
        .size(px(64.0), px(4.0))
        .background_color(hex_rgb(0xf2_aa_4c)),
]);

let mut canvas = Root::new(320.0)
    .padding(all(px(20.0)))
    .background_color(hex_rgb(0x10_18_20))
    .children([card])
    .render(&Renderer::new())?;

std::fs::write("card.png", canvas.to_buffer(Format::Png)?)?;
# // `Box` here is the node, so the heap allocation is spelled in full.
# Ok::<(), std::boxed::Box<dyn std::error::Error>>(())
```

A base many nodes share is a `Style` value instead, applied with `with_style`,
which merges — what the style names wins, what it leaves absent the node keeps:

```rust
use meo_canvas::{Row, Style, Styled, all, hex_rgb, px};

const CARD: Style = Style::new().padding(all(px(24.0))).gap(px(16.0));

let dark = Row::new().with_style(CARD).background_color(hex_rgb(0x10_10_14));
```

**The examples are the rest of the documentation, because they are the part this
repository can check.** `examples/bun` and `examples/rust` hold the same nine
scenes twice — block, flex and grid layout, text, images, paths, paint,
positioning and multi-page output — and `just example` renders both and
**compares the bytes**, so a scene that drifts on one surface fails against the
other.

```text
just example
```

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

## Licence

MIT. See [LICENSE](LICENSE).
