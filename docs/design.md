# Design

This document is the authority on how meo-canvas is built and why: the scene
contract and its two wire formats, the pipeline, the two public surfaces,
stacking and layout semantics with the Chrome measurements behind them, the
error and enum policies, and the performance baseline. `AGENTS.md` is the
authority on how work is done; this is what that work is done on.

## The scene contract

`Scene` is the contract: a plain data tree in `meo-canvas-scene` with no
dependencies at all -- not Skia, not the layout engine, not the Node bindings,
not a serialization framework. Its codec is written by hand, because the byte
layout is a specification a JavaScript writer also implements, and a derived
format is one no other language can target from documentation alone.

Every door into the renderer produces one:

```
meo-canvas          Element tree            -- Rust callers build it directly
meo-canvas-node     decode(arena, values)   -- JavaScript callers encode into it
meo-canvas-cli      read from disk          -- files and pipes
                             |
                             v
                      meo-canvas-core
                             |
                    resolve -> measure -> layout -> paint -> encode
                             |
                             v
                          Vec<u8>
```

**The core cannot tell which door a scene came through**, which is what keeps
the two surfaces honest. `meo-canvas-scene` is a crate of its own because the
CLI, the addon and the fixture tooling all read scenes, and none should link
Skia to do it.

### Two representations

**The boundary format is an `f64` arena.** JavaScript writes opcodes and numeric
properties into a growable `Float64Array`, with strings and buffers in a side
`values` array the records index into; Rust reads `&[f64]`. A store into a
`Float64Array` is one operation where writing varint bytes from JavaScript is
several, and reading a value out of V8 is what costs at that boundary. The
decoder lives in `meo-canvas-node`, since only the addon can hold the side
array.

**The persistence format is bytes**: self-contained and self-describing, with a
magic number, a version, and errors that name the byte offset. Strings live
inside it because there is no side channel. It is what the CLI reads and what a
golden fixture is stored as, and it lives in `meo-canvas-scene`.

Both decode to the same `Scene`, so a scene captured from JavaScript and written
to disk round-trips without loss. `NodeTag` is a `wire_enum!` in
`meo-canvas-scene`, so the byte codec's kind tag and the arena's opcode are the
same number by construction rather than by two tables agreeing.

**The property mask is carried in `f64` slots of 53 bits each**, not 64: a
double represents integers exactly only up to 2^53, and a 64-bit mask written
into one slot loses every bit above that silently. Two slots name 106
properties; a node kind passing 106 takes a third.

### Pages

A `Scene` carries one or more page trees. A single page encodes to a still
image; several become frames in gif and apng, sheets in pdf and tiff, sizes in
ico.

**Nothing in the core calls back to produce a page.** A caller wanting a page
per frame samples its own values at each time and hands over the resulting
trees, which is why animation needs no clock, no retained state and no re-entry
into caller code mid-render. Fonts and decoded images resolve once and are
shared across every page; the layout tree is built and dropped per page.

### A page as tall as its content

`Scene::content_height` asks for it, and `size.height` becomes the **floor**
rather than the height, so no field is ever meaningless and "at least this
tall" is expressible without a second flag. **The surface is allocated after
layout solves**, which is what lets the height be derived.

A width is required and a height is not. Solving needs a width before anything
can be measured, because text breaks its lines against it; a height is a
consequence of that measuring, so `MaxContent` on the height axis is not
circular. The Rust surface says so by chaining: `Root::new(w).height(h)`.
`content_height.rs` reads the **encoded PNG's own header**, because layout could
resolve any height and still be painted onto a sheet of the stated size.

### Where state lives

`Renderer` owns everything a render needs that is not the scene: registered
fonts and whatever caches the passes keep. **Nothing in this crate is global.**
Two `Renderer`s on two threads share no state and cannot contend. The one
process-wide state is outside this crate: `meo-skia-canvas` keeps a font
registry. This crate adds no second global.

`Renderer::render` returns a `RenderedCanvas`, and encoding is a separate call
on it, so two formats of one picture cost one resolve, measure, layout and paint
and two encodes. One call doing both would make the JavaScript surface, which
retains its canvas, strictly faster than the Rust one for identical work.

`RenderedCanvas::to_buffer` takes `&mut self`, because `Canvas::to_buffer`
prepares the surface before reading it; interior mutability would let two
encodes of one canvas read as independent when they are not.
`EncodeOptions::validate` runs inside `to_buffer`, because it needs the page
count, which lives on the painted surface: a page index past the end is a
property of the drawing, and `render` structurally cannot catch it.

### Pipeline

One pass, no re-entry into caller code.

**resolve** registers fonts, decodes images and inherits text styles down the
tree. It is the only stage performing I/O, and **whether that I/O leaves the
machine is a build-time decision**: without the `net` feature, the default, an
`ImageSource::Url` is refused with `Error::UnresolvedSource` and no HTTP stack
is linked. The facade forwards the flag, so a consumer of `meo-canvas` enables
it there. No async runtime, ever.

**measure** shapes each text node and breaks it into lines, in `crate::lines`.
Skia's `Paragraph` is not on this path; `measure.rs`'s `build_paragraph` is
`#[cfg(test)]`. Breaking lines here is what makes text behave like a browser's,
since a canvas has no paragraph.

**layout** solves with taffy, and text leaves answer its measure closure by
laying out at the offered width. `MinContent` lays out at zero and `MaxContent`
at infinity, so neither needs an API of its own. A measured leaf reports its
baseline offset by its own top padding and border, because CSS measures a flex
item's baseline from its border box and taffy reads a missing baseline as the
node's own height. **In a column direction taffy does not attempt baseline
alignment at all**, and neither does its grid.

**paint** walks the solved tree in z-order through `meo-skia-canvas`'s
`Context2D`. No drawing call crosses a language boundary.

**encode** produces png, jpg, webp, avif, tiff, bmp, ico, svg, pdf, gif, apng or
raw bytes.

`resolve` is the only stage that waits, and it waits synchronously; everything
after it is CPU-bound. Parallelism lives at the scene level -- many scenes
across a thread pool -- rather than inside a single render.

### The Node addon

`meo-canvas-node` owns the only `#[neon::main]` in the binary. A Node addon has
exactly one module-init symbol, so `meo-skia-canvas` is a dependency with
`default-features = false`. The addon re-exports `meo-skia-canvas`'s operations
beside its own, so one binary serves both the declarative surface and the
imperative canvas beneath it: two addons would mean two copies of Skia resident
in one process.

## The two surfaces

**They read the same way.** `Root` is the entry point on both, style properties
sit directly on the node rather than inside a nested object, and the output
methods are the same set. A person moving between them translates syntax, not a
design.

```rust
use meo_canvas::{Renderer, Root, Row, Styled, Text, hex, px};

let renderer = Renderer::new();

let mut canvas = Root::new(520.0)
    .height(180.0)
    .background_color(hex("#101014"))
    .children(
        Row::new()
            .gap(px(20.0))
            .padding(px(24.0))
            .children(Text::new("Ukasyah").font_size(26.0).bold()),
    )
    .render(&renderer)?;

canvas.to_file("out.png")?;
```

```js
const canvas = await Root({
  width: 800,
  height: 400,
  backgroundColor: '#101014',
  children: Row({ gap: 16, padding: 24, children: [Text('Ukasyah', { fontSize: 24 })] }),
})

const png = await canvas.toBuffer('png')
```

Each is idiomatic in its own language: builders and `px(16.0)` in Rust, object
literals and `16` in JavaScript. Same CSS names, same values.

### The Rust surface

`meo-canvas` is the authoring layer, because the scene contract is an arena --
a flat `Vec<Node>` indexed by `NodeId` -- which a codec can round-trip and a
person cannot comfortably write.

**Children take one or many, and a falsy child is skipped**, as v9's
`Children | Children[]` props do. `.children` accepts a single element, an
array, a `Vec`, and skips a `None` `Option<Element>`. An iterator goes through
`each(..)`, because a blanket `impl IntoElements for I: IntoIterator` overlaps
`Vec`, `[T; N]` and `Option`.

**Flat setters are the documented path, and `with_style` merges**: a `Some` in
the argument wins and a `None` keeps what the node had. The JavaScript surface
merges too (`Row` is `{ flexDirection, ...props }`), and the two surfaces
agreeing about the same call is the requirement.

**`Style::merge` destructures its argument without a rest pattern**, so a
property the merge forgot is a build error naming the field rather than a
property that silently does not carry. A merge written as a macro arm over the
property table would miss the properties whose setters convert their argument.

Setters are written once, on a trait every node implements through one
accessor. `Style` stays public and deliberately not `#[non_exhaustive]`, so a
property with no setter is still reachable by literal. Setters are `const fn`
wherever the field allows; a field that needs dropping cannot be (E0493), which
is why `gradient` and `mask` are not.

`px` takes an `f32`, so `px(16.0)` and not `px(16)`: Rust does not coerce an
integer literal, `impl Into<f32>` cannot be `const`, and an `i32` parameter
would lose `px(0.5)`.

### The JavaScript surface

`RootProps` is `Style & {...}` and `TextProps` is `Style & ParagraphOptions &
{...}`, so a property a node accepts is one a caller writes flat; there is no
`style` key on either surface. The string-literal unions in
`packages/meo-canvas/src/index.ts` make `'cover'` complete and `'covr'` a
compile error.

**One crossing per render:**

| call                    | what crosses                                                |
| ----------------------- | ----------------------------------------------------------- |
| `Row(...)`, `Text(...)` | nothing -- plain objects, no native call                    |
| `await Root({...})`     | the entire arena, one `Float64Array` and one `values` array |
| `toBuffer(fmt)`         | a format tag and options                                    |

**The tree is built before it is encoded**, because JavaScript evaluates
arguments inside out: `Row({children: [Text('a')]})` runs `Text` first, so
writing opcodes as each factory runs would land them post-order where the arena
is pre-order.

**An explicit `undefined` is not an absent key.** `exactOptionalPropertyTypes`
is on, so optional fields are spread conditionally --
`...(x === undefined ? {} : { x })` -- rather than assigned.

**Throwing and rejecting are different failures.** An argument of the wrong
shape throws synchronously; a failure inside the render rejects. Every V8 read
happens in one pass before the work is handed to the pool, so an argument error
is raised while there is a call to throw from and a render error when there is
not. A test asserting that either one always happens is wrong.

**The canvas exposes v9's output surface**, so a ported script need not change
how it writes a file: `toBuffer`, `toBufferSync`, `toFile`, `toURL`,
`toDataURL` and their sync pairs. The sync variants are ordinary functions, and
**each pair differs in what its names claim**: the async one encodes off the
event loop. `saveAs` is not carried over, since a deprecated alias reintroduced
in a rewrite is one nobody gets to remove later; `toSharp` is absent until
someone asks, because it means taking a position on another library's version.

### The retained canvas

`Root` returns a handle to a painted surface. `toBuffer` encodes that surface
again at a different format; **it does not re-render.**

**A retained surface and an off-loop paint are mutually exclusive**:
`RenderedCanvas` holds a `SkPictureRecorder` and an `Rc<RefCell<Gradient>>`, so
it is `!Send`. **The encode is separate**: `Canvas::prepare_export` hands back a
`Pages` that is `Send`, so the half needing the canvas runs on the owning thread
and the rest on a worker. At 4000x4000 the record costs 2.74 ms and the encode
97.23 ms, so almost all of the cost leaves the loop. **Nothing on the worker
needs a font** -- the pages carry drawings with their text already shaped --
which matters because a family is registered per thread.

The pool is rayon's, not `cx.task`'s: libuv's is shared with `fs`, `dns` and
`crypto` and has four threads by default, so a 130 ms encode there moves the
stall into `fs.readFile` rather than freeing the loop. The surface is held by
closures over an `Rc<RefCell<Option<RenderedCanvas>>>` rather than a `JsBox`,
because a `JsBox` is reachable only through `this` and
`const { encode } = canvas` would break silently.

### Rasteriser parity

The GPU backend is a Cargo feature named for the platform that has one: `metal`
on Apple targets, `vulkan` elsewhere. Neither is default, because a build with
no backend renders on the CPU, which is what a portable `cargo check` needs.
**Every crate a caller can depend on forwards the feature**; one declared only
on the addon leaves a Rust caller on the CPU with no way to ask otherwise.

`gpu` is a request, and `Canvas::gpu` reports the request, so a test asserting
`renderer.gpu()` passes on a CPU-only build. `Surface::engine` reports what the
surface actually got and is what to read when images disagree. The check that
is not fooled is the byte comparison in `just example`: CPU and GPU
rasterisation of one scene differ by one or two levels across antialiased
edges.

### Where the JavaScript overhead is

The crossing is one call, so the only JavaScript cost that scales with the scene
is building the tree and encoding it.

- **Node objects are monomorphic**: same keys in the same order on every node,
  absent fields present as `undefined`. A node that sometimes carries `src` gets
  a second hidden class and deoptimises every property read in the encoder.
- **Styles are read, never copied.** The defaults already exist in Rust.
- **Encoding is one pass** into a preallocated `Float64Array` grown by doubling.
  A `lineTo` in `meo-skia-canvas` costs 82 ns, of which 17 is the crossing and
  39 is reading two floats out of the arguments; decoding from a `&[f64]` skips
  V8 entirely.

## Layout and paint semantics

### A default that differs from its explicit value is absent

CSS spells `z-index: auto` and `z-index: 0` differently -- the first
establishes no stacking context and the second does -- so `z_index` is
`Option<i32>` where `None` is auto. The same reasoning puts `gpu` at
`Option<bool>` and leaves an unnamed inset edge `None` rather than zero.

### Layout defaults

`LayoutStyle::default()` is CSS's: **`Display::Block`**, row direction,
`flex_shrink: 1.0`, so a bare `Node` behaves like a bare `<div>`. Yoga's raw
defaults are a column direction and `flex-shrink: 0`.

**Every factory on both surfaces overrides it.** `Box::new`, `Row::new` and
`Column::new` in `element.rs` name `Display::Flex`, and so does npm's `Box` in
`node.ts`, because a container that inherited `block` would silently stop
honouring `gap`, `align_items` and `justify_content`. **A scene built through
the factories is a flex container; a scene assembled from `Node` values is a
block one.** So a hand-built repro and a ported JavaScript probe are not the
same probe, and nothing about them says so: the test helpers in `layout.rs`
name their display, and anything reproducing a surface's behaviour from raw
`Node` values has to name it too.

### Stacking

`z_index` follows CSS. CSS 2.1 applies it to **positioned** elements; Flexbox
5.4 and Grid 6.2 extend it to flex and grid items regardless of position. So a
child is stacked by `z_index` when it is positioned or when its parent lays out
as flex or grid; in a block container a static child ignores it. v9 documents
`z-index` for absolutely positioned nodes only, which is narrower, and the
reference wins.

**The rule is measured in Chrome**: 281 cases sampled with `elementFromPoint`
at the true intersection of two overlapping boxes, each measured with every
other case hidden. Four results carry the design:

- **Display does not change paint order.** All 25 position pairs agree under all
  five displays, so the painter reads position and `z-index`, never the
  parent's `display`.
- **A positioned child paints above a static sibling regardless of document
  order.**
- **For positioned children the `z-index` matrix is identical under block, flex
  and grid**, and `auto` ties with `0`.
- **For static children, block ignores `z-index` and flex and grid honour it**,
  and the matrix differs from the positioned one in exactly one cell: a static
  item at `z-index: 0` beats a later sibling at `auto`, where two positioned
  children tie. An implementation treating flex items as positioned gets that
  cell wrong.

`PositionType` carries three variants where taffy carries two. `Static` is
discriminant `2`, appended, because the discriminants are published in both
wire formats, and it is the `Default`. It is the only variant that does not
stack, and `inset` does not apply to it, so `to_taffy_inset` drops it: a static
child given `top: 30px` sits at its flow position in block, flex and grid alike.
`Fixed` and `Sticky` stack as positioned variants; they differ from `Relative`
in where they resolve, not in when they paint.

### Stacking contexts

A child at `z-index: -1` sinks behind its parent's background unless the parent
establishes a context, so hit testing at the child's centre names the parent
when a trigger made no context and the child when it did. 27 triggers measured:

**Creates one:** `position` plus a numeric `z-index`; `position: fixed` or
`sticky` with no `z-index`; `opacity` below 1 (`0.99` is enough); `transform`
other than `none`; `filter` and `backdrop-filter` (`blur(0px)` is enough);
`clip-path` and `mask-image`; `isolation: isolate`; `mix-blend-mode` other than
`normal`; `will-change: transform` and `will-change: opacity`; `contain: paint`
and `contain: layout`; `perspective`; a flex or grid item with a numeric
`z-index`, position irrelevant.

**Does not:** `overflow: hidden`; `position: relative` at `z-index: auto`;
`opacity: 1`; `transform: none`; `display: flex` or `grid` on the container; a
flex item at `z-index: auto`.

**`overflow: hidden` is the one to hold on to**: it is the trigger most often
assumed, it clips its children, and it leaves them in the parent's context, so
a negative child still paints behind the parent's background.
`will-change: opacity` creating a context while `opacity: 1` does not is its
mirror: the declaration is a promise about the future value, and Chrome
honours the promise.

## Errors and diagnostics

The core returns `Result<_, MeoError>` with a variant per failure class. The
addon maps those to JavaScript exceptions, the CLI to exit codes, and Rust
callers match on them.

**A value the renderer could not use does not fail the render.** It is reported
beside it, on both surfaces, because a markup tag with an unusable colour should
still draw the text. `Root::into_scene` returns
`Result<(Scene, Vec<Diagnostic>), BuildError>`: a `BuildError` means no scene,
and a diagnostic means a scene with something in it the caller wrote and did
not get. `Element::into_scene` carries the same pair; on the JavaScript side it
is `canvas.diagnostics`, a `readonly Diagnostic[]`.

**A `Diagnostic` names a path rather than a property**, because a value nested
inside another property has no single name that finds it: `<color=zzz>` and
`segments[2].color` are paths where `color` is a field. A markup diagnostic's
path is the whole tag as written, value included -- `<weight=1500>` rather than
`<weight>` -- and `Diagnostic::offset` is its byte offset into the markup, which
tells three `<color>` tags apart. `offset` is `None` for a diagnostic about a
scene value.

## What a public enum promises

**Closed because CSS closed it, open because we will add to it.**
`#[non_exhaustive]` is free to add before a first stable release and impossible
after one, so the marking is decided now.

**Marked, because they will grow:** the error types; `ImageFormat`; `NodeKind`,
`Mask`, `TrackSize` and `Spacing`, which are this project's vocabulary;
`FontVariant`, which names a subset of OpenType's features.

**Not marked, because the specification closed the set:** `Display`,
`FlexDirection`, `Justify`, `Align`, `Overflow`, `BoxSizing`, `Direction`. A
caller matching on `FlexDirection` wants the compiler to say when they missed
one. `GradientGeometry`, `BackgroundSize` and `LineHeight` look like they belong
on the first list and do not: the test is the specification, not the shape of
the type.

**Marking moves the exhaustiveness guarantee rather than removing it.** It stays
inside the defining crate and goes everywhere else, so a variant added later
takes a wildcard arm silently. So every marked enum has an exhaustive match with
no wildcard **in the crate that defines it** -- a `#[cfg(test)]` witness naming
every variant -- and `cargo test` catches an addition. Each wildcard arm
elsewhere says what it does and why, and none panics: a scene from a newer
writer renders what this build understands rather than refusing the page.

The same test applies to public structs, which carry the attribute too
(`Diagnostic`, `ImageWarning`, `LayoutResult` and the builder types).

**Mark what a caller reads; give a constructor to what a caller writes.** The
attribute on an enum stops a caller matching a new variant and does nothing
about a new field, so variant-level `#[non_exhaustive]` closes the struct-like
variants a caller only inspects: `Error::SourceFetch`, `FontRegister`,
`ImageRead` and `Encode`; `SceneError::CanvasSize`; those on `CodecError`. A
closed variant cannot be built with a struct expression outside its crate, so
the ones built elsewhere get a constructor whose doc says why it is public:
`SceneError::canvas_size` because `meo-canvas` reports it from both
`into_scene` entry points, `Error::image_read` because `meo-canvas-cli` builds
one to check its exit code. `NodeKind::Text`, `Image`, `Path` and `Mask::Path`
stay open, because every caller of the scene constructs them. **Ask whether the
outside builds it or only inspects it.**

## What is this a statement about

A check can be right and still be the wrong check, because something narrow is
standing where something wider is needed. `Reader::list` refuses a count larger
than the bytes remaining, because every value costs at least one byte: a
statement about **the count**, not about the memory a `Vec::with_capacity` of
that count reserves. `Fonts::registered` reports what **this registry**
registered, not what can be drawn, which is `Fonts::has` and answers about the
process.

**Ask what a check is actually a statement about, and what the next line will
take it to mean.** Where the two differ, say so at the narrow one.

**Ask which frame the evidence came from.** A fetch's size limit classified from
`ureq::Error::BodyExceedsLimit` is right when no timeout is configured; with a
timeout, the same over-size read reports a bare I/O error. `fetch` therefore
counts the bytes itself. A probe isolating one feature has removed the others
by construction, and the crate ships them together.

## Dependencies

| crate             |        |                                                           |
| ----------------- | ------ | --------------------------------------------------------- |
| `meo-skia-canvas` | 0.16.3 | Skia, text shaping, encoding. `default-features = false`. |
| `taffy`           | 0.14   | Flexbox, CSS grid, block layout. Without `calc`.          |
| `csscolorparser`  | 0.8    | CSS colour syntax. Channels are `f32`; see design notes.  |
| `neon`            | 1.1    | Node addon.                                               |
| `clap`            | 4.6    | CLI.                                                      |
| `thiserror`       | 2.0    | Error types.                                              |
| `ureq`            | 3.4    | Remote images, behind the optional `net` feature.         |
| `rustls`          | 0.23   | A floor, not a use -- see `Cargo.toml`. Under `net`.      |
| `rayon`           | 1.11   | The addon's asynchronous encode. Not an async runtime.    |

| tool       |       |                                                                |
| ---------- | ----- | -------------------------------------------------------------- |
| bun        | 1.4.1 | Package manager and the JavaScript examples' runtime.          |
| typescript | 6.0.3 | Not 7, which neither typescript-eslint nor TypeDoc loads.      |
| eslint     | 10    | typescript-eslint 8, `eslint-config-prettier` last.            |
| prettier   | 3.9   | Whole tree; `.prettierignore` names the machine-written files. |
| vitest     | 5     | Tests and the JavaScript coverage floor.                       |
| typedoc    | 0.28  | Its own package, so it pins the TypeScript it loads.           |
| playwright | 1.63  | Drives Chrome for the conformance tables.                      |

**`taffy::TaffyTree` is neither `Send` nor `Sync`**: taffy represents every
length as a tagged pointer, so `Style` itself holds a `*const ()`. A tree is
built and consumed on one thread and never crosses a boundary, which costs
nothing because `Scene` carries its own style type.

**`csscolorparser::Color` is `f32` on all four channels**, so
`rgba(0, 0, 0, 0.1)` would read back as `0.10000000149011612` on both surfaces.
The parser returns the number the author wrote, with the browser as tiebreak:
an alpha written as a decimal or a percentage is presented as the shortest
decimal that round-trips to its `f32`, and an alpha written as a hex byte is
`byte / 255`, computed where the byte is known, since no decimal-shortening
reaches 127/255 from an `f32`. Scaling to 0..255 stays in `f32`, where it is
exact, and the widening happens to the product; widening first lands `#808080`
a hair under 128, which the colour tests pin.

## Performance baseline

**A baseline for reading, not a gate.** `just bench` is an instrument; a
benchmark that fails CI on a shared runner teaches people to rerun until it
passes. The numbers exist so the next person can tell a regression from noise.

Taken at `c2035a8`, 5 September 2026, on an Apple M4 Pro, 14 cores, macOS
26.6.2, `rustc 1.98.0`, Node v26.4.0.

| `bench-rust`, 111-node page, GPU off | criterion median |
| ------------------------------------ | ---------------- |
| full pipeline                        | 13.92 ms         |
| draw, without encode                 | 2.86 ms          |
| re-encode a painted surface          | 9.16 ms          |
| `resolve`, 551 nodes                 | 52.90 us         |
| `z_ordered` over 551 nodes           | 2.12 us          |

`bench-js`, 500 renders of a 480x320 scene to 7.2 KiB PNGs: 72.7 renders/s,
13.71 ms p50, 15.14 ms p99, 90.6 MiB baseline rss, +8.3 MiB retained after idle.

**Encoding is more than half the pipeline**, which is why separating rendering
from encoding is worth more than any allocation fix.

**Read a row against its neighbours.** A single number has no control; a table
has one for free. One row moving while its neighbour holds is the code
changing; both moving together is the machine.

**About 10% is the noise floor.** A change smaller than that is not a result.

**The JavaScript and Rust figures answer different questions.** 13.71 ms per
render is a whole render through the addon including PNG encoding; the Rust
`draw` row is the drawing stage alone. The package README's "paint" is the whole
native call -- decode, resolve, measure, layout and drawing -- so the two agree
on the total and differ on where the line falls.

**Two allocations look wasteful and are not**: `resolve` clones a
`ResolvedText` per node, some fraction of 52.90 us against a 13.92 ms pipeline,
and `z_ordered` clones and sorts every container's children in 2.12 us across
551 nodes. Do not change either without a number that says otherwise.
Allocation in the **paint** stage is on the critical path of every frame of an
animated render: prefer reusing a buffer, and say what the reuse is worth when
it is not obvious.
