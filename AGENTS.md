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
meo-canvas          Scene { .. }            ── Rust callers build it directly
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

### Pipeline

One pass, no re-entry into caller code.

**resolve** registers fonts, fetches images, and inherits text styles down the
tree. This is the only stage that performs I/O.

**measure** builds one Skia `Paragraph` per text node, which the next stage
re-lays-out at different widths. Re-layout reuses run parsing, font resolution
and glyph shaping — see `meo-skia-canvas/src/text.rs:1387-1396` — so the work a
rebuild would repeat happens once per text node. How much that saves against
rebuilding is unmeasured; a criterion bench belongs here before the claim
becomes a number.

**layout** solves the tree with taffy. Text leaves answer taffy's measure
closure from their prepared paragraph. `AvailableSpace::MinContent` and
`MaxContent` are answered by `min_intrinsic_width()` and `max_intrinsic_width()`
without an additional layout — a `Paragraph` exists only post-layout, since
`ParagraphBuilder::build` lays it out at construction.

**paint** walks the solved tree in z-order and draws through
`meo-skia-canvas`'s `Context2D`. No drawing call crosses a language boundary:
the whole stage is Rust calling Rust.

**encode** produces png, jpg, webp, avif, tiff, bmp, ico, svg, pdf, gif, apng,
or raw bytes.

Only `resolve` is asynchronous work. Everything after it is CPU-bound, which is
why parallelism lives at the scene level — many scenes across a thread pool —
rather than inside a single render.

### The JavaScript boundary

A `Box()` or `Text()` call in JavaScript writes opcodes and numeric arguments
into a growable `Float64Array`. Strings, buffers, and anything else a float
cannot hold go into a side `values` array, and the record stores an index into
it. A bitmask per node names which properties are present, so a node that sets
five of its available properties consumes five slots rather than all of them.

The mask is carried in `f64` slots holding **53 bits each**, not 64: a double
represents integers exactly only up to 2^53, and a 64-bit mask written into one
slot loses every bit above that silently. Two slots therefore name 106
properties, and a node kind whose property count passes 106 takes a third.

`render()` hands the arena over in a single call. A scene of any size crosses
once.

This is the shape it is because reading a value out of V8 is what costs, not the
crossing: a `lineTo` in `meo-skia-canvas` costs 82 nanoseconds, of which 17 is
the crossing itself and 39 is reading two floats out of the arguments. Decoding
from a `&[f64]` skips V8 entirely.

The opcode table is generated from the `Scene` definition in Rust. The writer's
opcode and the decoder's opcode are the same number by construction rather than
by two lists agreeing.

### Node addon

`meo-canvas-node` owns the only `#[neon::main]` in the binary. A Node addon has
exactly one module-init symbol, so `meo-skia-canvas` is depended on with
`default-features = false`, leaving its own `node-addon` feature off and its
entry point uncompiled.

The addon re-exports `meo-skia-canvas`'s operations alongside its own, so one
55 MB binary serves both the declarative surface and the imperative canvas API
beneath it. Two addons would mean two copies of Skia resident in one process.

## Workspace

```
crates/meo-canvas-scene    Scene types and the binary codec. No Skia, no taffy, no neon.
crates/meo-canvas-core     resolve, measure, layout, paint, encode.
crates/meo-canvas          The crates.io surface. Struct literals over core.
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

**Golden fixtures** in `fixtures/` are scenes rendered through the CLI and
compared against committed images. This is how the paint stage is covered:
executing a fill proves the line ran, not that the pixels are right, so paint is
verified by comparison rather than assertion. The fixture runner is part of the
coverage harness, not outside it.

**Doctests** run every example in the crate documentation. Examples compile
against the real public API, so they cannot rot.

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

The three README files describe the design. Some of what they say is true of
the architecture and not yet of any code, which is fine while nothing is
published and false the moment something is.

Re-read every capability claim against what runs, and cut or qualify whatever
does not hold. The sentences that need checking are the ones asserting where
work happens, what formats encode, and what a surface accepts — "layout, text
shaping, painting and encoding all happen in Rust" is the shape of the problem.

The same applies to `repository` in the workspace manifest, which names a remote
that has to exist before `cargo publish` will accept it.

## Dependencies

Every dependency is on its latest stable release.

|                   |        |                                                           |
| ----------------- | ------ | --------------------------------------------------------- |
| `meo-skia-canvas` | 0.10.6 | Skia, text shaping, encoding. `default-features = false`. |
| `taffy`           | 0.13   | Flexbox, CSS grid, block layout. Without `calc`.          |
| `neon`            | 1.1    | Node addon.                                               |
| `clap`            | 4.6    | CLI.                                                      |
| `thiserror`       | 2.0    | Error types.                                              |
| `ureq`            | 3.4    | Remote images, behind the CLI's optional `net` feature.   |

The core performs no network I/O and requires no async runtime. It accepts bytes
or a reader, so a Rust caller with no runtime and the CLI are served by the same
code.

`taffy::TaffyTree` is neither `Send` nor `Sync`: taffy represents every length
as a tagged pointer, so `Style` itself holds a `*const ()`. No feature set
changes this. A tree is therefore built and consumed on one thread and never
crosses a boundary — which costs nothing, because `Scene` carries its own
own style type and taffy's `Style` exists only inside the layout stage.
