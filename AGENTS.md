# meo-canvas

Renders declarative scene trees to images. A caller describes what it wants —
boxes, rows, text, images, paths, grids, charts — and gets back encoded bytes.
Layout is flexbox and CSS grid; drawing is Skia; text is shaped and broken by
Skia's paragraph engine.

Two public surfaces, and they are siblings rather than layers: a Rust crate and
a Node addon. Both construct the same `Scene` and hand it to the same core, so
neither can grow a capability the other cannot reach.

## Contents

- [Architecture](#architecture)
  - [Two representations](#two-representations)
  - [Pages](#pages)
  - [A page as tall as its content](#a-page-as-tall-as-its-content)
  - [Where state lives](#where-state-lives)
  - [Pipeline](#pipeline)
  - [The JavaScript boundary](#the-javascript-boundary)
  - [Node addon](#node-addon)
- [The Rust surface](#the-rust-surface)
  - [Rasteriser parity](#rasteriser-parity)
- [The JavaScript surface](#the-javascript-surface)
  - [One crossing, and when](#one-crossing-and-when)
  - [Why the tree is built before it is encoded](#why-the-tree-is-built-before-it-is-encoded)
  - [Throwing and rejecting are different failures](#throwing-and-rejecting-are-different-failures)
  - [What the canvas exposes](#what-the-canvas-exposes)
  - [The retained canvas](#the-retained-canvas)
  - [Where the overhead is](#where-the-overhead-is)
- [Workspace](#workspace)
  - [Module layout](#module-layout)
- [The behavioural target](#the-behavioural-target)
  - [Three questions answered and closed](#three-questions-answered-and-closed)
  - [A property whose whole meaning is a DOM event](#a-property-whose-whole-meaning-is-a-dom-event)
- [Conventions](#conventions)
  - [Comments](#comments)
  - [Constants](#constants)
  - [Stacking](#stacking)
  - [Stacking contexts](#stacking-contexts)
  - [Layout defaults](#layout-defaults)
  - [Errors](#errors)
  - [What a public enum promises](#what-a-public-enum-promises)
  - [What is this a statement about](#what-is-this-a-statement-about)
  - [Performance and memory](#performance-and-memory)
- [Workflows](#workflows)
  - [The package manager is bun](#the-package-manager-is-bun)
  - [Formatting and linting](#formatting-and-linting)
  - [Windows runs the gate, and it found four faults in three runs](#windows-runs-the-gate-and-it-found-four-faults-in-three-runs)
  - [Local iteration against meo-skia-canvas](#local-iteration-against-meo-skia-canvas)
  - [CLAUDE.md](#claude-md)
- [Testing](#testing)
  - [Coverage: two floors, two reports, and who runs it](#coverage-two-floors-two-reports-and-who-runs-it)
  - [What a check can and cannot see](#what-a-check-can-and-cannot-see)
  - [What only a second implementation can see](#what-only-a-second-implementation-can-see)
  - [And what only a pixel can](#and-what-only-a-pixel-can)
  - [Pin the wrong answer when the right one is upstream](#pin-the-wrong-answer-when-the-right-one-is-upstream)
  - [Widening a matrix is the measurement, not the diligence](#widening-a-matrix-is-the-measurement-not-the-diligence)
  - [Could this evidence have failed?](#could-this-evidence-have-failed)
  - [Structural coverage is not positional coverage](#structural-coverage-is-not-positional-coverage)
  - [A function-valued option has one instrument where everything else has three](#a-function-valued-option-has-one-instrument-where-everything-else-has-three)
  - [A language's convenient default is not the other language's](#a-language-s-convenient-default-is-not-the-other-language-s)
  - [A repro must be minimal in what it is not testing](#a-repro-must-be-minimal-in-what-it-is-not-testing)
  - [A scripted revert needs an assertion that it reverted](#a-scripted-revert-needs-an-assertion-that-it-reverted)
  - [A test that fails is not evidence about which side is wrong](#a-test-that-fails-is-not-evidence-about-which-side-is-wrong)
  - [For shared code the question is not whether it is right](#for-shared-code-the-question-is-not-whether-it-is-right)
- [Porting a v1 component](#porting-a-v1-component)
- [Releasing](#releasing)
  - [The targets, and the one that is missing on purpose](#the-targets-and-the-one-that-is-missing-on-purpose)
  - [A Linux artefact is a property of its build base](#a-linux-artefact-is-a-property-of-its-build-base)
  - [The goldens are per architecture, and no tolerance was added](#the-goldens-are-per-architecture-and-no-tolerance-was-added)
  - [The addon does not ship inside the package](#the-addon-does-not-ship-inside-the-package)
  - [A target is named three times, and a test asserts all three agree](#a-target-is-named-three-times-and-a-test-asserts-all-three-agree)
  - [Packing is not installing](#packing-is-not-installing)
  - [What triggers a publish](#what-triggers-a-publish)
  - [A new platform package cannot start on OIDC](#a-new-platform-package-cannot-start-on-oidc)
  - [The reference publishes after the version resolves from npm](#the-reference-publishes-after-the-version-resolves-from-npm)
  - [The publishing audit](#the-publishing-audit)
- [Dependencies](#dependencies)

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

### A page as tall as its content

`Scene::content_height` asks for it, and `size.height` becomes the **floor**
rather than the height -- so no field is ever meaningless, and "at least this
tall" is expressible rather than a second flag.

**Solve, then allocate.** The surface used to be created from `page_size` before
any layout ran, which is why a derived height was impossible rather than merely
absent: there was nowhere for the answer to go. `render` now solves each page,
computes its size from the solved root, and brings the surface into being on the
first page.

**The circularity argument covers the width and stops there.** Solving needs a
width before anything can be measured, because that is what text breaks its
lines against. A height is a consequence of that measuring, so `MaxContent` on
the height axis is not circular. The two surfaces therefore agree that a width is
required and a height is not, and the Rust one says so by chaining
(`Root::new(w).height(h)`) because Rust has no optional argument and `Root`
configures everything else the same way.

**What the pinning tests are for.** `content_height.rs` in the core reads the
**encoded PNG's own header**, because layout could resolve any height at all and
still be painted onto a sheet of the stated size -- which is exactly what used to
happen. `content_height_surface.rs` in the surface crate checks the other half:
that the default a caller reaches by writing the least is the derived one, and
that `.height(120).min_height(90)` stays 120.

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
tree. This is the only stage that performs I/O, and **whether that I/O leaves
the machine is a build-time decision**: without the `net` feature -- the default
-- an `ImageSource::Url` is refused with `Error::UnresolvedSource` and no HTTP
stack is linked, and a surface with a fetcher resolves the URL to bytes before
handing the scene over. With `net` on, the core fetches it through a blocking
client. **The facade forwards the flag**, so a consumer of `meo-canvas` enables
it there rather than depending on the core to reach it. The rule that does not bend either way is the runtime: `ureq` brings
none.

**measure** shapes each text node and breaks it into lines, in `crate::lines`.
Skia's `Paragraph` is not on this path: `measure.rs`'s `build_paragraph` is
`#[cfg(test)]` and exists for the comparison report those tests run. Breaking
lines here is what makes the text behave like a browser's, since a canvas has no
paragraph -- see "The behavioural target".

**layout** solves the tree with taffy, and text leaves answer its measure
closure by laying out at the offered width. The two intrinsic questions are the
same call with a different budget: `MinContent` lays out at zero and `MaxContent`
at infinity (`measure.rs:399-400`), so neither needs an API of its own.

A measured leaf reports its baseline in the `LayoutOutput` it builds, offset by
its own top padding and border, because CSS measures a flex item's baseline from
its border box. taffy reads a _missing_ baseline as the node's own height
(`taffy-0.14.0/src/compute/flexbox.rs:1921`), which is what a row of text would
degenerate to without this. **In a column direction taffy does not attempt
baseline alignment at all**, and neither does its grid.

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

**Flat setters are the documented path, and `with_style` merges** (4 September
2026). A node is styled by naming properties on it — `Column::new().gap(px(10.0))`
— and that is how the crate is taught, in the `lib.rs` header, the `element`
module doc and the root `README.md`. `Element::with_style` stays for the case
the setters cannot express, a reusable `const CARD: Style` applied to many
nodes, and is documented second.

`with_style` merges rather than replacing: a `Some` in the argument wins, a
`None` leaves what the node already had. It replaced until this date, which
discarded whatever the constructor had set — `Column::new().with_style(..)`
became a row — and the reason recorded for replace was that a merge makes the
order of two calls significant. Three things answer that reason, and the doc on
`with_style` carries them: order was already significant and more sharply,
since replace threw the earlier call away entirely; every flat setter is
already a one-field merge, so replace was the one operation on the surface
whose semantics differed from the setters beside it; and **the JavaScript
surface has always merged** — `Row` and `Column` are `{ flexDirection,
...props }`, `Grid` is `{ display: 'grid', ...props }`, spread after the
default so the caller's value wins and the factory's survives where the caller
says nothing. Replace made the two surfaces disagree about the same call, which
is a defect here rather than a difference.

`Style::merge` is where it lives, and it destructures its argument **without a
rest pattern**. A sixty-ninth property that the merge forgot would be a
property that silently does not carry, visible only as a picture that came out
wrong; the destructure makes it a build error naming the field. Ten of the
sixty-eight properties have hand-written setters rather than macro-generated
ones, because their setter converts what the caller passes (`width` takes a
`Length` and stores a `Dimension`; `gap` takes one `Length` and stores a pair),
so a merge written as a macro arm over the property table would have covered
fifty-eight and let the other ten drift. Destructuring the struct covers what
the struct has, which is the thing that matters.

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
alternative is the same seventy-three methods repeated seven times, which is what
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

### An explicit `undefined` is not an absent key

`exactOptionalPropertyTypes` is on, so `{ duration: undefined }` and `{}` are
different types where a property is declared `duration?: number`. Optional
fields are therefore spread conditionally -- `...(x === undefined ? {} : { x })`
-- rather than assigned, because assigning `undefined` does not type-check
where omitting the key does. Found writing the animation walkers: a spring
track has no duration of its own to give, and the row builder had to leave the
key out rather than set it to nothing.

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

**Baselines from measured text: carried.** taffy 0.14's measure function
returns a whole `LayoutOutput` rather than a `Size<f32>`
(`taffy-0.14.0/src/tree/taffy_tree.rs:904`), so `layout.rs` reports a text
leaf's baseline in it, offset by the leaf's own top padding and border because
CSS measures a flex item's baseline from its border box.

**Confirm the _released_ signature rather than the changelog**, and confirm the
function the code actually calls. `compute_leaf_layout` misleads here: the
low-level helper still takes a `Size<f32>` closure in 0.14, so reading the
helper says the fix did not ship while reading `TaffyTree` says it did. One
release was spent waiting on a changelog entry; the second reading is what ends
a wait.

**The arrangement that can see it is `align-items: baseline` over mixed font
sizes, and nothing else is** -- taffy reads a missing baseline as the node's own
height, so a row of boxes with no text agrees with itself either way.
`flex-alignment.tsv`'s eighteen `baseline` rows are exactly that and stayed
green through the change; `fixtures/baseline-alignment` is the scene that moved.
**A check that passes before and after has not tested the change.**

And what it pins is the **sign**. Chrome's ink bottoms increase with font size
because aligned baselines let a larger descender hang lower; aligning box
bottoms makes them decrease. A fix that narrowed the spread while leaving the
order inverted would pass "the baselines should be level" and fail this.

**Whole-pixel rounding against Chrome's sixty-fourths: closed.** Kept in full
because the measurement is what makes the fix checkable, and because two of the
three candidate fixes were wrong for reasons worth not re-deriving.

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

**The fix, and it is in: snap each length into sixty-fourths before
accumulating**, so the tie never forms. **Not** _round differently_ -- rounding
is not where the two part -- and **not** _disable taffy's rounding_, which was
costed twice and was wrong both times: turning it off makes adjacent boxes meet
on fractions and antialias against each other, trading a bounded difference for
a visible one at every shared edge.

**Chrome truncates rather than rounds, and that is measured rather than
assumed.** `floor(x * 64) / 64`, applied at every boundary where a length enters
taffy -- sizes, min and max, flex basis, margins, insets, padding, borders, gaps,
and the measured text size, which is a used length like any other. The value
that settles it is `10.0234375`: exactly `641.5` sixty-fourths, an exact tie, and
Chrome takes `641`. **A tie excludes every rounding mode at once** where
`10.008` and `7.999` each exclude only one. All eight edges of the `10.3` stack
now agree, edge five having been `52` against Chrome's `51`.

**Percentages are deliberately not snapped**: they resolve against a containing
block this stage has not computed, and Chrome snaps the _resolved_ value, so
snapping the fraction would quantise a ratio.

**What the unknowns turned out to be.** Percentages are answered above. **It
moved no fixture at all** -- 517 passed with nothing changed -- and that is not
the reassurance it reads as: the pin watching this very edge was sign-blind, so
the tree could not see the fix. See the entry below. A third option nobody has
costed is to **solve in device pixels and paint 1:1**, which puts the rounding on the device grid and
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

### What a public enum promises

**Closed because CSS closed it, open because we will add to it.**

`#[non_exhaustive]` is the difference between adding a variant and breaking
every consumer who matched on the type. It is free to add before a first
release and impossible to add after one, so the marking happens now and the
question is only which enums get it.

Marked, because they will grow: the five error types -- `Error`, `BuildError`,
`SequenceError`, `SceneError`, `CodecError` -- which have grown three variants
in a week; `ImageFormat`, which grows whenever an encoder does; `NodeKind`,
`Mask`, `TrackSize` and `Spacing`, which are this project's vocabulary rather
than anyone else's; and `FontVariant`, which names 35 OpenType features out of
a specification that has more.

Not marked, because the specification closed the set: `Display`,
`FlexDirection`, `Justify`, `Align`, `Overflow`, `BoxSizing`, `Direction` and
their neighbours. **The attribute is hostile there for no gain** -- a caller
matching on `FlexDirection` wants the compiler to tell them when they have
missed one, and CSS is not going to invent a fifth direction.

Three that look like they belong on the first list and belong on the second,
because the check is the specification rather than the shape of the type:
`GradientGeometry` is `Linear | Radial | Conic`, which is every CSS gradient;
`BackgroundSize` is `PerAxis | Cover | Contain`, which is every
`background-size`; `LineHeight` is `Number | Length | Percent`, which is every
`line-height` that is not `normal`, and `normal` is the absent value rather
than a variant. `TrackSize` is on the first list for the opposite reason: CSS
grid also has `min-content`, `max-content`, `minmax()` and `fit-content()`, and
this names four of eight.

**What marking costs, stated rather than discovered.** `#[non_exhaustive]`
leaves exhaustiveness intact inside the defining crate and removes it
everywhere else -- so `meo-canvas-core` matching on a `meo-canvas-scene` enum
now needs a wildcard arm, and a variant added tomorrow would take that arm
silently instead of failing the build. That is a real loss and this repository
cares about it more than most: it is the same guarantee `Style::merge` spends
sixty-eight lines to keep.

So the guarantee moves rather than goes. Every marked enum has an exhaustive
match with no wildcard **in the crate that defines it**: `NodeKind::tag` was
already one, and `Mask`, `TrackSize` and `Spacing` have a witness test each
that names every variant and does nothing with them. Adding a variant fails to
compile there, and whoever adds it is then standing in the file that lists the
places needing an arm. The witnesses are `#[cfg(test)]`, so `cargo build` will
not catch it and `cargo test` will; that is the price of not inventing public
API to hold a compile-time check.

Each wildcard arm in another crate says what it does and why it exists. A
`NodeKind` this build cannot draw draws nothing; a `Mask` it cannot describe
clips nothing, which shows the subtree whole rather than hiding it; an unknown
`FontVariant` asks the shaper for nothing. None of them panic: a scene from a
newer writer should render what this build understands rather than refuse the
page.

#### Mark what a caller reads, give a constructor to what a caller writes

The attribute on the enum stops a caller matching a new **variant**. It does
nothing about a new **field**: a struct-like variant can still be destructured
exhaustively from outside, so `Error::SourceFetch { url, detail }` was breaking
to extend even with `Error` marked. Variant-level `#[non_exhaustive]` is what
closes that, and it has the same deadline -- free now, impossible after a
release.

Marked, because a caller only ever reads them: `Error::SourceFetch`,
`FontRegister`, `ImageRead` and `Encode`; `SceneError::CanvasSize`; the six on
`CodecError`. That is what let `SourceFetch` gain a `failure` field as an
addition rather than a break.

**A closed variant cannot be built with a struct expression outside its own
crate, so the ones we build elsewhere get a constructor rather than losing the
attribute.** `SceneError::canvas_size` exists because `meo-canvas` reports it
from both `into_scene` entry points; `Error::image_read` exists because
`meo-canvas-cli` builds one to check which exit code it maps to. The
constructor is the door left open, and it says so in its own doc.

**And the case where the rule says no**, which is the half that makes it usable
on something new. `NodeKind::Text`, `NodeKind::Image`, `NodeKind::Path` and
`Mask::Path` are struct-like and stay open. Every caller of the scene
constructs them -- `meo-canvas` writes `NodeKind::Text { .. }` to make a text
node, and so does anyone building a tree by hand -- so closing them would trade
a field addition nobody has asked for against the ability to write a node at
all. A constructor per variant would be the alternative and it is worse: nine
of them, wrapping nothing, to protect a change that has never been wanted.

The test is the same one the enums get, read at a finer grain. **Ask whether
the outside builds it or only inspects it.** An error is inspected; a node is
built.

### What is this a statement about

Two defects in one day had the same shape, and neither was a wrong statement.

`Reader::list` refuses a count larger than the bytes remaining, because every
value costs at least one byte. That is correct, and it is a statement about
**the count**. The next line was `Vec::with_capacity(count)`, which reads it as
a statement about **the memory** -- and a `Node` is 1048 bytes in memory
against 184 on the wire, so one megabyte of input reserved 1.02 GB. The comment
above the defect explained, accurately, why the count was safe.

`Fonts::registered` reports the families **this registry** registered. That is
correct, and it is a statement about **the instance**. A caller reads `Fonts`
as the scope of what it registers -- it is a value they hold, they pass it to a
renderer -- so they take it as a statement about **what can be drawn**, which
is `Fonts::has`, which answers about the process. Both methods work exactly as
written and contradict each other in front of a caller who is right to be
confused.

**Neither survives review by hiding. They survive because there is nothing
wrong to catch** -- only something narrow standing where something wider is
needed, with a correct comment above it. A reviewer checking whether the line
is true finds that it is.

So the question to ask is not whether a check or an accessor is right. It is:
**what is this actually a statement about, and what will the next line take it
to mean?** Where the two differ, say so at the narrow one -- `Wire::MIN_ENCODED`
exists because the count's bound was not the memory's, and `has` and
`registered` each name their scope because the type cannot.

#### A third: true in the frame it was measured in, false in the frame it ships in

Two features of one change can each be right and interact. The size limit on a
URL fetch was first classified from `ureq::Error::BodyExceedsLimit`, which is
exactly what `ureq` reports -- **when no timeout is configured**. The same
change also set `timeout_global`, and with a timeout set the identical
over-size read reports a bare `Io(Os { code: 22, InvalidInput })` instead.
Measured both ways against the same 33 MiB response.

So the classification was correct in every test written without a timeout and
wrong in the crate that sets one, and an isolated probe confirmed it while the
real path contradicted it three runs out of three. The fix was to stop asking
the dependency: `fetch` counts the bytes itself, which makes the answer the
same in every configuration and makes the limit this crate's in the sense that
matters.

**Ask which frame the evidence came from.** A probe that isolates one feature
has, by construction, removed the other -- and the crate ships both.

#### And its complement, which is cheaper to catch

The opposite of the two rules above is a statement **wider** than the
measurement behind it, and it is the easier of the two to make. It is one of
three worked examples under "Could this evidence have failed?", with the font
registry measured on a single thread as the case -- kept there rather than
repeated here, because three statements of one mechanism at two depths is a
thing a reader cannot arbitrate.

### Performance and memory

**A baseline for reading, not a gate.** `just bench` is an instrument: a
benchmark that fails CI on a shared runner teaches people to rerun until it
passes. These numbers exist so that the next person can tell a regression from
noise, which requires knowing what was measured, on what, and when.

Taken at `c2035a8`, 5 September 2026, on an **Apple M4 Pro, 14 cores, macOS
26.6.2**, `rustc 1.98.0`, Node v26.4.0, `cargo bench -p meo-canvas-core`
(release) and `node --expose-gc tools/bench.mjs`.

`bench-rust`, on a 111-node page with the GPU off:

|                             | criterion median |
| --------------------------- | ---------------- |
| full pipeline               | 13.92 ms         |
| draw, without encode        | 2.86 ms          |
| re-encode a painted surface | 9.16 ms          |
| `resolve`, 551 nodes        | 52.90 µs         |
| `z_ordered` over 551 nodes  | 2.12 µs          |

`bench-js`, 500 renders of a 480x320 scene to 7.2 KiB PNGs:

|                     |          |
| ------------------- | -------- |
| throughput          | 72.7 / s |
| per render, p50     | 13.71 ms |
| per render, p99     | 15.14 ms |
| baseline rss        | 90.6 MiB |
| retained after idle | +8.3 MiB |

**Encoding is still more than half the pipeline**, which is why separating
rendering from encoding is worth more than any allocation fix: a second format
costs a fraction of a fresh render rather than all of it.

Two allocations look wasteful and are not worth removing, measured rather than
argued. `resolve` clones a `ResolvedText` per node and again per child, which is
some fraction of 52.90 µs against a 13.92 ms pipeline. `z_ordered` clones and
sorts every container's children, 2.12 µs across a 551-node tree. Both are real
observations about the code and false as performance problems. Do not change
either without a number that says otherwise.

Allocation in the paint stage is on the critical path for every frame of an
animated render. Prefer reusing a buffer over allocating per node, and say in a
comment what the reuse is worth when it is not obvious.

#### Why several figures rather than one

**A single number has no control; a table has one for free.** The previous
table, recorded at `e2cc251` on 22 August 2026, gave `draw` as 9.86 ms against
2.86 ms here -- a 3.4x gap that looks like a measurement error. What settles it
is the row beside it: `re-encode` was 9.00 ms then and 9.16 ms now, within 2%.
**A faster or slower machine moves both.** One row transformed and its
neighbour unchanged is the machine holding still while the draw path changed,
and that control was free -- it was already in the run.

The rest was free too, and worth doing before paying for a rebuild. The bench
recipe is unchanged; `skia-safe` is 0.99.0 in both lockfiles; the benchmark
source differs by three lines adapting to `z_index` becoming an `Option`, so
the scene it builds is the same and the figures are comparable.

**The improvement is real and unattributed, which is the honest state of it.**
The arithmetic is consistent: the pipeline fell 9.03 ms while `draw` fell 7.00
and `re-encode` held. Thirty-five commits touched `paint.rs` in between. One
looked like a candidate on its title -- `e8c3c78`, which removed an `eprintln!`
from every dotted render -- and it is eliminated by measurement rather than
opinion: the bench scene sets no dotted or dashed border at all, so that print
never ran in it. No hunt was made beyond that, because a guess with a commit
hash attached is worth less than an honest gap.

`e2cc251` named no machine, no profile and no recipe. That absence is why this
section now names all three.

#### The JavaScript and Rust figures are not the same measurement

`bench-js` reports 13.71 ms per render at p50 and 72.7 renders a second; the
Rust `draw` row is 2.86 ms. **These do not disagree, they answer different
questions.** The JavaScript number is a whole render through the addon
including PNG encoding, and the Rust `draw` row is the paint pass alone with no
encode. `13.71 ms` against `2.86 + 9.16 = 12.02 ms` of Rust paint and encode
leaves about 1.7 ms for the boundary, the arena and the scene build, which is
the shape one would expect rather than a contradiction.

**And "paint" means two different spans in the two places it is written**, which
is the part that reads as a contradiction until someone says so. `draw` in the
table above is the drawing stage alone. The package README's ~9 ms is the whole
native call, which also decodes the arena, resolves, measures and lays out
before anything is drawn. **The two agree on the total** -- 2.86 + 9.16 against
9 + 4 -- and disagree only on where the line between paint and encode falls.
The roughly 6 ms between `draw` and the native call is where the flat floor
lives: the cost a 20x20 canvas pays and a 4000x4000 canvas pays equally, which
is why a thumbnail costs what a poster costs. Nobody has attributed it.

## Workflows

`just` drives everything. `just` alone lists every recipe with its one-line
doc; the table below groups them. A bare verb rewrites the tree; the `-check`
suffix is the variant that reports instead. `just ci` uses only the reporting
variants.

```
setup                 First-time setup on a fresh clone. Idempotent.
ci                    What CI runs, in order, refusing to start beside another gate.

build / addon         The workspace, and the debug addon into packages/meo-canvas.
build-js              tsc into packages/meo-canvas/dist.
test / test-js        Rust (twice, once per GPU feature) and vitest.
coverage / -js        The 90% floors. Exit non-zero below them.
lint / lint-check     clippy, then ESLint. `lint` rewrites.
fmt / fmt-check       rustfmt on the pinned nightly, then prettier over the whole tree.
typecheck             tsc --noEmit over the package and its tests.
docs / docs-js        rustdoc with warnings denied; TypeDoc with dead links denied.
layout-check          No mod.rs anywhere.
unused / runtime-free Declared-but-unused crates; any async runtime in the tree.

fixtures / -accept    Render every golden and compare; accept one by name.
fixtures-linux        The same, inside the release container, for the linux-x86_64 set.
fixture-scenes        Rewrite every fixture's scene.mcs from its source.
conformance           Re-measure Chrome with Playwright; rewrite the tables the tests read.
example               Render the nine scenes on both surfaces and compare every byte.
bench / -rust / -js   criterion; throughput, rss, heap, peak, idle.

arena-tables, arena-enums, arena-cases, media-types, doc-examples, platform-packages
                      Generated TypeScript from Rust sources; each has a -check that fails on drift.

addon-release         The optimised addon a release ships.
addon-container       The same, built inside containers/Dockerfile.{glibc,musl}.
pack / pack-container Two tarballs into release/: the platform package and the main one.
verify-pack / -packed Install those tarballs somewhere else and render through them.
abi-floor             What the Linux addon demands against what TARGETS declares.
acceptance            Load the Linux addon on six images with nothing installed.
bump-npm              npm version, and every platform pin with it.
release-npm(-dry)     Dispatch release.yml on the pushed HEAD and watch it.
release-crate(-dry)   Dispatch crates-io.yml.
surface-report        v1's prop surface against v2's.
clean
```

### The package manager is bun

`bun.lock` is the lockfile and `packageManager` in `package.json` names the
version. `ensure-deps`, every workflow and the reference tool install with
`bun install --frozen-lockfile`, which refuses when lockfile and manifest
disagree -- the `npm ci` equivalent. Two things stay npm on purpose: `npm pack`
and `npm publish`, because the release workflow derives its tarball globs from
npm's naming and provenance is npm's; and the consumer-side install in
`verify-package.mjs`, because consumers use npm.

The root TypeScript is **6.0.3**, not 7. typescript-eslint supports `<6.1.0`
and TypeDoc 0.28 supports up to 6.0.x; 7 is the Go compiler with a different
API and neither loads it. The reference tool pins the same version in its own
package for the same reason.

### Formatting and linting

`fmt` is one recipe: rustfmt on the nightly named by `fmt_toolchain`, because
`rustfmt.toml` uses options stable ignores -- stable `cargo fmt` reports clean
against weaker rules than CI applies -- then `prettier --write .` over the
whole tree, Markdown, YAML and JSON included. What prettier must not touch is
named in `.prettierignore` with its reason each time: the vendored v1 layer
kept in upstream's style so it can be diffed against upstream, the generated
tables, the golden scenes, and the Chrome measurement tables under
`crates/*/tests/assets`, which are machine-written and which prettier rewrote
by 3,460 lines the first time it saw them.

`lint` is clippy with `-D warnings` across the workspace and the examples, then
ESLint. `eslint.config.mjs` runs typescript-eslint's `recommendedTypeChecked`
against `tsconfig.test.json`, which is the config that includes the tests;
`eslint-config-prettier` goes last so no rule argues with the formatter. Two
rules are deliberate and say so in the config: `require-await` is off because
`Canvas.toBuffer` is `async` with no `await` on purpose -- the contract is a
rejection, not a throw -- and `no-unused-vars` has no `^_` escape for
variables, only for parameters a signature forces on you. `const _ = x` to
silence the rule is the thing the rule exists to stop.

`bun run lint` is ESLint then `prettier --check .`, for someone not using
`just`; the recipes call the same scripts so there is one definition.

### Windows runs the gate, and it found four faults in three runs

`ci.yml` runs on ubuntu, macOS and Windows. The sibling project keeps
from-source builds off pull requests and tests Windows only against a prebuilt
binary, and adding the runner here was questioned for the same reason. It stays
because every fault it found was in the tooling, none in the renderer, and none
was visible any other way: `run:` defaults to PowerShell (now `shell: bash` for
the workflow); `.gitignore`'s `* text=auto` checked out CRLF and prettier failed
69 files (now `eol=lf`); two tools split or prefix-matched paths on `/`; and
three tests turned `new URL(x, import.meta.url).pathname` into `D:\D:\a\...`
(now `fileURLToPath`). Before writing a `.mjs` tool or a test that touches the
filesystem, grep for `.pathname`, `split('/')`, `startsWith(dir + '/')` and
`execFileSync('npm'` -- npm on Windows is `npm.cmd` and Node refuses to spawn it
without `shell: true`.

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

### Coverage: two floors, two reports, and who runs it

`just coverage` measures two things and fails on either -- on macOS and Linux.
The workspace floor is 90% of regions and lines, on the pinned nightly so
branches are measured at all. The second floor is 60% of regions on
`meo-canvas-node/src/lib.rs` alone, the Neon boundary, which the workspace
number still excludes.

**On Windows only the first of the two runs.** Loading the instrumented addon
in-process under `--pool=threads` was a segmentation fault there -- `just ci`
exit 139, after nine test files had passed -- so the recipe guards that half
with `os() != "windows"`. Windows still gates on the workspace floor; what it
does not measure is the boundary, and that number does not vary by platform, so
CI measures it on the other two runners. Said here because a file about checks
that look like they measure and do not had acquired one of its own: this
paragraph existed for two days saying "two things" while one runner ran one.

Both floors are also stated in "What a check can and cannot see", which points
back here rather than repeating the mechanics -- one place to correct when the
recipe moves, which this paragraph is the argument for.

**Why the boundary is excluded from the workspace number.** Its 499 regions are
called by V8 and by nothing else, so no Rust test can execute them: from a Rust
caller the file measures 4.81% and always will. Counted in, the workspace sat at
90.06% -- six regions of margin -- and then fell to 89.92% with the **identical**
2,536 missed regions when a well-tested deletion shrank the denominator. A floor
that a well-tested deletion can breach is not measuring what it claims. Excluded,
the same tree is 91.69%.

**Why it is no longer a hole.** The boundary is now measured, by the suite that
actually calls it: the JavaScript tests run against an instrumented addon, and
`lib.rs` comes out at 64.13% of regions, 68.00% of lines, 57.89% of functions.
Raising that means writing JavaScript that reaches the error arms, not writing
Rust.

**Why two reports rather than one number.** `cargo llvm-cov` merges every
`.profraw` under its target directory, the JavaScript run's included, into one
`.profdata` -- and its `report` still puts `lib.rs` at 4.81%, because `report`
has no `--object` flag and never includes the crate's `cdylib`. The same
profdata read against the `.node` puts the same file at 64.13%. Same evidence,
two answers, and the wrong one is the one that looks ordinary. There is no flag
that fixes this; the route to a single number is upstream. So the recipe reports
twice: once through cargo for the workspace, once through the toolchain's own
`llvm-cov` for the artefact cargo cannot see.

**The second report names a source, not the object.** The `.node` links the core
and scene crates into itself, so a report over the artefact totals 45% -- a
number about layout and Skia wearing the boundary's label. `lib.rs` alone is the
file the recipe exists for.

**A measurement apparatus can fail in a way that looks precisely like the thing
it measures being absent.** Three of the four ways to instrument this reported
that the JavaScript suite never touches the addon, and it touches it constantly.
vitest's default `forks` pool loads the instrumented addon in worker processes
that never flush a profile, so all 15 test files write no `.profraw` at all --
indistinguishable from a suite that never loads the addon. Continuous mode
(`%c`) is worse: it writes files whose counters are all zero, and they merge
cleanly and report 0.00% across the workspace. `--pool=threads` is therefore not
a performance choice, and it belongs on that invocation only, not in
`vitest.config.mts`. This is the same failure as "Could this evidence have
failed?", one level up: there the inputs could not see the rule, here the
instrument could not see the execution. Both are silent, and both look like a
clean result.

**Two plumbing traps, both commented where they bite**, because each fails as a
clean report followed by a floor failure with no floor in it. `find ... | head -1`
under `pipefail` kills `find` with SIGPIPE the moment `head` is satisfied, and
`set -e` ends the recipe right after the Rust half printed its result; use
`-print -quit`. And `llvm-cov show-env` exports a dozen `CARGO_LLVM_COV_*`
variables, one of which makes a later `report` exit non-zero **after** printing a
clean result; the build and copy happen in a subshell so none of them leak.

**The addon is left instrumented** when the recipe finishes, deliberately -- the
suite already looks in the in-tree path, and `MEO_CANVAS_ADDON` cannot be used
as the harness because that variable is the subject of `addon.resolve.test.ts`,
where an ambient value fails 9 of its 12 tests. Anything run afterwards works on
that binary, correctly but slower. `just addon` puts an ordinary one back.

`just coverage` writes `target/llvm-cov-target`, so two sessions running it in
one checkout collide. The rule: per change, each session runs the stateless
gates -- `fmt-check`, `typecheck`, `cargo test`, `doc-examples-check`, clippy.
Coverage runs once per push, by whoever pushes, on the tree as pushed. CI runs
it again on the same commit and is the gate of record.

### What a check can and cannot see

Each of these cost a bug or most of a day to learn.

**A check has to be as wide as the claim it supports, and "wide" has more than
one axis.** Two gates went red in one evening on commits whose verification had
been called sufficient, and the two gaps were different in kind. **`cargo test
--lib` compiles `cfg(test)` code without linting it** -- clippy reaches test code
only with `--all-targets` -- so a green test run says nothing about whether a
test module lints. And **`cargo clippy --workspace --all-targets` stops at the
workspace edge**: `examples/rust` is its own workspace, so `lint-check` runs a
second invocation with `--manifest-path examples/rust/Cargo.toml`, and only the
gate runs both.

The second one also falsified the sentence it was committed under. A public API
break was recorded as reaching "no consumer" because nothing is published --
**but `examples/rust` exists precisely to be a consumer of the published
surface**, which its own manifest says. The repository had arranged to catch the
mistake and the claim talked past the arrangement.

**With no agreeing rows, the instrument is the suspect -- and a control you
already know the answer for is what tells you.** The conformance harness has
said this since it was written: _the rows that agree are what make the rows that
disagree trustworthy_, and a suite that is uniformly wrong looks exactly like a
renderer with one defect per row. It arrived from a new direction while a
Linux acceptance harness was being written, and the direction is worth having.

That harness loads the built `.node` on six images and reports which can run it.
Its first version put the loader in `node -e` with the script escaped into the
shell command. **Every row came back unreadable -- including `node:22`, which
was known to load.** The escaping mangled the script, node died on a syntax
error, and the harness reported six failures for a binary that had one. Nothing
in the output distinguished that from a binary that runs nowhere.

The only thing that separated them was a row whose answer was already known. So
`node:22` stays in the list permanently, labelled as the control: it ships
fontconfig, so it proves nothing about portability and it is not part of the
pass criterion -- **its entire job is to fail when the harness is broken rather
than when the binary is.** A harness with no such row can report a catastrophe
and a typo identically.

The same run taught the second half. Once the control was reading correctly it
showed that `node:22` had been a _soft pass_ all along in an earlier table: it
loads because that image happens to carry the libraries whose absence is the
whole question. A row can be green, honest, and still prove nothing -- which is
why the harness now checks the libraries are genuinely absent before it trusts
a load, and reports `SOFTENED` rather than a pass when they are not.

**A check that reads a proxy for the property can be confidently wrong about
which thing is broken.** The same lesson one layer down, and it cost an hour of
build time.

Three static archives had to be position-independent, because the addon they go
into is a shared object. The check written for it counted `R_X86_64_32`
relocations with `readelf` — a reasonable-looking proxy, since that relocation
is exactly what makes an archive unusable in a shared object. **It reported the
two good archives as broken and the broken one as clean.** meson builds with
debug info by default and `.rela.debug_*` is full of absolute 32-bit
relocations that are perfectly legal in a shared object, so the archive with
20931 of them was fine and the two with far fewer were the ones the linker
would refuse.

It was caught only because it disagreed with a real link error that named a
different library. Nothing about the check itself looked wrong.

The replacement asks the linker: `ld -shared --unresolved-symbols=ignore-all
--whole-archive`. If `ld` accepts the archive it can go in the addon, which is
the only property anyone wanted to know. **This is the ceiling-versus-gate rule
at a different layer** — the relocation count was ceiling-shaped, and it failed
the way ceilings fail: quietly, confidently, and about the wrong thing. Where
something will later be decided by a tool, ask that tool now rather than
reading a number that correlates with its answer.

**A boundary that names its own internals when a caller makes a mistake teaches
nothing.** Writing `ellipsis: true` from plain JavaScript throws `side value 2 is
neither a string nor a Buffer`. That message names a slot index in the arena's
side table: it does not name the property, the node, or the value the caller
wrote, and there is no way to get from it to `ellipsis` except by reading this
repository's encoder. The same message answers `fontFamily: 42` and `color: 42`,
differing only in the index. **The refusal is correct and the diagnosis is
useless**, which is the worst combination for the caller most likely to hit it --
someone porting a script, whose type checker is not in the way.

The other half is worse: `maxLines: '1'` is accepted with no complaint at all.
So the boundary rejects some wrong types opaquely and admits others in silence,
and a caller cannot tell from the outside which kind of mistake they made. The
arena writer's `text(value: string)` takes its argument on trust, and TypeScript
is the only thing checking it -- which means it is checked exactly for the
callers who did not need checking.

**A `want` column in a repro is arithmetic until somebody measures it.** A
fifty-line reproduction printed `want 68.0` beside eight configurations, and
that number was its author's expectation derived from the spec. It became a
measurement only when the same eight shapes were built as HTML and Chrome
answered 68 for all of them. **A reader meeting the file cold cannot tell those
two apart**, so the file itself has to say which it is -- an expectation that
reads as a measurement is the most portable kind of wrong, because the repro is
the artifact that gets pasted somewhere its author is not.

**Measure a defect against the fix branch before describing it in terms of that
fix.** Three negative-margin defects sit in one region of taffy and one of them
is fixed and merged. Two of the other two would have been written up as
"probably related to #1152" on the strength of sharing a symptom -- until the
repro was run against `main` and came back identical, which turns "an unfixed
corner of a known bug" into "a separate live defect". **Sharing a region and a
symptom is not evidence of sharing a cause**, and the check costs one build.

**When a comparison changes a container, check whether it changed the children
too.** A grid holding five cards measured 32 taller than "the same cards in a
flex row" -- and the row version had also wrapped each card in a `div`. Two
things changed and the difference was attributed to the one under suspicion. A
plain wrapper inside the **grid** turned out to fix it just as well, so the
container was never the variable. **Three separate comparisons failed this way
in one day**, and each time the arithmetic closed anyway, which is what made
them survive.

**Reduce from the top when reducing from the bottom keeps failing.** Six
minimal cases were built to reproduce that 32 and none did -- fixed heights,
content heights, wrapping text, the container's own `overflow`, border, radius
and shadow, the grid alone, the grid with the real children. Each failure costs
a build and says only that one candidate is not sufficient **alone**, so a
combination can hide from any number of them. **Deleting from a known
reproduction cannot**: every deletion either preserves the symptom, proving the
removed part irrelevant, or destroys it, naming a participant. Bottom-up
searches a space; top-down bisects one.

**When two of your own instruments disagree, run the doubted one over the
reference, where the answer is known.** Our straight run read `on:8 off:4` and
our arc read `off:3.1` on the same box in the same renderer -- an internal
disagreement, but read by two different instruments: a pixel scan along a row
against a six-hundred-step walk with each sample floored to a pixel. **A walk
could plausibly under-report a gap for reasons having nothing to do with what
was drawn.** What settled it was running the same walk over Chrome: it
reproduces a gap of 4 as `4.1`, `4.2`, `4.7` when the renderer draws 4, so it
does not systematically shrink gaps, so ours reading `3.1` is the drawing.

**That is the reference calibrating the comparison rather than being compared
against**, and it is the same rule as measuring already-pinned rows before
extending a table -- the known value simply happened to live in the other
engine. A second renderer is worth having for this on days when it agrees with
us about everything.

**Point an instrument at a known value before trusting it on an unknown one.**
Four instruments produced confident wrong answers in a single day: two colour
detectors that measured their own thresholds, a line-box marker that grew the
box it was measuring, and a grep over a truncated log that reported no tests had
run. **Care did not separate them and neither did plausibility** -- "no tests
ran" looked wrong enough to check twice, "the strip is 246 tall" did not, and a
plausibility check passes exactly the instruments that are subtly wrong.

What works is cheap and mechanical: **run the instrument over values already
known and check it reproduces them.** The line-box table is the worked example.
Measuring `line-height: 1` meant measuring three rows that were already pinned;
the marker reproduced two and gave `11.0` where the third says `8.0`, which
exposed the marker rather than the pin. **The table validated the measurer
before the measurer extended the table.** Without those three rows the fourth
would have been a plausible number from a broken instrument.

**A measurement taken through a boundary that truncates it reports the
boundary, and it reports it as a success.** The showcase card was measured at
`903 x 679` against a reference's `679.5` and read as agreement to within half a
pixel. The `Root` was 700 tall and clipping the card: on a 1000-tall page the
card is `810`, so the real difference was a hundred and thirty pixels rather
than ten. **The clipped number was more convincing than the true one**, because a
boundary is a clean value and agreement against it looks tight. An entire
conclusion -- "only the font metrics remain" -- rested on it, was repeated as
established, and was an artifact.

**Some properties need a count, not a pattern.** Two separate people converting
`lineHeight` call sites by regex, hours apart, each silently missed a subset:
one missed nested-brace objects and converted 5 of about 25; the other matched
`lineHeight: 1,` and missed twelve sites spelled `lineHeight: 1 }`. The second
then measured the patch and nearly filed a real defect as a rounding error.
**Count the sites first, then assert the edit touched that many.** The general
rule is to check an edit landed before measuring what it did; the specific one
is that this property's call sites are spelled inconsistently enough that any
pattern over them misses some.

**A trailing `echo` hides the exit status of the thing you care about.** A
backgrounded gate reported `completed (exit code 0)` while the run had exited
`101`: the wrapper reports the status of the whole shell line, and the line
ended in an `echo`, which always succeeds. **Do not put anything after the
command whose status matters**, and grep the output for your own marker rather
than trusting the notification.

**A quiet tree is a window someone has to be told to keep, not a state to be
read once.** Two lanes were told in the same round that the tree was still and
that a file-editing task could start. The gate then read `paint.rs` in a
half-written moment and failed to compile a symbol that exists. **Declaring the
window is the supervisor's job**: the other lane must be asked to hold edits,
not merely to hold gates, and told when it may resume.

**A comparison between two different kinds of number fails, and the failure
reads as a defect.** This file already says layout and paint are different
stages. The trap is what happens when you forget: a test asserting our box
against Chrome's `getBoundingClientRect` compares a **painted edge** to a
**layout rect**, and it fails. For a minute that reads as the snap not working,
because a failing assertion that is really a category error looks exactly like a
failing assertion. taffy rounds the solved tree to whole pixels, so a
`LayoutResult` cannot carry a sixty-fourth at all -- the snap is observable only
at the snap. **Before believing a failure, check that the two sides are the same
kind of measurement.**

**A test that needs a private item made public is asking the wrong question of
the wrong layer.** `snapped` was made `pub` so an integration test could reach
it, and `lib.rs` has `pub mod layout`, so that was not a testing affordance --
it was permanent public API of a published crate, decided as a side effect of
writing a test. The crate's own `#[cfg(test)] mod tests` could already see it.
Moving the test inward cost nothing and the public surface was unchanged.
**Widening a surface is a decision that deserves its own argument**, and the
argument is never "a test needed a door".

**A type is a claim that the code cannot contradict.** `renderLegendItem` was
declared in `BaseChartOptions` and read nowhere -- two occurrences, a doc
mention and the declaration. A caller could pass it and it silently did nothing.
A doc comment describing a transform the code never applies can be caught by
reading the two together; **a declared option with no reader has nothing to read
against it**, so nothing but a second implementation honouring it could find it.
This is the strongest argument for the two-surface rule in the file.

**An assertion on a magnitude cannot tell a defect from its fix when both sit
the same distance from the truth.** `rounding_drift.rs` was written to watch the
edge that comes out a pixel wrong, and it asserts that the worst
`|stack_bottom(n) - exact|` is exactly `0.5`. Edge five was `52` where Chrome
gives `51`, and the exact value is `51.5`: `|52 - 51.5|` and `|51 - 51.5|` are
both `0.5`. **So the pin passed before the fix and passed after it, and the whole
workspace reported 517 passed with nothing moved -- on a change that moved the
very number the test exists to watch.** The absolute value discards the sign,
and the sign was the entire content.

This is the rule about a pin having to be seen to fail, arriving from the other
direction. There, a pin that never fails is untested. Here, the pin **can** fail
-- it just cannot fail for the reason it was written. **Assert the value, not the
distance from the value**, whenever the direction of an error is what
distinguishes wrong from right.

The reporting matters as much as the finding. The run was green and the honest
headline was _no pin moved, and one of them should have_.

**Deleting a scratch file straight after using it is sufficient against a
snapshot and useless against a walk.** A probe named `zz_edges.rs` lived in
`crates/meo-canvas-core/tests/` for about a minute, and another session's
`just ci` failed with ``file `crates/meo-canvas-core/tests/zz_edges.rs` does not
exist``. Cargo auto-discovers `tests/*.rs` as test targets and `cargo fmt`
enumerates targets and then opens them, so a file present for the first step and
gone for the second breaks the run in a way a file that simply stays never
would. **Probes belong in the scratchpad**, and the reason is not tidiness: being
outside a `tests/` directory is what keeps them invisible to target discovery.

**A whole-tree gate cannot attribute a failure while another lane holds
uncommitted work.** The file split stops two workers editing the same file; it
does nothing about a process that reads everything. A gate run while a colleague
has seventy-seven uncommitted lines of shared layout arithmetic open reports a
failure that belongs to either of them, with nothing in the output to say which.
**Targeted checks are the signal while another lane is live**; the full gate runs
when the tree is quiet. Same lesson as the gate lock, in a place the lock does
not reach.

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

The two floors, which platforms run them, and why one file is excluded are in
"Coverage: two floors, two reports, and who runs it". What belongs here is the
instrument rather than the policy: lines and regions are the only dimensions
`cargo llvm-cov` can fail on -- there is no `--fail-under-branches` -- so branch
percentages reach the report and `target/lcov.info` for reading while regions is
what refuses a merge. A region is a span with its own arm count, so an untaken
arm still lands in the number that gates.

Source-based coverage instruments Rust only, so Skia is linked but never
instrumented and its lines never enter the denominator.

One file is excluded from the workspace denominator, and it is named by path in
the `coverage` recipe rather than by a pattern, one path at a time, so the list
is reviewable in a diff. `meo-canvas-node/src/lib.rs` earns it by being callable
only from V8 -- not by being generated -- and the exclusion is paid for by a
second floor that measures it from the suite that does call it. Everything else
this project implements stays in the denominator.

**A green result produced by the check breaking its own precondition is worse
than a red one.** The acceptance harness loads a built addon in stock base
images to prove it needs no font packages -- and four of the five images
(`debian:12-slim`, `rockylinux:9`, `amazonlinux:2023`, `almalinux:8`) ship no
Node at all, so the harness has to put one there first. The obvious way,
`dnf install nodejs` or `apt install nodejs`, can pull `libfontconfig` and
`libfreetype` in as transitive dependencies: **the check would install exactly
what it exists to prove is absent**, then report a clean load on an image where
a real consumer fails at `dlopen`. So Node arrives as a distro-independent
binary, and the harness asserts `libfontconfig.so.1` and `libfreetype.so.6` are
still missing after Node is in place and before the probe runs. That assertion
is the harness testing the artefact rather than testing itself, and it is worth
more than the probe it guards.

The direction is what makes this shape dangerous rather than merely wrong. A
check that breaks and goes red gets fixed that afternoon. A check that breaks
its own premise and goes **green** is indistinguishable from the thing working,
and it keeps that way until a user reports what the check was built to catch.

This repository has now met that shape four times, and the common thread is a
check that quietly repaired the condition it was meant to fail on:

- **The doc-example generator rewrote the specifier it should have refused.**
  `PACKAGE_SPECIFIER` is rewritten to a local path before the examples compile,
  so an example naming a package nobody can install compiled green -- the
  rewrite repaired it out of sight. It read `meo-canvas` for a while after the
  package was scoped, and the gate never once complained.
- **An exclusive claim was defended against the wrong axis.** The npm README
  said the animation helpers were the only JavaScript that runs; `Chart` runs
  too. It does not _draw_, which is the axis the sentence beside it defends, and
  that is what made the wrong claim read as safe.
- **A scripted revert silently replaced nothing**, and the suite that then
  passed read exactly like a test that could not discriminate. The check had
  been handed an unchanged tree and reported on it faithfully.

The question that catches all four is not "does this check pass" but **"what
would make this check pass while the thing it tests is broken"** -- and if the
answer is anything the check does to its own environment, that step needs an
assertion of its own.

**A test that passes either side of the behaviour it covers is not covering
it.** Twelve tests survived a change to how _every_ unreadable row in the
acceptance harness is reported — because they asserted the row's `status` and
never its `kind`, and the kind is the thing that decides what the reader is
told. `UNREADABLE` moved from "this image cannot load the binary" to "this is
not a verdict either way", and the suite could not tell the difference. The
check for this costs one run: **change the behaviour deliberately and confirm
the test fails.** If it does not, the test is describing something adjacent to
what you meant.

It is the same shape as the `flex-alignment` rows that agreed under both rules
and the box-shadow fixture whose spacer children could not discriminate: in each
the assertion was true, stayed true, and was true of the wrong thing.

**A predicate built from comparisons has a third case its author did not write,
and `NaN` falls into it silently.** Every comparison against `NaN` is false, so
a value that is not a number fails the test and takes the else branch -- and the
else branch is usually the one written for input that is unusual but valid. The
thing that is not a number is then handled as though it were an extreme one.

`formatColor` had exactly this. `inGamut` is
`[r, g, b].every(c => c >= 0 && c <= CHANNEL_MAX)`, its else branch is the
`color(srgb …)` form that carries a channel hex cannot hold, and a `NaN` channel
was therefore **judged out of gamut** and printed faithfully as
`color(srgb NaN NaN NaN / NaN)`. Nothing was broken. The predicate did what it
says, and 300 and `NaN` arrived at the same branch because the only question
asked distinguishes neither from the other.

The contrast is in the same package, and it is what the rule looks like applied.
`older` in `addon.ts` compares two dotted version numbers component by
component, and names the case rather than letting it fall through: **"a
non-numeric component makes the comparison meaningless rather than false"**, so
it returns early instead of asserting an ordering nobody established. One
function asked "is this outside the range", got `NaN`, and answered yes; the
other asked "can these be compared at all" first.

So when a branch exists for input that is extreme but legal, ask what arrives
there that is not a number at all. It is a different failure from the ones above
-- those are about a statement being narrower or wider than the code around it,
and this is about a total function having a case its author never named.

**A repository typechecks its own types in the one configuration no consumer
has.** Every `tsc` here runs with `"types": ["node"]` set and `skipLibCheck`
off. A consumer's runs with neither: `tsc --init` writes `skipLibCheck: true`,
and someone who has not needed a `types` field has not written one. The gap
between those two configurations is invisible from inside, and a defect that
lives in it compiles cleanly here and degrades there.

**TypeScript 6 does not auto-include `node_modules/@types` at all** -- not with
the default, not with an explicit `typeRoots`. A consumer resolves even
`process` only by writing `"types": ["node"]` or a reference of their own.

The general form is not about this package: **any `.d.ts` that names an ambient
global is a consumer-visible `any` under a default tsconfig**, and it is `any`
silently, because the error that would have said so is inside a declaration
file and `skipLibCheck` skips declaration files. `Buffer` is only the instance
we happened to ship. It is why this class of defect exists now and did not a
year ago, and why a package can be correct in its own repository and wrong
everywhere it is installed.

`dist/canvas.d.ts` named `Buffer`, and `const bad: string = await
canvas.toBuffer('png')` compiled clean in a consumer. A grep for `any` in the
emitted declarations found nine hits and all nine were prose: **the `any`
arrives from an unresolved name rather than from the keyword**, so the obvious
search cannot see it either.

Two fixes look right and are not, and both were measured on 6.0.3 and on 7.0.2
before either was believed. `import type { Buffer } from 'node:buffer'`
survives declaration emit -- the import is right there in the shipped `.d.ts`,
and any reader would nod at it -- and still does not resolve, because a bare
`node:` specifier needs `@types/node` already loaded, which is the thing the
consumer has not done. A `/// <reference types="node" />` in the source does
resolve and is elided from declaration emit: it reaches `dist/canvas.js` and
never `dist/canvas.d.ts`, with `types` set in the build config and with it
emptied to `[]`. What works is adding the reference to the emitted declaration
after `tsc`, which `tools/reference-node-types.mjs` does and `just build-js`
runs. **The failed attempts are the substance here**: anyone can be told to
inject a directive after `tsc`, and without them the next person tries the
import again.

**That is also why `@types/node` is a runtime dependency and not a development
one.** A reference is followed transitively, so the package has to sit beside
this one -- hoisted by npm, and inside pnpm's store because it is declared.
Making it a peer dependency would break the fix. The evaluation that found the
defect had recommended exactly that, on the code as it then stood, and the
recommendation stopped being right the moment the fix existed.

`verify-package.mjs` is what holds all of this: it compiles a consumer with the
default tsconfig, and its control -- the assignment above -- has to fail. **A
clean compile there is the defect, not the absence of one**, which is why the
assertion reads backwards.

**vitest does not typecheck, so `test-js` and `typecheck` are not
interchangeable.** Narrowing `TrackConfig.spring` to
`Omit<SpringConfig, 'from' | 'to'>` left `just test-js` green and `just
typecheck` red, and the red was the narrowing working: the test that covers the
runtime refusal could no longer be written in TypeScript. A green suite says
nothing about a change whose whole content is a type. It is the same division
_What a public enum promises_ describes on the Rust side, where the witness
tests are `#[cfg(test)]` and so `cargo build` will not catch a new variant and
`cargo test` will -- in both languages the instrument that sees the change is
not the one whose name suggests it. That narrowing had the same deadline as
`#[non_exhaustive]`, and for the same reason: it is free before a first release
and breaking after one, because it is only free while nobody holds a
`SpringConfig` variable.

### What only a second implementation can see

The chart port has three checks and they answer three questions. A geometry
table says the two surfaces compute the same numbers. A render says the numbers
put ink where the arithmetic claims. A byte comparison says the two surfaces
_assemble_ those numbers into the same tree. **Nothing but the third can see a
tree built wrongly out of right numbers**, and it found three of them in one
run:

- `with_style` **replaced** a style rather than merging it, so
  `Column::new().with_style(Style::new().width(…))` discarded the
  `flex-direction: column` the constructor just set. Three of the four sites
  with that shape were `Row::new()`, where the discarded value is the default
  and nothing changes — **which is why the pattern read as fine**. Chart code
  used the flat `Styled` setters after a container constructor for that reason.
  The workaround outlived the defect: `with_style` merges as of 4 September
  2026, and the finding is what made the case for changing it.
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

taffy **drops a negative margin on a growing flex item**. A flex container with
an automatic main size resolves to the child's size with the margin excluded
where it should resolve to `child + margin`, and it does not scale with the
margin: `-24` and `-0.5` both leave a 500-tall child in a 500-tall container.

Chrome gives `child + margin` in all seventeen measured rows and never lets
`flex-shrink` into the answer. **So it is a disagreement with the browser, not
with a reading of the specification** -- the distinction that decides whether
to file it, and the reason the measurement is worth waiting for rather than
asserting CSS from memory.

**A test asserting Chrome's numbers would fail today, and a failing test cannot
be committed.** So `taffy_negative_margin.rs` asserts what taffy _does_, with
Chrome's value beside each and the instruction in the failure message: when
this fails, the defect is fixed, delete it. **The wrong answer, pinned
deliberately, is the notification** -- and without it the defect is entirely
silent, because a caller sees a box of the wrong height and no error.

A second is live beside it and has a different trigger: **a grid item with
`overflow: hidden` makes an ancestor's height ignore a negative margin**, one
configuration of eight, where flex, `overflow: visible` and a bare wrapper are
all correct. `overflow: hidden` inflates the item's flex basis to its
max-content size and the margin vanishes into the same floor.

**Any v2 caller writing `flex-grow` with a negative main-axis margin gets the
margin ignored, and any caller clipping a grid item above one gets a container
too tall**, and nothing in the vocabulary hints at either.

**Both are filed, both are fixed on `main`, and neither is in a release** --
issues #1162 and #1163, closed by PR #1164, merged as `adef6dd`, which is two
commits ahead of the `v0.14.0` tag. A close is not a release and a merge is not
one either; the pins stay until a version we can resolve carries the fix, and
the release that does will fail them.

The pin is also what reads a release. One region of taffy holds two of these,
and the second -- a negative margin on a **non-shrinking** item applied as
`child x max(0, 1 + margin)` -- is issue #1151, fixed by PR #1152 and released
in `0.14.0`. Its rows were pinned the same way, so the upgrade that carried the
fix arrived as a failing assertion naming the browser's number, which is the
only form of notification a silent defect has. Those rows now assert Chrome and
double as the check that the version floor holds.

Check upstream before writing either the pin or an issue: `main` as well as the
release, the changelog, and the open issues. **A defect already fixed and
unreleased needs a wait, not a pin** -- and a wait needs a date to end: read the
_released_ signature, since a changelog entry and a merged branch are neither of
them a crate a build can resolve.

### Widening a matrix is the measurement, not the diligence

The first version of the entry above said _a negative **top** margin collapses a
**column** container_. Both halves were artefacts of the single case anyone had
looked at. Widening the matrix -- four edges, both axes, the container's own
properties, a sibling, an explicit basis, and a magnitude sweep -- **did not
confirm the finding, it replaced it**: not top, not column, not a collapse, and
proportional to the child's own size, which is what turns a symptom into a
mechanism a maintainer can go and look at.

**A report of the first version would have been closed as fixed-for-the-wrong-case
even by someone who acted on it.** So for anything leaving the machine, the
question is not _have I checked this_ but **is the matrix as wide as the claim**
-- and every dimension held fixed while another varies is a dimension the
conclusion silently asserts.

Its companion is the same demand made of the instrument: a row-direction box in
a column page stretches across the page's cross axis, so the first measurement
of two of those rows read the **page's** width rather than the child's effect.
It was caught because `903` was a recognisable number. **A measurement that
happens to be a number you know is not a check; ask what the case can produce
before reading what it did.**

### Could this evidence have failed?

**One rule, learned three times from three different defects.** A check that
passes tells you nothing until you know it was capable of failing, and every
way of getting that wrong looks identical from the outside: a green result.

The three worked examples below are kept whole, because the rule is unusable in
the abstract -- what makes it applicable to a fourth case is having seen what it
looked like in three. What they have in common is the question, asked of
whichever part is doing the work: **the assertion, the inputs, or the
instrument.**

#### The values the case can produce

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

#### The parameter the inputs never varied

Four times in one audit of the animation helpers (3 and 4 September 2026),
and every time the shape was the same. The spring floor: every pinned spring
ran 0→1, so a floor of `from` and a floor of literal zero were the same number,
and only `from: 5` at `t = -0.5` told them apart. `Track::duration`: every
pinned track had `delay: 0`, so Rust excluding the delay and JavaScript
including it agreed on every row, and the one duration row compared was
reported "same" — right about that input, wrong about the rule. A first
`sequence` probe that passed no stagger and concluded stagger was ignored.
And `parseColor`'s alpha: the colour block tested `formatColor` from an
object, where alpha never passes through the parser, and `mixColor` on opaque
colours, so alpha other than 1 through the parser was the parameter no row
varied — and then of the six alpha rows finally added, `0.25`, `0.5` and `0.75`
passed because they are exact in binary32 and only `0.1`, `0.33` and `0.9`
failed.

More rows at the same inputs do not help. Different inputs do. When a table is
generated, every parameter the helper takes gets rows where it is non-zero,
non-default and, where the domain allows, outside the range the tables
otherwise sample: non-zero `delay` and `stagger` together and separately,
non-zero `from`, `t` outside 0..1, counts of 0, 1, 2, 3 and a fraction, alpha
values that are not exact in binary32 and alphas written as hex bytes. A row
count that matches the generator's is necessary and not sufficient — one table
silently lost three rows because `#` is the comment character and a hex colour
begins with one, and it was the row census, not the values, that found it.

The same census applies to tests. When pinned values move into a table and the
tests that held them are rewritten as a walker, a test that was not a pin --
a refusal, a `throws` contract -- can go with them, and a deleted test does not
fail. The one time it happened here, an ESLint unused-import error was the only
signal. Diff the test names before and after any move between files.

#### The thing the probe could not have seen

The two above are statements **narrower** than the code beside them reads. The
opposite is a statement **wider** than the measurement behind it, and it is the
easier of the two to make.

The font registry is the worked example, and it is mine. I measured that a face
registered through one `Fonts` was visible to another built afterwards and to a
renderer made later, wrote _"registered for the whole process"_, and committed
it. The probe had only ever run on one thread. It could not have told a
thread-wide registry from a process-wide one -- **structurally**, in the same
way a conformance table whose inputs all zero a parameter cannot see a rule
about that parameter. The registry is the thread's: register on a worker and
the main thread still answers `false` after it joins, and register on the main
thread and a worker spawned afterwards answers `false` too. Both directions,
because one direction cannot distinguish the two scopes.

It mattered more than a word. Process-wide would mean one contaminated registry
that any request can poison for every other; thread-wide means a pool has one
copy each, which is better news and **more** work for a caller -- a
registration per worker rather than one at boot. A reader given the wrong one
would either over-fear it or under-plan for it.

The catch has a name: **ask what the probe could not have seen.** Not whether
it passed, and not whether the reasoning was sound -- what was outside the
frame. A single-threaded test cannot find a thread boundary. A table sampling
0..1 cannot find a rule about 1.5. Thirty-four chosen inputs are evidence about
those thirty-four. Say the claim the evidence supports, and if the wider claim
is the useful one, go and measure it.

### Two doc blocks in a row leave one of them attached to nothing

A forty-line comment describing `barLayout` -- its arithmetic against v1 line by
line, and the recorded reason it refuses negative values -- sat directly above
the function. It was also directly above `assertDrawable`'s own comment, with no
code between the two blocks. TypeScript attaches only the nearest, so the
nearest went to `assertDrawable` and the other went nowhere: the text was in the
file, in no reference, and read correctly in both. A diff cannot show this. Both
blocks are well-formed, both describe the thing under them, and the one that
lost is the one further away.

**The zero baseline is what catches it.** `just docs-js` reports a member as
undocumented that visibly has a doc above it, and that combination has one
cause: the doc attached to something else. Before the count reached zero the
signal was there and buried in ninety-one others. This is the same shape as the
row census and the test-name census -- the artefact looks right, and only
counting what actually arrived tells you otherwise.

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

Six ways a v1 component did not mean in v2 what it says, found by carrying
`gi-showcase-card.component.ts` across a line at a time. **One of them is now
closed**, and it is kept in place rather than deleted: a porter who has read an
older copy of this list needs to be told the hazard went away, and the reason it
was a hazard is the part worth keeping. **They are listed with
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

**3. `ellipsis` no longer changes type -- this hazard is closed.** It read
"boolean to the string that gets drawn, a type error, which is the good case",
and it was neither closed nor quite the good case. From TypeScript `true` was a
type error; from plain JavaScript or through an `as any` it crossed unchecked
and the arena refused it at the far end with `side value 2 is neither a string
nor a Buffer` -- a throw naming a slot index, which is the boundary problem
recorded above.

v2 now takes v1's `boolean | string` on both surfaces. `true` draws U+2026, the
character CSS uses and the one v1 draws (`text.canvas.ts:1244`), measured in
Chrome rather than assumed; `false`, `''` and leaving it out all truncate
without a marker, which is where v1's truthiness guard landed. **`false` is the
one to notice**: it is v1's own applied default (`text.canvas.ts:207`), so the
caller most likely to have written it explicitly is the one migrating.

The scene still carries a resolved `Option<String>` -- the boolean is resolved
at the edge, because no measurer, line-breaker or painter reads which spelling
asked for the marker. A Rust caller writes `scene::DEFAULT_ELLIPSIS` for the
same thing; the two surfaces hold one capability in two idioms.

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

## Releasing

### The targets, and the one that is missing on purpose

    linux-x64-gnu     linux-arm64-gnu
    linux-x64-musl    linux-arm64-musl
    darwin-arm64      win32-x64         win32-arm64

Seven, the same seven `meo-skia-canvas` builds. `win32-arm64` runs on the
`windows-11-arm` runner; it was listed here as "unbuilt, waiting on a runner"
until the day it was added and passed its first rehearsal in seven minutes,
because rust-skia ships a prebuilt Skia for that triple -- a commit message
said otherwise, from reading the wrong function, and the `TARGETS` comment
records the correction beside the code.

**`darwin-x64` is excluded because Apple stopped supporting Intel**, and
because it would need `macos-13`, the last Intel runner image, which GitHub is
retiring. Building it would mean owning a target we lose on someone else's
timetable rather than our own. `meo-skia-canvas` reached the same conclusion
independently and ships no Intel Mac build either.

**The rule the set comes from is the user's**: everything within our control,
and whatever is not gets named as such rather than left as a silent gap.

**Adding a target is one row in `TARGETS` and nothing else.** That is the
architecture the `TARGETS` → build matrix → `optionalDependencies` →
`PLATFORM_PACKAGES` → ABI floors chain exists to provide. Anywhere a second edit
is needed, that is a defect in the chain rather than a step in the task. The
seventh target was one row, and it also needed its first publish by hand --
see **A new platform package cannot start on OIDC** below -- which is a fact
about npm rather than about the chain.

The npm package is **`meo-canvas`**, unscoped, continuing v1's lineage at 10.x.
The cargo crate is `meo-canvas` too and versions independently, starting fresh
at 0.1.0.

It was published as `@l7aromeo/meo-canvas` for a while, to let v10 be installed
beside `meo-canvas@9`. That requirement was real; the scope was the wrong answer
to it. From a registry, an alias lets the consumer choose the local name:

```text
npm install meo-canvas-v10@npm:meo-canvas@next
```

### A Linux artefact is a property of its build base

The same source built on `ubuntu-latest` demanded glibc 2.35 and failed to load
on five of the six images the package claims; built in
`containers/Dockerfile.glibc` -- `manylinux_2_28`, which is AlmaLinux 8, gcc-
toolset-14, freetype and fontconfig compiled from source and linked statically
-- it demands 2.28 and loads on all six. Nothing in the tree changed between
those two runs. So `just addon-container <suffix>` builds the Linux addons in
that image, `Dockerfile.musl` is its Alpine sibling, and `release.yml` calls
the same recipes a person runs.

The musl image is a second instance of the work rather than a variant: Rust's
musl targets default to `crt-static`, which cannot produce a `cdylib` at all
(`-C target-feature=-crt-static` is the fix), rust-skia ships no prebuilt Skia
for musl so the image compiles all of it with clang -- 32 of a 35-minute build,
measured -- and an explicit `--target` must **not** be passed, because cargo
then builds build scripts without `RUSTFLAGS` and `skia-bindings`' script, now
static, cannot `dlopen` libclang. `addon-container` asserts the image's host
triple is the suffix's instead, which is what `--target` was buying.

Two instruments, and they are not substitutes. `just abi-floor` reads the
undefined symbols of the built `.node` and fails if it demands a GLIBC or
GLIBCXX version above what `TARGETS` declares; it also prints the measured
number beside the declared one, because **a floor declared too high fails
nothing** -- it under-promises quietly, and the declaration sat three commits
stale that way once. `just acceptance` mounts a checksum-verified Node into six
images with nothing installed -- `node:22-slim`, Debian 12, Rocky 9, Amazon
Linux 2023, Alma 8, and `node:22` as the control that ships the font libraries
and therefore proves nothing -- and loads the addon. A ceiling compares version
tags and an unversioned symbol has none: a binary under every ceiling still
failed on `_M_replace_cold`. **The floor diagnoses; the load decides.** The
musl pair carry no floors at all, because musl does not version its symbols,
so for them the load is the whole of the evidence.

Windows and macOS build on the runner and are verified by `verify-packed`,
which installs the tarballs elsewhere and renders. It is skipped for musl,
because it runs on the runner and a glibc runner cannot load a musl `.so`.

### The goldens are per architecture, and no tolerance was added

`tests/fixtures.rs` compares against `expected.<os>-<arch>.png` where one
exists and `expected.png` otherwise. On `linux-x86_64`, 15 of the 23 fixtures
are byte-identical to the macOS reference and 8 are not -- and the 8 are the
ones with a curve, a gradient, a blend or a glyph, the same dividing line the
file's header measured for the Metal backend. Every Chrome conformance suite
passes on Linux, so the pixels differ and what they mean does not. Windows
differs on the same 8, five of them byte-identical to Linux's -- the axis for
those is architecture, not operating system -- and the three that draw glyphs
differ per OS.

A variant exists only where a platform is measurably different; its absence is
a claim the run then checks. `just fixtures-linux` makes the Linux set in the
release container, and `.github/workflows/fixtures.yml` renders the others on
their runners and **uploads an artifact rather than committing**, because a
workflow that pushed accepted goldens could turn a rendering regression into a
commit nobody saw. It is triggered by a push to a `fixtures/**` branch, since
`workflow_dispatch` needs the file on the default branch and the default is
v1's.

### The addon does not ship inside the package

It is ~51 MB. One package per target carries one binary, named in
`optionalDependencies` with its own `os`, `cpu` and, where the platform has a
choice, `libc`, so an install downloads the one it can run. A postinstall
script that downloads was the alternative and was refused: it needs the network
at install time, and breaks offline installs, locked-down CI and
`--ignore-scripts`.

`resolveAddon` looks in three places in order — `MEO_CANVAS_ADDON`, the
`.node` beside the package in a working tree, then the platform package — and a
failure names all three. A checkout therefore tests what it just built rather
than what npm resolved, which is why `just addon` needs no reinstall to take
effect.

### A target is named three times, and a test asserts all three agree

`TARGETS` in `tools/stage-platform-package.mjs` is what a release **builds**,
`optionalDependencies` is what an install **fetches**, and `PLATFORM_PACKAGES`
in `src/addon.ts` is what a process **resolves**. `src/addon.test.ts` checks
them against each other, because any two agreeing while the third does not is a
separate silent failure: built and pinned but unresolved renders nothing with
the binary on disk; pinned and resolved but unbuilt fails every install; built
and resolved but unpinned works only in a checkout, which is where it would be
tested. The release workflow reads the same list rather than restating it.

**Only targets CI actually builds are named.** A key in that table is a promise
`npm install` has to keep, and the READMEs' own audit is what this rule comes
from. A host with no entry gets a message naming its own triple; a musl host is
told the build is glibc rather than being handed a link error.

### Packing is not installing

`npm pack` reports what is in the tarball and says nothing about whether a
consumer can reach it: `exports` can name a path the `files` allowlist dropped,
and a platform package's `main` can name a binary that is not there. Both pack
cleanly and fail at the first import. `just verify-pack` installs the tarballs
into a directory that is not this repository and renders through them.

### What triggers a publish

`release.yml`, and only `workflow_dispatch` -- nothing publishes on a push,
because a push is how code arrives and publishing is a decision about code that
already arrived. `just release-npm` dispatches it on the pushed `v10` HEAD and
refuses on a dirty tree, off the branch, or with unpushed commits; `-dry` runs
the whole thing and stops short of the registry, and is what to run after any
change to the workflow, because the workflow is the only thing that reads its
own YAML. Its `dry_run` input defaults to **true**.

The version is read from `package.json` as committed -- `just bump-npm` sets it
and rewrites every platform pin with it -- and the workflow only ever reads that
file. **Any version containing a hyphen goes to the `next` dist-tag**, never
`latest`: `npm install` resolves `latest`, and a semver range never matches a
prerelease, so nobody reaches it without naming it.

Seven runners build one addon each and pack two tarballs; the publish job
refuses unless all seven platform tarballs are present, then publishes them
**before** the main package, which pins them at an exact version -- the other
order points at versions that do not exist yet. `--provenance` through the
repository's trusted publisher, no token anywhere.

**The tag and the release come last, after the registry has accepted
everything.** A tag pushed first and a publish that then fails leaves a version
number that can never be reissued; `meo-skia-canvas` carries that scar. Tagging
last means a failed publish leaves the tree as it was. The release notes are
the commit subjects verbatim, not a conventional-commit parse that would drop
the two thirds of them written as sentences.

### A new platform package cannot start on OIDC

A trusted publisher is configured on a package's settings page, and a package
that has never been published has none. So the first release of every
`meo-canvas-<suffix>` was refused by the workflow -- correctly, with nothing
published and no tag, because the ordering above protects -- and the dry run
could not have seen it: `--dry-run` never authenticates.

The bootstrap is by hand: download the run's own `tarballs-*` artifacts, and
`npm publish --access public --tag next <tgz>` each one, **one at a time with a
pause between**. Four new names in ten seconds tripped npm's spam gate
(`E403 Package name triggered spam detection`) on the fifth and sixth; that is a
burst heuristic, not a name problem, and the way through is time or a support
ticket. Once a package exists, its trusted publisher is configured on its
settings page and every later version goes through OIDC. The publish loop skips
any `name@version` the registry already has, reading the name from inside the
tarball, so a re-run after a bootstrap publishes the main package and tags.
Before `just release-npm` on a version that adds a platform package:
`npm view meo-canvas-<suffix> version` -- an `E404` means bootstrap first.

**Every platform package must exist before the main one, and the check is per
name rather than per release.** npm and pnpm skip an optional dependency whose
`os`, `cpu` or `libc` excludes the host and never ask the registry about it, so
a missing platform package costs nothing to anyone who could not have used it.
**yarn resolves all seven before it installs any**, and a 404 on one fails the
whole install:

```text
➤ YN0035: │ meo-canvas-linux-x64-musl@npm:10.0.0-alpha.4: Package not found
➤ YN0035: │   Response Code: 404 (Not Found)
➤ YN0000: · Failed with errors in 0s 408ms
```

Measured with yarn 4.10.3, against the same tarballs npm and pnpm install
without complaint. So the window between publishing the main package and
finishing the platform bootstrap is a window in which every yarn install fails
outright -- and that window is **the expected path rather than an accident**,
because the bootstrap is by hand, one at a time, with a pause between. Publish
all seven first, `npm view` each name, and only then the package that depends on
them.

### The reference publishes after the version resolves from npm

`docs.yml` builds the TypeDoc reference on every pull request that touches the
surface, and publishes it to GitHub Pages when a release is published -- which
`release.yml` does only after every package is on npm -- and still polls
`npm view meo-canvas@<version>` before deploying, because published and
installable are separated by propagation. One directory per version,
prereleases included; `latest/` follows the newest **stable** version the way
npm's dist-tag does. The tool lives in `packages/meo-canvas/tools/typedoc/`,
fails on a dead link or a type reaching a signature unexported, and refuses any
undocumented member at all: `undocumented-baseline.txt` is `0`, which a ratchet
reached from the ninety-two there on the day it arrived and which a floor of
zero now holds. Its first build found twelve structural defects in the surface.

### The publishing audit

This document and the seven README files describe the design, and a sentence
true of the architecture but not of the code is fine while nothing is published
and false the moment something is.

**Every capability claim has been read against what runs**, and the sentences
that were wrong were the ones the rule predicts: where work happens, what
formats encode, and what a surface accepts.

What that pass found, kept here because each is a shape rather than an incident:

- **A claim outlived the constraint that made it true.** "The core performs no
  network access" was written before the `net` feature and survived it in three
  places, including the dependency table, which credited the feature to the CLI
  alone.
- **A claim outlived the code it described.** The pipeline said measure builds a
  Skia `Paragraph` per text node and that layout answers the intrinsic widths
  from `min_intrinsic_width()`. That path is `#[cfg(test)]` now; lines are
  broken in `crate::lines` and the intrinsic questions are the same call at a
  budget of zero and of infinity.
- **A document contradicted itself across two sections.** One said baseline
  alignment on measured text was wrong and unfixable through `TaffyTree`; the
  other said it was carried. Fixing a defect means finding every sentence about
  it, and prose has no compiler to say where they are.
- **A list was right about its members and wrong about its bounds.** "Frames for
  GIF and APNG" omitted WebP and AVIF, which animate too and say so in
  `ImageFormat`'s own documentation.
- **An exclusive claim was made against the wrong axis.** The npm README said
  the animation helpers were the only JavaScript that runs. `Chart` runs too --
  it computes bar widths, slice angles and path data. It does not _draw_, which
  is the axis the sentence beside it defends, and that is what made the wrong
  one read as safe.
- **A runnable line was never run.** The CLI README's example named `scene.mcsc`
  where every scene in the repository is `.mcs`.

## Dependencies

Every dependency is on its latest stable release, and the two exceptions say
why.

|                   |      |                                                                                                           |
| ----------------- | ---- | --------------------------------------------------------------------------------------------------------- |
| `meo-skia-canvas` | 0.11 | Skia, text shaping, encoding. `default-features = false`.                                                 |
| `taffy`           | 0.14 | Flexbox, CSS grid, block layout. Without `calc`. 0.14 changed the measure signature and moved `min_size`. |
| `csscolorparser`  | 0.8  | CSS colour syntax. Holds channels as `f32`, which is where alpha loses its author's digits -- see below.  |
| `neon`            | 1.1  | Node addon.                                                                                               |
| `clap`            | 4.6  | CLI.                                                                                                      |
| `thiserror`       | 2.0  | Error types.                                                                                              |
| `ureq`            | 3.4  | Remote images, behind the optional `net` feature the core and the CLI each carry.                         |
| `png`, `gif`      | dev  | Decoding output back in tests; a byte count proves nothing.                                               |

|            |       |                                                                                              |
| ---------- | ----- | -------------------------------------------------------------------------------------------- |
| bun        | 1.4.0 | Package manager and the runtime the JavaScript examples use. `packageManager` pins it.       |
| typescript | 6.0.3 | Not 7: typescript-eslint supports `<6.1.0` and TypeDoc 0.28 up to 6.0.x. Moves when both do. |
| eslint     | 10    | With typescript-eslint 8 (`recommendedTypeChecked`) and eslint-config-prettier last.         |
| prettier   | 3.9   | Over the whole tree; `.prettierignore` names the machine-written files it must not touch.    |
| vitest     | 4.1   | Tests and the JavaScript coverage floor.                                                     |
| typedoc    | 0.28  | In its own package under `tools/typedoc/`, so it can pin the TypeScript it loads.            |
| playwright | 1.62  | Drives Chrome for the conformance tables. `just setup` installs Chromium.                    |

`csscolorparser::Color` is `f32` on all four channels, so `parse_channels`
returns `[f32; 4]` and `rgba(0, 0, 0, 0.1)` reads back as `0.10000000149011612`
on both surfaces -- inherited, not a JavaScript defect. The ruling is that the
parser returns the number the author wrote, with the browser as tiebreak (it
answers `0.1`; v1 answered `0.102`). The mechanism has two halves, and a
twelve-row measurement is why: an alpha written as a decimal or a percentage
is presented as the shortest decimal that round-trips to its `f32`, which is
what Rust's `Display` prints and which recovers every such spelling tried; an
alpha written as a hex byte is `byte / 255` computed where the byte is known,
because no decimal-shortening reaches 127/255 from an f32. The rows that prove
the second half are `#0000007f` (0.4980392156862745) and `#0008`
(0.5333333333333333); `#000000cc` is the row that **cannot**, since both rules
give 0.8, and it is in the table to say so. One parser, one boundary, no
second parse. The scaling to 0..255 stays in f32, where it is exact, and the
widening happens to the product -- widening first lands `#808080` a hair under
128, which is the trap the fix's own test pins.

The core requires no async runtime, and performs no network I/O unless built
with `net`, which `meo-canvas` forwards under the same name. `just runtime-free`
fails if a runtime enters the tree.

`taffy::TaffyTree` is neither `Send` nor `Sync`: taffy represents every length
as a tagged pointer, so `Style` itself holds a `*const ()`. A tree is therefore
built and consumed on one thread and never crosses a boundary -- which costs
nothing, because `Scene` carries its own style type and taffy's `Style` exists
only inside the layout stage.
