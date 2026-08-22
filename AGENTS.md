# meo-canvas

Renders declarative scene trees to images. A caller describes what it wants —
boxes, rows, text, images, paths, grids, charts — and gets back encoded bytes.
Layout is flexbox and CSS grid; drawing is Skia; text is shaped and broken by
Skia's paragraph engine.

Two public surfaces, and they are siblings rather than layers: a Rust crate and
a Node addon. Both construct the same `Scene` and hand it to the same core, so
neither can grow a capability the other cannot reach.

## Architecture

`Scene` is the contract. It is a plain data tree in `meo-canvas-scene` with no
dependencies at all — not Skia, not the layout engine, not the Node bindings,
and not a serialization framework. Its codec is written by hand because the byte
layout is a specification a JavaScript writer also implements, and a derived
format is one no other language can target from the documentation alone.

Every door into the renderer produces one:

```
meo-canvas          Element tree            ── Rust callers build it directly
meo-canvas-node     decode(arena, values)   ── JavaScript callers encode into it
meo-canvas-cli      read from disk          ── files and pipes
                             │
                             ▼
                      meo-canvas-core
                             │
                    resolve → measure → layout → paint → encode
                             │
                             ▼
                          Vec<u8>
```

The core cannot tell which door a scene came through, which is what keeps the
two surfaces honest.

### Two representations

A `Scene` is reached two ways, and they are not the same format.

**The boundary format is an `f64` arena.** JavaScript writes opcodes and numeric
properties into a growable `Float64Array`, with strings and buffers in a side
`values` array that the records index into. Rust reads `&[f64]`. It is shaped
that way because a store into a `Float64Array` is one operation where writing
varint bytes from JavaScript is several, and because reading a value out of V8
is what costs at that boundary. Its decoder lives in `meo-canvas-node`, since
only the addon can hold the side array.

**The persistence format is bytes.** Self-contained and self-describing, with a
magic number, a version, and errors that name the byte offset. Strings live
inside it because there is no side channel. It is what the CLI reads, what a
golden fixture is stored as, and it lives in `meo-canvas-scene`.

Both decode to the same `Scene`, so a scene captured from JavaScript and written
to disk round-trips without loss.

### Pages

A `Scene` carries one or more page trees, not one. A single page is the common
case and encodes to a still image; several pages become frames in gif and apng,
sheets in pdf and tiff, and sizes in ico.

Pages are trees, already built, by the time the core sees them. Nothing in the
core calls back to produce one. A caller that wants a page per frame of an
animation samples its own values at each time and hands over the trees that
result, which is why animation needs no clock, no retained state, and no
re-entry into caller code mid-render.

Fonts and decoded images are resolved once and shared across every page in a
scene. The layout tree is built and dropped per page.

### Where state lives

`Renderer` owns everything a render needs that is not the scene: the registered
fonts, and whatever caches the passes keep. It exists because those outlive any
one scene — a server rendering a thousand pictures registers its fonts once —
and because the alternative is process-wide statics, which is what v1 has five
of.

Nothing in this crate is global. Two `Renderer`s on two threads share no state
and cannot contend. The one exception is outside our control: `meo-skia-canvas`
keeps its own process-wide font registry, so registering a face is visible
process-wide whether or not our `Renderer` is. What this crate controls is
adding no second global, and it adds none.

`Renderer::render` takes a scene and returns a `RenderedCanvas` — the painted
surface. Encoding is a separate call on that canvas, so two formats of one
picture cost one resolve, one measure, one layout, one paint and two encodes.
Rendering and encoding in one call would make the JavaScript surface, which
retains its canvas, strictly faster than the Rust one for identical work.

`RenderedCanvas::to_buffer` takes `&mut self`. Every encode entry point upstream
does, because `Canvas::to_buffer` prepares the surface before reading it.
Interior mutability would let two encodes of one canvas read as independent when
they are not; `&mut` says encoding consumes preparation.

`EncodeOptions::validate` runs inside `to_buffer`, because it needs the page
count and that lives on the painted surface. A page index past the end is a
property of the drawing rather than of the scene, so `render` structurally
cannot catch it.

`render_to_buffer` is the one-format convenience, and most callers are that: the
CLI writes one file, a fixture compares one image, the addon encodes once. It
returns bytes rather than an `EncodedImage`, since a caller who passed the
format in already knows it.

### Pipeline

One pass, no re-entry into caller code.

**resolve** registers fonts, decodes images, and inherits text styles down the
tree. This is the only stage that performs I/O, and the I/O is local: an
`ImageSource::Url` is refused with `Error::UnresolvedSource`, because the core
performs no network access. Resolving a URL to bytes is the job of whichever
surface has a fetcher.

**measure** builds one Skia `Paragraph` per text node, which the next stage
re-lays-out at different widths. Re-layout reuses run parsing, font resolution
and glyph shaping — see `meo-skia-canvas/src/text.rs:1409-1419` — so the work a
rebuild would repeat happens once per text node. How much that saves against
rebuilding is unmeasured; a criterion bench belongs here before the claim
becomes a number.

**layout** solves the tree with taffy. Text leaves answer taffy's measure
closure from their prepared paragraph. `AvailableSpace::MinContent` and
`MaxContent` are answered by `min_intrinsic_width()` and `max_intrinsic_width()`
without an additional layout — a `Paragraph` exists only post-layout, since
`ParagraphBuilder::build` lays it out at construction.

Baseline alignment on measured text is wrong today, and wrong in a way that
looks nearly right. The measurer reports a baseline — `MeasuredLeaf` carries one
— but taffy's high-level tree has nowhere to receive it: a measure-function leaf
reports `first_baselines: Point::NONE` (`taffy-0.13.0/src/compute/leaf.rs:102`)
and the flexbox solver reads a missing baseline as the node's own height
(`src/compute/flexbox.rs:1522`). A row of text aligned `baseline` therefore
lines up on the bottom edges of its runs, which differs from true baseline
alignment by the descender depth, so two runs at different sizes sit subtly
wrong rather than obviously so. In a column direction taffy does not attempt
baseline alignment at all.

Correcting it means either the low-level `LayoutPartialTree` API, where a caller
constructs the `LayoutOutput` and its `Baselines` itself, or a post-pass that
shifts baseline-aligned children using the baselines already measured.

**paint** walks the solved tree in z-order and draws through
`meo-skia-canvas`'s `Context2D`. No drawing call crosses a language boundary:
the whole stage is Rust calling Rust.

**encode** produces png, jpg, webp, avif, tiff, bmp, ico, svg, pdf, gif, apng,
or raw bytes.

`resolve` is the only stage that waits on anything, and it waits synchronously:
nothing in the core is async and it needs no runtime. Everything after it is
CPU-bound, which is why parallelism lives at the scene level — many scenes
across a thread pool — rather than inside a single render.

### The JavaScript boundary

The encoder walks the tree the node factories built and writes opcodes and
numeric properties into a growable `Float64Array`. Strings, buffers, and
anything else a float cannot hold go into a side `values` array, and the record
stores an index into it. A bitmask per node names which properties are present,
so a node that sets five of its available properties consumes five slots rather
than all of them.

The mask is carried in `f64` slots holding **53 bits each**, not 64: a double
represents integers exactly only up to 2^53, and a 64-bit mask written into one
slot loses every bit above that silently. Two slots therefore name 106
properties, and a node kind whose property count passes 106 takes a third.

`Root()` hands the arena over in a single call. A scene of any size crosses
once, and `toBuffer` afterwards carries only a format tag — see "The JavaScript
surface" for what that buys.

This is the shape it is because reading a value out of V8 is what costs, not the
crossing: a `lineTo` in `meo-skia-canvas` costs 82 nanoseconds, of which 17 is
the crossing itself and 39 is reading two floats out of the arguments. Decoding
from a `&[f64]` skips V8 entirely.

`NodeTag` lives in `meo-canvas-scene` as a `wire_enum!`, so the byte codec's
kind tag and the arena's opcode are the same number by construction rather than
by two tables agreeing. Nothing generates code from the `Scene` definition; the
macros produce Rust from one declaration, and the JavaScript writer that has to
agree with them does not exist yet.

### Node addon

`meo-canvas-node` owns the only `#[neon::main]` in the binary. A Node addon has
exactly one module-init symbol, so `meo-skia-canvas` is depended on with
`default-features = false`, leaving its own `node-addon` feature off and its
entry point uncompiled.

The addon re-exports `meo-skia-canvas`'s operations alongside its own, so one
55 MB binary serves both the declarative surface and the imperative canvas API
beneath it. Two addons would mean two copies of Skia resident in one process.

## The Rust surface

`meo-canvas` is the authoring layer. It exists because the scene contract is an
arena — a flat `Vec<Node>` indexed by `NodeId`, which a codec can round-trip and
a person cannot comfortably write.

A node carries a constructor, a style, and its children. Three methods, and the
set never grows: a new property is a new method on `Style`, not on nine node
types.

```rust
Row::new()
    .style(Style::new().gap(px(16.0)).padding(all(px(24.0))).background(hex("#101014")))
    .children([
        Image::path("avatar.png").style(Style::new().size(px(64.0), px(64.0)).fit(Cover)),
        Text::new("Ukasyah").style(Style::new().font_size(24.0).bold()),
    ])
```

`Style` is one flat type, not four. Authoring is flat because CSS is flat and
because v1's `BoxProps` mixes layout, paint, text and effect into one object —
so a reader never has to know which group `gap` lives in versus `background`.
The scene keeps them grouped, because the codec needs them grouped;
`Style::into_parts` splits at `into_scene` time.

**The names and the behaviour are CSS's.** `color` is the inherited text colour,
`background` is the fill, and a `color` set on a container reaches every
descendant. Chrome is the reference, so someone porting a design does not
translate. Where a CSS name is a known trap — `color` and `background` sitting
adjacent and meaning different things — it is CSS's trap and not one invented
here.

Setters are `const fn` wherever the property allows, which makes a reusable base
a `const`:

```rust
const CARD: Style = Style::new().padding(all(px(24.0))).gap(px(16.0));

Row::new().style(CARD.background(hex("#101014")))
Row::new().style(CARD.background(hex("#1c1c22")))
```

A `const` is substituted at each use, so every `CARD` is a fresh value and a
`self`-taking setter can consume it. No clone, no lifetime.

The line a setter cannot cross is **whether the field's type needs dropping**,
not whether it holds a `String` or a `Vec` — assigning over an owning field in a
`const fn` is E0493, and `gradient` and `mask` hit it despite carrying no
`String` of their own. Eight setters are therefore not `const`:
`font_family`, `grid_columns`, `grid_rows`, `box_shadow`, `text_shadow`,
`filter`, `backdrop_filter`, `gradient`, `mask`. A function returning a `Style`
serves the same purpose there, and each one says so in its own doc.

`px` takes an `f32`, so `px(16.0)` and not `px(16)` — Rust does not coerce an
integer literal, `impl Into<f32>` cannot be `const`, and an `i32` parameter
would lose `px(0.5)`.

`Style` is deliberately not `#[non_exhaustive]`: the rest pattern above is the
documented escape hatch for a property with no setter, and `non_exhaustive`
forbids exactly that.

Every field stays public, so a property with no setter is still reachable:

```rust
.style(Style { aspect_ratio: Some(1.618), ..Style::new().gap(px(8.0)) })
```

## The JavaScript surface

**This section is the design, not a description.** `packages/meo-canvas/src`
holds the type vocabulary and nothing else — the encoder, the node factories and
`Root` are unwritten. Read it as the specification to build against, and treat
any sentence here as false about the tree until that lands.

Object literals, not builders. The two surfaces are siblings, so each is
idiomatic in its own language rather than one imitating the other — and this is
the shape v1 already has.

```js
const canvas = await Root({
  width: 800,
  height: 400,
  children: [
    Row({
      style: { gap: 16, padding: 24, background: '#101014' },
      children: [
        Image({ src: 'avatar.png', style: { width: 64, height: 64, fit: 'cover' } }),
        Text('Ukasyah', { style: { fontSize: 24, fontWeight: 'bold' } }),
      ],
    }),
  ],
})

const png = await canvas.toBuffer('png')
const jpg = canvas.toBufferSync('jpg')
```

Same `style` key as the Rust surface, same CSS names, same values — `'row'`
where Rust has `Row`, `16` where Rust has `px(16.0)`. The string-literal unions in
`packages/meo-canvas/src/index.ts` are what make `'cover'` complete and `'covr'`
a compile error.

`width` and `height` belong on `Root` because the canvas is the sized thing and
the tree is drawn into it. A retained canvas has a size.

### One crossing, and when

| call                | what crosses                                                |
| ------------------- | ----------------------------------------------------------- |
| `Row(…)`, `Text(…)` | nothing — plain objects, no native call                     |
| `await Root({…})`   | the entire arena, one `Float64Array` and one `values` array |
| `toBuffer(fmt)`     | a format tag and options                                    |

A scene of any size is **one** crossing. There is no per-node call and no
per-property call.

### Why the tree is built before it is encoded

JavaScript evaluates arguments inside out, so `Row({children: [Text('a')]})`
runs `Text` before `Row`. Writing opcodes as each factory runs would land them
post-order, and the arena is pre-order — a parent's opcode and child count
precede its children. Buffering to reorder is the intermediate tree again, less
visibly. So the factories build plain objects and `Root` encodes them in one
pass.

### The retained canvas

`Root` is async because resolve performs I/O, and it runs the render on a
`cx.task` thread so a server's event loop is never blocked by a paint.

It returns a handle holding a `JsBox<RenderedCanvas>` — the painted `Surface`
and its `Renderer`. `toBuffer` encodes that surface again at a different format;
**it does not re-render.** Two formats of one picture cost one resolve, one
measure, one layout, one paint, and two encodes.

`JsBox` requires `Finalize`, so the Skia surface is freed when the handle is
collected. No `FinalizationRegistry`: v1 needed one because its canvas lived in
a worker, and this one does not. `toBufferSync` needs no `Atomics` bridge for
the same reason. `release()` exists for a caller that will not wait for a
collection.

### Where the overhead is

The crossing is one call, so the only JavaScript cost that scales with the scene
is building the tree and encoding it.

**Node objects are monomorphic** — the same keys in the same order on every
node, absent fields present as `undefined`. A node that sometimes carries `src`
gets a second hidden class and deoptimises every property read in the encoder.

**Styles are read, never copied.** No spread, no per-node defaults merge; the
defaults already exist in Rust.

**Encoding is one pass** into a preallocated `Float64Array` grown by doubling,
written with plain typed-array stores. `meo-skia-canvas`'s `drawlist.js`
measured why that shape matters: a store into a `Float64Array` is one operation,
and reading the value back out of V8 from Rust was 39ns of the 82ns a `lineTo`
cost.

## Workspace

```
crates/meo-canvas-scene    Scene types and the binary codec. No Skia, no taffy, no neon.
crates/meo-canvas-core     resolve, measure, layout, paint, encode.
crates/meo-canvas          The crates.io surface. Nodes, one flat `Style`, units.
crates/meo-canvas-node     The cdylib. The only #[neon::main].
crates/meo-canvas-cli      The binary.
packages/meo-canvas        The npm surface. TypeScript, and the arena encoder.
```

`meo-canvas-scene` is separate from the core because the CLI, the addon, and the
fixture tooling all need to read a scene, and none of them should link Skia to
do it.

### Module layout

`src/lib.rs` declares `mod foo;`, which is the file `src/foo.rs`. That module's
children are `src/foo/bar.rs`, declared `mod bar;` inside `src/foo.rs`.

**There is no `mod.rs` in this repository at any depth.** No lint enforces this
— not rustc, not clippy, not rustfmt — so `just layout-check` does, and it runs
as part of `just ci`.

Every file carries a `//!` module doc, because `missing_docs` is denied. Use
`//!` inside the file, never `///` above the `mod` declaration; both compile,
and a tree that mixes them reads as two conventions.

## The behavioural target

Chrome. Where a question has a CSS answer, the answer is what Chrome does.

`../meo-canvas-old` is the reference implementation of that target. It was built
to match Chrome, so where this renderer and that one disagree, that one is
right — read it before inventing a rule. Its line boxes come from the face's own
ascent and descent rather than a paragraph engine's default, and that is why its
text measures like a browser's.

The exception is where v1 itself diverges from Chrome. Its bare `Box` inherits
Yoga's raw defaults, a column direction with `flex-shrink: 0`, where Chrome's
`display: flex` is a row with `flex-shrink: 1`. Chrome wins there. A divergence
of this kind is worth a comment naming both behaviours, because the next reader
will otherwise assume the reference was not consulted.

## Conventions

### Comments

1. **A comment states what the code is, today.** Never what it was, what it will
   be, or what changed. Git records history; comments record the present. No
   "used to", "changed from", "now", "no longer" — and no TODO, FIXME, or XXX.
2. **A comment earns its place by answering "why this and not the obvious
   alternative".** If the code already says what it does, the comment says why
   it does it that way. Restating the code is worse than silence.
3. **When performance is the reason, cite the measurement.** A number, not an
   adjective. "82 nanoseconds, of which 17 is the crossing" is a reason; "for
   speed" is not.
4. **When a test pins a behavior, name the test.** A reader who wants to change
   the behavior should be told where it will fail.
5. **`//!` for module rationale, `///` for item rationale, `//` for a decision
   only a maintainer needs.** If a reader outside the file would act on it, it
   is a doc comment.
6. **Present tense, indicative.** "Rejects a radius below zero, as a browser
   does." Not "will reject", not "should reject".

### Constants

Every value that is a judgement gets a named `const` whose doc comment justifies
the magnitude, not merely the strategy. "Bounded so a long-running process
cannot grow it without limit" explains the bound; it does not explain 4096.

No clippy lint checks this. `clippy::magic_numbers` and its plausible spellings
do not exist, and `unreadable_literal` only demands `100_000` over `100000`.
The rule is enforced at review. `missing_docs` catches an undocumented public
constant, which is half the job.

### Layout defaults

`LayoutStyle::default()` is CSS's: `Display::Flex`, row direction, `flex_shrink:
1.0`. A bare `Box` therefore lays its children out in a row and lets them
shrink.

Yoga's raw defaults are a column direction and `flex-shrink: 0`, so a bare box
changes meaning between the two. `Column` and `Row` are unaffected, and they are
what most trees name: both set their direction explicitly, and both already set
`flex_shrink: 1.0` rather than inheriting Yoga's `0`. Following CSS makes the
bare case agree with the named ones instead of inheriting an exception.

### Errors

The core returns `Result<_, MeoError>` with a variant per failure class. The
addon maps those to JavaScript exceptions, the CLI to exit codes, and Rust
callers match on them.

`unwrap` is denied. `expect` warns, and is allowed where its message explains
the invariant that makes it unreachable.

### Performance and memory

Measured on a 111-node page, GPU off, by `just bench`:

|                             |          |
| --------------------------- | -------- |
| full pipeline               | 22.95 ms |
| draw, without encode        | 9.86 ms  |
| re-encode a painted surface | 9.00 ms  |
| `resolve`, 551 nodes        | 43.71 µs |
| `z_ordered` over 551 nodes  | 1.92 µs  |

**Encoding is more than half the pipeline.** Nothing in resolve, layout or paint
is where the time goes at this size, which is why separating rendering from
encoding is worth more than any allocation fix: a second format costs 39% of a
fresh render rather than 100%.

Two allocations look wasteful and are not worth removing, measured rather than
argued. `resolve` clones a `ResolvedText` per node and again per child, which is
some fraction of 43.71 µs against a 22.95 ms pipeline. `z_ordered` clones and
sorts every container's children, which is 1.92 µs across a 551-node tree. Both
are real observations about the code and false as performance problems; a `Cow`
in that signature would make the hot path read worse to save nothing. Do not
change either without a number that says otherwise.

Allocation in the paint stage is on the critical path for every frame of an
animated render. Prefer reusing a buffer over allocating per node, and say in a
comment what the reuse is worth when it is not obvious.

## Workflows

`just` drives everything. `just` alone lists the recipes.

```
just                  List every recipe.
just setup            First-time setup on a fresh clone. Idempotent.
just ci               What CI runs. Everything below that reports rather than rewrites.
just build            Build the workspace, including the addon.
just test             Unit, integration, and doctests.
just typecheck        tsc --noEmit over the TypeScript surface.
just coverage         Enforce the 90% floor. Exits non-zero below it.
just coverage-open    HTML report, no floor.
just lint             clippy with autofix. Rewrites the tree.
just lint-check       clippy without fixing.
just fmt              rustfmt and prettier. Rewrites the tree.
just fmt-check        rustfmt and prettier without writing.
just layout-check     Fail if a mod.rs exists.
just docs             Fail on any rustdoc warning.
just unused           Dependencies declared but never imported.
just fixtures         Render every fixture and compare it to its committed image.
just fixtures-accept  Accept one fixture's current render as its expected image.
just clean            Remove all build output.
```

A bare verb rewrites the tree; the `-check` suffix is the variant that reports
instead. `just ci` uses only the reporting variants.

`fmt` formats Rust, JavaScript, TypeScript, and Markdown in one command. One
recipe rather than a per-language pair, because a narrower recipe lets someone
format half the tree, see it succeed, and push a tree that `fmt-check` refuses.

The Rust half runs on a pinned nightly named by the `fmt_toolchain` variable,
because `rustfmt.toml` uses unstable options that stable silently ignores —
stable `cargo fmt` would report clean against weaker rules than CI applies. The
prettier half runs the copy in `node_modules`, installed from the tracked
lockfile, rather than reaching the network for whatever `npx` resolves to.

The vendored JavaScript under `packages/meo-canvas/vendor/` is excluded from
formatting. It arrives in another repository's style, and reformatting it would
rewrite every line and destroy the diff against upstream that makes applying a
fix from there possible.

That tree carries its own `LICENSE`, copied from the project it came from. MIT
asks for the notice to travel with a substantial portion of the code, and
another project's entire JavaScript surface is one. It sits beside the code
rather than in the root `LICENSE`, so it stays true if the vendored copy is
ever dropped and so nothing suggests the upstream author holds copyright in
this repository as a whole.

### Local iteration against meo-skia-canvas

`meo-skia-canvas` is a crates.io version pin. To build against a sibling
checkout, create `.cargo/config.toml`, which is not tracked:

```toml
[patch.crates-io]
meo-skia-canvas = { path = "../meo-skia-canvas" }
```

A path dependency in `Cargo.toml` would break CI, which clones this repository
alone.

### CLAUDE.md

`AGENTS.md` is the only internal prose in this repository. `README.md` and
`LICENSE` are tracked because crates.io and npm render them and
`clippy::cargo` requires the metadata that names them; they are outward-facing
package documentation rather than notes to contributors. Nothing else prose-like
is tracked, and `.gitignore` denies by default so it stays that way.

`CLAUDE.md` is a symlink to `AGENTS.md`, created by `just setup` and excluded by
`.gitignore`, so the same text is reachable under both names without a second
copy to keep in step.

## Testing

Three layers.

**Unit tests** cover `meo-canvas-scene`, the codec, and each core stage. These
are pure logic and carry most of the coverage.

**Golden fixtures** in `fixtures/` are scenes rendered by a `Renderer` inside
`crates/meo-canvas-core/tests/fixtures.rs` and compared against committed
images byte for byte. This is how the paint stage is covered:
executing a fill proves the line ran, not that the pixels are right, so paint is
verified by comparison rather than assertion. The fixture runner is part of the
coverage harness, not outside it.

**Doctests** run every example in the crate documentation. Examples compile
against the real public API, so they cannot rot.

A fixture is portable because the harness makes it so, and a contributor adding
one needs to know how: it registers exactly one font from this repository under
the family `Fixture` and refuses a scene naming any other, pins the scale, and
turns `gpu` off. The platform's installed faces answer `has_family` too, so a
fixture asking for Helvetica would pass here and differ on any other machine.

Coverage is measured with `cargo-llvm-cov` on the pinned nightly, because
`--branch` needs `-Z coverage-options=branch` and stable rustc refuses it. That
is the same toolchain `fmt` uses, so the pin is one date to move rather than
two.

The floor is 90% on lines and regions, which are the only dimensions the tool
can fail on — there is no `--fail-under-branches`. Branch percentages reach the
report and `target/lcov.info` for reading; regions is what refuses a merge. A
region is a span with its own arm count, so an untaken arm still lands in the
number that gates.

Source-based coverage instruments Rust only, so Skia is linked but never
instrumented and its lines never enter the denominator.

Nothing is excluded from the denominator. A file earns an exclusion by being
generated rather than written, and the rule when one does is that it is named by
path in the `coverage` recipe, one path at a time, so the list is reviewable in
a diff. Code this project implements stays in the denominator.

## Before publishing

This document and the three README files describe the design. Some of what they say is true of
the architecture and not yet of any code, which is fine while nothing is
published and false the moment something is.

Re-read every capability claim against what runs, and cut or qualify whatever
does not hold. The sentences that need checking are the ones asserting where
work happens, what formats encode, and what a surface accepts — "layout, text
shaping, painting and encoding all happen in Rust" is the shape of the problem.

AGENTS.md is the likeliest of them to be ahead of the code, because being ahead
is what it is for: a section describing something unbuilt says so at its top,
and that marker comes off in the change that builds it.

The same applies to `repository` in the workspace manifest, which names a remote
that has to exist before `cargo publish` will accept it.

## Dependencies

Every dependency is on its latest stable release.

|                   |      |                                                             |
| ----------------- | ---- | ----------------------------------------------------------- |
| `meo-skia-canvas` | 0.11 | Skia, text shaping, encoding. `default-features = false`.   |
| `taffy`           | 0.13 | Flexbox, CSS grid, block layout. Without `calc`.            |
| `neon`            | 1.1  | Node addon.                                                 |
| `clap`            | 4.6  | CLI.                                                        |
| `thiserror`       | 2.0  | Error types.                                                |
| `ureq`            | 3.4  | Remote images, behind the CLI's optional `net` feature.     |
| `png`, `gif`      | dev  | Decoding output back in tests; a byte count proves nothing. |

The core performs no network I/O and requires no async runtime. It accepts bytes
or a reader, so a Rust caller with no runtime and the CLI are served by the same
code.

`taffy::TaffyTree` is neither `Send` nor `Sync`: taffy represents every length
as a tagged pointer, so `Style` itself holds a `*const ()`. No feature set
changes this. A tree is therefore built and consumed on one thread and never
crosses a boundary — which costs nothing, because `Scene` carries its own
style type and taffy's `Style` exists only inside the layout stage.
