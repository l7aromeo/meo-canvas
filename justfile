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
#
# Refuses to start while another gate is running in this tree, because two of
# them share `target/llvm-cov-target` and one relinks a test binary while the
# other executes it. The second sees `signal: 9 (SIGKILL)` or
# `No such file or directory (os error 2)` from a doctest, both of which read
# as defects in the code and neither of which points here. It cost two
# sessions a wrong diagnosis before the pair of symptoms gave it away.
#
# A lock rather than a probe at the start: two gates can begin minutes apart
# and still meet inside `coverage`, so a check that passes at the door proves
# nothing about the next twenty minutes.
#
# `CARGO_TARGET_DIR` is the escape hatch and not the default, for two measured
# reasons. It is a cold build, so it trades a full workspace rebuild for the
# wait it avoids. And it does not cover everything: `coverage` writes
# `--output-path target/lcov.info`, a literal relative path that no target-dir
# setting moves, so two gates still write one file -- milder by a long way,
# and not nothing.
ci:
    #!/usr/bin/env bash
    # One shell for the whole recipe, which is what lets the trap outlive the
    # first command. A per-line shell would release the lock immediately.
    set -euo pipefail
    lock="${CARGO_TARGET_DIR:-target}/.gate-lock"
    # A gate that has run for two hours has not; the number is a bound on
    # plausibility, not a timeout on the work.
    stale_after=7200
    mkdir -p "$(dirname "$lock")"
    if ! mkdir "$lock" 2>/dev/null; then
        held=$(cat "$lock/pid" 2>/dev/null || echo "")
        began=$(cat "$lock/started" 2>/dev/null || echo "0")
        age=$(( $(date +%s) - began ))
        # `kill -0` alone is not enough: PIDs are reused, and a recorded one
        # that now belongs to something unrelated would read as a live gate
        # forever. The age is what bounds that.
        if [[ -n "$held" ]] && kill -0 "$held" 2>/dev/null && (( age < stale_after )); then
            echo "a gate is already running in this tree (pid $held, ${age}s ago)." >&2
            echo "  wait for it, or build against your own directory:" >&2
            echo "      CARGO_TARGET_DIR=target-mine just ci" >&2
            echo "  if you are sure no gate is running, remove the lock:" >&2
            echo "      rm -rf $lock" >&2
            exit 1
        fi
        echo "clearing $lock, left by pid ${held:-unknown} (${age}s ago, not running)" >&2
        rm -rf "$lock"
        mkdir "$lock"
    fi
    echo $$ > "$lock/pid"
    date +%s > "$lock/started"
    trap 'rm -rf "$lock"' EXIT INT TERM
    just ci-steps

# The gate itself. Run `ci`, which takes the lock first.
[private]
ci-steps: fmt-check doc-examples-check typecheck arena-tables-check arena-enums-check arena-cases-check media-types-check lint-check layout-check docs test addon test-js coverage coverage-js example runtime-free unused

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
    npx playwright install chromium
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
# would pass vacuously. A run without the features is a run where the assertion
# that matters cannot fail.
test:
    cargo test --workspace
    cargo test -p meo-canvas-node --features "{{ host_features }}"
    cargo test -p meo-canvas --features "{{ host_features }}"
    # The golden fixtures pin `gpu` to false, and a build with no backend
    # compiled cannot tell whether the pin holds -- the two rasterisers differ on
    # eight of the ten scenes. This is the run that reads the pin.
    cargo test -p meo-canvas-core --features "{{ host_features }}"

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
    cargo clippy --manifest-path examples/rust/Cargo.toml --all-targets --features "{{ host_features }}" -- -D warnings

# Rust on the pinned nightly, then JavaScript, TypeScript and Markdown through
# prettier. Both halves in one recipe, because `just ci` checks both and a pair
# of narrower recipes would let someone format one half, see green, and push a
# tree the check still refuses.
[doc("Format Rust, JavaScript, TypeScript and Markdown (rewrites the tree).")]
fmt: ensure-deps
    cargo +{{ fmt_toolchain }} fmt --all
    # `examples/rust` is its own workspace, so `--all` and `--workspace` stop at
    # its edge. A consumer of the published surface is the last place worth
    # leaving unformatted or unlinted, since a lint about that surface shows
    # there and nowhere else.
    cargo +{{ fmt_toolchain }} fmt --manifest-path examples/rust/Cargo.toml --all
    ./node_modules/.bin/prettier --write "**/*.{js,mjs,ts,mts,md}"

# Verify formatting without writing.
fmt-check: ensure-deps
    cargo +{{ fmt_toolchain }} fmt --all -- --check
    cargo +{{ fmt_toolchain }} fmt --manifest-path examples/rust/Cargo.toml --all -- --check
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

# The addon, optimised, where a release takes it from.
#
# `addon` builds debug because that is what a working loop wants; a 51 MB
# binary shipped to anyone is built with optimisation or it is a different
# product. Both write to the same path, so whichever ran last is what the
# TypeScript surface loads -- which is the point of the in-tree path winning
# over an installed platform package.
[doc("Build the native addon in release mode.")]
addon-release:
    cargo build --release -p meo-canvas-node --features "{{ host_features }}"
    @cp target/release/{{ lib_name }} {{ addon_path }}
    @echo "built {{ addon_path }} (release)"

# Everything a release publishes, packed and installable, for this host only.
#
# Two tarballs, because that is what npm resolves at install time: the platform
# package holding the binary, and the main package that names it in
# `optionalDependencies`. Packing only the main one produces something that
# installs and then cannot render, which is the failure this recipe exists to
# make impossible to reach by accident.
#
# Host only, deliberately. Cross-compiling the addon is the release workflow's
# job on one runner per target; here the question is whether the packaging is
# right, and one target answers it.
[doc("Pack the installable tarballs for this host into release/.")]
pack: ensure-deps build-js addon-release
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf release
    mkdir -p release/npm
    suffix="{{ if os() == "macos" { "darwin-arm64" } else { "linux-x64-gnu" } }}"
    node packages/meo-canvas/tools/stage-platform-package.mjs \
        "${suffix}" {{ addon_path }} release/npm
    npm pack --pack-destination "$PWD/release" ./release/npm/"${suffix}" >/dev/null
    npm pack --pack-destination "$PWD/release" ./packages/meo-canvas >/dev/null
    echo ""
    ls -lh release/*.tgz | awk '{print $9, $5}'
    echo ""
    echo "Install both, platform package first:"
    echo "  npm install $PWD/release/l7aromeo-meo-canvas-${suffix}-$(node -p "require('./packages/meo-canvas/package.json').version").tgz"
    echo "  npm install $PWD/release/l7aromeo-meo-canvas-$(node -p "require('./packages/meo-canvas/package.json').version").tgz"

# Install what `pack` produced into a throwaway project and render with it.
#
# Packing is not installing. `npm pack` lists what is in the tarball and says
# nothing about whether a consumer can reach it -- `exports` can name a path the
# `files` allowlist dropped, and a platform package's `main` can name a binary
# that is not there. Both pack cleanly and fail at the first import.
[doc("Install the packed tarballs elsewhere and render with them.")]
verify-pack: pack
    node packages/meo-canvas/tools/verify-package.mjs release

# The consumer projects: typecheck them against the built package, run every
# example in both, and compare every file the two of them wrote.
#
# Both of them, deliberately. They draw the same pictures from the two surfaces,
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
#
# Neither half names its examples: each runs every source file it has, so an
# example added to one surface and forgotten on the other is reported by name
# rather than quietly compared against nothing. `diff -rq` is what compares
# them, because it names a file that differs *and* a file only one side wrote,
# and both of those are the same failure -- the surfaces disagreed.
#
# The trees are removed first. A stale file from a renamed example would
# otherwise sit in both trees, match itself, and be counted as agreement.
#
# `addon` as well as `build-js`, because the JavaScript half draws through the
# compiled `.node` and the Rust half compiles from the same sources at run
# time. Without it a painter change reaches one surface and not the other, and
# the comparison reports a divergence that exists only between a stale binary
# and a fresh one -- which it did, over background-image tiling, and reads
# exactly like a real defect. The rule the two halves rest on is that both are
# built from the tree as it stands.
[doc("Run every example on both surfaces and compare every byte they wrote.")]
example: build-js addon
    # The example resolves the package by name, so its own `node_modules` has
    # to exist before anything typechecks against it. It is gitignored, so a
    # fresh clone has none -- which is how this recipe passed for weeks on a
    # machine that happened to have one and would have failed on the first CI
    # run. `--frozen-lockfile` so the example installs what `bun.lock` names
    # rather than resolving afresh and reporting on a tree nobody committed.
    cd examples/bun && bun install --frozen-lockfile
    ./node_modules/.bin/tsc --noEmit -p examples/bun/tsconfig.json
    rm -rf examples/bun/out examples/rust/out
    cd examples/bun && for source in src/*.ts; do [ "$source" = "src/write.ts" ] && continue; bun run "$source"; done
    cd examples/rust && for source in src/bin/*.rs; do cargo run --quiet --features "{{ host_features }}" --bin "$(basename "$source" .rs)"; done
    @test -d examples/bun/out || { echo "error: the JavaScript surface wrote nothing to compare"; exit 1; }
    @diff -rq examples/bun/out examples/rust/out \
      || { echo "error: the two surfaces did not write the same bytes; each line above names a file they disagree on"; exit 1; }
    @echo "both surfaces wrote the same bytes in $(find examples/bun/out -type f | wc -l | tr -d ' ') files"

# Re-measure Chrome and rewrite the conformance tables.
#
# **Deliberately not part of `ci`.** The harness produces tables and the gates
# walk them: `chrome_tables.rs` reads what is checked in and needs no browser,
# so a clone that never runs this never downloads one. A re-measurement should
# arrive as a diff someone reads -- if a future Chrome changes an answer, that
# belongs in a commit rather than in a suite going red on whichever machine
# updated first.
#
# Every number this writes comes from a page that **asserts its font loaded**
# rather than assuming it, and every sample point is derived from a rectangle
# the browser reported rather than written down. Both rules exist because the
# hand-written pages these replace got them wrong.
[doc("Re-measure Chrome with Playwright and rewrite the conformance tables.")]
conformance: ensure-deps
    node packages/meo-canvas/tools/conformance/ellipsis.mjs
    node packages/meo-canvas/tools/conformance/gradients.mjs
    node packages/meo-canvas/tools/conformance/flex.mjs
    node packages/meo-canvas/tools/conformance/borders.mjs
    node packages/meo-canvas/tools/conformance/dotted.mjs
    node packages/meo-canvas/tools/conformance/blend.mjs
    node packages/meo-canvas/tools/conformance/boxshadow.mjs
    node packages/meo-canvas/tools/conformance/objectfit.mjs
    node packages/meo-canvas/tools/conformance/grid.mjs
    node packages/meo-canvas/tools/conformance/mincontent.mjs

[doc("Type-check the shipped TypeScript surface.")]
typecheck: ensure-deps
    ./node_modules/.bin/tsc --noEmit -p packages/meo-canvas/tsconfig.json
    ./node_modules/.bin/tsc --noEmit -p packages/meo-canvas/tsconfig.test.json

# Lifts the fenced examples out of the doc comments into a compiled file.
#
# TypeScript compiles nothing inside a comment, so a `.ts` doc example is prose
# and can name a property that no longer exists while every gate stays green.
# Renaming a style property leaves every example using the old name compiling,
# because none of them is compiled at all. The Rust half has no such exposure --
# `just docs` runs its doctests.
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
test-js: ensure-deps addon
    # `addon` as well, because these read the compiled `.node`: a stale one
    # makes the byte comparisons report a colour of zero, which reads as an
    # encoder defect rather than as a stale binary. `ci` is already safe --
    # it builds the addon first -- so this covers the recipe a person runs
    # alone while working. Incremental, so it is free on a warm tree.
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
coverage-js: ensure-deps addon
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

# Rewrites the percentage fixture's scene from the Rust that describes it.
#
# The one golden whose scene is authored rather than committed as bytes alone.
# A `.mcs` is opaque and a codec change makes every one of them unreadable with
# no source to rebuild from -- which is a cost already paid once, by hand, for
# the gradient fixture.
#
# Its picture is the only check in the project that pins what a percentage
# *means*. Nothing that compares bytes can: a probe and the bytes it is compared
# against are written from the same number, so they agree whether or not the
# arithmetic is right.
[doc("Rewrite the percentage fixture's scene from its source.")]
percentage-fixture:
    @cargo test -q -p meo-canvas --test percentage_fixture -- --ignored --exact emit_percentage_scene > /dev/null
    @echo "wrote fixtures/percentages/scene.mcs; run \`just fixtures-accept percentages\` if the picture should move"

# Rewrites every golden's scene from the Rust that describes it.
#
# The scenes are authored in `crates/meo-canvas/tests/fixture_scenes.rs`, and an
# ordinary test there asserts that each one encodes to exactly the bytes
# committed beside its picture -- so the source and the artefact cannot drift,
# and a codec change is a re-run rather than decoding old bytes with old code.
#
# Byte equality rather than picture equality: if the bytes match the picture
# cannot have moved, where comparing pictures would let a scene change that
# happens to render the same slip through.
[doc("Rewrite every golden fixture's scene from its source.")]
fixture-scenes:
    @cargo test -q -p meo-canvas --test fixture_scenes -- --ignored --exact emit_fixture_scenes > /dev/null
    @echo "rewrote every fixtures/*/scene.mcs from source"

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

# Compare v1's prop surface against v2's.
#
# Deliberately not in `ci`: it reads `../meo-canvas-old`, which a CI machine has
# no reason to hold, and a recipe that fails for being run somewhere ordinary
# gets removed from the gate rather than fixed.
#
# It replaces a checked-in Markdown table whose own header told the reader to
# regenerate it. A document cannot enforce that -- a transcribed list is a copy,
# and a copy is only correct at the moment it is made.
[doc("Print v1's prop surface against v2's, naming v1's tag and commit.")]
surface-report:
    node packages/meo-canvas/tools/surface-report.mjs

# Fail if an async runtime has entered the dependency tree.
#
# `meo-canvas-core`'s README promises "runtime-free always, and fetch-free by
# default", and the second half is pinned by `tests/manifest_claims.rs`. This is
# the first half. A runtime here is a runtime in every consumer -- it is the one
# constraint the crate's manifest states about itself -- and the `net` feature
# relaxed *fetching* without relaxing it.
#
# `-e normal` excludes dev and build dependencies, which may pull whatever they
# like: the claim is about what a consumer links, not what we test with.
# `--all-features` because a feature nobody enables today is still a runtime the
# moment someone does.
#
# The names are the runtimes and their reactors rather than everything
# async-flavoured: `async-trait` is a macro and `futures-core` is a trait
# definition, and refusing those would be refusing a vocabulary rather than a
# runtime.
[doc("Fail if an async runtime is anywhere in the dependency tree.")]
runtime-free:
    #!/usr/bin/env bash
    set -euo pipefail
    found=$(cargo tree -e normal --workspace --all-features \
        | grep -oE "(tokio|async-std|smol|mio|futures-executor) v[0-9][0-9.]*" \
        | sort -u || true)
    if [ -n "$found" ]; then
        echo "an async runtime is in the tree:" >&2
        echo "$found" >&2
        echo "`meo-canvas-core` promises runtime-free; see its README." >&2
        exit 1
    fi

# Report dependencies declared in Cargo.toml that nothing imports.
unused:
    cargo machete

# Measure what a render costs. Not part of `ci`.
#
# A bench is an instrument, not a gate: it answers "what is this worth" rather
# than "is this correct", and a number that varies with the machine cannot fail
# a build honestly. The golden fixtures are what say a change moved no pixels.
[doc("Benchmark both surfaces: criterion, then throughput and memory.")]
bench: bench-rust bench-js

# The pipeline's own timings, in Rust, through criterion.
[doc("Benchmark the core pipeline (criterion).")]
bench-rust:
    cargo bench -p meo-canvas-core

# What a long-lived Node process holding the addon costs, in time and memory.
#
# A different question from `bench-rust` rather than the same one twice:
# criterion times a function, and this asks whether a process that has rendered
# a few thousand scenes settles back to where it started. `--expose-gc` is what
# separates "retained" from "not collected yet" -- without it the idle reading
# measures when V8 felt like running, and the harness says so in its output
# rather than reporting the number as if it meant something.
#
# Runs the release addon: a debug build's numbers describe a binary nobody
# ships, and reporting them as performance is worse than not measuring.
[doc("Benchmark the Node surface: throughput, rss, heap, peak, idle.")]
bench-js: ensure-deps build-js addon-release
    node --expose-gc packages/meo-canvas/tools/bench.mjs

# Remove all build output.
clean:
    cargo clean
