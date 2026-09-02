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
npm install meo-canvas
```

Requires Node 22 or newer. The package is ESM only.

**The scope is deliberate.** The unscoped `meo-canvas` is the 9.x line, and npm
resolves one directory per package name, so the two can only be installed beside
each other under two names. Nothing here replaces 9.x until you replace it.

The renderer itself is a native addon of about 51 MB, and it is **not** in this
package. One package per platform carries one binary, named in
`optionalDependencies` with its own `os` and `cpu`, so an install downloads the
one it can run and skips the rest. Nothing is fetched by a postinstall script,
which is what keeps offline and `--ignore-scripts` installs working.

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
