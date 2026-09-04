# Migrating from v9

v10 is a rewrite: the renderer, the layout engine and the whole boundary to it are Rust now, and
there is a second public surface — a crate — that did not exist before. Most of your calls survive.
The ones that do not are listed here, and the ones that survive _and behave differently_ are listed
first among them, because those are the only ones your compiler will not find for you.

Everything below was derived by running v9.0.3 from npm and v10 from this branch side by side, not
from memory. Where something was not checked, it says so.

## Installing

**v10 is not published yet.** `npm install meo-canvas` gives you 9.0.3 today, and will keep doing so
until the first 10.x reaches npm.

```bash
npm install meo-canvas@next    # ← works once the first 10.x is published, not before
```

There is no `next` tag at the moment: `npm view meo-canvas dist-tags` answers `latest: 9.0.3` and
nothing else. When the release happens, that line is the whole install change.

v10 needs **Node 22 or newer**. It is ES modules, and unlike v9 it also answers `require` — through
Node's own `require(esm)`, unflagged from 22.12.

## The part your compiler will not catch

Three things kept their name, kept compiling, and changed what they do. If you read nothing else,
read these.

### `mixColor` takes colour objects now, and does not clamp

v9 took CSS strings and returned one. v10 takes and returns `Rgba`.

```ts
// v9
mixColor('#000000', '#ffffff', 0.5) // '#808080'

// v10
formatColor(mixColor(parseColor('#000000')!, parseColor('#ffffff')!, 0.5)) // '#808080'
```

**Passing v9's strings now fails at the call**, with a message naming the argument and pointing
here:

```
[canvas] mixColor takes a colour rather than a string, and `from` is "#000000".
v9's took strings and parsed them; parse it first with `parseColor`.
```

Until that guard existed it returned `{ r: null, g: null, b: null, a: null }`, and `formatColor` of
that was `color(srgb NaN NaN NaN / NaN)` — a valid string that painted nothing you wanted, from code
that compiles in JavaScript. If you are reading this because you hit the error, that is the error
doing its job.

It also no longer clamps. `mixColor(black, white, 1.25)` was `#ffffff` in v9 and is
`{ r: 318.75, … }` in v10, which survives all the way to
`color(srgb 1.25 1.25 1.25)`. That is deliberate — overshooting curves need the range — but if you
were relying on the clamp, apply it yourself.

### `parseColor` returns `null` instead of throwing

```ts
parseColor('potato') // v9: throws.  v10: null
```

A `try`/`catch` around it now catches nothing and the `null` flows onward. **TypeScript finds half
of this and not the other half**: the type is `Rgba | null`, so it flags every place you dereference
the result, and it says nothing at all about the `catch` block that used to be your error path and
is now unreachable. Search for `parseColor` and read what surrounds each call, rather than trusting
the compiler to have found them.

### `parseColor` alpha is no longer rounded

`parseColor('#0000007f').a` was `0.498` in v9 and is `0.4980392156862745` in v10 — the exact
`127/255` rather than three decimal places. Formatting round-trips to the same hex, so this matters
only if you compare alphas for equality or print them.

## What stops your build

### The layout enums are types now, not values

v9 exported yoga's enum objects — `Align`, `Display`, `FlexDirection`, `Justify`, `Overflow`,
`PositionType`, `BoxSizing`, `Direction`, `Wrap`, `TextAlign`, `TextDecoration`, `VerticalAlign`,
`ObjectFit`, `BackgroundRepeat`, `BackgroundSize`, `BlendMode`, `PaintOrder`, `GradientType`,
`Edge`, `Gutter`, `Unit`, `MeasureMode`, `NodeType`, `Dimension`, `Border` — as runtime values you
could index. v10 has no layout engine of its own to expose, so these are **CSS strings**, and the
exports of the same name are types only.

```ts
// v9
Box({ alignItems: Align.Center, display: Display.Flex })

// v10
Box({ alignItems: 'center', display: 'flex' })
```

`Align` is `undefined` at runtime in v10. If you imported it as a value, that import fails; if you
imported it as a type, the names still work and the members are strings.

### Removed with no replacement

| Gone                                                 | What to do                                                                  |
| ---------------------------------------------------- | --------------------------------------------------------------------------- |
| `workerMode`, `workers`, `terminate`, `WorkerCanvas` | Render on a `worker_thread` of your own — see the deployment note below.    |
| `useDiskCache`, `clearDiskCache`, `setDiskCacheDir`  | There is no disk cache. Cache the bytes you get back, if you need to.       |
| `imageConcurrency`                                   | Not configurable.                                                           |
| `pagedChildren`                                      | Use the page-builder form of `children` with `pages` or `duration`.         |
| `GridItem`                                           | Grid placement is style on the child — `gridColumn`, `gridRow` on any node. |
| `saveAs`, `newPage`, `canvas.pages`                  | `toFile` writes; pages come from `pages`/`duration` at `Root`.              |
| `Errata`, `ExperimentalFeature`, `LogLevel`          | yoga's knobs, and yoga is gone.                                             |

### Renamed or reshaped

| v9                                               | v10                                                         |
| ------------------------------------------------ | ----------------------------------------------------------- |
| `easings.outCubic(0.5)`                          | `ease('outCubic', 0.5)`, with `EASING_NAMES` listing all 31 |
| `Text(42)`                                       | `Text('42')` — content is a string, and a number now fails  |
| `Root(...)` → `WorkerCanvas` or `RenderedCanvas` | always `Promise<Canvas>`                                    |
| `Style` (exported value)                         | style props sit flat on every node, as CSS names them       |

The easing set itself is unchanged: 31 names in v9, 31 in v10, and `cubicBezier`, `steps` and
`resolveEasing` all answer identically.

## What did not change

More than you would expect for a rewrite, and this is the reassuring half.

`parallel` is unchanged and worth naming, since a migration guide that lists `track` and `sequence`
and not the third combinator reads as though it went somewhere. It did not: same call, same answer.

These `Root` props mean exactly what they did: `children`, `pages`, `duration`, `fps`, `width`,
`height`, `scale`, `fonts`, `gpu`, `colorType`, `colorSpace`. The node factories `Box`, `Row`,
`Column`, `Grid`, `Image`, `Path`, `Chart` and `Text` are still there and still take props objects.
`toBuffer`, `toBufferSync`, `toFile`, `toFileSync`, `toURL`, `toURLSync`, `toDataURL` and `release`
are all still on the canvas.

Of 23 behavioural cases run through both versions — `lerp`, `mapRange`, `interpolate`, `mix`,
`spring`, `springDuration`, `cubicBezier`, `steps`, `resolveEasing`, `track`, `sequence`,
`formatColor`, `isColor` — **18 gave byte-identical answers** and the 5 that differed are the colour
cases listed above.

## Two things that change how you deploy

Neither is a call you have to rewrite; both will bite a service that does not know them.

**Font registration is process-wide and permanent, per thread.** A family registered by one render
serves every later render on that thread, and a render that names a family it never registered uses
whatever an earlier one left behind instead of failing. Register at start-up, not per request. See
`FontRegistration` in the API reference for the whole of it.

**A render is about 13 ms and only half of it depends on the picture.** Painting — the whole native
call, from arena decode through layout to the drawing — is a flat ~9 ms whatever it draws; encoding
is what grows with area. One thread does about 73 renders a second at 480×320 and about four a
second at 4000×4000. v9's worker pool is gone, so if you were leaning on
it, a pool of `worker_thread`s doing their own renders is the replacement — they share nothing and
scale.

## New in v10, briefly

A migration guide is not a feature tour, so only what you might go looking for: `RichText` for
per-span styling within one paragraph, `Canvas` as a named export you can annotate, chart internals
(`barLayout`, `gridLines`, `linePath`, `linePoints`, `sliceAngles`, `slicePath`, `seriesColor`) if
you are drawing your own, `httpOptions` on `Root` for fetching image URLs, and CSS grid and block
display alongside flex. There is also a Rust crate now, covering the same ground — with the one difference that fetching a URL is behind its `net` feature there and always available here.

## Scope of this guide

**Verified by running both versions:** the export lists, every behavioural case above, the `Root`
prop names, the canvas methods, the layout enums being values in v9 and types in v10, and the
`mixColor` and `parseColor` differences.

**Not verified, and worth your own check:** whether any _rendering_ differs for a scene both
versions accept. The two use different layout engines — yoga in v9, taffy here — and this guide
compares APIs rather than pixels. If a migrated page lays out differently, that is expected to be
rare and is worth an issue with the scene attached rather than a workaround.

Also unchecked: v9's `Chart` props against v10's, beyond both taking `type` and `data`; and the
image and path prop sets in detail. If you hit a difference there, it belongs in this document —
please say so.
