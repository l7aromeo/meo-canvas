# Migrating from v9

v10 is a rewrite: the renderer, the layout engine and the whole boundary to it are Rust now, and
there is a second public surface — a crate — that did not exist before. Most of your calls survive.
The ones that do not are listed here, and the ones that survive _and behave differently_ are listed
first among them, because those are the only ones your compiler will not find for you.

Everything below was derived by comparing v9 from npm against v10 from this branch, not from
memory -- the behavioural cases by running both, the surface differences by reading what each one
declares. Where something was not checked, it says so.

## Installing

**v10 is on npm under the `next` tag.** `npm install meo-canvas` still gives you the 9.x line, and
will keep doing so for as long as `latest` points there — a semver range never matches a prerelease,
so nothing reaches 10.x without asking for it by name.

```bash
npm install meo-canvas@next
```

Measured on 10 September 2026: `npm view meo-canvas dist-tags` answers `latest: 9.0.4` and
`next: 10.0.0-alpha.5`. Check it yourself before you plan around a version — that line moves.

**To run both at once, install v10 under a second name.** An alias gives the same package a local
name of your choosing, so a file can import one while the rest of your code keeps the other, and you
migrate a module at a time rather than a repository at a time.

```bash
npm install meo-canvas-v10@npm:meo-canvas@next
```

```js
import { Root as RootV9 } from 'meo-canvas'
import { Root as RootV10 } from 'meo-canvas-v10'
```

Nothing in v10 is aware of the alias; it is a package-manager feature and the import name is the
only thing that changes.

v10 needs **Node 22 or newer**. It is ES modules, and unlike v9 it also answers `require` — through
Node's own `require(esm)`, unflagged from 22.12.

## The part your compiler will not catch

Four things kept their name, kept compiling, and changed what they do. If you read nothing else,
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

### `release()` is on every canvas now, and frees something else

In v9 it depended on how you called `Root`. Worker mode -- the default -- returned a canvas carrying
`release()`, and v9's own types say the other branch "Returns plain Canvas without `.release()`
method". What it released was the canvas back to the worker pool.

There is no worker pool here, so `release()` is on every canvas and frees the Skia surface. Calling
it twice is not an error. Encoding after it is: the canvas throws `this canvas was released; encode
before calling release()` rather than handing back an empty picture.

If your v9 code called it, keep calling it -- it still wants to be called, and it is still the line
that returns the memory. If your v9 code ran with `workerMode: false` and therefore never had it,
add it: the surface it frees is the largest thing a canvas holds.

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
| `toSharp`, `toSharpSync`                             | `toBuffer('png')` and hand the bytes to `sharp` yourself.                   |
| `onLoad`, `onError` on `Image`                       | Read `canvas.warnings` after the render. See below.                         |
| `key` on any node                                    | Nothing needs one. See below.                                               |

### Renamed or reshaped

| v9                                               | v10                                                         |
| ------------------------------------------------ | ----------------------------------------------------------- |
| `easings.outCubic(0.5)`                          | `ease('outCubic', 0.5)`, with `EASING_NAMES` listing all 31 |
| `Text(42)`                                       | `Text('42')` — content is a string, and a number now fails  |
| `Root(...)` → `WorkerCanvas` or `RenderedCanvas` | always `Promise<Canvas>`                                    |
| `Style` (exported value)                         | style props sit flat on every node, as CSS names them       |

The easing set itself is unchanged: 31 names in v9, 31 in v10, and `cubicBezier`, `steps` and
`resolveEasing` all answer identically.

## Errors: what throws and what rejects

**v9 and v10 divide this differently, and a `try`/`catch` that worked will silently stop catching.**

Building the tree throws **synchronously**. `Text`, `Row`, `Box` and the rest validate their
arguments the moment you call them, and a bad one is a `TypeError` before any rendering starts --
`Text(42)` is this, and so is a props object carrying a name no node has.

`Root` is `async`. Everything it does -- fetching an image URL, laying out, painting, encoding --
fails by **rejecting** the promise it returned.

So the two halves of a v9-shaped `try` land in different places:

```js
// Catches the Text throw. Does NOT catch anything Root rejects with:
// the promise is created, the try block ends, and the rejection lands
// on an unhandled-rejection handler you may not have.
try {
  const canvas = Root({ width: 400, children: Text(42) })
} catch (error) {
  /* ... */
}

// Catches both.
try {
  const canvas = await Root({ width: 400, children: Text('42') })
} catch (error) {
  /* ... */
}
```

The `await` inside the `try` is the whole difference. If you are migrating a file that never had to
`await` its construction, this is the change most likely to turn a handled error into a process-level
one.

## What the renderer could not use, reported rather than thrown

**New in v10 and there is nothing in v9 to migrate from**, so this is here because you will see it
rather than because you have to change anything.

A value the renderer cannot use does not always stop the render. A markup tag it does not know, a
colour that is not a colour, a font weight outside the range -- each of those renders as if the
value were absent, which means nothing in the picture tells you it was there. Those are reported
beside the render:

```js
const canvas = await Root({ width: 400, children: Text('<color=zzz>hello</color>') })
for (const one of canvas.diagnostics) {
  console.warn(`${one.path}: ${one.detail}`)
  // <color=zzz>: not a colour any CSS syntax spells; the tag was ignored
}
```

Each carries `path` -- the value as you wrote it -- and `detail`. Markup diagnostics also carry
`offset`, a byte index into the string you passed, so two tags spelled the same way can be told
apart. It is absent for anything that did not come from markup.

**Image sources are the other half and are reported separately**, on `canvas.warnings`, because an
image that will not load is a different kind of problem from a value that would not parse: it has a
`url`, a `failure` naming which way it failed, and how many nodes wanted it. `onImageError` on
`Root` decides whether a failure is tolerated or thrown; the default tolerates it and draws a
placeholder.

**This is what replaces v9's `onLoad` and `onError`.** They were callbacks on `Image`, and v9's own
types record that they were "dropped: a function cannot cross into a worker" in worker mode -- so
they already did not fire on the default path. Here there is no DOM for a load event to happen in
and nothing to call back from: the render is one call that either resolves with a canvas or rejects,
and what went wrong along the way is on the canvas when you get it.

**And `key` is gone for the same reason.** It exists in a framework that reconciles one tree against
the next. Nothing here does: a scene is built, rendered, and finished.

## What did not change

More than you would expect for a rewrite, and this is the reassuring half.

`parallel` is unchanged and worth naming, since a migration guide that lists `track` and `sequence`
and not the third combinator reads as though it went somewhere. It did not: same call, same answer.

These `Root` props mean exactly what they did: `children`, `pages`, `duration`, `fps`, `width`,
`height`, `scale`, `fonts`, `gpu`, `colorType`, `colorSpace`. The node factories `Box`, `Row`,
`Column`, `Grid`, `Image`, `Path`, `Chart` and `Text` are still there and still take props objects.
`toBuffer`, `toBufferSync`, `toFile`, `toFileSync`, `toURL`, `toURLSync` and `toDataURL` are all
still on the canvas. `release` is there too and is listed above instead, because what it does is not
what it did.

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
second at 4000×4000.

**v9's worker pool is gone, and you need less of a replacement than that sounds.** `toBuffer`,
`toFile` and `toURL` do their encoding off the event loop, so the part that scales with area no
longer blocks: a 4000×4000 encode takes 277 ms with the loop running throughout, against 294 ms of a
completely starved loop for `toBufferSync`. What still blocks is the paint, and that is the flat
~9 ms. A pool of `worker_thread`s is the replacement if you want more paint throughput — they share
nothing and scale — but not something you need in order to keep a request path responsive.

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

**Verified by reading declarations rather than by running them**, so a behaviour they do not spell
out could still surprise you: `release()`, `toSharp`, `onLoad`, `onError` and `key` were read out of
the published `meo-canvas@9.0.4` type declarations and out of this branch's source. The dist-tags
were measured against the registry on 10 September 2026 and will have moved by the time you read
this. The throw-against-reject split is read from the signatures -- `Root` is `async`, the node
factories are not -- rather than from a test that provokes both.

**Not verified, and worth your own check:** whether any _rendering_ differs for a scene both
versions accept. The two use different layout engines — yoga in v9, taffy here — and this guide
compares APIs rather than pixels. If a migrated page lays out differently, that is expected to be
rare and is worth an issue with the scene attached rather than a workaround.

Also unchecked: v9's `Chart` props against v10's, beyond both taking `type` and `data`; the image
and path prop sets in detail; and whether a v9 render that relied on `onLoad` firing has any
observable equivalent here beyond the warning list. If you hit a difference there, it belongs in this document —
please say so.
