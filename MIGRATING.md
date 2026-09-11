# Migrating from v9 to 10.x

Two generations of one package. **v9** is the JavaScript renderer, frozen at `9.0.4`; **10.x** is a
rewrite whose renderer, layout engine and whole boundary are Rust, with a second public surface — a
crate — that did not exist before.

The npm package name is the same and the import name is the same, which is what makes this a
migration rather than a port to something else. Most calls survive unchanged. This file is about the
ones that do not, and about the smaller set that survive **and mean something different**, which are
the only ones no tool will find for you.

Everything here was measured against the two sources rather than recalled: the v9 branch for what v9
declares and does, this tree for what 10.x declares and does. Where something was carried from the
project's own notes rather than re-measured, it says so.

## Read this in three passes

A migration has three kinds of difference and they cost wildly different amounts. Work down the
list; do not skip to the interesting one.

| Pass                                                                | What it costs you                            | Found by      |
| ------------------------------------------------------------------- | -------------------------------------------- | ------------- |
| [One: what stops your build](#pass-one-what-stops-your-build)       | minutes, and the compiler points at each one | `tsc`         |
| [Two: what breaks when it runs](#pass-two-what-breaks-when-it-runs) | a render, and the error names the problem    | the first run |
| [Three: what comes out wrong](#pass-three-what-comes-out-wrong)     | a render that looks plausible and is not     | reading this  |

**The third pass is the one to read even if you are in a hurry.** Its entries compile, run, throw
nothing, and produce a picture that is wrong by a factor you will not guess.

## Installing

```bash
npm install meo-canvas          # 10.x
npm install meo-canvas@9        # stay on the previous generation
```

**A version containing a hyphen never reaches `latest`.** A semver range does not match a
prerelease, so while 10.x was `10.0.0-alpha.n` it was reachable only by naming the `next` tag. Once
a version without a hyphen ships, a bare install gives it. Run `npm view meo-canvas dist-tags` if
you need to know where the tags point today — this file does not quote an answer, because the answer
moves and a quoted one goes stale silently.

**To run both generations at once, install the second under a different local name.** An alias is a
package-manager feature; nothing in either package is aware of it, and the import name is the only
thing that changes. This is what lets you migrate a module at a time rather than a repository at a
time.

```bash
npm install meo-canvas-v9@npm:meo-canvas@9
```

```js
import { Root } from 'meo-canvas' // 10.x
import { Root as RootV9 } from 'meo-canvas-v9' // 9.x
```

10.x needs **Node 22 or newer**. It is ES modules and, unlike v9, it also answers `require` —
through Node's own `require(esm)`, unflagged from 22.12.

## Pass one: what stops your build

### Audit your imports against three modules, not one

v9's `src/index.ts` is thirty-four lines, and **two of them are wildcards**:

```ts
export * from '@/constant/common.const.js'
export * from '@/canvas/canvas.type.js'
```

Anyone auditing what they import by reading that file alone will undercount by sixty-eight names.
This is worth saying out loud because it is not a hypothetical: the first two counts taken while
writing this guide both did exactly that, reported a surface of 55, and concluded the change was
almost entirely additive. It is not.

### About half of v9's exported names have no counterpart here

Measured across all three modules, deduplicated:

```
v9 exports        123 names
10.x exports      130 names
carried over       63
gone               60
new                67
```

**Sixty of v9's hundred and twenty-three have no name in 10.x.** Most of that tail is prop and shape
types rather than things a caller writes — `BoxProps`, `GridOptions`, `RootPropsWithWorker`,
`ChartItem`, `ResolvedChartProps`, `StillContent` and their neighbours — but each one is a compile
error for anyone who imported it.

The ones a caller plausibly did import:

| v9                                                                    | 10.x                                                              |
| --------------------------------------------------------------------- | ----------------------------------------------------------------- |
| `BoxNode` `ImageNode` `PathNode` `TextNode` `GridNode` `GridItemNode` | gone — the factories return `SceneNode`                           |
| `GridItem`                                                            | gone — place any child with `gridColumn` / `gridRow` / `gridArea` |
| `terminate` `WorkerCanvas`                                            | gone — there is no worker for a caller to manage                  |
| `clearDiskCache` `setDiskCacheDir`                                    | gone — neither surface keeps a disk cache                         |
| `easings`                                                             | `EASING_NAMES` for the names, `ease` to apply one                 |
| `Easing` `EasingFn`                                                   | gone — `EasingName` carries over                                  |
| `Track`                                                               | gone — `TrackConfig` carries over                                 |
| `RenderImageCache`                                                    | gone — caching is the renderer's, not the caller's                |

**Do not work from that table alone.** It lists what a reader recognises, and the sixty are what
your build will actually complain about. Ask your own code instead:

```bash
# every name your code imports from the package
grep -rhE "from 'meo-canvas'" src \
  | sed -E "s/.*import[[:space:]]*(type)?[[:space:]]*\{?([^}]*)\}?[[:space:]]*from.*/\2/" \
  | tr ',' '\n' \
  | sed -E 's/^[[:space:]]*(type[[:space:]]+)?//;s/[[:space:]]+as[[:space:]]+.*//;s/[[:space:]]*$//' \
  | grep -E '^[A-Za-z_$][A-Za-z0-9_$]*$' | sort -u
```

Then let `tsc` do the rest. This pass is cheap precisely because nothing here is silent.

### Four types you imported from us were never ours

`ExportFormat`, `ExportOptions`, `SaveOptions` and `RenderOptions` are re-exported by v9 **from
`meo-skia-canvas`**, with a comment in `index.ts` explaining that it saves callers from reaching
into a transitive dependency. So they are not v9 types that were renamed — they are another
package's types that v9 passed through.

10.x has its own: `Format` for the format tag and `EncodeOptions` for the options. The v9 names still
exist upstream, so if you go looking for them you will find them, attached to a different package
than the one you were importing from.

### The layout enums are types now, not values

v9 exported enum **objects** — you wrote `Style.FlexDirection.Column` and the value existed at
runtime. 10.x has string-literal union **types**: you write `'column'`, and `'colunm'` is a compile
error rather than a runtime `undefined`.

```ts
// v9
{ flexDirection: Style.FlexDirection.Column, position: Style.PositionType.Absolute }

// 10.x
{ flexDirection: 'column', position: 'absolute' }
```

Capitalisation goes with it, everywhere: `Style.PositionType.Absolute` becomes `'absolute'`, and an
inset written `position: { Top: 8 }` becomes `top: 8`.

### Edge groups are gone

v9 accepted `padding: { Horizontal: 2, Bottom: 2 }`. 10.x has only `top`, `right`, `bottom`, `left`.

**From TypeScript this is a compile error and the whole class disappears at once.** From JavaScript
it is not — see the next pass.

## Pass two: what breaks when it runs

### An edge group is dropped in silence

If you are porting without TypeScript, this is the entry to fear. Measured: a node given
`padding: { Horizontal: 16 }` renders **with no padding at all**, and nothing is thrown. The key is
not one 10.x knows, so it is ignored, and the result is a layout that is wrong by exactly the
padding you thought you had set.

There is no runtime guard for this and there is not going to be one that catches every shape of it.
**Port in TypeScript if you possibly can**, and this entire class is a red squiggle instead of an
afternoon.

### `<b>` inside a plain `Text` is literal text

v9 read markup inside `Text`. 10.x does not: `Text('a <b>bold</b> word')` draws the angle brackets.
`RichText` is the node that reads markup.

This one announces itself the first time you look at the output, which is why it is in this pass
rather than the next.

## Pass three: what comes out wrong

### A bare `Box` runs the other way

Verified in both sources rather than recalled:

|            | direction                                                                                   |
| ---------- | ------------------------------------------------------------------------------------------- |
| v9 `Box`   | **column** — `layout.canvas.ts` sets `flexDirection: Style.FlexDirection.Column` explicitly |
| 10.x `Box` | **row** — sets `display: 'flex'` and no direction, so it takes CSS's default                |

So a `Box` that said nothing about its axis changes meaning between the two. **Write the axis out**
on every container you port, or use `Row` and `Column`, which name it for you on both generations.

**The shrink is a second trap pointing the wrong way**, and it is the one that looks like the
careful choice. Yoga defaults `flex-shrink` to `0`, so a v9 node _looks_ like it means zero — but
v9's constructors put CSS's value back, and all four declare `flexShrink: 1`. 10.x means `1` too.
**So a v9 node that says nothing about shrinking means one, and the faithful port writes no
`flex-shrink` at all.** Writing `0` is a divergence dressed as a reproduction; pinned into a wrapper
that every node passes through, it moved a whole card's geometry and stopped its background
painting.

### `lineHeight` is a different quantity

|      | what the number means                                            |
| ---- | ---------------------------------------------------------------- |
| v9   | the line box in **pixels** (`@unit Pixels` in its own type docs) |
| 10.x | a **multiple of the em size**                                    |

`lineHeight: 24` at `fontSize: 18` is twenty-four pixels in v9 and **four hundred and thirty-two**
in 10.x.

**The trap inside the trap:** a component written in pixels may still hold a bare ratio like `1.4`,
and those are the only values that carry over unchanged. So a file can be half-converted and look
fine in the places you checked.

Wrong by a factor of the font size does not look like a unit mistake. It looks like a layout defect,
and you will go looking in the wrong place.

### Four calls kept their name and changed their meaning

#### `mixColor` takes colour objects, and no longer clamps

```ts
// v9:  (from: string, to: string, t: number): string
mixColor('#000000', '#ffffff', 0.5) // '#808080'

// 10.x: (from: Rgba, to: Rgba, t: number): Rgba
formatColor(mixColor(parseColor('#000000')!, parseColor('#ffffff')!, 0.5))
```

Passing v9's strings fails at the call with a message naming the argument. Before that guard existed
it returned `{ r: null, g: null, b: null, a: null }`, and `formatColor` of that was
`color(srgb NaN NaN NaN / NaN)` — a valid string painting nothing you wanted, from code that
compiles in JavaScript.

**v9 clamped `t` to `0..1` and 10.x does not.** `mixColor(black, white, 1.25)` was `#ffffff` and is
now `{ r: 318.75, … }`, which survives to `color(srgb 1.25 1.25 1.25)`. That is deliberate —
overshooting curves need the range — but if you relied on the clamp, apply it yourself.

#### `parseColor` returns `null` instead of throwing

```ts
parseColor('potato') // v9: throws.  10.x: null
```

**TypeScript finds half of this.** The type is `Rgba | null`, so it flags every dereference — and it
says nothing about the `catch` block that used to be your error path and is now unreachable. Search
for `parseColor` and read what surrounds each call rather than trusting the compiler to have found
them all.

#### `parseColor` alpha is no longer rounded

`parseColor('#0000007f').a` was `0.498` in v9 and is `0.4980392156862745` in 10.x — the exact
`127/255` rather than three decimal places, which is what v9's formatter produced on the way back
out. Formatting round-trips to the same hex, so this matters only if you compare alphas for equality
or print them.

#### `release()` is on every canvas, and frees something else

In v9 it depended on how you called `Root`: worker mode, the default, returned a canvas carrying
`release()`, and v9's own types say the other branch returns a plain canvas without it. What it
released was the canvas back to the worker pool.

There is no worker pool here, so `release()` is on **every** canvas and frees the Skia surface.
Calling it twice is not an error. Encoding after it is — the canvas throws rather than handing back
an empty picture.

If your v9 code called it, keep calling it. If your v9 code ran with `workerMode: false` and
therefore never had it, **add it**: the surface it frees is the largest thing a canvas holds.

## Errors: what throws and what rejects

**An argument of the wrong shape throws synchronously; a failure inside the render rejects.**

Every read of your arguments happens in one pass before the work is handed to the thread pool, so an
argument error is raised while there is still a call to throw from, and a render error is raised when
there is not.

```ts
Root({ width: 'wide' }) // throws, synchronously
await Root({ width: 800, children: /* an unreachable image */ }) // rejects
```

A test asserting that either one _always_ happens is wrong, and it will fail for the right reason and
get repaired the wrong way.

## Diagnostics: what the renderer could not use

**This is genuinely new.** v9 has no equivalent — `git grep -il diagnostic` over its sources returns
nothing at all.

A value the renderer could not use does not fail the render. It is reported alongside it, because a
markup tag with an unusable colour should still draw its text:

```ts
const canvas = await Root({ width: 400, children: RichText('<color=zzz>hi</color>') })
canvas.diagnostics // readonly Diagnostic[]
```

A `Diagnostic` names a **path** rather than a property — `<color=zzz>` as written, value included —
because a value nested inside another property has no single name that would find it.

## What did not change

Do not rewrite these looking for a new API, because there is not one.

**v9 already had charts, gradients, masks, grid layout and `httpOptions`.** Each appears in five or
more of v9's own source files. They are not new in 10.x and the ports are shallow.

The output surface is deliberately the same set, so a script that writes a file does not have to
change how it writes it: `toBuffer`, `toBufferSync`, `toFile`, `toURL`, `toDataURL` and their sync
pairs. **The sync variants are ordinary functions here** — v9 needed `Atomics.wait` on a
`SharedArrayBuffer` because its canvas lived in a worker, and that constraint is gone.

`saveAs` is not carried over. A deprecated alias reintroduced in a rewrite is one nobody gets to
remove later.

## Scope of this guide

This covers the **npm surface**, because that is the one a v9 caller is already using. The Rust crate
is a sibling rather than a layer, and nothing in v9 corresponds to it; `README.md` is where that
starts.

**What is not here:** a rule for every one of the sixty removed names, because most are internal
shape types and a table of sixty rows is a list that goes stale rather than a thing you can act on.
The instrument above answers it against your code, which is the question you actually have.
