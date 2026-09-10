# meo-canvas -- guidance for agents and contributors

Renders declarative scene trees to images. A caller describes what it wants --
boxes, rows, text, images, paths, grids, charts -- and gets back encoded bytes.
Layout is flexbox and CSS grid; drawing is Skia; text is shaped and broken by
Skia's paragraph engine.

Two public surfaces, siblings rather than layers: a Rust crate and a Node addon.
Both construct the same `Scene` and hand it to the same core.

**Two generations, and only one of them has a stable number.** The predecessor
is **v9**: frozen at `9.0.4`, preserved as the `v9` branch, never to be bumped
again -- so `v9` is a fixed label rather than a version that moves, and the
prose below uses it. Its source is the checkout at `../meo-canvas-old`.

**This renderer is not named by number anywhere in this file.** It ships as npm
10.x and as a crate starting fresh at `0.1.0`, and both of those move, so the
prose says "this renderer" and does not have to be rewritten the next time
either one does. (`meo-canvas-old/package.json` reads `1.0.0` because that
lineage used semantic-release, which never commits the bump.)

This file is the authority on how work is done here. Where it names a fact that
lives somewhere else -- a recipe, a workflow, a constant, a line number -- it
points at that place rather than restating it, because a copy goes stale in
silence and a reader cannot tell a stale copy from a maintained one.

---

## The rules that decide everything

**The browser is the baseline for behaviour. v9 is the baseline for the API.**
What a property _does_ is what Chrome does with it; which properties _exist_ is
what `../meo-canvas-old` offers. The two are answered by different sources and
neither overrides the other in the other's half.

That resolves the case where v9's shape comes from a limitation rather than a
decision. Its radial gradient is a circle because `ctx.createRadialGradient`
makes only circles, and CSS's default is an ellipse -- so the property exists
because v9 has it and behaves as an ellipse because a browser does. It is the
opposite of the text pipeline, where v9 breaks its own lines _because_ a canvas
has no paragraph, and doing it v9's way is what makes the behaviour a browser's.
**Ask which of the two a v9 choice is before copying it.** Where a question has a
CSS answer, the answer is what Chrome does.

**Measure the browser rather than arguing from the specification.** A reading of
the prose is a claim about what the words permit, never evidence about what
anything does. The conformance harness and the rules for pinning an answer are
under _Evidence_.

**Neither surface ships a capability the other lacks, and neither is finished
first.** A change to one is not done until the other has it. A capability behind
a feature flag is one the consumer can have, so the rule is about reach rather
than defaults: `net` puts URL fetching behind a flag on the crate and
`RootProps.httpOptions` has it always on npm, because the addon ships the client
already built and a crate consumer compiles it. Same reach, different price, and
the flag is what lets whoever pays decide.

**That sentence used to say "identical capability", and it was false for as long
as it stood.** `RootProps.httpOptions` could send an `Authorization` header and
the crate had no header, auth or agent surface at any spelling -- `fetch` took a
URL and nothing else -- so one surface could reach an authenticated asset and the
other could not. It is a demonstration of what the rule is for rather than an
exception to it: **the sentence asserting parity is exactly the sentence nobody
re-checks**, because it reads as a policy and is in fact a claim about the code.
`HttpOptions` closes it, per source rather than per scene, which is the finer of
the two -- and a capability the other surface lacks in the other direction is
the same defect wearing a different sign.

**A change's two sides land in one commit.** Ownership decides who edits a file,
not what a commit is: splitting a wire change across two commits to respect a
boundary leaves `ci` red between them for a change that was never in two parts.

**Adding a feature is a commitment to it.** The edge cases and the error paths
get the attention the example path gets, the shape still holds when the next
feature lands beside it, and it survives the thing underneath it moving. A
half-finished capability is worse than an absent one: a caller builds on it, and
the cost of finishing it transfers to them at the least convenient moment.

**Do not leave a known defect in place to satisfy a weak argument** -- schedule
pressure, a passing test that does not test it, or "no one hits this". If
something is wrong and cannot be fixed now, say so plainly and record why.

---

## Working agreements

**Nothing is pushed and nothing is released without an explicit instruction.**
Commit freely, gate the work, then stop and report. Pushing, opening a pull
request, publishing and tagging are maintainer decisions taken one at a time.
Approval for one does not carry to the next.

**Nothing an agent produces for its own benefit belongs in this repository.**
Plans, specs, design notes, task lists, scratch analyses, session transcripts,
progress trackers, review write-ups -- none of it. Work in a scratch directory
outside the repository and let the commit message carry whatever needs to
survive. Such files are written for one moment and are wrong by the next
release; they describe intentions rather than the code that shipped, and nobody
updates them, so they become confidently misleading documentation a future
reader cannot tell from the maintained kind.

What _does_ belong: the commit message, a comment next to the code that needs
explaining, and this file.

**The maintainer's working arrangements stay out of the repository.** Commits,
issues, pull requests and code comments describe the code and the reasoning.

### Git safety

**Never run `git reset --hard`, `git checkout --`, `git clean` or any other
destructive git command without stashing first.**

```bash
git stash push -m "backup before reset"
git reset --hard <target>
git stash pop   # if it went wrong
```

Prefer the non-destructive form where one exists: `git branch -f` moves a ref
without touching the working tree, where `reset --hard` does both.

**`.gitignore` denies by default.** Line 2 is `*`, and every tracked path is
re-included by name. Two consequences that have each cost a day: a new file is
untracked and invisible to `git status` unless an allow names it, so
`git status --porcelain` reporting nothing is **not** evidence that a worktree
holds no work; and a file a workflow requires can be uncommittable, which reads
as the workflow being broken. Before deleting a worktree or trusting "clean" in
a handover, enumerate:

```bash
git ls-files --others --ignored --exclude-standard -- .tmp
```

`git status --ignored=matching` reports the _directory_ rather than its
contents, so it answers a different question than the one being asked.

**Branch names take a conventional type**, matching how commits are written:
`fix/`, `feat/`, `bump/`, `ci/`, `docs/`, `chore/`. Not the author's initial --
git authorship already records who did the work. Renaming a branch that has an
open pull request **closes the pull request** and it cannot be reopened, so get
the name right at branch time.

---

## Evidence

The recurring failure here is not bad code. It is a claim nobody checked,
repeated until it is load-bearing.

**Verify a claim before you act on it or pass it on.** A figure recalled from
memory, quoted from a report, or inherited from another agent is a claim, not a
measurement. Run the command and use what it printed. A wrong number in a brief
is the most expensive kind, because the reader cannot tell it from a target.

### A check that cannot fail is not a check

Before trusting a probe, make it produce the wrong answer: mutate the thing it
guards, add a case that must be refused, run a version known to be broken. A
green result from an instrument never shown to go red says nothing.

- **A comparison that passes with the change absent is a guard, not evidence.**
  `flex-alignment.tsv`'s eighteen `baseline` rows stayed green through the
  baseline change; `fixtures/baseline-alignment` is the scene that moved.
- **Could this evidence have failed?** Ask it of every measurement before
  reporting it, not afterwards.
- **A control the change deletes is worse than no control.**
- **An assertion on a magnitude cannot tell a defect from its fix** when both
  sit on the same side of the threshold. Pin the **sign** where the sign is what
  the defect inverts.
- **A bound satisfied exactly by the worst case cannot see the worst case.**
- **Choose at least one case that amplifies**, and read its gain off the formula
  rather than hoping the case is representative.

### The check has to be about the change

**A check has to be as wide as the claim it supports**, and a green gate means
the thing it examines is right -- which is reassurance only when the thing it
examines is the thing that can be wrong.

- **Byte parity is not capability parity.** `just example` compares what the two
  surfaces write, so a divergence producing identical bytes is invisible to it.
  The Rust `to_file` buffered a whole spanning export in memory after the
  JavaScript one had stopped streaming, and the gate was green throughout. When
  a change gives one surface a _property_ rather than an _output_, its parity
  check has to be something other than fixture bytes.
- **A check that reads a proxy for the property can be confidently wrong about
  the property.**
- **A check written in the same currency as the thing it checks agrees with it
  by construction.**
- **Structural coverage is not positional coverage.**
- **A fixture with one value per type checks the shape of a read, not its
  arithmetic.**
- **The sampling has to be finer than the feature.** Four samples per pixel hid
  it once; a walk of a dashed border needs finer still.

### Prove a search pattern in both directions

A grep, a regex or a glob is an instrument, and its two failures point opposite
ways. Too loose, it agrees with whatever you already believed. Too tight, it
returns nothing -- which reads as "the tree is clean" rather than "the pattern is
broken", and that is the direction that ends a sweep early.

Run the pattern against something it **must** match and something it **must
not**. **An absence produced by a pattern you chose is a measurement of the
pattern**, not of the world -- a cache audit here grepped `No cache found.` and
`Restored from key` and got three empty rows, where the real phrase was
`Cache hit for:`; reported as "no restore recorded" the wrong conclusion would
have gained three supporting rows.

- **Some properties need a count, not a pattern.** Count the sites first, then
  assert the edit touched that many.
- **A scripted replace that matches nothing succeeds silently.**

### Instruments, and pointing them somewhere known first

- **Point an instrument at a known value before trusting it on an unknown one.**
- **When two of your own instruments disagree, run the doubted one over the
  reference** -- that is the reference calibrating the comparison rather than
  being compared.
- **A measurement taken through a boundary that truncates it reports the
  boundary.**
- **A `want` column in a repro is arithmetic until somebody measures it.**
- **Floor a sample point; round a reported value.** They are the same expression
  and they are not the same operation.
- **Two guards fail against different mistakes, so a row wants both.**

### Reducing, and what a failure is evidence of

- **Reduce from the top when reducing from the bottom keeps failing.**
- **A repro must be minimal in what it is _not_ testing.**
- **A test that fails is not evidence about which side is wrong.**
- **A failure's most available explanation is not evidence either** -- the
  explanation arrives with the failure and is not the finding.
- **Measure a defect against the fix branch before describing it in terms of
  that fix.**
- **When a comparison changes a container, check whether it changed the
  children.**

### Stale state reports something plausible rather than failing

This family has cost more time here than any other, and every member of it
reads as a defect in the code.

- **A stale artifact does not announce itself as stale** -- it reports a type
  error, or a plausible number.
- **`just coverage` leaves an instrumented addon behind**, deliberately; any
  benchmark taken in that window is silently wrong and the command producing the
  number never mentions the addon. Check
  `otool -l <addon> | grep -ciE 'sectname.*(llvm|prf|cov)'` is 0 before trusting
  a measurement.
- **`just addon-container` writes a Linux `.so` over the native addon path.**
  Back the binary up first and confirm with `file` that it is Mach-O again.
- **An artifact replaced underneath a running process** produces symptoms that
  look like unrelated bugs -- a killed doctest reads as a stack overflow. The
  _second_ symptom is the diagnostic one.
- **`cargo check --workspace` does not compile `#[cfg(test)]` code**, so a
  struct used only by tests can be wrong and stay green.
- **`cargo test` stops at the first failing target**, so a truncated failure
  list is not a failure count.
- **A trailing `echo` hides the exit status of the thing you care about**, and
  `PIPESTATUS` is a bashism this shell does not have. Nothing after `just ci` on
  the same command line.

### The machine is shared

Other projects build on this box by design, so a repository-scoped ownership
check clears a machine that is already saturated.

**All timeouts and no assertion failures is contention until proven otherwise.**
`uptime` first and it is decisive on its own: above about 8 on the fifteen-minute
average, do not time anything and do not trust a timeout-shaped red. The process
list answers a different question -- who to talk to -- and cannot tell you the
box is quiet.

The rule is one-sided, and the other side is the useful half: load makes tests
slower and never faster, so a **green** under load is stronger than a quiet
green, a **red** under load is uninformative, and a **timing** is worthless in
both directions.

**Targeted checks are the signal while another lane is live**; the full gate runs
when the tree is yours.

### Say what you checked and what you did not

A sweep that reports only findings cannot be told from one that never ran. Name
the negative results and the parts you judged rather than verified. **A green
with no denominator is not a result**: "eight gates green" and "four CI legs
green" fail identically, because neither says what did _not_ run.

Report shape: the invocation verbatim, its exit status and the summary line it
printed, and what did not run -- skipped, filtered, feature-gated, or
unreachable on this host.

---

## Architecture

`Scene` is the contract. A plain data tree in `meo-canvas-scene` with no
dependencies at all -- not Skia, not the layout engine, not the Node bindings,
and not a serialization framework. Its codec is written by hand because the byte
layout is a specification a JavaScript writer also implements, and a derived
format is one no other language can target from the documentation alone.

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

**The core cannot tell which door a scene came through**, which is what keeps the
two surfaces honest.

### Two representations, and they are not the same format

**The boundary format is an `f64` arena.** JavaScript writes opcodes and numeric
properties into a growable `Float64Array`, with strings and buffers in a side
`values` array the records index into. Rust reads `&[f64]`. Shaped that way
because a store into a `Float64Array` is one operation where writing varint
bytes from JavaScript is several, and because reading a value out of V8 is what
costs at that boundary. Its decoder lives in `meo-canvas-node`, since only the
addon can hold the side array.

**The persistence format is bytes.** Self-contained and self-describing, with a
magic number, a version, and errors that name the byte offset. Strings live
inside it because there is no side channel. It is what the CLI reads and what a
golden fixture is stored as, and it lives in `meo-canvas-scene`.

Both decode to the same `Scene`, so a scene captured from JavaScript and written
to disk round-trips without loss.

`NodeTag` lives in `meo-canvas-scene` as a `wire_enum!`, so the byte codec's kind
tag and the arena's opcode are the same number by construction rather than by two
tables agreeing.

**The property mask is carried in `f64` slots holding 53 bits each**, not 64: a
double represents integers exactly only up to 2^53, and a 64-bit mask written
into one slot loses every bit above that silently. Two slots name 106
properties; a node kind passing 106 takes a third.

### Pages

A `Scene` carries one or more page trees. A single page encodes to a still
image; several become frames in gif and apng, sheets in pdf and tiff, sizes in
ico.

Pages are trees, already built, by the time the core sees them. **Nothing in the
core calls back to produce one.** A caller wanting a page per frame samples its
own values at each time and hands over the trees that result -- which is why
animation needs no clock, no retained state, and no re-entry into caller code
mid-render.

Fonts and decoded images resolve once and are shared across every page. The
layout tree is built and dropped per page.

### A page as tall as its content

`Scene::content_height` asks for it, and `size.height` becomes the **floor**
rather than the height -- so no field is ever meaningless, and "at least this
tall" is expressible rather than a second flag.

**Solve, then allocate.** The surface used to be created from `page_size` before
any layout ran, which is why a derived height was impossible rather than merely
absent: there was nowhere for the answer to go.

**The circularity argument covers the width and stops there.** Solving needs a
width before anything can be measured, because that is what text breaks its lines
against. A height is a consequence of that measuring, so `MaxContent` on the
height axis is not circular. Both surfaces therefore require a width and not a
height, and the Rust one says so by chaining (`Root::new(w).height(h)`).

`content_height.rs` reads the **encoded PNG's own header**, because layout could
resolve any height at all and still be painted onto a sheet of the stated size --
which is exactly what used to happen.

### Where state lives

`Renderer` owns everything a render needs that is not the scene: registered fonts
and whatever caches the passes keep. Those outlive any one scene, and the
alternative is process-wide statics, which is what v9 has five of.

**Nothing in this crate is global.** Two `Renderer`s on two threads share no
state and cannot contend. The one exception is outside our control:
`meo-skia-canvas` keeps a process-wide font registry. What this crate controls is
adding no second global, and it adds none.

`Renderer::render` returns a `RenderedCanvas`; encoding is a separate call on it,
so two formats of one picture cost one resolve, one measure, one layout, one
paint and two encodes. Rendering and encoding in one call would make the
JavaScript surface, which retains its canvas, strictly faster than the Rust one
for identical work.

`RenderedCanvas::to_buffer` takes `&mut self`, because `Canvas::to_buffer`
prepares the surface before reading it. Interior mutability would let two encodes
of one canvas read as independent when they are not.

`EncodeOptions::validate` runs inside `to_buffer`, because it needs the page count
and that lives on the painted surface -- a page index past the end is a property
of the drawing rather than of the scene, so `render` structurally cannot catch it.

### Pipeline

One pass, no re-entry into caller code.

**resolve** registers fonts, decodes images, and inherits text styles down the
tree. The only stage performing I/O, and **whether that I/O leaves the machine is
a build-time decision**: without the `net` feature -- the default -- an
`ImageSource::Url` is refused with `Error::UnresolvedSource` and no HTTP stack is
linked. The facade forwards the flag, so a consumer of `meo-canvas` enables it
there. The rule that does not bend: no async runtime.

**measure** shapes each text node and breaks it into lines, in `crate::lines`.
Skia's `Paragraph` is not on this path -- `measure.rs`'s `build_paragraph` is
`#[cfg(test)]`. Breaking lines here is what makes the text behave like a
browser's, since a canvas has no paragraph.

**layout** solves with taffy; text leaves answer its measure closure by laying
out at the offered width. The two intrinsic questions are the same call with a
different budget: `MinContent` lays out at zero and `MaxContent` at infinity, so
neither needs an API of its own. A measured leaf reports its baseline offset by
its own top padding and border, because CSS measures a flex item's baseline from
its border box; taffy reads a _missing_ baseline as the node's own height. **In a
column direction taffy does not attempt baseline alignment at all**, and neither
does its grid.

**paint** walks the solved tree in z-order through `meo-skia-canvas`'s
`Context2D`. No drawing call crosses a language boundary.

**encode** produces png, jpg, webp, avif, tiff, bmp, ico, svg, pdf, gif, apng or
raw bytes.

`resolve` is the only stage that waits, and it waits synchronously. Everything
after it is CPU-bound, which is why parallelism lives at the scene level -- many
scenes across a thread pool -- rather than inside a single render.

### Node addon

`meo-canvas-node` owns the only `#[neon::main]` in the binary. A Node addon has
exactly one module-init symbol, so `meo-skia-canvas` is depended on with
`default-features = false`.

The addon re-exports `meo-skia-canvas`'s operations alongside its own, so one
binary serves both the declarative surface and the imperative canvas API beneath
it. **Two addons would mean two copies of Skia resident in one process**, which
is the reason, not the size.

---

## The two surfaces

**They read the same way.** `Root` is the entry point on both, style properties
sit directly on the node rather than inside a nested object, and the output
methods are the same set. A person moving between them should be translating
syntax, not a design.

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

Each is idiomatic in its own language rather than one imitating the other:
builders and `px(16.0)` in Rust, object literals and `16` in JavaScript. Same CSS
names, same values.

### The Rust surface

`meo-canvas` is the authoring layer, because the scene contract is an arena -- a
flat `Vec<Node>` indexed by `NodeId`, which a codec can round-trip and a person
cannot comfortably write.

**Children take one or many, and a falsy child is skipped**, matching v9, whose
props type is `Children | Children[]` where `Children` includes `false`. Rust
reaches the same place through a trait: `.children` accepts a single element, an
array, a `Vec`, and a `None` `Option<Element>` is skipped. An iterator goes
through `each(..)`, because a blanket `impl IntoElements for I: IntoIterator`
overlaps `Vec`, `[T; N]` and `Option`.

**Flat setters are the documented path, and `with_style` merges.** A `Some` in
the argument wins; a `None` leaves what the node already had. It replaced until
4 September 2026, which discarded whatever the constructor had set --
`Column::new().with_style(..)` became a row. Three things answered the argument
for replace: order was already significant and more sharply, since replace threw
the earlier call away; every flat setter is already a one-field merge; and **the
JavaScript surface has always merged**, since `Row` is `{ flexDirection,
...props }`. Replace made the two surfaces disagree about the same call, which is
a defect here rather than a difference.

**`Style::merge` destructures its argument without a rest pattern.** A
sixty-ninth property the merge forgot would be a property that silently does not
carry, visible only as a picture that came out wrong; the destructure makes it a
build error naming the field. Ten of the sixty-eight properties have hand-written
setters because their setter converts what the caller passes, so a merge written
as a macro arm over the property table would have covered fifty-eight and let the
other ten drift.

Setters are written once, on a trait every node implements through a single
accessor. `Style` stays public and deliberately not `#[non_exhaustive]`, so a
property with no setter is still reachable by literal. Setters are `const fn`
wherever the field allows -- the line they cannot cross is a field that needs
dropping, which is E0493 and is why `gradient` and `mask` are not `const`.

`px` takes an `f32`, so `px(16.0)` and not `px(16)`: Rust does not coerce an
integer literal, `impl Into<f32>` cannot be `const`, and an `i32` parameter would
lose `px(0.5)`.

### The JavaScript surface

`RootProps` is `Style & {...}` and `TextProps` is `Style & ParagraphOptions &
{...}`, so a property a node accepts is a property a caller writes flat. There is
no `style` key on either surface. The string-literal unions in
`packages/meo-canvas/src/index.ts` are what make `'cover'` complete and `'covr'` a
compile error.

**One crossing, and when:**

| call                    | what crosses                                                |
| ----------------------- | ----------------------------------------------------------- |
| `Row(...)`, `Text(...)` | nothing -- plain objects, no native call                    |
| `await Root({...})`     | the entire arena, one `Float64Array` and one `values` array |
| `toBuffer(fmt)`         | a format tag and options                                    |

A scene of any size is **one** crossing. No per-node call, no per-property call.

**The tree is built before it is encoded** because JavaScript evaluates arguments
inside out: `Row({children: [Text('a')]})` runs `Text` first, so writing opcodes
as each factory runs would land them post-order where the arena is pre-order.

**An explicit `undefined` is not an absent key.** `exactOptionalPropertyTypes` is
on, so optional fields are spread conditionally --
`...(x === undefined ? {} : { x })` -- rather than assigned.

**Throwing and rejecting are different failures.** An argument of the wrong shape
throws synchronously; a failure inside the render rejects. Every V8 read happens
in one pass before the work is handed to the pool, so an argument error is raised
while there is still a call to throw from and a render error is raised when there
is not. A test asserting either one always happens is wrong, and it would fail
for the right reason and be repaired the wrong way.

**The canvas exposes v9's output surface**, because a ported script should not
have to change how it writes a file: `toBuffer`, `toBufferSync`, `toFile`,
`toURL`, `toDataURL` and their sync pairs. The sync variants are ordinary
functions here -- v9 needed `Atomics.wait` on a `SharedArrayBuffer` because its
canvas lived in a worker. **The pairs differ in the thing their names claim**,
which they did not always: `toBuffer` was once `return this.toBufferSync(...)`, a
promise handed over already settled after the loop had been blocked for the whole
encode.

`saveAs` is not carried over -- a deprecated alias reintroduced in a rewrite is
one nobody gets to remove later. `toSharp` is absent until someone asks, because
reintroducing it means taking a position on another library's version.

### The retained canvas

`Root` returns a handle to a painted surface. `toBuffer` encodes that surface
again at a different format; **it does not re-render.**

**A retained surface and an off-loop _paint_ are mutually exclusive**, and the
compiler settles it: `RenderedCanvas` holds a `SkPictureRecorder` and an
`Rc<RefCell<Gradient>>`, so it is `!Send`. **The encode is a separate question,
and the answer changed** in meo-skia-canvas 0.13.0: `Canvas::prepare_export`
hands back a `Pages` that _is_ `Send`, so the half needing the canvas runs on the
owning thread and the half that does not runs on a worker.

**Almost all of the cost is on the far side of that line.** At 4000x4000 the
record costs 2.74 ms and the encode 97.23 ms. And **nothing on the worker needs a
font** -- the pages carry drawings with their text already shaped, which matters
because a family is registered per _thread_.

The pool is rayon's, not `cx.task`'s: libuv's is shared with `fs`, `dns` and
`crypto` and has four threads by default, so a 130 ms encode there moves the
stall into `fs.readFile` rather than freeing the loop.

The surface is held by closures over an `Rc<RefCell<Option<RenderedCanvas>>>`
rather than a `JsBox`, because a `JsBox` is reachable only through `this` and
`const { encode } = canvas` would break silently.

### Rasteriser parity

The GPU backend is a Cargo feature named for the platform that has one: `metal`
on Apple targets, `vulkan` elsewhere. Neither is default, because a build with no
backend renders on the CPU, which is what a portable `cargo check` needs.

**Every crate a caller can depend on forwards that feature.** A feature declared
only on the addon leaves a Rust caller on the CPU with no way to ask otherwise.

`gpu` is a request rather than an outcome, and `Canvas::gpu` reports the request
-- so a test asserting `renderer.gpu()` passes on a CPU-only build.
`Surface::engine` reports what the surface actually got, and is what to read when
images disagree. The check that is not fooled is the byte comparison in
`just example`: CPU and GPU rasterisation of the same scene differ by one or two
levels across antialiased edges.

### Where the overhead is

The crossing is one call, so the only JavaScript cost that scales with the scene
is building the tree and encoding it.

- **Node objects are monomorphic** -- same keys in the same order on every node,
  absent fields present as `undefined`. A node that sometimes carries `src` gets
  a second hidden class and deoptimises every property read in the encoder.
- **Styles are read, never copied.** The defaults already exist in Rust.
- **Encoding is one pass** into a preallocated `Float64Array` grown by doubling.
  A `lineTo` in `meo-skia-canvas` costs 82 ns, of which 17 is the crossing and 39
  is reading two floats out of the arguments; decoding from a `&[f64]` skips V8
  entirely.

---

## Workspace

```
crates/meo-canvas-scene    Scene types and the binary codec. No Skia, no taffy, no neon.
crates/meo-canvas-core     resolve, measure, layout, paint, encode.
crates/meo-canvas          The crates.io surface. Nodes, one flat `Style`, units.
crates/meo-canvas-node     The cdylib. The only #[neon::main].
crates/meo-canvas-cli      The binary.
packages/meo-canvas        The npm surface. TypeScript, and the arena encoder.
```

`meo-canvas-scene` is separate from the core because the CLI, the addon and the
fixture tooling all need to read a scene, and none of them should link Skia to do
it.

**There is no `mod.rs` in this repository at any depth.** `src/lib.rs` declares
`mod foo;` for `src/foo.rs`, whose children are `src/foo/bar.rs`. No lint
enforces this -- not rustc, not clippy, not rustfmt -- so `just layout-check`
does, as part of `just ci`.

Every file carries a `//!` module doc, because `missing_docs` is denied. Use
`//!` inside the file, never `///` above the `mod` declaration: both compile, and
a tree that mixes them reads as two conventions.

---

## Conventions

### Comments

1. **A comment states what the code is, today.** Never what it was, what it will
   be, or what changed. Git records history; comments record the present. No
   "used to", "changed from", "now", "no longer" -- and no TODO, FIXME or XXX.
2. **A comment earns its place by answering "why this and not the obvious
   alternative".** If the code already says what it does, the comment says why it
   does it that way. Restating the code is worse than silence.
3. **When performance is the reason, cite the measurement.** A number, not an
   adjective. "82 nanoseconds, of which 17 is the crossing" is a reason; "for
   speed" is not.
4. **When a test pins a behaviour, name the test.** A reader who wants to change
   the behaviour should be told where it will fail.
5. **`//!` for module rationale, `///` for item rationale, `//` for a decision
   only a maintainer needs.** If a reader outside the file would act on it, it is
   a doc comment.
6. **Present tense, indicative.** "Rejects a radius below zero, as a browser
   does." Not "will reject".

**A reason written in two places gets corrected in one.** Write it once, at the
place a reader lands, and point at it from the other.

**Re-read the comments around everything you changed**, including the ones the
diff did not show you -- the doc block above the function, the header of the
file, and any comment elsewhere describing the behaviour you moved. A comment
that is now wrong is a defect in this commit, not a task for later.

### Working around an upstream defect

Two halves, and **the probe half already existed before the tag did**.
`crates/meo-canvas-core/tests/taffy_negative_margin.rs` has pinned inherited
taffy defects since long before any of them were compensated for, and its own
doc says why the assertions are of the wrong numbers: a test asserting the
browser's values would fail today and could not be committed, so it pins what
the dependency actually does with the right answer beside it, and **the day the
dependency is fixed it fails, and that failure is the notification.**

What was missing is a way to find the compensation from the other end. A comment
explaining that some code works around a dependency is findable only by someone
who already suspects it is there.

**So a workaround carries `[WORKAROUND]` and a probe, and neither substitutes
for the other.** The tag makes it greppable; the probe makes it impossible to
leave behind.

```rust
// [WORKAROUND] <what the dependency does wrong, and what this does instead>.
// `owner/repo#N`, tracked as `l7aromeo/meo-canvas#M`. Retires when <the
// observable that will become true>, which <the test that will say so>
// asserts.
```

Four things and each earns its place. The **tag** is a fixed string so a grep
cannot go loose or tight by accident -- prove the pattern against a site it must
match and something it must not, as with any other search here. The **short
description** says what the compensation does, so a reader need not reconstruct
it. The **qualified references** name the upstream defect and our tracking issue,
which `issue-refs` enforces. And the **retirement observable** is a fact that
will become true rather than an event someone has to notice.

**It is not a TODO, and the distinction is not a technicality.** TODO, FIXME and
XXX are refused because they describe work that ought to happen later.
`[WORKAROUND]` describes what the code **is** -- compensation is its present
nature, which is what rule 1 asks a comment to state. The tag is a category
rather than a deferral.

**Every site the compensation reaches carries the tag**, so a grep returns the
whole of it rather than its entry point, and the symbol names carry the upstream
reference too: a tag is findable and a name survives a refactor, and they fail
against different mistakes.

**The probe asserts two things and both are required.** First, in
`taffy_negative_margin.rs`'s form, that the dependency still gets it wrong: pin
the wrong numbers, name the right ones beside them, and say in the failure
message that the compensation can be deleted. Second -- and this is the half
that is easy to leave out -- **the property the compensation depends on**.

A workaround rests on the dependency being right about something, and that
something is untested by every other check in the tree. The ratio compensation
reads a fit-content width taffy reports correctly only when no ratio is on the
node; if that stopped holding, all twenty-six conformance rows would stay green
and the first sign would be a golden moving, with nothing saying why. So the
probe pins it, beside the defect, and a file asserting only that the defect
exists is half a probe.

**A check enumerating every tagged site and asserting each still has a live
probe belongs in `portable`**, beside `issue-refs` and `gate-lists-check`. It
does not exist yet; the convention here is written for it.

### A default that means something different from the same value stated explicitly cannot be a default

It has to be absent. CSS spells `z-index: auto` and `z-index: 0` differently --
the first establishes no stacking context and the second does -- so `z_index` is
`Option<i32>` where `None` is auto. The same reasoning puts `gpu` at
`Option<bool>` and leaves an unnamed inset edge `None` rather than zero: not
pinned is not the same as pinned to zero.

### Constants

Every value that is a judgement gets a named `const` whose doc comment justifies
**the magnitude**, not merely the strategy. "Bounded so a long-running process
cannot grow without limit" explains the bound; it does not explain 4096.

No clippy lint checks this -- `clippy::magic_numbers` does not exist, and
`unreadable_literal` only demands `100_000` over `100000`. Enforced at review.

### Stacking

`z_index` follows CSS, which is neither v9's rule nor "every sibling". CSS 2.1
applies it to **positioned** elements; Flexbox 5.4 and Grid 6.2 extend it to flex
and grid items regardless of position. So a child is stacked by `z_index` when it
is positioned, or when its parent lays out as flex or grid. In a block container
a static child ignores it.

v9 documents `z-index` as applying only to absolutely positioned nodes, which is
narrower than CSS -- and where v9 diverges from the reference, the reference wins.

**The rule is measured in Chrome, not assembled from three specifications.** 281
cases sampled with `elementFromPoint` at the true intersection of two overlapping
boxes, each case measured with every other hidden -- a visible `position: fixed`
box leaves its parent for the viewport and answers for the wrong pair, which four
cases did. Four results carry the design:

- **Display does not change paint order.** All 25 position pairs agree under all
  five displays, so the painter reads position and `z-index`, never the parent's
  `display`.
- **A positioned child paints above a static sibling regardless of document
  order** -- the only pairs with no `z-index` anywhere in which the _earlier_
  sibling wins.
- **For positioned children the `z-index` matrix is identical under block, flex
  and grid**, and `auto` ties with `0`.
- **For static children, block ignores `z-index` and flex and grid honour it** --
  and the matrix differs from the positioned one in exactly one cell: a static
  item at `z-index: 0` beats a later sibling at `auto`, where two positioned
  children tie. It is the cell an implementation that reads "flex items honour
  z-index" as "flex items are positioned" gets wrong.

`PositionType` carries three variants where taffy carries two. `Static` is
appended as discriminant `2` rather than given the `0` it would take today,
because the discriminants are published in both wire formats, and it is the
`Default`. Two things follow, both settled here because taffy cannot express
them: `Static` is the only variant that does not stack, and `inset` does not
apply to it, so `to_taffy_inset` drops it -- measured, a static child given
`top: 30px` sits at its flow position in block, flex and grid alike.

`Fixed` and `Sticky` stack as positioned variants; they differ from `Relative` in
where they resolve, not in when they paint.

### Stacking contexts

A child at `z-index: -1` sinks behind its parent's background unless the parent
establishes a context, so hit testing at the child's centre names the parent when
the trigger made no context and the child when it did. 27 triggers run through it.

**Creates one:** `position` plus a numeric `z-index`; `position: fixed` or
`sticky` with no `z-index` at all; `opacity` below 1 (`0.99` is enough);
`transform` other than `none`; `filter` and `backdrop-filter` (`blur(0px)` is
enough); `clip-path` and `mask-image`; `isolation: isolate`; `mix-blend-mode`
other than `normal`; `will-change: transform` and `will-change: opacity`;
`contain: paint` and `contain: layout`; `perspective`; a flex or grid item with a
numeric `z-index`, position irrelevant.

**Does not:** `overflow: hidden`; `position: relative` at `z-index: auto`;
`opacity: 1`; `transform: none`; `display: flex` or `grid` on the container; a
flex item at `z-index: auto`.

**`overflow: hidden` is the one to hold on to.** It is the trigger most often
assumed, it clips its children, and it leaves them in the parent's context -- a
negative child still paints behind the parent's background. `will-change:
opacity` creating a context while `opacity: 1` does not is its mirror: the
declaration is a promise about the future value, and Chrome honours the promise.

### Layout defaults

`LayoutStyle::default()` is CSS's: **`Display::Block`**, row direction,
`flex_shrink: 1.0`. Yoga's raw defaults are a column direction and
`flex-shrink: 0`, so a bare box would change meaning between the two, and
following CSS is what makes a bare `Node` behave like a bare `<div>`.

**Every factory on both surfaces overrides it, and that is the fact with
consequences.** `Box::new`, `Row::new` and `Column::new` in `element.rs` each
name `Display::Flex`, and so does npm's `Box` in `node.ts` -- because a
container that inherited `block` would silently stop honouring `gap`,
`align_items` and `justify_content`. So the default is not what a caller
ordinarily meets. **A scene built through the factories is a flex container; a
scene assembled from `Node` values is a block one.**

**Which means a hand-built repro and a ported JavaScript probe are not the same
probe**, and nothing about them says so. Two numbers from one investigation:
900 from cross-axis stretch through `Box()`, 200 from block-level fill in a
hand-assembled test, treated as one symptom for an hour. The scene that came
through a factory was laying out under a different display than the one that did
not, and both were "the same scene" in the report.

The test helpers in `layout.rs` name their display for exactly this reason.
Anything reproducing a surface's behaviour from raw `Node` values has to name it
too, or it is measuring a different box.

### Errors

The core returns `Result<_, MeoError>` with a variant per failure class. The
addon maps those to JavaScript exceptions, the CLI to exit codes, and Rust
callers match on them.

**`unwrap` is denied. `expect` warns**, and is allowed where its message explains
the invariant that makes it unreachable.

**Where a validation repair goes is decided by whether the other surface can
express the bad input.** The writer refuses what the type forbids; the
consumption side clamps what arrives as bytes.

### Diagnostics are a channel, not an error

A value the renderer could not use does not fail the render. It is reported
alongside it, on both surfaces, because a markup tag with an unusable colour
should still draw the text.

`Root::into_scene` returns `Result<(Scene, Vec<Diagnostic>), BuildError>` -- the
pair is the shape: a `BuildError` means no scene, and a diagnostic means a scene
with something in it the caller wrote and did not get. `Element::into_scene`
carries the same pair. On the JavaScript side it is `canvas.diagnostics`, a
`readonly Diagnostic[]`.

**A `Diagnostic` names a path rather than a property**, because a value nested
inside another property has no single name that would find it: `<color=zzz>` and
`segments[2].color` are paths where `color` is a field. **Every diagnostic raised
today is a markup tag**, so the path is the tag as written, value included --
the whole of `<weight=1500>` rather than `<weight>`. `Diagnostic::offset` is the
byte offset into the markup, which is what tells three `<color>` tags apart, and
it is `None` for a diagnostic about a scene value -- the spelling the shape has
to hold for, and which nothing produces yet.

### What a public enum promises

**Closed because CSS closed it, open because we will add to it.**
`#[non_exhaustive]` is free to add before a first release and impossible after
one, so the marking happens now and the only question is which enums get it.

**Marked, because they will grow:** the five error types; `ImageFormat`;
`NodeKind`, `Mask`, `TrackSize` and `Spacing`, which are this project's
vocabulary; `FontVariant`, naming 35 OpenType features out of a specification
with more.

**Not marked, because the specification closed the set:** `Display`,
`FlexDirection`, `Justify`, `Align`, `Overflow`, `BoxSizing`, `Direction`. The
attribute is hostile there for no gain -- a caller matching on `FlexDirection`
wants the compiler to tell them when they missed one. `GradientGeometry`,
`BackgroundSize` and `LineHeight` look like they belong on the first list and do
not, because the check is the specification rather than the shape of the type.

**What marking costs, stated rather than discovered.** It leaves exhaustiveness
intact inside the defining crate and removes it everywhere else, so a variant
added tomorrow takes a wildcard arm silently instead of failing the build. **So
the guarantee moves rather than goes:** every marked enum has an exhaustive match
with no wildcard **in the crate that defines it** -- a witness test naming every
variant and doing nothing with them. The witnesses are `#[cfg(test)]`, so
`cargo build` will not catch an addition and `cargo test` will.

Each wildcard arm elsewhere says what it does and why. None panic: a scene from a
newer writer renders what this build understands rather than refusing the page.

**The attribute is on ten public structs as well as twelve enums** --
`Diagnostic`, `ImageWarning`, `LayoutResult` and the seven builder types. This
section is titled for enums, and they follow the same test.

**Mark what a caller reads; give a constructor to what a caller writes.** The
attribute on the enum stops a caller matching a new _variant_ and does nothing
about a new _field_, so variant-level `#[non_exhaustive]` closes the struct-like
ones a caller only inspects: `Error::SourceFetch`, `FontRegister`, `ImageRead`
and `Encode`; `SceneError::CanvasSize`; the six on `CodecError`. **That is what
let `SourceFetch` gain a `failure` field as an addition rather than a break** --
the marking paying for itself.

A closed variant cannot be built with a struct expression outside its crate, so
the ones we build elsewhere get a constructor instead, and each says in its own
doc why it is public: `SceneError::canvas_size` because `meo-canvas` reports it
from **both** `into_scene` entry points, `Error::image_read` because
`meo-canvas-cli` builds one to check which exit code it maps to.

`NodeKind::Text`, `Image`, `Path` and `Mask::Path` stay open, because every
caller of the scene constructs them. **Ask whether the outside builds it or only
inspects it.** An error is inspected; a node is built.

### What is this a statement about

Two defects in one day had the same shape, and neither was a wrong statement.

`Reader::list` refuses a count larger than the bytes remaining, because every
value costs at least one byte -- correct, and a statement about **the count**.
The next line was `Vec::with_capacity(count)`, which reads it as a statement
about **the memory**, and a `Node` is 1048 bytes in memory against 184 on the
wire, so one megabyte of input reserved 1.02 GB.

`Fonts::registered` reports what **this registry** registered -- correct, and a
statement about **the instance**. A caller holding a `Fonts` reads it as a
statement about **what can be drawn**, which is `Fonts::has`, which answers about
the process.

**Neither survives review by hiding. They survive because there is nothing wrong
to catch** -- only something narrow standing where something wider is needed,
with a correct comment above it. So the question is not whether a check is right.
It is: **what is this actually a statement about, and what will the next line take
it to mean?** Where the two differ, say so at the narrow one.

**A third shape: true in the frame it was measured in, false in the frame it
ships in.** A URL fetch's size limit was classified from
`ureq::Error::BodyExceedsLimit` -- exactly what `ureq` reports _when no timeout is
configured_. The same change set `timeout_global`, and with a timeout the
identical over-size read reports a bare `Io(Os { code: 22 })`. Correct in every
test written without a timeout, wrong in the crate that sets one. The fix was to
stop asking the dependency: `fetch` counts the bytes itself. **Ask which frame the
evidence came from** -- a probe isolating one feature has, by construction,
removed the other, and the crate ships both.

---

## Build, test and development

`just` drives everything. **`just` alone lists every recipe with its one-line
doc, and the `justfile` is the authority on what each one does** -- read the
recipe rather than a description of it, including this one. A bare verb rewrites
the tree; the `-check` suffix reports instead. `just ci` uses only the reporting
variants, and is split into `just portable` (reads the tree, no cargo) and
`just native` (compiles, links or runs the addon), because CI runs the portable
half once and the native half per host.

**A prose-only change is not gated by `portable` alone, and the split is why.**
`fmt-check` is in `native` -- it runs rustfmt on the pinned nightly before
prettier, so it needs cargo and cannot sit in the other half -- and prettier
covers Markdown, YAML and JSON. So `portable` sees a Markdown change's
_content_, through `issue-refs` and `docs-js`, and never its _formatting_.
Editing a `.md` file and gating on `portable` because "the native half only
compiles Rust" misses the one check that would have failed. Run `just fmt-check`
beside it, and say that you did.

```
setup                 First-time setup on a fresh clone. Idempotent.
ci                    What CI runs, in order, refusing to start beside another gate.
portable / native     The two halves of it. `gate-lists-check` asserts both are invoked.

build / addon         The workspace, and the debug addon into packages/meo-canvas.
test / test-js        Rust (twice, once per GPU feature) and vitest.
coverage / -js        Two floors: 90% workspace, 60% on the addon boundary.
lint / fmt            clippy then ESLint; rustfmt on the pinned nightly then prettier.
typecheck             tsc --noEmit over the package and its tests.
docs / docs-js        rustdoc with warnings denied; TypeDoc with dead links denied.
layout-check          No mod.rs anywhere.
unused / runtime-free Declared-but-unused crates; any async runtime in the tree.

fixtures / -accept    Render every golden and compare; accept one by name.
conformance           Re-measure Chrome with Playwright; rewrite the tables the tests read.
example               Render the nine scenes on both surfaces and compare every byte.
bench / -rust / -js   criterion; throughput, rss, heap, peak, idle.

arena-tables, arena-enums, arena-cases, media-types, doc-examples, platform-packages,
release-tags          Generated or coupled artefacts; each has a -check that fails on drift.

addon-release / -container    The optimised addon a release ships.
pack / verify-pack            Tarballs into release/, installed elsewhere and rendered through.
abi-floor / acceptance        What the Linux addon demands; loading it on six bare images.
release-npm / release-crate   Dispatch the workflows and watch them.
```

**The package manager is bun.** `bun.lock` is the lockfile and `packageManager`
names the version; everything installs with `bun install --frozen-lockfile`. Two
things stay npm on purpose: `npm pack` and `npm publish`, because the release
workflow derives its tarball globs from npm's naming and provenance is npm's; and
the consumer-side install in `verify-package.mjs`, because consumers use npm.

The root TypeScript is **6.0.3, not 7**: typescript-eslint supports `<6.1.0` and
TypeDoc 0.28 supports up to 6.0.x, and 7 is the Go compiler with a different API
that neither loads.

**`fmt` is rustfmt on the nightly named by `fmt_toolchain`**, because
`rustfmt.toml` uses options stable ignores -- stable `cargo fmt` reports clean
against weaker rules than CI applies -- then `prettier --write .` over the whole
tree. What prettier must not touch is named in `.prettierignore` with a reason
each time, including the Chrome measurement tables, which are machine-written and
which prettier rewrote by 3,460 lines the first time it saw them.

`lint` is clippy with `-D warnings` across the workspace and the examples, then
ESLint. Two rules are deliberate: `require-await` is off because `toBuffer` is
`async` with no `await` on purpose -- the contract is a rejection, not a throw --
and `no-unused-vars` has no `^_` escape for variables, because `const _ = x` to
silence it is the thing the rule exists to stop.

**Local iteration against meo-skia-canvas** goes through an untracked
`.cargo/config.toml`; a path dependency in `Cargo.toml` would break CI, which
clones this repository alone.

```toml
[patch.crates-io]
meo-skia-canvas = { path = "../meo-skia-canvas" }
```

`CLAUDE.md` is a symlink to this file, so one text is reachable under both
names. `just setup` creates it (`test -L CLAUDE.md || ln -s AGENTS.md
CLAUDE.md`) and nothing else does -- **a fresh `git worktree add` does not**, so
a worktree has no `CLAUDE.md` until someone runs setup in it, and all four here
were missing it at once. It is ignored by `.gitignore` line 2's `*` rather than
by an entry naming it, so nothing would ever report its absence.

### Windows runs the gate, and it found four faults in three runs

Every fault it found was in the tooling, none in the renderer, and none was
visible any other way: `run:` defaults to PowerShell (now `shell: bash`);
`.gitignore`'s `* text=auto` checked out CRLF and prettier failed 69 files (now
`eol=lf`); two tools split or prefix-matched paths on `/`; and three tests turned
`new URL(x, import.meta.url).pathname` into `D:\D:\a\...` (now `fileURLToPath`).

**Before writing a `.mjs` tool or a test that touches the filesystem, grep for**
`.pathname`, `split('/')`, `startsWith(dir + '/')` and `execFileSync('npm'` --
npm on Windows is `npm.cmd` and Node refuses to spawn it without `shell: true`.

### What a gate can and cannot see

**A gate examines a proxy, and the proxy is narrower than the rule it is cited
for.** `docs-js` resolves `{@link}` targets and refuses a type reaching a
signature unexported -- but a string literal written in prose is neither a link
nor a type. `RootProps.colorType` documented `'F32'`, which is not a member of
`ColorType`, and the reference built clean until the sentence was rewritten for
an unrelated reason.

**A check that matches on text can match the checker.** `pgrep -f 'llvm-cov'`
finds the polling loop, because the pattern is in that loop's own command line,
so `until ! pgrep -qf 'llvm-cov'` never terminates. Ten accumulated in one
session, each keeping the others alive, and one was inside the check written to
find the first nine. **Match on identity instead** -- a pid, `ps -o comm`, an
exit status you arranged to be the one you read.

**An exit status, or a match, describes the last thing that ran, which is only
the thing you care about if you arranged for it to be.** `${PIPESTATUS[0]}` is
bash's and expands to nothing in zsh; a watch chain ending in `gh run view` exits
0 for successfully printing a failing run; `git merge-base --is-ancestor` exits 1
both for "not an ancestor" and for a ref that does not exist.

**A correct shape buys its contents an assumption of care they have not
earned**, and that is why a careful-looking thing is worse than a careless one:
it stops the next reader. The soft-fail path downgrades only a named list of
failures with no catch-all arm -- the right shape, deliberately chosen, and
checking the shape is what made the contents look checked. They were not: a
decoder that _panicked_ reported the same variant a fetched-and-refused image
uses, so a crash was on the softenable list. The same failure in prose: a comment
correctly said absence has two causes, and the code told them apart with
`matches!(source, ImageSource::Url(_))`, which is the source's _type_ and not
what happened to it.

**A shape, a comment and a name are claims about the contents, not evidence about
them.** When one of them is what makes something look considered, that is the
moment to read the contents rather than the moment to stop.

### Performance and memory

**A baseline for reading, not a gate.** `just bench` is an instrument; a
benchmark that fails CI on a shared runner teaches people to rerun until it
passes. The numbers exist so the next person can tell a regression from noise,
which requires knowing what was measured, on what, and when.

Taken at `c2035a8`, 5 September 2026, on an **Apple M4 Pro, 14 cores, macOS
26.6.2**, `rustc 1.98.0`, Node v26.4.0.

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

**A single number has no control; a table has one for free.** A previous table
gave `draw` as 9.86 ms against 2.86 ms here -- a 3.4x gap that looks like
measurement error until you read the row beside it: `re-encode` was 9.00 ms then
and 9.16 ms now. A faster machine moves both. One row transformed and its
neighbour unchanged is the machine holding still while the draw path changed.

**About 10% is this repository's noise floor**, not a result. A Skia bump moved
`resolve` by 12% while `draw` and `re-encode` held at -0.3% and +0.1%, and that
is backwards for the mechanism claimed -- the instrument, not the code.

**The JavaScript and Rust figures answer different questions.** 13.71 ms per
render is a whole render through the addon including PNG encoding; the Rust
`draw` row is the paint pass alone. And "paint" means two different spans in the
two places it is written -- the table's `draw` is the drawing stage, the
package README's ~9 ms is the whole native call. They agree on the total and
differ on where the line falls.

**Two allocations look wasteful and are not**, measured rather than argued:
`resolve` clones a `ResolvedText` per node, some fraction of 52.90 us against a
13.92 ms pipeline, and `z_ordered` clones and sorts every container's children at
2.12 us across 551 nodes. Do not change either without a number that says
otherwise. Allocation in the **paint** stage is on the critical path for every
frame of an animated render; prefer reusing a buffer, and say what the reuse is
worth when it is not obvious.

### The three test layers

**Unit tests** cover `meo-canvas-scene`, the codec and each core stage. Pure
logic, and most of the coverage.

**Golden fixtures** in `fixtures/` are scenes rendered by a `Renderer` in
`crates/meo-canvas-core/tests/fixtures.rs` and compared against committed images
byte for byte. This is how the paint stage is covered: **executing a fill proves
the line ran, not that the pixels are right**, so paint is verified by comparison
rather than assertion.

**Doctests** run every example in the crate documentation, compiled against the
real public API, so they cannot rot.

**The goldens are per architecture, and no tolerance was added.** `fixtures.rs`
compares against `expected.<os>-<arch>.png` where one exists and `expected.png`
otherwise, byte for byte -- **a refusal, not a description**: no comparison here
has a threshold and none is to acquire one. On `linux-x86_64` 15 of the 23
fixtures are byte-identical to the macOS reference and 8 are not, and the 8 are
the ones with a curve, a gradient, a blend or a glyph.

**A variant exists only where a platform is measurably different; its absence is
a claim the run then checks.** That is the rule for when to add an
`expected.<variant>.png`, and without it the next person facing a Windows diff
adds a variant instead of investigating.

**`fixtures.yml` uploads an artifact rather than committing**, because a workflow
that pushed accepted goldens would turn a rendering regression into a commit
nobody saw.

**A fixture is portable because the harness makes it so.** It registers exactly
one font from this repository under the family `Fixture` and refuses a scene
naming any other, pins the scale, and turns `gpu` off. The platform's installed
faces answer `has_family` too, so a fixture asking for Helvetica would pass here
and differ on any other machine. The reference platform is `("macos",
"aarch64")`; other platforms compare against `expected.<variant>.png`.

### The reference publishes after the version resolves from npm

`docs.yml` builds the TypeDoc reference on every pull request that touches the
surface and publishes it when a release is published -- which `release.yml` does
only after every package is on npm -- and **still polls `npm view` before
deploying, because published and installable are separated by propagation.** The
wait loop in that file has that as its reason; without it, it reads as defensive
padding and gets shortened.

One directory per version, prereleases included. **`latest/` follows the newest
_stable_ version**, the way npm's dist-tag does, and two files implement that
rule -- `docs.yml` picks it and `site-index.mjs` refuses a prerelease stamp.

The tool fails on a dead link or a type reaching a signature unexported, and
**refuses any undocumented member at all: `undocumented-baseline.txt` is `0`, a
floor rather than a ratchet.** It reached zero from the ninety-two there on the
day it arrived; it does not go back up.

**Coverage: two floors.** The workspace floor is 90% of regions and lines, on the
pinned nightly so branches are measured at all; the second is 60% of regions on
the Neon boundary alone, which the workspace number excludes because its regions
are called by V8 and by nothing else. **On Windows only the first runs** --
loading the instrumented addon under `--pool=threads` segfaults there -- so CI
measures the boundary on the other two runners.

---

## Porting a v9 component

Six ways a v9 component does not mean in this renderer what it says, found by carrying one
across a line at a time. **Listed with what each does when you get it wrong**,
because that decides how much of the port has to be re-checked: a type error
costs nothing; a value silently wrong by a factor of the font size costs the
whole render.

1. **A bare `Box` runs the other way, and its shrink is a trap pointing the wrong
   direction.** v9's direction is Yoga-defaulted to `column` where this renderer follows CSS
   and uses `row`, so every container writes its axis out. The shrink is not
   simple: Yoga defaults `flex-shrink` to `0`, so a v9 node _looks_ like it means
   zero -- but v9's constructors put CSS's value back and all four declare
   `flexShrink: 1`. **So a v9 node that says nothing about shrinking means `1`,
   and taffy already means `1`.** The faithful port writes no `flex-shrink` at
   all. _Writing `0` is a divergence dressed as a reproduction_, and pinned into
   a wrapper every node passes through it moved a whole card's geometry and made
   its background stop painting.

2. **`lineHeight` is a different quantity.** v9's is the line box in **pixels**;
   this renderer's is a **multiple of the em size**. `lineHeight: 24` at 18px is 24 pixels
   there and 432 here. The trap inside the trap: a component written in pixels
   may still hold a bare ratio, and those are the only values that carry over
   unchanged. _Wrong by a factor of the font size, and it does not look like a
   unit mistake -- it looks like a layout defect._

3. **`ellipsis` no longer changes type -- this hazard is closed**, and is kept
   here because a porter who read an older copy needs telling it went away. this renderer
   takes v9's `boolean | string` on both surfaces: `true` draws U+2026, measured
   in Chrome rather than assumed; `false`, `''` and omitting it all truncate
   without a marker. **`false` is the one to notice** -- it is v9's own applied
   default, so the caller most likely to have written it explicitly is the one
   migrating.

4. **Edge groups are gone.** v9 spells `padding: { Horizontal: 2, Bottom: 2 }`;
   this renderer has only `top`, `right`, `bottom`, `left`. **From TypeScript this is
   caught. At runtime it is not**: measured, a node given
   `padding: { Horizontal: 16 }` renders with no padding at all and nothing is
   thrown. _Keep the port in TypeScript and the whole class is a compile error;
   leave it and the class is invisible._

5. **`<b>` inside a plain `Text` is markup there and literal text here**, which
   has `RichText` for the purpose. _Visible immediately -- the tags draw._

6. **Capitalisation throughout**: `Style.PositionType.Absolute` to `'absolute'`,
   `position: { Top }` to `{ top }`. _Same as 4: a compile error from TypeScript,
   silently dropped at runtime._

**And the method**, since the first attempt at this was tuned by eye and thrown
away. A v9 component's numbers are the geometry Chrome laid out for the template
it replaced. **The numbers are already the answer:** carry them, do not re-derive
them, and when something is off, measure both renders and say by how much. A card
built from a third of the source and an impression of the rest is not a port and
cannot be corrected into one.

**Assets that are not reachable get a hatched plate at exactly the box the real
image would fill** -- never a guess at the layout around them, and never nothing.
_A missing asset must not be readable as a layout defect._

---

## Releasing

**Nothing is released without an explicit instruction**, and the version is the
maintainer's decision.

### Two channels, numbered independently

The npm package is **`meo-canvas`**, unscoped, continuing v9's npm lineage rather than starting a new package.
The cargo crate is `meo-canvas` too and versions independently, starting fresh at
0.1.0. **They are not comparable and are never synchronised for tidiness.** Tags
carry the channel: `npm-v*` and `rust-v*`. Release notes are hand-written per
channel at `docs/releases/{npm,rust}/<version>.md`, against the previous release
_of that channel_, and a missing note fails the release.

**The 68 existing `v*` tags were not rewritten, and a bare `v10.0.0` is still a
valid npm tag.** `docs.yml`'s filter is `^(npm-)?v...` on purpose, because
`workflow_dispatch` backfills exactly those releases, and `release-tags-check`
asserts it. Narrowing that filter to `npm-v` only, as tidying, breaks the
backfill.

**The tag carries the channel prefix and the site directory does not**, and that
asymmetry is deliberate: `site-index.mjs` parses directory names as
`^v(\d+)\.(\d+)\.(\d+)`, so a directory named `npm-v10.0.0-alpha.6` deploys,
is never listed, and never advances `latest/` -- with nothing red. Someone
"fixing" the inconsistency breaks the site quietly.

It was published as `@l7aromeo/meo-canvas` for a while, to let v10 install beside
`meo-canvas@9`. The requirement was real; the scope was the wrong answer to it --
an alias lets the consumer choose the local name:
`npm install meo-canvas-v10@npm:meo-canvas@next`.

### The targets, and the one missing on purpose

    linux-x64-gnu     linux-arm64-gnu
    linux-x64-musl    linux-arm64-musl
    darwin-arm64      win32-x64         win32-arm64

**`darwin-x64` is excluded because Apple stopped supporting Intel**, and because
it would need `macos-13`, which GitHub is retiring -- building it means owning a
target we lose on someone else's timetable. The rule the set comes from:
everything within our control, and whatever is not gets named as such rather than
left a silent gap.

**Adding a target is one row in `TARGETS` and nothing else.** That is what the
`TARGETS` -> build matrix -> `optionalDependencies` -> `PLATFORM_PACKAGES` -> ABI
floors chain exists to provide. Anywhere a second edit is needed, that is a
defect in the chain rather than a step in the task.

**A target is named three times and a test asserts all three agree.** `TARGETS`
is what a release **builds**, `optionalDependencies` is what an install
**fetches**, `PLATFORM_PACKAGES` is what a process **resolves**. Any two agreeing
while the third does not is its own silent failure: built and pinned but
unresolved renders nothing with the binary on disk; pinned and resolved but
unbuilt fails every install; built and resolved but unpinned works only in a
checkout, which is where it would be tested.

### A Linux artefact is a property of its build base

The same source built on `ubuntu-latest` demanded glibc 2.35 and failed to load
on five of the six images the package claims; built in
`containers/Dockerfile.glibc` it demands 2.28 and loads on all six. **Nothing in
the tree changed between those two runs.**

The musl image is a second instance of the work rather than a variant: Rust's
musl targets default to `crt-static`, which cannot produce a `cdylib` at all;
rust-skia ships no prebuilt Skia for musl so the image compiles all of it, 32 of
a 35-minute build; and an explicit `--target` must **not** be passed, because
cargo then builds build scripts without `RUSTFLAGS` and `skia-bindings`' script
cannot `dlopen` libclang.

**Two instruments, and they are not substitutes.** `just abi-floor` reads the
built `.node`'s undefined symbols and fails if it demands more than `TARGETS`
declares -- and prints measured beside declared, because **a floor declared too
high fails nothing** and under-promises quietly. `just acceptance` loads the
addon in six images with nothing installed. A ceiling compares version tags and
an unversioned symbol has none: a binary under every ceiling still failed on
`_M_replace_cold`. **The floor diagnoses; the load decides.** The musl pair carry
no floors at all, because musl does not version its symbols, so the load is the
whole of the evidence.

### The addon does not ship inside the package

It is 28 MB staged for release (53 MB unstripped from `just addon`, which is
the one in a working tree). One package per target carries one binary, named in
`optionalDependencies` with its own `os`, `cpu` and `libc`. A postinstall script
that downloads was the alternative and was refused: it needs the network at
install time and breaks offline installs, locked-down CI and `--ignore-scripts`.

`resolveAddon` looks in three places in order -- `MEO_CANVAS_ADDON`, the `.node`
beside the package in a working tree, then the platform package -- and a failure
names all three. So a checkout tests what it just built, which is why
`just addon` needs no reinstall to take effect.

**Packing is not installing.** `npm pack` says nothing about whether a consumer
can reach what is inside: `exports` can name a path the `files` allowlist
dropped, and a platform package's `main` can name a binary that is not there.
Both pack cleanly and fail at the first import. `just verify-pack` installs the
tarballs somewhere that is not this repository and renders through them.

### What triggers a publish

`release.yml`, **`workflow_dispatch` only** -- a push is how code arrives, and
publishing is a decision about code that already arrived. `just release-npm`
refuses on a dirty tree, off the branch, or with unpushed commits. `dry_run`
defaults to **true**, and the dry run is what to run after any change to the
workflow, because the workflow is the only thing that reads its own YAML.

**Any version containing a hyphen goes to the `next` dist-tag**, never `latest`:
a semver range never matches a prerelease, so nobody reaches it without naming
it.

Seven runners build one addon each; the publish job refuses unless all seven
tarballs are present, then publishes them **before** the main package, which pins
them at an exact version -- the other order points at versions that do not exist
yet.

**The tag and the release come last, after the registry has accepted
everything.** A tag pushed first and a publish that then fails leaves a version
number that can never be reissued; `meo-skia-canvas` carries that scar.

### A new platform package cannot start on OIDC

**The platform packages are scoped: `@meo-canvas/<suffix>`.** Not
`meo-canvas-<suffix>`, which is what this file said until a prerelease check
queried the unscoped names, got seven `E404`s, and reported that all seven needed
bootstrapping by hand. They did not -- all seven exist. `optionalDependencies` in
`packages/meo-canvas/package.json` is the authority, and the suffixes come from
`TARGETS` in `tools/stage-platform-package.mjs`.

A trusted publisher is configured on a package's settings page, and a package
never published has none. So the first release of every `@meo-canvas/<suffix>` is
refused -- correctly, with nothing published and no tag -- and **the dry run
could not have seen it, because `--dry-run` never authenticates.**

Bootstrap by hand: `npm publish --access public --tag next <tgz>` each tarball,
**one at a time with a pause between**. Four new names in ten seconds tripped
npm's spam gate on the fifth and sixth; that is a burst heuristic, not a name
problem. Before releasing a version that adds a platform package:
`npm view @meo-canvas/<suffix> version` -- an `E404` means bootstrap first, and a
version means it is already on OIDC. **Query the name `optionalDependencies`
names**, because the wrong spelling answers `E404` for every package including
the ones that exist, and that reads as a release blocked rather than a typo.

**Every platform package must exist before the main one, and the check is per
name rather than per release.** npm and pnpm skip an optional dependency whose
`os`/`cpu`/`libc` excludes the host and never ask the registry. **yarn resolves
all seven before installing any**, and a 404 on one fails the whole install --
measured with yarn 4.10.3 against tarballs npm and pnpm install without
complaint. So the window between publishing the main package and finishing the
bootstrap is one in which every yarn install fails, and that window is **the
expected path rather than an accident.**

### The publishing audit

A sentence true of the architecture but not of the code is fine while nothing is
published and false the moment something is. Each of these is a shape rather than
an incident:

- **A claim outlived the constraint that made it true.** "The core performs no
  network access" was written before the `net` feature and survived it in three
  places.
- **A claim outlived the code it described.** The pipeline said measure builds a
  Skia `Paragraph` per text node; that path is `#[cfg(test)]` now.
- **A document contradicted itself across two sections.** Fixing a defect means
  finding every sentence about it, and prose has no compiler to say where they
  are.
- **A list was right about its members and wrong about its bounds.** "Frames for
  GIF and APNG" omitted WebP and AVIF, which animate too.
- **An exclusive claim was made against the wrong axis.** "The animation helpers
  are the only JavaScript that runs" -- `Chart` runs too. It does not _draw_,
  which is the axis the sentence beside it defends, and that is what made the
  wrong one read as safe.

---

## Dependencies

Every dependency is on its latest stable release, and the exceptions say why.

| crate             |      |                                                           |
| ----------------- | ---- | --------------------------------------------------------- |
| `meo-skia-canvas` | 0.16 | Skia, text shaping, encoding. `default-features = false`. |
| `taffy`           | 0.14 | Flexbox, CSS grid, block layout. Without `calc`.          |
| `csscolorparser`  | 0.8  | CSS colour syntax. Holds channels as `f32` -- see below.  |
| `neon`            | 1.1  | Node addon.                                               |
| `clap`            | 4.6  | CLI.                                                      |
| `thiserror`       | 2.0  | Error types.                                              |
| `ureq`            | 3.4  | Remote images, behind the optional `net` feature.         |
| `rayon`           | 1.11 | The addon's asynchronous encode. Not an async runtime.    |

| tool       |       |                                                                |
| ---------- | ----- | -------------------------------------------------------------- |
| bun        | 1.4.1 | Package manager and the JavaScript examples' runtime.          |
| typescript | 6.0.3 | Not 7. Moves when typescript-eslint and TypeDoc do.            |
| eslint     | 10    | typescript-eslint 8, `eslint-config-prettier` last.            |
| prettier   | 3.9   | Whole tree; `.prettierignore` names the machine-written files. |
| vitest     | 5     | Tests and the JavaScript coverage floor.                       |
| typedoc    | 0.28  | Its own package, so it pins the TypeScript it loads.           |
| playwright | 1.62  | Drives Chrome for the conformance tables.                      |

**The core requires no async runtime and performs no network I/O unless built
with `net`.** `just runtime-free` fails if a runtime enters the tree.

**`taffy::TaffyTree` is neither `Send` nor `Sync`** -- taffy represents every
length as a tagged pointer, so `Style` itself holds a `*const ()`. A tree is
built and consumed on one thread and never crosses a boundary, which costs
nothing because `Scene` carries its own style type.

**`csscolorparser::Color` is `f32` on all four channels**, so
`rgba(0, 0, 0, 0.1)` reads back as `0.10000000149011612` on both surfaces --
inherited, not a JavaScript defect. The ruling is that the parser returns the
number the author wrote, with the browser as tiebreak. Two halves, and a
twelve-row measurement is why: an alpha written as a decimal or percentage is
presented as the shortest decimal that round-trips to its `f32`; an alpha written
as a hex byte is `byte / 255` computed where the byte is known, because no
decimal-shortening reaches 127/255 from an `f32`. Scaling to 0..255 stays in
`f32`, where it is exact, and the widening happens to the product -- widening
first lands `#808080` a hair under 128, which is the trap the fix's own test
pins.

---

## Commit messages

**The subject says what, in Conventional Commits form. The body says why.** Both
are needed and they do different jobs: scanning `git log` a year later, the
subject is all you get.

```
type(scope): what changed, in the imperative

Why it changed. What was wrong, how that was found, what else was tried,
and what is true now that was not before.
```

**Types:** `feat`, `fix`, `perf` (for a _measured_ change), `refactor` (no
behavioural difference), `docs`, `test`, `build`, `ci`, `chore`, `revert`. A
breaking change takes a `!` before the colon and says so in the body.

**The scope names the part of the tree the change touches**, and has to match it
-- a commit whose scope says `layout` and whose diff moves the release workflow
is worse than one with no scope, because the scope is what a later search filters
on. Use what a reader would look for: `scene`, `codec`, `core`, `layout`,
`paint`, `text`, `arena`, `node`, `cli`, `justfile`, `workflows`.

Subject: imperative, no trailing period, under about seventy characters.

The body:

- Lead with the defect or the gap, in the terms someone hitting it would use,
  not in the terms of the fix.
- Give the evidence: a measurement, a decoded byte, an assertion that failed --
  something checkable, not an assurance.
- Say what was rejected and why, where a reader would otherwise wonder.
- Name what is still not right. A commit fixing one of two problems says so.
- Prose, not bullets. Wrap at 72 columns.

Body length follows from the reasoning, not from a limit. Most commits here run
twenty to fifty lines; a genuinely small change takes three.

**A change's two sides land in one commit** -- see the rule at the top.

---

## Writing for people

- Be concise. Simple sentences. Technical jargon is fine.
- Do not overexplain. Assume the reader is technically proficient.
- No flattering, corporate or marketing language. No weasel words.
- No vague claims the context does not support.

---

## Before every commit

1. **`just ci` must pass, all of it.** The `justfile` is the authority on what it
   runs; do not trust a list of it written anywhere else, including this file.

2. **Nothing after `just ci` on the same command line.** A pipe, an `&&` or a
   trailing `echo` replaces the gate's exit status with its own, and
   `${PIPESTATUS[0]}` is a bashism that expands to nothing here.

3. **Every `unwrap()` you added is gone and every `expect()` explains the
   invariant that makes it unreachable.**

4. **Re-read the comments around everything you changed**, including the ones the
   diff did not show you. A comment that is now wrong is a defect in this commit,
   not a task for later.

5. **Both surfaces have the change**, or you can say why the other does not need
   it.

6. **Report the shape, not a summary:** the invocation verbatim, its exit status
   and summary line, and what did **not** run -- skipped, filtered, feature-gated
   or unreachable on this host.

7. **Then stop.** Pushing, opening a pull request, publishing and tagging are
   separate decisions, taken one at a time.
