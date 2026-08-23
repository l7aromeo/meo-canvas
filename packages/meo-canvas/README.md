# meo-canvas

Server-side image generation for Node. Describe a layout the way you would describe a page — boxes, rows, text, images, paths, charts — and get back encoded bytes.

## Nothing is drawn in JavaScript

This package is a thin surface over a native addon. Your calls describe a scene; layout, text shaping, painting and encoding all happen in Rust, and the whole description crosses into it once per render rather than once per drawing call.

What that buys you is that a scene of any size costs one crossing, and that the drawing itself runs at native speed with no per-call boundary tax.

## Installation

```text
npm install meo-canvas
```

Requires Node 22 or newer. The package is ESM only.

## Usage

This section holds usage examples. It is empty while the API they would show is still moving, because an example that does not run is worse than none.

## What runs in JavaScript

The animation helpers, and only those: easings, springs, interpolation and
colour. They compute values rather than drawing them, and the value goes into a
scene like any other number. **The colour parser is the renderer's own**, reached
through the addon, so a string that renders is a string that animates.

## Types

Every enumerated value is a string-literal union, so an editor completes `'flex-start'` as you type it and the compiler rejects `'flexstart'` before anything renders.

## Licence

MIT. See [LICENSE](LICENSE).
