set shell := ["bash", "-euo", "pipefail", "-c"]

# The formatting toolchain, by name. rustfmt.toml turns on options that only
# nightly reads, and nightly rustfmt drifts daily, so the date is pinned and
# the fmt job in .github/workflows/ci.yml installs this exact one.
fmt_toolchain := "nightly-2026-08-10"

# The GPU backend meo-skia-canvas compiles on this platform. Only
# `meo-canvas-node` names it: it builds the cdylib, so it is the crate that
# forwards a backend feature to meo-skia-canvas. Every other crate builds with
# no features at all.
host_features := if os() == "macos" { "metal" } else { "vulkan" }

# What a cdylib is called on this platform, and what Node wants it called.
#
# Node loads a native addon only under `.node`, and cargo names a cdylib by the
# platform's own convention. The copy is the whole difference.
lib_name := if os() == "macos" { "libmeo_canvas_node.dylib" } else if os() == "windows" { "meo_canvas_node.dll" } else { "libmeo_canvas_node.so" }
addon_path := "packages/meo-canvas/meo-canvas.node"

# Default: show available recipes.
default:
    @just --list

# Prettier runs from node_modules, never through `npx`: `npx` reaches the
# network when the binary is absent, so a formatting check could pull a
# different version than the one package.json names and report against rules
# nobody agreed to.
[private]
ensure-deps:
    @test -d node_modules || npm ci --ignore-scripts

# Aggregate: what CI runs. Uses non-fixing variants.
ci: fmt-check doc-examples-check typecheck arena-tables-check arena-enums-check arena-cases-check media-types-check lint-check layout-check docs test addon test-js coverage coverage-js example unused

# First-time setup on a fresh clone. Idempotent -- safe to re-run.
#
# The nightly carries llvm-tools-preview as well as rustfmt, because `coverage`
# runs on it: `--branch` rests on `-Z coverage-options=branch`, which stable
# rustc refuses. A nightly with rustfmt alone formats the tree and then fails
# the first `just coverage` on a fresh clone.
#
# CLAUDE.md is a symlink, never a file: AGENTS.md is the only prose document in
# the tree, and the symlink is what makes the same text reachable under the
# other name without a second copy to keep in step. The .gitignore denies it,
# so it stays local.
[doc("Install the toolchain, the cargo tools, and the local symlink.")]
setup:
    rustup component add rustfmt clippy llvm-tools-preview
    rustup toolchain install {{ fmt_toolchain }} --component rustfmt --component llvm-tools-preview
    cargo install --locked cargo-llvm-cov cargo-machete
    @test -L CLAUDE.md || ln -s AGENTS.md CLAUDE.md
    @echo "ready -- run \`just ci\`"

# Build every crate, plus the addon with its platform backend.
#
# Two invocations because they take different feature sets and one command
# cannot: a workspace-wide `--features` names features on every member, and
# only the node crate has a backend to name.
[doc("Build the workspace and the native addon for this platform.")]
build:
    cargo build --workspace
    cargo build -p meo-canvas-node --features "{{ host_features }}"

# Build the addon and put it where the TypeScript surface loads it from.
#
# The `.node` is a build artefact and stays untracked: deny-by-default already
# refuses it, so there is nothing to keep out of a commit by hand.
[doc("Build the native addon into packages/meo-canvas.")]
addon:
    cargo build -p meo-canvas-node --features "{{ host_features }}"
    @cp target/debug/{{ lib_name }} {{ addon_path }}
    @echo "built {{ addon_path }}"

# Run the test suite.
#
# Twice for the two crates that name a GPU backend, because a build without one
# rasterises on the CPU and a test asserting that the two rasterisers differ
# would pass vacuously. That is not hypothetical: `gpu` reached nothing for a
# while and every test that asserted the flag had been copied stayed green.
test:
    cargo test --workspace
    cargo test -p meo-canvas-node --features "{{ host_features }}"
    cargo test -p meo-canvas --features "{{ host_features }}"

# Coverage floor is 90%. `--fail-under-*` exits non-zero, so this is the gate
# rather than a report.
#
# Regions, not lines, is the dimension that rots -- the one nothing guards
# always does, and it is the one that drifts while lines hold.
#
# Runs on the pinned nightly so branches are measured at all: `--branch` needs
# `-Z coverage-options=branch` and stable rustc refuses it. The same toolchain
# formats the tree, so the pin is one date to move rather than two.
#
# The floor is lines and regions because those are the only ones the tool can
# fail on -- there is no `--fail-under-branches`. Branch percentages reach the
# report and `target/lcov.info` for reading; regions is what refuses a merge. A
# region is a span with its own arm count, so an untaken arm still lands in the
# number that gates.
#
# `--doctests` counts what `just test` already runs. Without it, code reached
# only from a documentation example reads as uncovered and pulls the floor down
# for being tested the one way the floor cannot see.
#
# Nothing is excluded from the denominator. A file earns an exclusion by being
# generated rather than written, and it is named here one path at a time so
# that the list is reviewable in a diff.
[doc("Measure coverage and fail below the 90% floor.")]
coverage:
    cargo +{{ fmt_toolchain }} llvm-cov --workspace --branch --doctests \
      --fail-under-lines 90 --fail-under-regions 90 \
      --lcov --output-path target/lcov.info

# The report, opened, with no floor to fail. What to run while writing tests.
[doc("Open the coverage report in a browser.")]
coverage-open:
    cargo +{{ fmt_toolchain }} llvm-cov --workspace --branch --doctests --open

# Run clippy with autofix (modifies working tree).
lint:
    cargo clippy --workspace --fix --allow-dirty --allow-staged --all-targets -- -D warnings
    cargo clippy -p meo-canvas-node --fix --allow-dirty --allow-staged --all-targets --features "{{ host_features }}" -- -D warnings

# Run clippy without fixing (CI-safe).
#
# Two passes, because one feature set does not lint the crate. Code reachable
# only with a backend compiled is dead code without one, and `-D warnings`
# refuses it -- so the addon goes unlinted unless its own pass names a backend.
[doc("Run clippy without fixing (CI-safe).")]
lint-check:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p meo-canvas-node --all-targets --features "{{ host_features }}" -- -D warnings

# Rust on the pinned nightly, then JavaScript, TypeScript and Markdown through
# prettier. Both halves in one recipe, because `just ci` checks both and a pair
# of narrower recipes would let someone format one half, see green, and push a
# tree the check still refuses.
[doc("Format Rust, JavaScript, TypeScript and Markdown (rewrites the tree).")]
fmt: ensure-deps
    cargo +{{ fmt_toolchain }} fmt --all
    ./node_modules/.bin/prettier --write "**/*.{js,mjs,ts,mts,md}"

# Verify formatting without writing.
fmt-check: ensure-deps
    cargo +{{ fmt_toolchain }} fmt --all -- --check
    ./node_modules/.bin/prettier --check "**/*.{js,mjs,ts,mts,md}"

# The TypeScript surface is what the npm package publishes as its types, and
# nothing else reads it: prettier parses the file without checking it, and no
# Rust recipe sees it at all.
#
# Not `check`: the `-check` suffix on every other recipe here means "the variant
# that reports instead of rewriting", and a bare `check` reads as the same idea
# one word short.
# Emit what the npm package publishes.
#
# `exports` names `dist`, so the package cannot be resolved by a consumer until
# this has run -- which is what the example project exists to catch.
[doc("Build the TypeScript package into dist.")]
build-js: ensure-deps
    ./node_modules/.bin/tsc -p packages/meo-canvas/tsconfig.build.json

# The consumer projects: typecheck them against the built package, run them, and
# compare what they drew.
#
# Both of them, deliberately. They draw the same picture from the two surfaces,
# so a surface left behind fails this command rather than being noticed later.
# Each reaches `meo-canvas` the way anyone else would -- the JavaScript one
# through the package's exports rather than into its source, the Rust one
# through a dependency rather than from inside the workspace -- so it catches
# what the test suites cannot: an exports map, a `types` field, an entry point,
# or a public item that a caller needs and the crate does not export.
#
# **The byte comparison is the point, not a flourish.** One input through two
# surfaces is a check neither surface's own tests can perform, and it has
# already earned its place: the two pictures differed in 5,872 bytes because the
# addon named a GPU backend and no Rust caller could, so one rasterised on the
# GPU and the other on the CPU while both reported `gpu: true` -- `Surface::gpu`
# reports the request rather than the outcome. Hence `--features` here, and the
# `metal`/`vulkan` forwarding in `meo-canvas` and `meo-canvas-core`.
[doc("Typecheck and run both consumer examples.")]
example: build-js
    ./node_modules/.bin/tsc --noEmit -p examples/bun/tsconfig.json
    cd examples/bun && bun run index.ts
    cd examples/rust && cargo run --quiet --features "{{ host_features }}"
    @cmp -s examples/bun/out.png examples/rust/out.png \
      || { echo "error: the two surfaces drew different pictures; examples/bun/out.png and examples/rust/out.png differ"; exit 1; }
    @echo "both surfaces drew the same bytes"

[doc("Type-check the shipped TypeScript surface.")]
typecheck: ensure-deps
    ./node_modules/.bin/tsc --noEmit -p packages/meo-canvas/tsconfig.json
    ./node_modules/.bin/tsc --noEmit -p packages/meo-canvas/tsconfig.test.json

# Lifts the fenced examples out of the doc comments into a compiled file.
#
# TypeScript compiles nothing inside a comment, so a `.ts` doc example is prose
# and can name a property that no longer exists while every gate stays green.
# That happened once: renaming `background` to `backgroundColor` left two
# examples on the old key and `just typecheck` passed. The Rust half has no such
# exposure -- `just docs` runs its doctests.
#
# The emitted file lands under `src`, which `typecheck` already covers, so this
# reuses a gate rather than adding one.
[doc("Emit the TypeScript doc examples as compilable code.")]
doc-examples:
    node packages/meo-canvas/tools/generate-doc-examples.mjs

# Fails when the checked-in examples no longer match the doc comments.
#
# Regenerates to a disposable path and diffs, for the reason
# `arena-tables-check` does: git reports a file as changed whether it is
# untracked, written or staged, so a check built on it refuses the workflow it
# exists to support.
[doc("Fail if the extracted doc examples have drifted from the comments.")]
doc-examples-check:
    @mkdir -p target
    @node packages/meo-canvas/tools/generate-doc-examples.mjs target/doc-examples.check.ts
    @diff -u packages/meo-canvas/src/generated/doc-examples.ts target/doc-examples.check.ts \
      || { echo "error: the extracted doc examples are stale; run \`just doc-examples\` and commit the result"; exit 1; }

# The JavaScript suite.
#
# vitest is invoked from `node_modules` rather than through an npm script, the
# same way prettier and tsc are here: one place names the command, and it is
# this file.
[doc("Run the JavaScript tests.")]
test-js: ensure-deps
    ./node_modules/.bin/vitest run

# The JavaScript suite again, with the same 90% floor the Rust half has.
#
# A separate recipe rather than a flag on `test-js`, mirroring `test` and
# `coverage`: what to run while writing a test is not what gates a build, and
# instrumenting every local run to find out whether one test passes is a cost
# for no answer.
#
# The floor and the exclusions live in `vitest.config.mts`, next to the reason
# for each. Only generated files are excluded, one path at a time.
[doc("Measure JavaScript coverage and fail below the 90% floor.")]
coverage-js: ensure-deps
    ./node_modules/.bin/vitest run --coverage

# Regenerates the TypeScript arena tables from the Rust that defines them.
#
# The property indices live in `arena_group!` invocations in
# `crates/meo-canvas-node/src/arena.rs` and a writer needs every one. Emitting
# them rather than transcribing them keeps one table rather than two agreeing
# by inspection -- the failure already removed twice here, once for the format
# table that was `pub(crate)` upstream and once for the node tags hand-written
# in the byte codec.
#
# Generated rather than exported at runtime because the encoder runs per
# property per node and that path has to stay cheap; a static table is
# single-sourced and free, where a runtime-described one pays per write.
[doc("Emit the TypeScript arena tables from the Rust tables.")]
arena-tables:
    node packages/meo-canvas/tools/generate-arena-tables.mjs

# Regenerates the round trip's expected bytes.
#
# One case per arena property plus one setting every property at once, each a
# scene with that property set and the bytes the byte format writes for it.
# Keyed by Rust field name; the TypeScript spelling is the public API and lives
# in the encoder.
arena-cases:
    cargo test -p meo-canvas-node --lib -- --ignored --exact \
      arena::cases::tests::emit_arena_cases

# Fails when the checked-in cases no longer match the Rust.
#
# Regenerates to a disposable path and diffs, for the same reason
# `arena-tables-check` does: `git status` reports a file as changed whether it
# is untracked, written or staged, so a check built on it refuses the workflow
# it exists to support.
arena-cases-check:
    @mkdir -p target
    @MEO_ARENA_CASES="$PWD/target/arena-cases.check.json" cargo test -q \
      -p meo-canvas-node --lib -- --ignored --exact \
      arena::cases::tests::emit_arena_cases > /dev/null
    @diff -u fixtures/arena-cases.json target/arena-cases.check.json \
      || { echo "error: the arena cases are stale; run \`just arena-cases\` and commit the result"; exit 1; }

# Fails when the checked-in tables no longer match the Rust.
#
# Regenerates to a disposable path and diffs. Drift fails a build rather than a
# round trip, which is the point of generating them: a writer reading a stale
# index writes the right number of slots into the wrong field, and no length
# check catches that.
#
# A diff of two files rather than a question to git. `git status` reports a file
# as changed whether it is untracked, written or staged, so a check built on it
# refuses the workflow it exists to support -- edit the Rust, regenerate, run
# the gate -- and passes only after a commit. `diff` also fails when the
# checked-in file is absent, which is the other case worth catching.
[doc("Fail if the checked-in arena tables have drifted from the Rust.")]
arena-tables-check:
    @mkdir -p target
    @node packages/meo-canvas/tools/generate-arena-tables.mjs target/arena-tables.check.ts
    @diff -u packages/meo-canvas/src/generated/arena-tables.ts target/arena-tables.check.ts \
      || { echo "error: the arena tables are stale; run \`just arena-tables\` and commit the result"; exit 1; }

# Regenerates the TypeScript wire-enum tables from the Rust that declares them.
#
# The arena writes an enum as one number: the same discriminant the byte codec
# writes, because both sides read `from_wire`. Those numbers are declared
# explicitly in 26 `wire_enum!` blocks in `crates/meo-canvas-scene/src` -- the
# macro's own comment says why explicitly, and it is the same reason this is
# generated. Hand-copying them would be a fourth copy of each list, and the
# drift is silent in the worst available way: a variant inserted upstream does
# not fail to decode, it decodes as a *different variant*.
[doc("Emit the TypeScript wire-enum tables from the Rust declarations.")]
arena-enums:
    node packages/meo-canvas/tools/generate-arena-enums.mjs

# Fails when the checked-in enum tables no longer match the Rust.
#
# Regenerates to a disposable path and diffs, for the reason
# `arena-tables-check` does. `$PWD` rather than a bare relative path: a
# relative destination resolves against wherever the recipe's shell started,
# and a temp file written somewhere nothing compares is a check that passes
# without checking.
[doc("Fail if the checked-in wire-enum tables have drifted from the Rust.")]
arena-enums-check:
    @mkdir -p target
    @node packages/meo-canvas/tools/generate-arena-enums.mjs "$PWD/target/arena-enums.check.ts"
    @diff -u packages/meo-canvas/src/generated/arena-enums.ts target/arena-enums.check.ts \
      || { echo "error: the wire-enum tables are stale; run \`just arena-enums\` and commit the result"; exit 1; }

# The TypeScript format table, emitted from the Rust one rather than kept in
# step by hand. Browsers accept both `image/x-icon` and the renderer's
# `image/vnd.microsoft.icon` for `ico`, so a transcribed table can disagree with
# the renderer and still serve, render and pass every test.
#
# A Rust test rather than a source parser, for the reason `arena-cases` is one:
# the values come from upstream's trait table at runtime and are not in any
# source text this side could read.
[doc("Emit the TypeScript format table from the Rust one.")]
media-types:
    @MEO_MEDIA_TYPES="$PWD/packages/meo-canvas/src/generated/media-types.ts" cargo test -q \
      -p meo-canvas --test media_types -- --ignored --exact emit_media_types > /dev/null

# Fails when the checked-in format table no longer matches the Rust.
[doc("Fail if the checked-in format table has drifted from the Rust.")]
media-types-check:
    @mkdir -p target
    @MEO_MEDIA_TYPES="$PWD/target/media-types.check.ts" cargo test -q \
      -p meo-canvas --test media_types -- --ignored --exact emit_media_types > /dev/null
    @diff -u packages/meo-canvas/src/generated/media-types.ts target/media-types.check.ts \
      || { echo "error: the format table is stale; run \`just media-types\` and commit the result"; exit 1; }

# Golden fixtures: a scene, and the picture it must produce.
#
# The only check here that looks at the image rather than at whether a line
# ran. Comparison is byte for byte, with no tolerance: five renders of one
# scene across two processes produced a single hash, and a build with the Metal
# backend compiled produced the same bytes as one without, so a disagreement is
# a regression until someone measures otherwise.
#
# The harness registers one font, from this repository, and refuses a fixture
# naming any other family -- the platform's installed faces answer `has_family`
# too, so a fixture asking for Helvetica would pass here and differ anywhere
# else.
#
# A failure writes `actual.png` and `diff.png` under `target/fixtures/<name>/`
# and reports the differing pixel count and the box containing them. "Differs"
# on its own means reproducing locally before you can even look.
# Not in the `ci` chain, and deliberately: the harness is an ordinary test, so
# `test` already runs it and `coverage` already counts it. This recipe is the
# focused runner for someone iterating on one image -- naming it in `ci` as well
# would run the same comparison twice.
[doc("Render every fixture and compare it against its committed image.")]
fixtures:
    cargo test -p meo-canvas-core --test fixtures

# Rewrites one fixture's expected image.
#
# One name, and there is no bulk form on purpose: accepting every difference at
# once is how a regression becomes a commit, and a legitimate mass change is
# still worth looking at one image at a time.
[doc("Accept one fixture's current render as its expected image.")]
fixtures-accept name:
    MEO_FIXTURE_ACCEPT={{ name }} cargo test -p meo-canvas-core --test fixtures

# No legacy module layout: `foo.rs` beside a `foo/` directory, never a
# `mod.rs`. No lint expresses this -- rustc, clippy and rustfmt all accept
# either layout -- so a find is the gate.
[doc("Fail the build on a mod.rs anywhere under crates/.")]
layout-check:
    @! find crates -name mod.rs -print | grep . || { echo "error: mod.rs is banned; use foo.rs beside foo/"; exit 1; }

# `-D warnings` is the whole gate; the rustdoc lint table in Cargo.toml decides
# which warnings. `--no-deps` keeps dependency documentation out of the build.
[doc("Fail on a rustdoc warning -- broken intra-doc links above all.")]
docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Report dependencies declared in Cargo.toml that nothing imports.
unused:
    cargo machete

# Measure what a render costs. Not part of `ci`.
#
# A bench is an instrument, not a gate: it answers "what is this worth" rather
# than "is this correct", and a number that varies with the machine cannot fail
# a build honestly. The golden fixtures are what say a change moved no pixels.
[doc("Measure render timings against the release profile.")]
bench:
    cargo bench -p meo-canvas-core

# Remove all build output.
clean:
    cargo clean
