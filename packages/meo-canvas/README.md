# meo-canvas

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/l7aromeo/meo-canvas/v10/docs/assets/brand/banner-dark.webp" />
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/l7aromeo/meo-canvas/v10/docs/assets/brand/banner-light.webp" />
  <img src="https://raw.githubusercontent.com/l7aromeo/meo-canvas/v10/docs/assets/brand/banner.webp" alt="meo-canvas — four easing curves animating, each drawn by the library itself" width="1280" />
</picture>

Server-side image generation for Node. Describe a layout the way you would describe a page — boxes, rows, text, images, paths, charts — and get back encoded bytes.

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

Requires Node 22 or newer. The package is written as ES modules and `require`
works too, through Node's own `require(esm)`, which is unflagged from 22.12.
Below that version `require` reports `ERR_REQUIRE_ESM` and `import` is the way
in.

The API reference for every published version is at
**<https://l7aromeo.github.io/meo-canvas/>**, generated from the type
declarations each release ships. `latest/` follows the newest stable release
the way npm's `latest` dist-tag does, so while 10.x is in prerelease it is
absent and the index says so — go to the version's own directory.

**Coming from v9?** Read
[MIGRATING.md](https://github.com/l7aromeo/meo-canvas/blob/v10/MIGRATING.md) first. Most calls
survive, but three keep their name and change what they do — `mixColor` above all, which takes
colour objects now and refuses a v9 string call rather than answering it.

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
| Windows       | x64, arm64   | —                                             |

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

### Bundlers

**Mark this package external.** A bundler inlines the JavaScript and cannot
follow the addon, which is resolved at run time from the platform package
rather than imported by a literal path — there is nothing static for a bundler
to trace. Bundling it anyway appears to work while the output sits beside the
`node_modules` it was built in, and fails once the bundle is copied somewhere
on its own, which is what a container image or a function archive does.

```text
esbuild app.js --bundle --platform=node --external:meo-canvas
```

Webpack takes `externals`, Rollup `external`, Vite `build.rollupOptions.external`
or `ssr.external`. The package and its platform package then travel as ordinary
dependencies, installed where the bundle runs.

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

`height` is optional, and that is the one thing worth knowing before you write
anything: leave it out and the page is as tall as its content, the way it is in
9.x. A width is always stated and cannot be otherwise, because text breaks into
lines against a width and nothing can be measured until one is known.

```ts
import { Box, Root } from 'meo-canvas'

const children = [Box({ height: 40, backgroundColor: '#f2aa4c' })]

Root({ width: 520, children }) // as tall as the content
Root({ width: 520, minHeight: 200, children }) // ...and at least 200
Root({ width: 520, height: 180, children }) // exactly 180
```

`release()` frees the native surface. Without it the bytes are still correct and
the memory is reclaimed whenever the collector gets to it, which under load is
later than you want.

More, and all of it checked: the repository holds the same nine scenes written
twice, once per surface, and `just example` renders both and compares every
byte.

## Fonts

Pass the families you need on `fonts`, and pass **the same list every time**:

```ts
import { Root, Text } from 'meo-canvas'

const FONTS = [{ family: 'Brand', paths: ['./fonts/Brand-Regular.ttf', './fonts/Brand-Bold.ttf'] }]

const canvas = await Root({
  width: 600,
  height: 400,
  fonts: FONTS,
  children: [Text('Hello', { fontFamily: 'Brand', fontSize: 24 })],
})
canvas.release()
```

**Registration is process-wide and permanent.** A family registered by one
render is registered for every render after it, nothing unregisters anything,
and giving different files under the same name changes what that name draws from
then on. A render that names a family it never registered does not fail — it
uses whatever an earlier render left behind, which means the failure is not an
error in a log but the wrong typeface in a picture nobody looks at twice.

So faces belong at start-up rather than per request. A server that registers one
tenant's face per request is a server where the next request renders in the
previous tenant's font. Registering the same list on every render is fine, and
costs nothing beyond reading the files; varying it is the hazard.

Worker threads each keep their own registry, so each has to register its own
faces and each is affected only by itself. Two renders already in flight keep
the faces they were given.

## Fetching images

An `Image` whose `src` is a URL is fetched before anything is drawn, and
`httpOptions` is passed to `fetch` for every one of them — headers, credentials,
a proxy agent, an `AbortSignal`, whatever the runtime's `fetch` accepts.

```ts
import { Image, Root } from 'meo-canvas'

const token = process.env['IMAGE_TOKEN'] ?? ''

const canvas = await Root({
  width: 600,
  httpOptions: { headers: { authorization: `Bearer ${token}` } },
  children: [Image({ src: { url: 'https://example.invalid/photo.png' } })],
})
canvas.release()
```

**Bytes cross to the renderer, never URLs**, so anything set here reaches the
origin and nothing else.

Two bounds apply whatever you pass, and they match the ones the Rust crate sets
for itself: **a fetch has 60 seconds** and **an image may not exceed 32 MiB**.
The size is counted while reading rather than taken from `content-length`, which
a server may omit and may lie about. Both refuse with a message naming the limit
as this renderer's.

**The 60 seconds is a ceiling, not a default.** An `AbortSignal` you pass is
composed with it rather than replacing it, so it can only make the wait shorter:
ask for five seconds and you get five, and nothing gets sixty-one. A bound you
could raise would be the same hang with a supported spelling. Your existing
signal keeps behaving exactly as it did, because tightening is all it could ever
have done.

If you need a different policy, fetch the bytes yourself and pass them as an
inline source — that path carries no limits of ours at all, and it is the same
escape the Rust crate offers.

One difference between the surfaces, since it is real: **you can tighten these
bounds and a Rust caller cannot.** `httpOptions` is already a public object
here, so composing with it honours a contract that exists rather than inventing
a knob; the crate has nowhere equivalent to put one. The capability is the same
on both — fetch a URL, under a bound you did not choose — and only the price of
adjusting it differs, which is the reasoning that governs the crate's `net`
feature too.

## Sizing a service

Measured on an Apple M4 Pro, macOS 26.6.2, Node v26.4.0 — the shape rather than
a specification, but the shape is what you cannot guess.

**Getting an image is two costs, and only one of them depends on the picture.**

Painting is a flat **~9 ms per render whatever it draws**: a 20×20 canvas with
one node costs what a 4000×4000 canvas costs, because painting records a drawing
rather than pixels. That is the floor, and no scene is small enough to get under
it.

Encoding is what turns the drawing into pixels, and it grows with area — so it
is the half that answers "how big". A **480×320 render takes about 13 ms end to
end, which is about 73 a second on one thread**: ~9 ms of paint, ~4 ms of encode. The
same thread does about four a second at 4000×4000, where the encode alone is
~256 ms.

**"Paint" here is the whole native call** — the arena decode, resolve, measure,
layout and the drawing. `AGENTS.md`'s benchmark table splits the same render
differently, at 2.86 ms of _drawing_ against 9.16 ms of encode, because its
`draw` is only the last of those stages. The two agree on the total and measure
different things; the roughly 6 ms between them is where the flat floor lives.

So plan with the whole render rather than the floor. **A thumbnail and a poster
cost the same to paint and nothing like the same to encode**, and the floor is
what stops small scenes being free.

### What costs and what blocks are different numbers

The times above are what a render _costs_. What it _blocks_ is smaller, and the
two stopped being the same thing when the encode moved off the event loop.

Measured by watching a 1 ms timer during each call — the sample count is the
measurement, because a fully blocked loop runs no callbacks at all:

|                             | wall clock | timer callbacks during it |
| --------------------------- | ---------- | ------------------------- |
| `Root` at any size          | ~9–12 ms   | none — fully blocked      |
| `toBuffer` at 4000×4000     | 277 ms     | 218 — free                |
| `toBufferSync` at 4000×4000 | 294 ms     | none — fully blocked      |

**`toBuffer` costs the same wall clock as `toBufferSync` and gives the loop
back**: 0–3% difference across 480², 2000² and 4000². There is no reason to
reach for the synchronous form in a server, and every reason to use it in a
script, where there is nothing else for the loop to do.

What still blocks is the paint, and it is flat — so **a server's throughput and
its worst request-blocking are no longer the same number.** Concurrency is
bounded by the ~9 ms of paint per render rather than by the encode, however
large the picture.

`worker_threads` still help, and for less than they did. Each gets its own
renderer and they neither share state nor contend, so they scale the paint —
but reaching for one _to get an encode off the request path_ is no longer
necessary, because it already is.

Encode against area, to size the second half: roughly 1 ms at 200×200, 4 ms at
480×320, 11 ms at 800×800, 65 ms at 2000×2000, 256 ms at 4000×4000.

**Building the scene is not the cost.** Constructing the tree and encoding it
for the addon is 0.03 ms of a 480×320 render — under one per cent, and still
only five per cent at five thousand nodes. So if you are wondering whether to
optimise how you build props, the answer is no. Node count is what makes
painting stop being flat, and it scales linearly: 100 000 nodes take 838 ms,
500 000 take 4125 ms.

### Size limits

There are none, other than what the machine can allocate — and because the
allocation happens at `toBuffer` rather than at `Root`, **a size that cannot
work fails after the render has been paid for, not when it was set.** `Root`
returns for a 200000×200000 canvas with the process still under 80 MB.

Measured: 8000×8000 succeeded at 610 MB, 16384×16384 at 2244 MB and 5.7 s,
32768×32768 threw `Could not allocate new 32768×32768 bitmap`. Failure at the
top is a clean error; the hazard is the middle, where two gigabytes go without
anything objecting. **If a width or height can come from a request, bound it
yourself** — and remember a container's memory limit will kill the process
before an allocator returns null.

The one ceiling this package enforces is on node count: above 1048576 nodes a
scene is refused with `the arena declares N nodes, the limit is 1048576`.

## What it renders

- **Layout** — flexbox, CSS grid and block, with margins, padding, borders, gaps
  and absolute positioning.
- **Text** — shaped by Skia and broken into lines here, with per-span styling,
  letter and word spacing, decorations, line clamping and ellipsis.
- **Images** — from a file, a buffer or a URL, with object-fit and
  object-position placement.
- **Paths** — arbitrary shapes from SVG path data, filled and stroked, with an
  optional `viewBox` so a path scales to the box that holds it.
- **Charts** — bar, line, pie and doughnut.
- **Effects** — gradients, masks, shadows, opacity groups, blend modes and CSS
  filters.
- **Export** — PNG, JPEG, WebP, AVIF, TIFF, BMP, ICO, SVG, PDF, GIF, APNG and
  raw pixels.

Multi-page renders produce frames for GIF, APNG, WebP and AVIF, sheets for PDF
and TIFF, and sizes for ICO — and ICO is the only one whose pages may differ in
size.

## Where the work happens

This package is a thin surface over a native addon. Your calls describe a scene; layout, text shaping, painting and encoding all happen in Rust, and the whole description crosses into it once per render rather than once per drawing call.

What that buys you is that a scene of any size costs one crossing, and that the drawing itself runs at native speed with no per-call boundary tax.

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
