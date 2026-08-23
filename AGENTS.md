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

**Children take one or many, and a falsy child is skipped.** v1's props type is
`Children | Children[]` where `Children` includes `false` and `undefined`
(`canvas.type.ts:29`, `:1016`), and its constructor wraps a bare child before
filtering (`layout.canvas.ts:43`). So `children: Row(...)` and
`children: [Row(...), cond && Text(...)]` are both valid, and a conditional that
does not render writes `false` rather than an empty node.

Rust reaches the same place through a trait rather than a union: `.children`
accepts a single element, an array, a `Vec`, and an `Option<Element>` that is
`None` is skipped. An iterator goes through `each(..)` rather than directly: a
blanket `impl IntoElements for I: IntoIterator` overlaps `Vec`, `[T; N]` and
`Option`, and rustc refuses the narrowed form because a future std release could
make `Option` an iterator. The syntax differs because the languages do; what a
caller can express does not.

**A change's two sides land in one commit.** Ownership decides who edits a file,
not what a commit is: splitting a wire change across two commits to respect a
boundary leaves `ci` red between them for a change that was never in two parts.

**Neither surface ships a capability the other lacks, and neither is finished
first.** A change to one is not done until the other has it. The two examples
are the check: `examples/bun` and `examples/rust` draw the same picture, and
`just example` runs both, so a surface left behind fails the command rather than
being noticed later.

`just example` compares more than exit status. Both halves render the same scene
at the same size, so their PNGs are compared byte for byte — a difference that
survives identical input is a difference between the surfaces, and the pixels
say so where a passing test does not. That check found the GPU feature gap
recorded under **Rasteriser parity**.

### Rasteriser parity

The GPU backend is a Cargo feature, named for the platform that has one: `metal`
on Apple targets, `vulkan` elsewhere. Neither is default, because a build with no
backend named renders on the CPU, which is what a portable `cargo check` needs.

Every crate a caller can depend on forwards that feature. `meo-canvas-node` is
not the only entry point — a Rust caller reaches Skia through `meo-canvas` and
`meo-canvas-core`, and a feature declared only on the addon leaves that caller
on the CPU with no way to ask otherwise.

`gpu` is a request rather than an outcome, and `Canvas::gpu` reports the request,
so a test asserting `renderer.gpu()` passes on a CPU-only build. The check that
does not is the byte comparison in `just example`: the two surfaces render the
same scene, and CPU and GPU rasterisation of the same scene differ by one or two
levels across the antialiased edges. `Surface::engine` reports what the surface
actually got, and is the thing to read when the images disagree.

**The two surfaces read the same way.** `Root` is the entry point on both, style
properties sit directly on the node rather than inside a nested object, and the
output methods are the same set. A person moving between them should be
translating syntax, not a design.

```rust
let mut canvas = Root::new(520.0, 180.0)
    .background_color(hex("#101014"))
    .children(Row::new().gap(px(20.0)).padding(px(24.0)).children(
        Text::new("Ukasyah").font_size(26.0).bold(),
    ))
    .render(&renderer)?;

canvas.to_file("out.png", Format::Png)?;
```

Setters are flat and chained. They are written once, on a trait every node
implements through a single accessor, rather than once per node type — the
alternative is the same sixty-five methods repeated nine times, which is what
made a nested style object look attractive before the shape was seen in use.

`Style` remains a public type because the scene carries it and because a
reusable base is worth having:

```rust
const CARD: Style = Style::new().padding(all(px(24.0))).gap(px(16.0));
```

A `const` is substituted at each use, so every `CARD` is a fresh value a
`self`-taking setter can consume. That is why those setters are `const fn`
wherever the field allows. The line a setter cannot cross is whether the field
needs dropping — assigning over an owning field in a `const fn` is E0493, which
`gradient` and `mask` hit despite carrying no `String` of their own.

`px` takes an `f32`, so `px(16.0)` and not `px(16)`: Rust does not coerce an
integer literal, `impl Into<f32>` cannot be `const`, and an `i32` parameter would
lose `px(0.5)`.

Every field stays public and `Style` is deliberately not `#[non_exhaustive]`, so
a property with no setter is still reachable by literal.

## The JavaScript surface

Object literals, not builders. The two surfaces are siblings, so each is
idiomatic in its own language rather than one imitating the other — and this is
the shape v1 already has.

```js
const canvas = await Root({
  width: 800,
  height: 400,
  backgroundColor: '#101014',
  children: Row({
    gap: 16,
    padding: 24,
    children: [Image({ src: 'avatar.png', width: 64, height: 64, objectFit: 'cover' }), Text('Ukasyah', { fontSize: 24, fontWeight: 'bold' })],
  }),
})

const png = await canvas.toBuffer('png')
const jpg = canvas.toBufferSync('jpg')
```

Style properties sit directly on the props object, as they do on the Rust
setters — there is no `style` key on either surface. `RootProps` is
`Style & {...}` and `TextProps` is `Style & ParagraphOptions & {...}`, so a
property a node accepts is a property a caller writes flat.

Same CSS names, same values — `'row'` where Rust has `Row`, `16` where Rust has
`px(16.0)`. The string-literal unions in `packages/meo-canvas/src/index.ts` are
what make `'cover'` complete and `'covr'` a compile error.

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

### Throwing and rejecting are different failures

An argument of the wrong shape **throws synchronously**. A failure inside the
render **rejects the Promise**. The split is not incidental: every V8 read
happens in one pass before the work is handed to the pool, so an argument error
is raised while there is still a call to throw from, and a render error is
raised when there is not.

```js
expect(() => render(notATypedArray)).toThrow(TypeError)
await expect(render(malformedArena)).rejects.toThrow()
```

A test asserting that either one always happens is wrong, and it would fail for
the right reason and be repaired the wrong way.

### What the canvas exposes

v1's output surface, because v1 is the reference and a ported script should not
have to change how it writes a file:

|                                                |                     |
| ---------------------------------------------- | ------------------- |
| `toBuffer(format, options)`                    | bytes, as a Promise |
| `toBufferSync(format, options)`                | bytes               |
| `toFile(path, options)` / `toFileSync`         | write it            |
| `toURL(format, options)` / `toURLSync`         | a data URL          |
| `toDataURL(format, quality)` / `toDataURLSync` | v1 spells both      |

The sync variants are ordinary functions here. v1 needed `Atomics.wait` on a
`SharedArrayBuffer` for them because its canvas lived in a worker; this one does
not, so `toBufferSync` is the same call without the `await`.

`release()` frees the Skia surface without waiting for a collection. v1's method
of that name released a canvas from _worker_ memory, which is bookkeeping this
version does not have — here `JsBox`'s `Finalize` frees it on collection and
`release` only makes it sooner. A server rendering thousands of images wants it;
a script does not need it.

`saveAs` and `saveAsSync` are not carried over. They are v1's deprecated alias
for `toFile`, and a deprecated name reintroduced in a rewrite is one nobody gets
to remove later.

`toSharp` is deliberately absent until someone asks for it: it exists in v1 to
hand pixels to another library, and reintroducing it means taking a position on
that library's version.

### The retained canvas

`Root` returns a handle to a painted surface. `toBuffer` encodes that surface
again at a different format; **it does not re-render.** Two formats of one
picture cost one resolve, one measure, one layout, one paint, and two encodes.

**A retained surface and an off-loop paint are mutually exclusive**, and the
compiler settles it: `RenderedCanvas` holds a `SkPictureRecorder` and an
`Rc<RefCell<Gradient>>`, so it is `!Send`, and `cx.task` requires a `Send`
result. The paint therefore runs on the event loop and blocks it for its
duration. `render`, which returns bytes, is unaffected and still runs off the
loop — bytes are `Send` — so the one-shot path keeps the property the retained
path cannot have.

A server that must not block has `render`; a caller wanting several formats of
one picture has the retained canvas. Buying both means a thread owning the
surface with encode requests marshalled to it over a channel: one OS thread per
live canvas, `encode` blocking the loop on a round trip for no gain over
encoding inline, and `release` having to join. That is the shape v1 had, and it
had it because its canvas lived in a worker.

`Root` is async because a page builder may fetch, not because the paint is off
the loop.

The surface is held by closures over an `Rc<RefCell<Option<RenderedCanvas>>>`
rather than in a `JsBox`. A `JsBox` is reachable only through `this`, so
`const { encode } = canvas` would break silently — and the TypeScript side keeps
the native object in a private field, one refactor from doing exactly that. napi
frees the captured data when the closures are collected, so the finalizer
property is intact and `release` only makes the free sooner. No
`FinalizationRegistry`, and `toBufferSync` needs no `Atomics` bridge: v1 needed
both because its canvas lived in a worker. `release()` exists for a caller that will not wait for a
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

**The browser is the baseline for behaviour. v1 is the baseline for the API.**
What a property _does_ is what Chrome does with it; which properties _exist_ is
what `../meo-canvas-old` offers. The two are answered by different sources and
neither overrides the other in the other's half.

That resolves the case where v1's shape comes from a limitation rather than a
decision. Its radial gradient is a circle because `ctx.createRadialGradient`
makes only circles, and CSS's default is an ellipse -- so the property exists
because v1 has it and behaves as an ellipse because a browser does. It is the
opposite of the text pipeline, where v1 breaks its own lines _because_ a canvas
has no paragraph, and doing it v1's way is what makes the behaviour a browser's.
Ask which of the two a v1 choice is before copying it.

Where a question has a CSS answer, the answer is what Chrome does.

### Three questions answered and closed

**Baselines from measured text: fixed upstream, unreleased.** taffy 0.13's
measure closure returns `Size<f32>`, so a text leaf has nowhere to report a
baseline and `align-items: baseline` falls back to the box's bottom edge. The
maintainers' own changelog has the measure function returning `LayoutOutput`
instead, **explicitly so measure functions can set baselines** -- so there is
nothing to file and nothing to work around. **When a release carries it,
confirm the _released_ signature rather than the changelog**: this whole
question exists because a released signature said otherwise. And test it on
`align-items: baseline` over mixed font sizes, which is the only arrangement
where the fallback and a real baseline differ -- **a fixture that passes before
and after an upgrade has not tested the upgrade.**

**Whole-pixel rounding against Chrome's sixty-fourths: measured, characterised,
not opened.**

**Layout and paint are different stages and the claim differs between them.**
Chrome's _layout_ is fractional -- it snaps a length into sixty-fourths once and
accumulates exactly, so `getBoundingClientRect` reports `67.171875`. Chrome's
_painting_ is not: each edge is rounded to a whole CSS pixel and then multiplied
by the device scale, so at `dpr 2` it discards precision it holds. **"Chrome
never rounds to integers" is true of layout and false of paint**, and a
statement that does not say which stage it means is wrong half the time.

**Both engines paint crisp edges at both scales, and still disagree.** Rendered,
eight boxes of `10.3`:

```text
ours    dpr 1   10  21  31  41  52  62  72
chrome  dpr 1   10  21  31  41  51  62  72
```

**Crisp said nothing about where.** Our per-box rounding does not accumulate --
`round_layout` rounds _cumulative_ coordinates and differences them, so every
edge is `round(its exact position)` and the _wobble in the box heights is what
keeps the edges true_ rather than what drifts. The disagreement is not drift.

**It is a whole CSS pixel exactly where the accumulated exact position lands on
a half, and nowhere else.** Five boxes of `10.3` sum to exactly `51.5` and we
round up; Chrome snapped to `10.296875` first, reaches `51.484375`, and rounds
down. **Chrome never sees a tie because the snap has already nudged the value
below it.** Six of the seven edges agree to the integer. In `f32` the
representation error cancels rather than pushing past the boundary, so the tie
is real in our arithmetic and not an artefact of checking it in doubles.

**The fix, named so nobody starts from the wrong one: snap each length into
sixty-fourths before accumulating**, so the tie never forms. **Not** _round
differently_ -- rounding is not where the two part -- and **not** _disable
taffy's rounding_, which was costed twice and was wrong both times: turning it
off makes adjacent boxes meet on fractions and antialias against each other,
trading a bounded difference for a visible one at every shared edge.

**Unknowns, stated rather than discovered later**: it is a quantisation applied
to every length entering layout; percentages and `auto` are unconsidered; and it
moves every fixture in the tree. A third option nobody has costed is to **solve
in device pixels and paint 1:1**, which puts the rounding on the device grid and
keeps the seam-free property. `LAYOUT_SCALE`'s note argues against solving at a
device scale because paint would round a second time; **that rules out the
version where both happen**, and which one its author meant is not recorded.

**CSS table layout: nobody has asked for it.** v1 has none, no fixture wants
one, and taffy has four display modes of which none is a table -- so it would
be a layout algorithm we write and maintain in a tree whose whole layout story
is that taffy does it. Three of the four things a table adds over a grid are a
grid with different spelling; the fourth, **row groups with header repetition
across pages**, is the one a grid cannot express and the only reason to revisit.
**A decision to defer is a decision**, and this is it.

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

### A property whose whole meaning is a DOM event

**The two-baseline rule was not written for a property that only means
something inside a document.** v1 is a browser-side renderer, so a callback has
somewhere to fire and `alt` reaches an accessibility tree. This renderer takes
a scene and returns bytes: there is no DOM, no observer, and nothing between
the call and the image for an event to happen in.

Three of v1's props are that case, and they **do not exist here**:

- **`onLoad` and `onError`** are v1's asynchronous loading model rather than a
  capability. Loading happens before `render` returns, and a source that cannot
  be read is already an error at the call — a callback would fire into a
  program that is holding the result.
- **`key`** is reconciliation identity, and there is no reconciliation: one
  scene in, one image out, nothing retained between calls to match against.
  `name` is the identity field, carried on the wire for diagnostics.

**`alt` is the one to revisit rather than refuse forever.** It has no behaviour
today, and it has an obvious one waiting: SVG has `<title>`, PDF has alternate
text, and an encoder that wrote either would make `alt` measurable. It stays
out until an encoder uses it, because a property that reaches the wire and is
read by nobody is the defect this project has found six times.

The test to apply: **could a Chrome comparison ever measure it?** If the answer
is no because a browser is answering a question this renderer is not asked, the
property does not exist here, and the omission belongs in this file rather than
in silence.

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

A reason written in two places gets corrected in one. The recipe and the tool
it runs, the emitter and the file it emits, the test and the type it pins —
where the same explanation appears twice, one copy drifts and the other is the
one somebody reads. Write the reason once, at the place a reader lands, and
point at it from the other.

A default that means something different from the same value stated explicitly
cannot be a default. It has to be absent. CSS spells `z-index: auto` and
`z-index: 0` differently -- the first establishes no stacking context and the
second does -- so `z_index` is an `Option<i32>` where `None` is auto, and an
`i32` defaulting to zero would say every positioned node establishes a context.
The same reasoning puts `gpu` at `Option<bool>`, where `None` is the renderer's
choice and `Some(true)` is a caller insisting, and leaves an unnamed inset edge
`None` rather than zero: not pinned is not the same as pinned to zero.

### Constants

Every value that is a judgement gets a named `const` whose doc comment justifies
the magnitude, not merely the strategy. "Bounded so a long-running process
cannot grow it without limit" explains the bound; it does not explain 4096.

No clippy lint checks this. `clippy::magic_numbers` and its plausible spellings
do not exist, and `unreadable_literal` only demands `100_000` over `100000`.
The rule is enforced at review. `missing_docs` catches an undocumented public
constant, which is half the job.

### Stacking

`z_index` follows CSS, which is neither v1's rule nor "every sibling".

CSS 2.1 applies `z-index` to **positioned** elements. Flexbox §5.4 and Grid §6.2
extend it: a flex or grid item takes its `z-index` regardless of position,
because being an item of that container is what gives it a place in the stack.

So a child is stacked by `z_index` when it is positioned, or when its parent
lays out as flex or grid. In a block container a static child ignores it.

v1 documents `z-index` as applying only to absolutely positioned nodes, which is
narrower than CSS. This renderer follows CSS, under the rule that where v1
diverges from the reference the reference wins.

**The rule is measured in Chrome, not assembled from the three
specifications.** A rule read out of three documents is a rule nobody has seen
run. 281 cases are sampled with `elementFromPoint` at the true intersection of
two overlapping boxes: every pair drawn from `static`, `relative`, `absolute`,
`sticky` and `fixed`, under each of `block`, `flex`, `grid`, `inline-block` and
`table-cell`; then every `z-index` pair drawn from `auto`, `-1`, `0`, `1` and
`2`, with the children positioned and again with them static. Hit testing walks
the painting order, so the topmost element at a point is the paint answer.

Each case is measured with every other case hidden. With the whole grid visible
a `position: fixed` box leaves its parent for the viewport and can land on a
neighbouring case, which answers for the wrong pair — four cases did — and
isolating each one also keeps it inside the viewport, which `elementFromPoint`
requires. A case whose two boxes do not actually overlap reports `NO-OVERLAP`
rather than a winner, because a row-direction flex container separates its
children and would otherwise report the later sibling as "on top" of a box it
never covers.

Four results carry the design:

**Display does not change paint order.** All 25 position pairs give the same
answer under all five displays. The painter therefore reads position and
`z-index`, never the parent's `display`.

**A positioned child paints above a static sibling regardless of document
order** — `relative`, `absolute`, `sticky` and `fixed` alike, in all five
displays. These are the only pairs with no `z-index` anywhere in which the
_earlier_ sibling wins.

**For positioned children the `z-index` matrix is identical under block, flex
and grid**, and `auto` ties with `0`: both paint in CSS 2.1 Appendix E step 8,
so tree order decides.

**For static children, block ignores `z-index` and flex and grid honour it** —
and the flex and grid matrix differs from the positioned one in exactly one
cell. A static item at `z-index: 0` beats a later sibling at `auto`, where two
positioned children in the same configuration tie. A static flex or grid item
with any `z-index` creates a stacking context (Flexbox §5.4, Grid §6.2) and
paints in step 8; one at `auto` is not a context and paints as an inline-block,
in step 7. It is one cell out of 25, and it is the cell an implementation that
treats "flex items honour z-index" as "flex items are positioned" gets wrong.

The full table is regenerated by the probe rather than transcribed; the four
statements above are what the painter is written against.

`PositionType` therefore carries three variants where taffy carries two.
`Static` is appended as discriminant `2` rather than given the `0` it would take
if the enum were written today — the discriminants are published in both wire
formats — and it is the `Default`, because CSS's initial value is `static`.

Two things follow from `Static` existing, and both are settled on our side
because taffy cannot express them:

- **Stacking**, as above: `Static` is the only variant that does not stack.
- **`inset`**, which CSS does not apply to a static element and taffy's
  `Relative` would honour. `to_taffy_inset` drops it. Measured too: a static
  child given `top: 30px; left: 30px` sits at its flow position in a block, a
  flex and a grid container alike, so the drop reads only the child.

`fixtures/block-stacking` and `fixtures/block-stacking-relative` are the same
scene differing only in that variant, and their images differ only in the
overlap.

`Fixed` and `Sticky` stack as positioned variants: each beats a static sibling
in all five displays, and each sits in the same cell of the matrix as
`Relative`. They differ from `Relative` in where they resolve, not in when they
paint.

### Stacking contexts

A child at `z-index: -1` sinks behind its parent's background unless the parent
establishes a stacking context, so hit testing at the child's centre names the
parent when the trigger made no context and the child when it did. That is the
probe; 27 triggers were run through it.

Creates one:

| trigger                                      | note                         |
| -------------------------------------------- | ---------------------------- |
| `position` + a numeric `z-index`             | `relative`, `absolute` alike |
| `position: fixed`                            | with no `z-index` at all     |
| `position: sticky`                           | with no `z-index` at all     |
| `opacity` below 1                            | `0.99` is enough             |
| `transform` other than `none`                | `translateZ(0)` measured     |
| `filter`, `backdrop-filter`                  | `blur(0px)` is enough        |
| `clip-path`, `mask-image`                    |                              |
| `isolation: isolate`                         |                              |
| `mix-blend-mode` other than `normal`         |                              |
| `will-change: transform`                     | and `will-change: opacity`   |
| `contain: paint`                             | and `contain: layout`        |
| `perspective`                                |                              |
| a flex or grid item with a numeric `z-index` | position irrelevant          |

Does not:

| trigger                                 | note                                                           |
| --------------------------------------- | -------------------------------------------------------------- |
| `overflow: hidden`                      | clips, and stacks nothing                                      |
| `position: relative` at `z-index: auto` | positioned is not enough                                       |
| `opacity: 1`                            | the property is not the trigger, the value is                  |
| `transform: none`                       | likewise                                                       |
| `display: flex` or `grid`               | the container is not a context; its items with a `z-index` are |
| a flex item at `z-index: auto`          |                                                                |

`overflow: hidden` is the one to hold on to. It is the trigger most often
assumed, it clips its children, and it leaves them in the parent's stacking
context — a negative child still paints behind the parent's background.
`will-change: opacity` creating a context while `opacity: 1` does not is the
mirror of it: the declaration is a promise about the future value, and Chrome
honours the promise.

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

### What a check can and cannot see

Each of these cost a bug or most of a day to learn.

**Where a v1 API rests on a TypeScript type-level construct, the JavaScript
surface carries it and the Rust surface carries what the language expresses
instead.** v1's `parallel` takes a record of named members and returns a record
of their values, assembled by TypeScript's mapped types. TypeScript can express
that, so the JavaScript surface keeps it whole -- v1 is the baseline for which
APIs exist, and narrowing a surface that can carry one is a loss for no gain.
Rust has no need of the construct: a caller with three tracks writes a struct
with three fields and calls each, which is what the mapped type was
reconstructing. What does **not** fall out is the timing, because the members
differ in value type and only their durations compare -- so the Rust side is
`animate::sequence::longest` and its doc says why it is not `parallel`.

**This is not the same as a policy asymmetry, and the two must not be confused.**
A URL that fetches on one surface and refuses on the other has to be equalised,
because a caller issuing the same request gets different behaviour. The test is
one question: **could a caller observe the difference as behaviour, or only as a
different way of writing the same thing?** `parallel` against a struct is the
second -- identical values, different assembly. Fetching against refusing is the
first.

**A stale artifact does not announce itself as stale -- it reports a type error
that is false.** `just docs` failed with `the trait bound ImageSource: Hash is
not satisfied` against a derive sitting three lines above the enum, while
`cargo clippy --workspace` and `cargo build -p meo-canvas-core` both passed on
the same tree seconds either side of it. `cargo doc` had cached a
`meo-canvas-scene` from before the edit and type-checked against that;
`touch`ing the file cleared it and nothing else changed. **If a compile error
names something the source plainly has, suspect the artifact before the
source: `touch` and re-run before investigating.** The cause is ours and it
will recur -- an edit landing while another session's cargo holds the build
records a fingerprint against the old file. It is the tree-moving-under-a-run
hazard in the one form that does not look like it: **not a wrong result, a
wrong error.**

**A stale instrument reports something plausible rather than failing, and it
has now done so three ways.** `cargo doc` type-checked against a cached crate
and named a derive that was plainly present. A probe left in `tests/` outlived
its deletion by long enough to catch another session's gate. A built addon on
the old wire layout reported `slot 23 holds 2.5, which is not an integer` --
a perfectly sensible complaint about a world that no longer existed, and the
first instinct was to look for a field-order mistake. **Each was diagnosed as a
defect in the thing being measured before anyone suspected the thing
measuring**, which is what makes the family expensive rather than merely
annoying. `touch` the file, delete the probe, `just addon`.

**The near neighbour of that family, which is not in it, and the one test that
tells them apart: re-run and see whether the answer changes on its own.** A
stale instrument keeps its answer until you clear it. A moving subject does not
need you to. At the moment of the report the two are indistinguishable -- both
are a tool you trust saying something the source contradicts -- and they take
opposite guards, so the discrimination has to come first.

A clippy run reported `more than 3 bools in a struct` at `bar.rs:55` and the
gate was genuinely red. Minutes later it exited 0, unchanged by `touch`, so
nothing was cached. The line number placed the read: the struct sits at 66 now
and at 51 in `HEAD`, and 55 is where it sat in the window **after** the fourth
bool arrived -- the one that makes the lint fire -- and **before** the `expect`
above it did. The reading caught the tree between two edits about a minute
apart.

Nothing here was stale. The instrument was current, its inputs were current,
and its answer was true of the tree it read. **What moved was the subject** --
so the guard is the opposite of `touch`: not "make the tool prove what it
looked at" but **a reading of a shared tree is a reading of a moment, so report
it with the commit or the timestamp that names which moment.** A line number
alone is unresolvable from the other side without reconstructing the window by
hand, which is what it cost here.

This entry then happened to itself. Two of us were asked to write it, an hour
apart, neither told the other had it -- **two readings of one shared tree at
different moments, each acted on.** It was caught only because `git status`
still listed `AGENTS.md` as modified. **An uncommitted file is precisely where
this guard has no purchase**, because the commit is the thing that makes a
moment nameable.

**A third mechanism in that family: an artifact replaced underneath a running
test.** Two `just ci` runs in this tree share one `target/llvm-cov-target`.
One process relinks a test binary while the other is executing it, and the
second sees a `SIGKILL` or a missing path depending on which side of the swap
it lands on:

```
element::Element::max_lines   Test executable failed (signal: 9 (SIGKILL))
element::Element::with_style  Couldn't run the test: No such file or directory (os error 2)
```

**Both read as defects in the code.** A killed doctest looks like a stack
overflow or a runaway allocation; a missing file looks like a broken build
script. Neither points anywhere near the real cause, and a plausible wrong
explanation was to hand -- the machine was under load -- **which is what made
it worth checking rather than accepting.** `No such file or directory` is not
something memory pressure produces, and that mismatch is the only thing in
either report that pointed at the truth.

**A failure's most available explanation is not evidence either.** This one was
read as memory pressure on a machine that genuinely was under load, and a
scripted revert that had silently replaced nothing was read as a test that
could not discriminate. Plausible, available, wrong, twice in one day. It is
_a test that fails is not evidence about which side is wrong_, one level out:
**the failure offers an explanation, and the explanation is not the finding.**

**The second symptom is the diagnostic one**, and whoever meets this next will
most likely have only the first. A lone `SIGKILL` is genuinely ambiguous and an
environmental story is usually available -- ours was a machine under load, and
it was accepted once before the pair turned up. One kill _and_ one empty path
is what a relink underneath a running executable looks like from the two sides
of the swap.

The discriminator is the same: re-run and see whether the answer changes on its
own. It changed for both sessions.

**The remedy is `CARGO_TARGET_DIR` per session, with two limits worth stating
because a written-down remedy gets trusted.** Measured, not assumed: `cargo
llvm-cov` honours it, and `target/llvm-cov-target` is then untouched, so the
binary-swap collision does go away. But the `coverage` recipe writes
`--output-path target/lcov.info`, **a literal relative path that no target-dir
setting moves**, so two gates still write one file -- a far milder collision,
written once at the end rather than executed while being relinked, and not
nothing. And a fresh target directory is **a cold build**, so this is a choice
between paying a full rebuild and waiting, not a free fix; for a single run,
waiting is usually cheaper.

Note the tension it sits in, because it is real: each session verifying the
gate independently rather than reading another's word for it is the right
discipline, **and it is exactly what puts two `cargo` processes in one target
directory.**

**A bound satisfied exactly by the worst case cannot see the worst case.**
`rounding_drift.rs` asserts our edges stay within half a pixel of exact.
Edge five is **exactly** `0.5` away -- it passes, and it is the one edge where
we disagree with Chrome by a whole pixel. The test is not wrong and it was one
ulp from being the test that caught it. **An inequality whose boundary is the
interesting case is an assertion that stops meaning anything at the moment it
matters**; pin the worst case as a value instead, so a change that moves it
fails in both directions. Same family as an absence assertion with no presence
beside it.

**Two guards fail against different mistakes, so a row wants both.** A browser
row screenshotted with `scale: 'device'` rather than `deviceScaleFactor` came
back the same size at both settings -- **the second scale was never tested and
both rows reported the same answer for one reason rather than two**; the frame's
dimensions were the tell. A second attempt laid eight boxes in a _row_ rather
than a stack and reported one edge at 10: **it did not look like an error, it
looked like a stack with one boundary**, and the frame size would not have
caught it. **Print the frame and assert the feature count.** One catches a scale
that never varied; the other catches a subject that was never built.

**An instrument can also be silent about what it never saw, and that is the
quietest of the three.** Four scoping documents were written to `docs/`, a
directory the deny-by-default `.gitignore` does not allow. They were correct,
every gate passed, and **they were not in the repository** -- `git status` does
not list an ignored file, so nothing distinguished them from files that did not
exist. **Not a wrong result and not a wrong error: an absence.** An instrument
that lies, an instrument that errs, and an instrument that says nothing at all.

**A `.gitignore` that denies by default is the right design and this is its one
cost.** Anything written outside a path the allowlist already names needs that
path checked once, deliberately, at the moment it is invented -- not later,
because there is no later signal.

**`cargo check --workspace` does not compile `#[cfg(test)]` code, so a struct
field can be complete everywhere the library looks and absent everywhere the
tests do.** A new field on `NodeKind::Path` passed a clean `check --workspace`
and then failed `lint-check`, which uses `--all-targets`, in the scene crate's
own tests and in two other crates' test modules. **It is the `-p is not the
gate` rule one axis over: not the wrong crate, the wrong target.** Anything
that changes a type's shape wants `--all-targets` before it is called done.

**A probe belongs outside `tests/` entirely.** A module under `src/` is opt-in
and is inert until something declares it; **a file under `tests/` is opt-out
and there is no declaration to withhold** -- every file there is its own cargo
target, compiled unconditionally. The window between writing a probe and
deleting it is long enough to catch someone else's gate, and it did. Write
probes in a scratch directory the compiler cannot reach.

**A capability promised at the layer that has it, and blocked at a layer that
never mentions it, is invisible to every test we write.** `ImageFormat::Ico` is
in `ALL`, encodes, round-trips, and its doc at `encode.rs:56` promises the
thing only it can do -- _an icon at 16, 32, 48 and 256 pixels is one file_. No
caller can ask for that: `lib.rs:277` begins every page at `scene.size`, so the
pages cannot differ. **The encoder is correct and proves it; the promise is
unreachable and nothing fails.** Both halves are tested and the defect is in
neither. This is the same shape as `fitted_dash(48.0, 4.0)` asserting the
arithmetic while the renderer handed it a different length -- **the claim and
the check on opposite sides of the boundary the defect lives on.** So: when a
doc comment promises a capability, find the caller that can reach it, and if
there is none, that is the finding.

**A claim about the format and a test of the arithmetic sit on different sides
of the defect between them.** Two in one day, both in the dashed border. The
renderer fitted the centre line it strokes where Chrome fits the border box, so
a 48-pixel edge got the pattern for 44 -- and `chrome_border_rhythm.rs` never
saw it, because every assertion there called `fitted_dash(48.0, 4.0)` directly
and got the right answer to a question the renderer was not asking. In the same
file a claim that a dash array _cannot_ express Chrome's `5, 6, 5` stood for
hours and was wrong: a gap of `16 / 3` puts its boundaries at 8, 13.33, 21.33,
26.67, 34.67 and 40, and rounding each where it falls draws exactly that. It
survived because it was reasoning about the format while every check asked the
arithmetic, and nothing in the suite could contradict it until something
rendered. **So a helper's own test is not a test of the feature: one assertion
has to go through the renderer and read the ink back**, which is what
`the_renderer_draws_the_runs_chrome_draws` is for.

**A layout needs an input its own rules can act on, not merely a property
that does.** The rule above is usually applied to a value -- an opaque asset for
an alpha mask, a tile a box divides evenly. It applies to arrangements too. A
grid table measuring `grid-auto-flow` gave its spanning item a span of two in a
three-column grid: that **fits** beside the first item, leaves no hole, and
`dense` has nothing to go back for -- so `row` and `row-dense` came out
byte-identical and the table would have reported two working keywords as one. A
span of three cannot fit beside anything, starts a new line, and leaves the hole
the keyword exists for. Ask what the _arrangement_ has to contain before the
property has anything to do, not only what the value has to be.

**The sampling has to be finer than the feature.** Four samples per pixel hid
the marks where two per-side dashes butt at a corner, and made a radius of 8
read as a radius of 4 -- and on a curve several consecutive samples land in one
pixel and read as ink continuing rather than as a mark ending. The same error
in the other axis is the sixty-pixel window that made a 137-pixel edge look
uniform when its slack had simply not accumulated yet. **A run that looks even
because the remainder has not arrived is not an even run.**

**A check written in the same currency as the thing it checks agrees with it
whatever it does.** A probe and the bytes it is compared against are written
from one number, so a units error in that number encodes identically on both
sides: `'50%'` reaching the painter as five thousand per cent passed the round
trip, the 54-property boundary comparison and the two surfaces' own byte
comparison at once. `fixtures/percentages` is the answer, in rendered pixels.
The same shape recurs -- a float pixel layout changes compositing depth whether
or not the rasteriser falls back, so no comparison of buffers can say which one
drew them, and `Canvas::engine` reports a string instead. When a check cannot be
made in one currency, reach for another currency rather than concluding there is
no check.

**A fixture that pins a defect needs a cell that must not move.** Two cells
flipping while a third holds says the fix was the one intended rather than a
change of sign: `fixtures/stacking-hoist` would pass with only its two defective
cells if the painter started hiding every negative-`z_index` child instead of
hoisting the ones that belong to a grandparent. The control cell is what
separates the two outcomes, and it is chosen from a property the surface can
already express, so it does not have to be built twice.

**A fixture with one value per type checks the shape of a read and not its
kind.** While every slot held `1`, an `Option`'s presence flag read through the
raw slot path and read as an integer were the same read. Only a value the suite
did not contain separates them, which is why the probe reader answers by what
the read asks for -- whole where a whole number is demanded, `0.25` where the
slot is taken as written.

**The sampling has to be finer than the feature.** A walk of a dashed border
at four samples per pixel reported the corners of a radius-4 box and a radius-8
box as identical, and the difference between them is the whole question: on a
curve several consecutive samples round to the same pixel, so an under-sampled
walk reads ink as _continuing_ rather than reporting nothing. Sixteen samples
per pixel separated them cleanly. **The instrument did not fail — it returned a
smooth, plausible run**, which is the same family as the degenerate shape and
expensive for the same reason. State the density beside the number, because a
reader with no density will pick the one that looks sufficient.

**Floor a sample point; round a reported value. They are the same expression
and different quantities.** A sample point asks _which pixel does this location
fall in_, and the answer is the floor: `Math.round(0.5)` is 1, so a half-pixel
inset on a one-pixel band samples the row **outside** it and reports a painted
border as blank. A box origin from `getBoundingClientRect` is a value the
browser computed, landing on a device pixel, and rounds -- flooring it shifts
the window rather than fixing it. In `tools/conformance/borders.mjs` both live
a few lines apart: `pixel(shot, Math.floor(point[0]), ...)` in every path walk,
`Math.round(geometry.left)` for every window bound. A later reader tidying up
will unify them; the comment at each site says why they differ.

**A signature has a quantity attached; a sighting only has a location.** Ink
present at a dashed border's corner is a sighting, and reading it as the mark of
per-side fitting inverts the answer: the _continuous_ case has ink there too --
`on:8.1` at a width whose dash is exactly 8, one ordinary dash crossing the
corner. What distinguishes the two is length against `2w`: 26.8 against a 16
dash is two dashes meeting, 8.1 against an 8 dash is one dash passing through.
Before trusting a signature, name the quantity that makes it one, and check the
case that should _not_ show it does not.

**Choose at least one case that amplifies, and read its gain off the formula
before anything renders.** A conformance table full of cases with a gain near
one cannot see a fault smaller than its tolerance -- every row passes and the
table reports a clean sweep it has not earned. Seventeen blend modes were
compared against Chrome and thirteen disagreed; eleven of those were off by a
single unit in one channel, which a tolerance of 1 would have swallowed
whole. The fault was one unit of rounding in the gradient underneath, and the
only reason it was visible at all is that `saturation` divides by the
backdrop's channel spread -- eight units out of 255 at that pixel -- and
returned it twelve units wide, with `color-dodge` returning it three. **The
gain is arithmetic, not an observation**: `saturation` divides by a small
quantity and `hue` multiplies by the same one, which is why the first amplifies
and the second does not, and both facts are readable from Compositing 1 without
drawing anything. So the selection rule is: for each measured quantity, ask
which case's formula magnifies a small error in it, and put that case in.
Corollary, learnt in the same hour: **when the amplifier fires, suspect the
shared input before the amplifier.** Thirteen failing modes were not thirteen
defects.

**A value that depends on a dithered source cannot be pinned across
renderers.** Dither is a per-pixel offset from a pattern tied to device
coordinates and to a Skia build, so matching it is not something to attempt --
and switching ours on would swap one unreproducible offset for another rather
than converge. It is visible inside a table we already had: `gradient-truth`
reads `linear 0deg` at 126 and 125 at two points that are analytically
identical on a vertical ramp, and `180deg` at 130 against 129. The only form of
the check that can pass is to **measure the source on our own surface and
compare in its currency** -- which is what `chrome_blend.rs` does, reading our
own backdrop-only cell and putting Compositing 1's formula through that instead
of pinning Chrome's outputs. And the amplifier rule has a second edge here:
**the higher a mode's gain, the more of its row is dither**, so the case that
is best at finding a fault is also the case least suited to being pinned
against another renderer. Use it to detect, not to certify.

**A field named for a geometric quantity must not hold an instrument's
internal one.** The perimeter walk reports `perimeter=662.2` for a box whose
centreline is 666.2, because the walk takes its extents on pixel _indices_ --
`width - 1 - inset` -- which is correct for sampling pixel centres and four
short round a loop. Two readers took that number and predicted mark counts with
it, and a third of a dash period is enough to move a prediction across a
rounding boundary. **The fix is the name, not the arithmetic**: `walk-length=`,
with the geometric figure noted beside it.

The failure that led there is worth more than the rename. A perimeter was
computed two ways, the two differed by four, and the difference was diagnosed
from the numbers alone -- an implementation _inferred_ to explain a gap, when
the file was open and said otherwise two lines from where it was claimed not
to. **A close-but-wrong agreement is more persuasive than a distant one and
worth less**: the wrong model reproduced the figure to within 0.6, which is why
it convinced, and it convinced its way to a proposal to re-run every reading
that walks along an arc. Before attributing a discrepancy to a mechanism, read
the mechanism. It is the citation rule pointed at our own tools: **take the
measurement, then write the label.**

**A difference no larger than a renderer's disagreement with itself is not a
difference.** Measure the self-disagreement first: it is the resolution floor,
it needs one surface rather than two, and it costs a single read. A dashed
corner cost an hour and three hypotheses over one pixel, and the thing that
ended it was Chrome disagreeing with **itself** -- the same mark reading nine
pixels wide at `y = 0` and eight at `y = 2`, on a radius where the two
renderers agree completely. One pixel, which was the entire size of the
difference under investigation. A between-engine reading of that size always
leaves room to argue about frames, thresholds and conventions; a
within-engine one leaves none, which is why it settles what the comparison
could not. **Take the floor before investigating anything at or beneath it**,
and where a difference survives, say what the floor was so the reader knows the
difference cleared it.

Name the hypotheses that died and what killed each, not only the one that
lived. Three died here -- a shared rasteriser constant, a path measured short
by conic flattening, and patterns differing inside the corner -- each to a
measurement rather than an argument. **A reader who meets only the surviving
explanation will re-derive one of the dead ones**; a reader who meets all three
will not.

**A count is only as good as the separation under it, and nothing in a count
shows you the separation.** This is the previous rule applied one level up: a
signature needs a quantity, so a _count_ of signatures needs the distribution
those quantities fall in. Counting dashed corner marks longer than `1.15 * dash`
gave three at width 4, then two after an unrelated fix, and the move looked like
a regression. The histogram said otherwise -- the marks sat at 10.1 and 10.2
against a continuous control whose own marks reached 9.0 nine times, so a mark
near 9 had never carried information and the count had simply lost one of its
bad members. At width 8 the same measurement separates cleanly: 20.3 and 21.3
against a control ceiling of 17.0, three empty pixels wide on a dash of 16.
**Publish the distribution beside any count that decides something**, and where
two widths disagree in strength, say which one the conclusion rests on rather
than averaging them into a confidence neither supports.

**When a coincidence-prone reading and a structural signature disagree, the
signature wins.** Ink spanning a box's straight portion exactly is _sometimes_
per-side fitting and sometimes a dash boundary landing on a tangent by
arithmetic; a mark longer than a dash can only be two dashes meeting. The first
is an absence that has to be interpreted, the second is a positive signature of
one mechanism. They disagreed three times in one investigation and the
signature was right each time -- including where the coincidence-prone reading
was **not wrong**, merely coincidental, which is what makes it dangerous:
radius 5 really did land flush at both tangents while already being fitted
continuously.

**A scene has to be able to hold the feature being measured.** The
control-pair rule says a property needs an input it can act on; this is the
same requirement on the _shape_. A perimeter walk of a 240x48 box with a 24px
radius reported a seam — and that box has no vertical sides at all, because two
24px corners consume its whole 48px height. The path model produced a
negative-length segment and the walk put a structure exactly there. **The
instrument did not fail; it reported a feature of my own arithmetic**, which is
worse, because a failure would have been noticed. Before walking, tiling or
wrapping a shape, check that the shape still has the parts being measured:
`height - 2 * radius > 0` is a precondition, not a detail.

**Test the subject, not its neighbour.** A check written against the nearest
convenient node measures whatever that node happens to route through, and two
routes to one behaviour is the commonest way for half a fix to look whole. The
refusal of a URL source was written against a _background image_ and passed;
`Image` — the node the property is actually for — went on encoding a URL,
because `writeSource` and `writeImagePayload` each carried their own copy of the
same three-armed branch. One writer was fixed, one reader was satisfied, and the
subject was untouched. When a property has an obvious home, exercise it there
first and reach for a neighbour only to prove the behaviour generalises.

**A control pair needs an input the property can act on.** A pair renders the
same scene with a property and without it and asks only whether the two differ,
which a property that reaches the painter and is dropped there cannot satisfy --
but neither can a working property given nothing to change. Five instances in
one day, each reported as a dead property that was not one: a mask image whose
every pixel was opaque, so the alpha it is read for had no shape; `dither` on a
ramp too steep to band; `vertical_align` on a text node sized to its own text,
which has no leftover space to move the paragraph within; `Round` and `Space` on
a tile the box divides evenly, where there is no remainder to share and nothing
to round to, so both collapse onto `Repeat`; and `MaskShape::Ellipse` in a
square cell, which is the largest circle that fits -- the two keywords drew one
picture, and a fixture of square cells passes with the arms swapped.

**A scene built for the harness's convenience can change the property under
test.** The sixth instance of the rule above and the first at the level of the
whole scene rather than a value in it: a walker comparing 120 overflow cases
against Chrome placed its outermost box with absolute insets, because that is
the natural way to put a box at a known point -- and an absolutely positioned
box **establishes a block formatting context**, which is one of the four things
that stop margin collapsing. Chrome's box was `position: relative`, so margins
escaped there and could not escape here, and fifty-one rows came back exactly
twenty pixels apart. It read as fifty-one renderer defects and as a missing
layout feature, and taffy had implemented that feature in full. The same walker
then forced `overflow` to `Visible` in order to measure the box it clips, which
removed the same formatting context a second time. Placement, sizing and
visibility are not neutral: before trusting a harness, ask which properties it
had to set to build the scene and whether any of them is in the answer. Their siblings failed
the same way from the other side: `backdrop_filter` under an _opaque_ square,
where the filtered backdrop is covered by the node that asked for it, reported
as broken by three separate readers. Before writing the pair, ask what the
property would have to change and whether the subject offers it.

**An assertion that cannot fail on the machine running it is not a test.** A
rasteriser comparison passes vacuously where no backend is compiled, a
`--features`-less run never reaches the branch that matters, and an assertion
against a fake renderer can only ever say that a field was copied -- which is
how `gpu` reached the scene while reaching no pixel. Guard on the precondition
and assert the trivial case explicitly, so the run that cannot check the claim
says so rather than passing.

**"Has this been reported" is a different question from "is this real".** An
hour spent confirming a defect in the source answers only the second, and
reading the code cannot tell you whether the maintainers already know: taffy's
baseline gap was issue #199 with the fix merged as PR #1091 two days after the
release we pin. Search the tracker before drafting a report, not after.

**A conformance run that confirms is not a wasted one.** `object-fit`'s fixture
ended with "needs a Chrome number" for weeks; the measurement agreed with it on
all five rules and on the discriminator, and not one pixel changed. The fixture
is a different artefact afterwards all the same — it moved from _our arithmetic,
twice_ to _our arithmetic and a browser_, which is the only difference between a
suite that is self-consistent and one that is right. A suite where every
measurement is expected to find something is a suite that stops taking the easy
measurements, and the easy ones are what make the hard ones trustworthy.

**A qualification that does not reach the conclusion is the same as not having
made it.** The far-corner reading of a dashed border was declared decisive in a
message whose own earlier paragraph said the edge "divides almost evenly, which
is why only one gap widened" -- and near-even division is exactly what makes a
continuous phase and a per-side fit predict the same picture at that corner.
The caveat was correct, it was written down, and it sat one paragraph above the
claim it should have blocked. Two other readers had to enforce it.

So: after writing a caveat, read the conclusion again **against** it. If the
conclusion would survive the caveat being false, the caveat is decoration; if it
would not, the conclusion needs a measurement the caveat does not undercut. This
is the coinciding-answers rule failing at the level of prose rather than of a
probe, and it is harder to see, because the evidence that should have stopped
you is in your own hand.

**Before trusting a green probe, ask what the wrong answer would have looked
like.** If it looks the same, the probe is measuring nothing. Four probes in the
v1 conformance sweep needed redesigning after their first measurement and every
one had the same fault -- the two answers coincided: an absolute child whose
containing block and parent both sat at x=0, a content-box probe sampled on the
row where the top and bottom borders mitre, a grid whose track origin and page
origin agreed. Each would have reported "we match" and been wrong.

**A sample point has to be on the feature, and on a curve the feature
moves.** Three readings of one border fixture were taken at a location chosen
for convenience rather than for where the thing being measured was: a border's
bottom edge read at the vertical middle, which crosses the left and right edges
and never the bottom; a corner's arc read down the `x = 0` column, where the
outer boundary sits at x≈2.7 at the row in question and the column is not on
the arc until the arc has nearly finished; and a cell cropped and eyeballed
that turned out to have the very defect it was being called the control for.
Each reading was of real pixels, and each answered a question nobody had asked.
Work out where the feature is at the row or column being sampled, and sample
there.

**Two numbers off one picture are not the same measurement until you say what
each one is.** The rule above is about _where_ a sample was taken; this is
about _what the number is_, and it is the same failure one level up. Two
readings of a text row were compared as though they were one: the ink bottom of
the whole word `Hxgp`, which is the descender of the `p`, against the ink bottom
of its leftmost glyph, which is the baseline. Both were real, both were
correctly measured, and the comparison was meaningless. The calibration was
already in hand and unused — the control row's full-word bottoms sat one pixel
from the browser's, where no glyph-only reading lands within ten. Name the
quantity beside the number, and where two readings of one picture are in play,
check that the pair agrees somewhere it must before trusting it where it must
not.

An ink span carries a second name: **the threshold, and whether the background
is near it**. Reading `text-descenders` at a channel below 200 puts the browser
one row longer at every foot; at 240 the `#eee` cell background counts as ink
and three cells become one span. A span without its threshold is not a
measurement, and a threshold near the background is not one either.

**The size of a disagreement says where to look.** Ninety-nine rows of a
hundred and twenty failing is not one defect, it is a premise -- a harness that
suppressed the behaviour it measured, a scene built wrong, an assumption about
a dependency nobody opened. A handful of rows failing is a defect. Doubt the
instrument in proportion to how much of it is failing, and run the reference
cases through the code before changing the code: the margin-collapsing gap that
prompted a task and two dispatches did not exist, and seven of fifteen cases
already matched the browser exactly.

**Every row failing is a sharper trigger than most rows failing.** The first
ellipsis walker had all six rows disagree and the cause was its own reference
render, which was given no width and wrapped into the column it was handed --
three stacked lines' ink read as one line's. It is worth noticing at _all_
rather than at _most_ for the reason the same walker showed a run later: **the
rows that agree are what make the rows that disagree trustworthy.** Four of six
agreeing narrowed two defects to two sentences; six of six failing said only
that something upstream of every row was wrong.

**A column of a data table is data, not a caption.** The same walker abbreviated
a long string in its first column so the table would read nicely, and then fed
that abbreviation to the renderer as the source text. A table read by a machine
has no room for anything written for a person: the moment one column is for
reading, some other column's meaning is quietly conditional on it. Write the
whole value and let the reader scroll.

**The page a thing is measured on is part of the measurement.** The two rules
above are about the sample point and about what the number is; this one is
about the **frame** the reading is taken in, and the three are one rule with
three surfaces. A walker measured a wrapped flex line on a page the same size
as the box, so thirty-two of the second line's forty-four rows fell off the
page and a child two-thirds missing read as a child never placed. A fixture
cell drew three identical children twice, so two lines of identical colours in
identical columns merged into one bounding box and read as one line. Both
reported a renderer defect that was not there. Before believing a reading, ask
what the frame around it could be doing to it: a page that crops, a scene whose
own cells cannot be told apart, a control rendered at a different size from the
subject.

**A citation is a measurement, not a label.** A `grep -n` result is evidence that
a string occurs, not evidence that the line quoted is the line that matters:
taffy's baseline fallback sits at `flexbox.rs:1522` and `:1524` with the `} else
{` between them, and it is spelled two ways -- `baseline.unwrap_or(height)`
there and `first_baselines.y.unwrap_or(size.height)` at `:2037` and `:2047` --
so grepping one spelling finds half the sites. Re-read each cited line in the
file before the number leaves your hands, and re-read a claim against its own
evidence before it does: a measurement printed and then generalised over is a
different failure from a measurement never taken, and only the second is
prevented by measuring.

The rule points at our own reporting as well as at the code, and it costs two
habits. **Check the exit status**: `just ci | grep -E '^error'` returns
_grep's_ status, not `just`'s, so "no lines matched" is a pattern match on
output rather than a passing gate, and a failure phrased outside the pattern
reads as success. **And `-p` is not the gate.** `cargo fmt -p meo-canvas-core` leaves comments
unwrapped where `just fmt-check` fails on them, because the gate runs the pinned
nightly and only that honours `wrap_comments`; `cargo clippy -p` likewise checks
one crate where `lint-check` checks the workspace. A scoped command is a
faster question, not the same question -- so verify with the recipe, and know
that fixing your own file with `fmt --all` reformats a tree you may be sharing.

**Quote the recipe names from the justfile**, not from what
scrolled past -- a chain reported off a terminal is the prefix that happened to
be visible, which is how six recipes came to be described as not having run
when they had.

**An absence test needs a presence test beside it, or it decays into
always-true.** _Nothing here fetches_, _no async runtime_, _no intermediate
pixel_, _this list is empty_ — every one passes when the thing it checks has
been renamed, moved, or deleted, and passes for the wrong reason without
saying so. So pair it: assert the feature exists before asserting it is off,
assert the stack was drawn before asserting it has no blend, keep a row that
**must** discriminate beside the rows that must not. Four instances in a day —
a seam control on a walk, a must-not-discriminate row in a table, three
presence tests beside three absence assertions in a README's guarantees, and a
detector fed a synthetic tree containing `tokio` to prove it fires at all.
**That last is the form to copy**: a guard whose author checked it could report
the thing it exists to report.

The fourth form is the one with no test at all. A README said text was _shaped
and broken by Skia's paragraph engine_; the paragraph engine had been replaced
by our own line breaker, which was most of a port. **True when written, quietly
stopped being, nothing watching.** Prose about the architecture is an assertion
with no assertion mechanism — so either pin it to something that fails, or
expect it to rot at exactly the rate the code moves.

**On a surface with no background, ask alpha — a colour test cannot tell
_nothing_ from _black_.** An unpainted pixel is transparent black: `0, 0, 0, 0`.
A reader that calls anything dark "ink" therefore counts every pixel that was
never drawn, and a chart's first render passed three assertions against a page
containing nothing and a fourth for the wrong reason. **This is worse than a
threshold being slightly off**, which the day produced several of: there the
two readings are close, and here **absence and the darkest possible ink are the
same reading.** So test `a === 255` before testing the colour, or give the page
an explicit background so that "not drawn" has a value of its own. The same
trap sits behind `Format::Raw`: the buffer is premultiplied RGBA and its
transparent regions are zero, not white.

**A correct transform of nothing is nothing, and nothing looks like a bug in
the transform.** Three failures in one day shared a shape: the mechanism was
right and the input was empty, and the empty result read as a defect in the
mechanism. A sampler read a one-pixel band off its own row and returned zero
ink; a chart resolved percentages against a box with no size and drew two bars
nine pixels wide on a two-hundred-pixel canvas; a path was scaled into a node
with no intrinsic size and drew **nothing at all**. **The unit tests passed in
every case** — the `viewBox` transform _is_ right, and it was handed an empty
node. So when something draws nothing, measure the input before reading the
mechanism, and remember that a zero-size box is a legitimate thing for a caller
to produce: a node sized by flex growth, by a percentage of an unsized parent,
or hidden deliberately. **That makes it documentation rather than an error to
throw** — which is the opposite of a chart dividing zero by zero, where the
arithmetic produces a value nobody asked for.

**`cargo check --workspace` does not compile `#[cfg(test)]` code.** A struct
field can be complete everywhere the library looks and absent everywhere the
tests do: a new field on `NodeKind::Path` left six test constructors broken
across three crates while `check --workspace` was clean. **`--all-targets` is
what closes it**, and `lint-check` uses it. This is the `-p` rule one axis over
— not the wrong crate, the wrong _target_.

**A no-op edit is invisible to every check that asks whether the code is
valid.** An edit matched on a line a formatter had since reflowed, so it
replaced nothing and reported nothing; `typecheck` stayed at 0 throughout,
because unchanged code is still valid code. **What caught it was a render
reporting no text ink at all** — a check that asked what the code _did_ rather
than whether it was well formed. After a scripted edit, assert the change is
present rather than that the tree still compiles: `grep` for what you inserted,
or read back the region you claimed to have replaced.

**This vocabulary is complete for things laid out and incomplete for things
drawn.** Three capability gaps surfaced in one sitting — a URL source that no
surface could resolve, an ICO page size that no scene could express, and path
geometry with no normalised space — and **each was invisible from the API and
obvious on first real use.** Charts found all three because a chart is the
first feature that is mostly drawn: a rectangle can be a percentage and a path
cannot, so the one shape that could not be expressed was the one nobody had
drawn yet. Expect the next gap where something is painted rather than placed.

**And a question answered for one consequence of a mechanism has not been
answered for the others.** A `viewBox` under non-uniform scale was checked for
its effect on the **pen** — `Path2D::transform` moves geometry and leaves the
stroke alone, so a line's width is safe. Both of us stopped there. It still
stretches geometry, which is what a **circle** is: a marker authored as an arc
in that viewBox comes out an ellipse, by the ratio of the two scales. The pen
and the circle are distorted by the same thing and only one of them had been
asked about.

**A test that navigates by position is coupled to every future change in
shape, and none of that coupling is about the thing it tests.** Adding a
legend put one wrapper level into a chart's tree, and nine tests broke at once
— **not one of them wrong about what it asserted, every one of them wrong
about where to look.** They now find nodes by name through a recursive search,
which survives a restructure because it never encoded one. `chart.children[0]`
is a claim about layout that a test about gridlines never meant to make.

**A crate's own tests can name what the crate does not export.** Using the
surface finds what testing it cannot: a gradient whose argument had no
exportable name, `into_scene` documented "to write to disk" with nothing that
turns a scene into bytes, `left(..)` unable to express an inset, and
`IntoSides<Dimension>` refusing a `Length` so a `margin` could not be written at
all. `examples/bun` and `examples/rust` reach the package the way a stranger
does, which is what makes them worth running in `ci`.

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

### What only a second implementation can see

The chart port has three checks and they answer three questions. A geometry
table says the two surfaces compute the same numbers. A render says the numbers
put ink where the arithmetic claims. A byte comparison says the two surfaces
_assemble_ those numbers into the same tree. **Nothing but the third can see a
tree built wrongly out of right numbers**, and it found three of them in one
run:

- `with_style` **replaces** a style rather than merging it, so
  `Column::new().with_style(Style::new().width(…))` discards the
  `flex-direction: column` the constructor just set. Three of the four sites
  with that shape were `Row::new()`, where the discarded value is the default
  and nothing changes — **which is why the pattern reads as fine**. Chart code
  uses the flat `Styled` setters after a container constructor for this reason.
- `Iterator::max_by_key` returns the **last** maximum; JavaScript's
  `reduce` with a strictly-greater test keeps the **first**. A five-division
  axis ties constantly (`1.6`, `1.2`, `0.8`, `0.4` are all three characters),
  so the two surfaces sized a gutter from different strings.
- A doc comment described the axis label as pulled up by half its own height
  and the code never applied the transform. **The prose was the specification
  and nothing checked it against the code.**

A rendered check would have asked none of these: the first is invisible where
a degenerate flex line still fills its parent, the second is a few pixels of
gutter, and the third moves a label by half a line.

**A wrong claim leaves a trace and a missing one does not.** Two of those
findings were the same defect in opposite forms. The axis label was caught
because a doc comment described a transform the code never applied -- **the
prose was the evidence**, and a reader comparing the two could see the gap
without running anything. The pie's slice label was missing the same kind of
transform and nothing beside it claimed otherwise, so there was nothing to
contradict: it read as complete code doing exactly what it said, which was
nothing. **An omission with no prose against it is invisible to review**, and
only a second implementation that _did_ write the transform could say it was
missing.

The same shape decides where the fix goes. `pct(p) = Length::Percent(p / 100.0)`
takes an `f32`, so a caller holding an `f64` fraction writes `pct(f * 100.0)`
and the division runs on the already-narrowed value -- one ulp, four bytes of a
line chart, nothing a render could show. **Widening `pct` to `f64` is the
tempting fix and it is the wrong one**: `f * 100.0 / 100.0` in `f64` is within
an ulp of `f` rather than equal to it, and that ulp usually dies in the
narrowing but is not guaranteed to. **A fix that is probably bit-exact is not a
fix for a check whose whole value is bit-exactness.** `fraction()` beside `pct`
removes the round trip instead of shortening it -- exact by construction. Both
names stay, because a caller that genuinely holds a percentage still wants
`pct`, and a note at the site says so before someone unifies them.

### And what only a pixel can

The converse of the byte comparison's strength is its blind spot: **it can only
see a disagreement, so a mistake both surfaces make identically is invisible to
it.** Both wrote `align-items: center` on the label strip's per-slot box —
which is the _cross_ axis, so each label centred vertically and sat against its
slot's left edge. On a 200-wide chart the two labels inked at x 2 and x 102
where the slot centres are 50 and 150.

The bytes matched, because both trees were assembled the same wrong way. No
geometry row covered it, because the slots' own numbers were right and the
mistake was in what the box did with them. Only ink could say where the label
landed. **A cross-surface agreement test measures agreement, not correctness**
— two ports of one misreading agree perfectly.

The fix moves the pinned bytes, so it lands on both surfaces at once: a
one-sided change makes the agreement test fail for the right reason at the
wrong moment, and the next reader diagnoses a port defect.

### Pin the wrong answer when the right one is upstream

taffy resolves a column flex container with an automatic height to **zero**
when its child has `flex-shrink: 0` and a negative main-axis margin. The
container disappears, its background with it, while the child lays out
correctly at the negative offset. Positive margins are right, zero is right,
`flex-shrink: 1` is right -- **and it is the child's `flex-shrink` that
triggers it, not the container's**, which is the opposite of what the symptom
suggests, since the box that vanishes is the container.

Chrome gives the child's outer hypothetical main size in all six rows of that
table and never lets `flex-shrink` into the answer. **So it is a disagreement
with the browser, not with a reading of the specification** -- the distinction
that decided whether to file it, and the reason the measurement was worth
waiting for rather than asserting CSS from memory.

**A test asserting Chrome's 476 would fail today, and a failing test cannot be
committed.** So `taffy_negative_margin.rs` asserts what taffy _does_, with
Chrome's number in the table beside it and the instruction in the failure
message: when this test fails, the defect is fixed, delete it. **The wrong
answer, pinned deliberately, is the notification** -- and without it the defect
is entirely silent, because a caller sees a missing subtree and no error.

**Any v2 caller writing `flex-shrink: 0` with a negative top margin loses the
whole subtree**, and nothing in the vocabulary hints at it.

Check upstream before writing either the pin or an issue: `main` as well as the
release, the changelog, and the open issues. Here `main` at `88125ce` still
had it, the only unreleased negative-margin entry was for block and float
layout, and the one related issue -- #706, closed -- reports sibling sizing and
padding and mentions neither `flex-shrink` nor a container resolving to zero.
**A defect already fixed and unreleased needs a wait, not a pin.**

### A green row is not coverage of a number the case cannot produce

**The four-kinds rule one level down: not a kind that is missing, but a value
the case cannot generate.**

`pct` narrowed to `f32` in the middle of a percentage round trip, so a fraction
scaled up and divided down again rounded twice and moved its last bit. The bar
agreement case never saw it. Its values are 1 and 2 against a maximum of 2,
which gives halves and ones — **dyadic fractions, which survive a `×100 ÷100`
round trip exactly.** Bar was green for the whole life of the defect, and when
the fix landed its bytes did not move: same hash before and after. It took a
chart whose points land on **thirds** — the line case — to produce a number the
defect could damage.

Nothing about the passing row said so. Not the test, not the code, not the
pass.

**The guard: when a check compares numbers, ask what values the case can
actually produce, and whether the defect being guarded against could appear in
them.** A dyadic-only case cannot exhibit a rounding bug; an all-integer case
cannot exhibit a formatting one; a case whose maximum equals every value cannot
exhibit a scaling one. **The case has to be able to fail before its passing
means anything** — which is the same demand as "a case that cannot discriminate
is not a case that agreed", asked of the inputs rather than the assertion.

### Structural coverage is not positional coverage

The chart's byte comparison puts the legend on a different side in each case:
`left` on the line, `top` on the pie, `bottom` on the doughnut. **`right` is
byte-checked nowhere.** It takes the same branch as `left` -- both produce a
`Row` -- so every branch of the frame is covered and one of the four positions
is not.

That is not a hole to fill so much as a claim to stop making: **"all four
positions are pinned" is false and "every branch the positions take is pinned"
is true**, and the two sound alike in a summary. A case named after a value
covers the branch that value reaches, not the value.

Same family as the entry above: a green row that cannot fail for the thing its
name implies.

### A function-valued option has one instrument where everything else has three

A chart's numbers are checked three ways -- rendered, tabulated against the
other surface's arithmetic, and byte-compared between the two ports. **The
formatters and the `render*Item` hooks reach only the first.** A function has
no counterpart to encode, so it cannot appear in a byte comparison at all, and
there is nothing to tabulate.

**Worth stating as a limit of the technique rather than a gap in the work.** No
amount of care makes a callback byte-comparable; the hole is permanent, and
knowing its shape is the whole of what can be done about it. What follows in
practice: a behaviour reachable only through a callback needs its render to
carry the weight three checks carry elsewhere, and should be written knowing
that.

### A language's convenient default is not the other language's

`Iterator::max_by_key` returns the **last** maximum; JavaScript's `reduce` with
a strictly-greater test keeps the **first**. Neither is wrong and neither
surface wrote the rule down, so the two agreed until a tie appeared — and a
five-division axis ties on every chart. **The same family as the fused
multiply-add**: an idiom each language reaches for by default, where the
default differs and nothing in either file says which was meant. Where two
surfaces must agree, the tie-break is part of the specification.

### A repro must be minimal in what it is not testing

Two paint defects were reported against `paint.rs` -- a clipping ancestor that
painted nothing, and a `zIndex: -1` subtree that never appeared -- with
narrowed cases and controls for each. **Neither existed.** Both were one wrong
property in the helper every node in the port was built from: `flexShrink: 0`
where v1 means `1`.

**The repros reproduced faithfully because they imported the same helper.** They
varied the margin, the clip, the nesting depth and the ancestor's height, and
held the wrapper fixed -- so the wrapper was the one thing they could never
implicate. A case that is minimal in the property under test and unexamined
everywhere else is not a small version of the bug; **it is the bug plus a
smaller stage to perform it on.**

**Corrupt a tree at the root and every part of it acquires a plausible local
cause.** That is what made the investigation feel productive: each render was
strange in a way that pointed at something real and nearby, and each answer
survived its own control. The rule that would have caught it: **before
reporting a defect against shared code, run the repro with every one of your
own helpers removed** -- the finding either survives being written in the
library's plain vocabulary or it was yours.

The corroborating evidence is the same shape and worth as little: the second
surface not reproducing it was read as _a difference between the surfaces_,
which is a real category and was the wrong one. **When one side cannot
reproduce what the other sees, the asymmetry is a hypothesis about the
surfaces and equally a hypothesis about the harness** -- and the harness is the
cheaper one to eliminate first.

### A scripted revert needs an assertion that it reverted

Proving a new test catches the bug it was written for means putting the bug
back. Doing that with a scripted string replacement, where the string had since
been reformatted onto one line by `just fmt`, replaced nothing — and the test
passed, which read exactly like a test that could not discriminate. **A revert
script asserts that its target was found**, the same rule as any other
instrument that can quietly say nothing.

And it needs the file to look changed. Restoring from a copy taken _before_ the
edit gives the restored file the older copy's timestamp, so cargo judges its
artifact fresh and the test runs against the binary built from the bug — which
reads as a fix that did not take. `touch` the file, or restore by writing it
rather than moving one over it. **The stale-artifact trap, manufactured by the
revert mechanism itself.**

**The fourth member of that family**, after a cached crate reporting a type
error the source does not have, a probe that survived the window in which it
was deleted, and an addon binary answering on an old wire layout. What the four
share is the whole difficulty: **the tool is correct, its inputs are stale, and
nothing in the output distinguishes that from a correct answer.** A wrong tool
eventually says something impossible; a stale one says something plausible, and
it is usually the thing that was expected. So the guard is never _read the
result carefully_ — it is to make the instrument prove it looked at what it is
thought to have looked at: assert the revert found its target, check the
binary's hash, delete the artifact rather than trusting its date.

### A test that fails is not evidence about which side is wrong

A render corrected a prediction three times in one day. The sharpest: a legend
on the right was expected to leave the bars starting at 10, and they start at
7 — **the plot keeps its left edge and loses its right, so 5% of a narrower
plot is a smaller number.** The arithmetic was right and the expectation was
not.

**Two of the three corrections were the render teaching the test what the
layout does, and one was a real defect. Nothing about the failure told them
apart.** Only re-deriving the number for the case in hand did. So the pull to
reuse a number remembered from the last case, and to read a disagreement as a
bug in the code, is the thing to resist: derive for the case in front of you,
and when the picture disagrees, find out which of the two is wrong before
changing either.

### For shared code the question is not whether it is right

`framed` and `legend` are reached by all four chart kinds, so the failure that
matters is not that they draw wrongly but that **a kind never calls them** —
and every legend assertion asked a bar chart, so not one of them could see it.

**The four-kinds rule inverted.** Where separate code lets two kinds hide each
other's gaps, shared code lets one kind's coverage look like everyone's.

A kind never calling `framed` is **an absent node and needs no ink to see**, so
a tree assertion is the better instrument here — the JavaScript surface's is
one, and the renders on the Rust side stay only because they are written and
passing.

## Porting a v1 component

Six ways a v1 component does not mean in v2 what it says, found by carrying
`gi-showcase-card.component.ts` across a line at a time. **They are listed with
what each does when you get it wrong**, because that is what decides how much
of the port you have to re-check: a type error costs nothing, a value that is
silently wrong by a factor of the font size costs the whole render.

**1. A bare `Box` runs the other way -- and its shrink is a trap that points
the wrong direction.** v1's direction is Yoga-defaulted to `column` where v2
follows CSS and uses `row`, so every container writes its axis out. That half
is simple.

**The shrink is not, and the obvious reading is backwards.** Yoga defaults
`flex-shrink` to `0`, so a v1 node looks like it means zero. It does not: v1's
constructors put CSS's value back, and all four declare `flexShrink: 1` --
`BoxNode` at `layout.canvas.ts:99`, `ColumnNode` at `:1670`, `RowNode` at
`:1700`, `TextNode` at `text.canvas.ts:56`, with `GridNode` inheriting through
`RowNode`. It is applied rather than merely declared: `layout.canvas.ts:267`
calls `setFlexShrink` whenever the value is defined, and the defaults always
define it.

**So a v1 node that says nothing about shrinking means `1`, and taffy already
means `1`.** The faithful port therefore **writes no `flex-shrink` at all** and
matches v1 by agreeing with the same specification. _Writing `0` is a
divergence dressed as a reproduction_ -- and it is not quiet. Pinned into a
wrapper every node passes through, it moved a whole card's geometry and made
its background stop painting.

**2. `lineHeight` is a different quantity.** v1's is the line box in **pixels**;
v2's is a **multiple of the em size**. `lineHeight: 24` at 18px is 24 pixels
there and 432 here. The evidence is v1's own code, in both places that read the
property -- `text.canvas.ts:585` and `:1198`, each taking `lineHeight` as the
target line-box height in pixels when it is a positive number, with no ratio
path anywhere. v2's field says the opposite in as many words: _"Line box height
as a multiple of the font size"_ (`style/text.rs:254`). The trap inside
the trap: a component written in pixels may still hold a bare ratio or two
(this one holds `0.72`), and those are the only values that carry over
unchanged. _Wrong by a factor of the font size, and it does not look like a
unit mistake -- it looks like a layout defect._

**3. `ellipsis` changed type**, boolean to the string that gets drawn. _A type
error, which is the good case._

**4. Edge groups are gone.** v1 spells `padding: { Horizontal: 2, Bottom: 2 }`
and `border: { Left: 1, Right: 1, Bottom: 1 }`; v2 has only `top`, `right`,
`bottom` and `left`. **From TypeScript this is caught** -- `tsc` rejects the
unknown key and suggests the right one. **At runtime it is not**: measured, a
node given `padding: { Horizontal: 16 }` renders with **no padding at all**,
identical to a node given none, and nothing is thrown. So the exposure is a
plain-JavaScript caller or anything that has reached for `as any`, and the rule
is: _keep the port in TypeScript and the whole class is a compile error;
leave it and the class is invisible._

**5. `<b>` inside a plain `Text` is markup in v1 and literal text in v2**, which
has `RichText` for the purpose. _Visible immediately -- the tags draw._

**6. Capitalisation throughout**: `Style.PositionType.Absolute` to
`'absolute'`, `position: { Top }` to `{ top }`, `borderRadius: { BottomLeft }`
to `{ bottomLeft }`. _Same as 4 in both halves: a compile error from
TypeScript, silently dropped at runtime._

**And the method, since the first attempt at this card was tuned by eye and had
to be thrown away.** The v1 component's own doc says every number in it is the
geometry Chrome laid out for the template it replaced. **The numbers are
already the answer**: carry them, do not re-derive them, and when something is
off measure both renders and say by how much. A card built from a third of the
source and an impression of the rest is not a port and cannot be corrected into
one.

**Assets that are not reachable get a hatched plate at exactly the box the real
image would fill**, never a guess at the layout around them and never nothing.
_A missing asset must not be readable as a layout defect._

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
