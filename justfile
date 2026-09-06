set shell := ["bash", "-euo", "pipefail", "-c"]

# The formatting toolchain, by name. rustfmt.toml turns on options that only
# nightly reads, and nightly rustfmt drifts daily, so the date is pinned and
# `.github/workflows/ci.yml` installs this exact one, reading the value from
# here rather than restating it. **There is no `fmt` job**: `ci.yml` has one
# job, named `ci`, and this comment named a different one for long enough
# that an audit had to find it.
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

# Where a cross-build lands, and why it is not `addon_path`.
#
# **A container build must not overwrite the native addon.** It did, and the
# consequence was invisible from the recipe's name: `just addon-container
# linux-arm64-gnu` on a Mac left a Linux `.so` at `addon_path`, so `test-js`,
# `example` and every other check needing the native binary failed with a format
# error until someone rebuilt it -- and on a shared checkout it did that to
# whoever else was mid-run, as a failure that reads like their own regression.
#
# One path per target, under `target/` where build output belongs. The suffix is
# the same spelling `TARGETS` uses, because a second spelling of one target is
# how a lookup silently matches no key.
container_addon := "target/container"

# Default: show available recipes.
default:
    @just --list

# Prettier runs from node_modules, never through `npx`: `npx` reaches the
# network when the binary is absent, so a formatting check could pull a
# different version than the one package.json names and report against rules
# nobody agreed to.
[private]
ensure-deps:
    @test -d node_modules || bun install --frozen-lockfile

# The browser `conformance` measures with, checked only where it is needed.
#
# **Not folded into `ensure-deps`.** That recipe is on the path of `typecheck`,
# `lint-check` and everything else that needs `node_modules`, and only this one
# recipe drives a browser -- putting the check there makes every gate pay for a
# probe it has no use for. The cost being weighed is *every recipe pays for a
# browser check*, and it is written down because a taste question gets reversed
# by someone who does not know it was weighed.
#
# `setup` installs it on a fresh clone. This exists so a clone that skipped
# `setup` is told what is missing, rather than failing inside Playwright.
#
# **It asks Playwright where the binary is and looks.** The first spelling here
# was `playwright install --dry-run chromium`, which reports what *would* be
# installed and exits 0 whether or not anything is there -- a check that could
# not fail, in a commit about a tool that did the wrong thing quietly. Measured
# both ways before it was believed: with the browser present, exit 0; with
# `PLAYWRIGHT_BROWSERS_PATH` pointed at nothing, `ENOENT` and exit 1.
[private]
ensure-browser:
    @node -e 'import("playwright").then(async p => { const { accessSync } = await import("node:fs"); accessSync(p.chromium.executablePath()) })' > /dev/null 2>&1 \
      || { echo "error: no chromium for Playwright -- run \`just setup\`, or \`npx playwright install chromium\`"; exit 1; }

# The examples are a consumer of the package and carry their own lockfile.
#
# Their `meo-canvas` is a `file:` dependency, and bun installs one of those by
# **copying the directory at install time** -- so the copy has whatever
# `dist/` had when `bun install` ran, and nothing afterwards. Installed before
# `build-js`, it has no `dist` at all and every import resolves to an error
# type; installed once and left, it keeps yesterday's declarations. So this
# reinstalls whenever the copy's `index.d.ts` is missing or older than the real
# one, which is why every recipe that needs it lists `build-js` first.
[private]
ensure-example-deps:
    #!/usr/bin/env bash
    set -euo pipefail
    real=packages/meo-canvas/dist/index.d.ts
    copy=examples/bun/node_modules/meo-canvas/dist/index.d.ts
    if [[ ! -f "$copy" || "$real" -nt "$copy" ]]; then
        (cd examples/bun && bun install --frozen-lockfile)
    fi

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
[doc("Run every gate, once, in this tree.")]
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
ci-steps: fmt-check doc-examples-check platform-packages-check typecheck arena-tables-check arena-enums-check arena-cases-check media-types-check lint-check layout-check docs docs-js private-docs test addon test-js coverage coverage-js example runtime-free unused

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
    # **A fresh inode every build, deliberately.** `cp` over an existing file
    # truncates it in place and keeps the inode, so anything the operating
    # system has attached to that path's identity survives every rebuild. On
    # this machine something -- Gatekeeper's cache, XProtect, or an endpoint
    # agent -- marked this exact inode and killed node with SIGKILL on load: no
    # output, no throw, exit 137, indistinguishable from an allocation abort.
    # The identical bytes loaded from /tmp, loaded under any other name in this
    # same directory, and loaded again the moment the file was removed and
    # copied back. Rebuilding never cleared it, because `cp` handed the new
    # bytes to the marked inode. `rm -f` first costs nothing and makes that
    # class of failure unreproducible.
    @rm -f {{ addon_path }}
    @cp target/debug/{{ lib_name }} {{ addon_path }}
    @echo "built {{ addon_path }}"

# Run the test suite.
#
# Twice for the two crates that name a GPU backend, because a build without one
# rasterises on the CPU and a test asserting that the two rasterisers differ
# would pass vacuously. A run without the features is a run where the assertion
# that matters cannot fail.
[doc("Run the Rust tests, the doctests and the golden fixtures.")]
test:
    cargo test --workspace
    cargo test -p meo-canvas-node --features "{{ host_features }}"
    cargo test -p meo-canvas --features "{{ host_features }}"
    # The golden fixtures pin `gpu` to false, and a build with no backend
    # compiled cannot tell whether the pin holds -- the two rasterisers differ on
    # eight of the ten scenes. This is the run that reads the pin.
    cargo test -p meo-canvas-core --features "{{ host_features }}"
# What the dependency tree is known to be vulnerable to.
#
# **Not in `ci-steps`.** An advisory is a fact about the lockfile, not about the
# platform, so running it on three runners would buy three copies of one answer.
# CI calls it once, on Linux, as its own step.
#
# **Vulnerabilities fail; unmaintained notices report.** `cargo audit` already
# draws that line -- an `unmaintained` advisory is a warning and exits zero --
# and two crates sit there today, `paste` and `ttf-parser` through
# `owned_ttf_parser`, both transitive. Failing on those would be red for a
# condition nobody here can resolve, which is how a gate stops being read.
#
# **The ignores are flags rather than a config file, so the reasons live beside
# them.** `cargo audit` reads `.cargo/audit.toml`, and this repository excludes
# `.cargo/` on purpose -- it is where a local `config.toml` points at a sibling
# checkout. A policy in a file git does not track is a policy that applies on
# one machine.
#
# **RUSTSEC-2026-0194 and -0195**, both `quick-xml` 0.37.5, both denial of
# service: quadratic time checking a start tag for duplicate attribute names,
# and unbounded namespace-declaration allocation in `NsReader`. Both fixed in
# 0.41.0, which **we cannot take**: the chain is
# `quick-xml <- little_exif 0.6.23 <- meo-skia-canvas <- us`, and 0.6.23 is
# little_exif's newest release requiring `^0.37.5`, which cannot resolve to
# 0.41.0. On reachability, what was checked rather than what is comfortable:
# `meo-skia-canvas` uses little_exif in `encode/webp.rs` and `context/page.rs`,
# writing metadata during encode, and images are decoded through Skia rather
# than little_exif -- so the path from untrusted input into the XML parser is
# probably not open. That is a reading of the call sites, not a proof.
# **Remove both the moment little_exif publishes a release requiring
# quick-xml 0.41 or newer.** That is the entire condition.
#
# This exists because nothing scanned at all until 4 September 2026, when three
# advisories were found by querying OSV by hand against the 385 crates in
# `Cargo.lock`. A finding that needs someone to think of looking is not a gate.
[doc("Fail on a known vulnerability in the dependency tree.")]
audit:
    cargo audit --ignore RUSTSEC-2026-0194 --ignore RUSTSEC-2026-0195

# The `net` feature, compiled and tested once.
#
# **Not in `ci-steps`**, for the reason `audit` is not: whether the HTTP path
# compiles is a fact about the code rather than about the platform, so three
# runners would buy three copies of one answer. `ci.yml` calls it on Linux as
# its own step, where its verdict is legible on its own.
#
# **Nothing built this at all until 5 September 2026.** `--features` everywhere
# else in this file means `metal` or `vulkan`, so `fetch_policy.rs` -- which
# carries `#![cfg(feature = "net")]` at file scope -- compiled to nothing, and
# the second arm of `net_feature.rs` never existed. Nor did the rustdoc: no
# crate carries `[package.metadata.docs.rs]`, so docs.rs builds default features
# and the derivation of the sixty-second timeout and the thirty-two mebibyte cap
# rendered nowhere, while `MIGRATING.md` pointed readers at it.
#
# The cost was measured before it was accepted rather than argued about: 10 s
# cold and 0 s warm, 17 crates added to the graph (157 to 174) -- `ring`,
# `rustls`, `webpki-roots` and their tree. A cache miss on one runner, and
# `Swatinem/rust-cache` covers `target/`.
#
# **The price of asking once is that a local `just ci` still does not build it**,
# so a developer can break `net` and hear about it only from CI. That is already
# true of `audit`, and it is the trade this shape makes.
[doc("Compile and test the `net` feature, which no other recipe builds.")]
net-check:
    cargo clippy -p meo-canvas -p meo-canvas-core --all-targets --features net -- -D warnings
    cargo test -p meo-canvas-core --features net --test fetch_policy
    cargo test -p meo-canvas --features net --test net_feature
# The README banners, drawn by this library.
#
# **Not in `ci`.** It regenerates rather than checks, so it belongs with
# `conformance`: the output is a diff someone looks at, and a gate that rewrites
# four binaries on every run would make every unrelated change carry them.
#
# Reads `packages/meo-canvas/dist` and the addon, so it wants both built first,
# the way `example` does.
[doc("Redraw the README banners with the library itself.")]
brand: build-js addon
    node tools/brand/banner.mjs


# Does an ordinary addon survive being loaded in a worker thread?
#
# **This is a question about what we ship, not about coverage.** The `coverage`
# recipe loads an *instrumented* addon under vitest's threads pool, and on
# Windows that segfaults -- exit 139, nine test files in. Two candidates: the
# profiling runtime writing a profile from a thread, or the addon itself under
# Windows threads. Only the second matters to a consumer, and it matters a lot:
# `worker_threads` is how a server keeps a render off its event loop, and a
# crash there is a crash we published.
#
# The discriminator is this recipe: the same pool, the same suite, an **ordinary**
# addon. Green means the instrumentation was the problem and the guard in
# `coverage` is the whole fix. A segfault means we have a shipping defect on a
# platform we build for, and it is worth finding before someone else does.
#
# Not in `ci-steps`. It runs as its own step so its verdict is legible on its
# own, and it does not gate while the answer is unknown.
[doc("Check an ordinary addon survives a worker-thread pool.")]
threads-probe: ensure-deps addon
    ./node_modules/.bin/vitest run --pool=threads


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
# One file is excluded, named here so the list is reviewable in a diff. The rule
# used to be that only a generated file earns an exclusion; this is the second
# category, and it is narrower than it sounds.
#
# **`meo-canvas-node/src/lib.rs` is 500 regions that no Rust test can execute.**
# It is the Neon boundary -- `FunctionContext`, `JsBuffer`, the `paint` and
# `encode` closures -- and every function in it is called by V8 and by nothing
# else. `cargo llvm-cov` measures it at 4.80%, and no amount of Rust testing
# moves that, because there is no Rust caller to write. The rest of the crate is
# not excluded and does not need to be: `arena.rs` sits at 92.7%.
#
# The measurement that decided this. At `a54d259`, the last commit this gate
# passed on, the workspace was at **90.06% -- six regions of margin**. Eight
# commits later it was 89.92%, with the **identical 2536 missed regions**: no
# new uncovered code, a denominator that shrank by 356 covered regions, and a
# percentage that fell because of it. A floor that a well-tested deletion can
# breach is not measuring what it claims to.
#
# Excluding this one file puts the same tree at 91.65%, which is the number that
# describes the Rust code someone can actually write a test for.
#
# The pattern accepts either separator, because llvm-cov matches it against the
# path the compiler reports and on Windows that is `meo-canvas-node\src\lib.rs`.
# Written with `/` alone it excluded the file on Linux and macOS and nothing on
# Windows, where the floor then failed for a reason the other two did not have.
#
# **The exclusion stays; the hole in it does not.** Those 499 regions used to be
# measured by nothing: exercised thoroughly by the JavaScript suite, and
# counted by neither gate, since `coverage-js` measures TypeScript rather than
# these Rust regions. Since 5 September 2026 this recipe instruments the addon,
# runs the JavaScript suite against it, and reports `lib.rs` from the profile
# that run writes -- **64.13% of regions, 68.00% of lines, 57.89% of
# functions**, against 4.81% from every Rust caller that will ever exist.
#
# It keeps its own floor rather than joining the workspace's, because 64% under
# a 90% average would drag the whole number down to say something about the
# Neon boundary instead of about the code. Two floors, one gate, one profile
# directory.
[doc("Measure coverage and fail below the 90% floor.")]
coverage: ensure-deps
    #!/usr/bin/env bash
    set -euo pipefail
    # One profile directory, two reports. `cargo llvm-cov` merges every
    # `.profraw` under its target directory into one `.profdata` -- the
    # JavaScript run's included -- but its own report cannot name the addon's
    # `cdylib` as an object, so those counters land nowhere. Measured: with the
    # JavaScript profile merged into that profdata, `report` still puts
    # `lib.rs` at 4.81%, and the same profdata against the `.node` puts it at
    # 64.13%. So the second report is `llvm-cov` invoked directly on the
    # artefact the first one cannot see.
    cargo +{{ fmt_toolchain }} llvm-cov clean --workspace
    cargo +{{ fmt_toolchain }} llvm-cov --workspace --branch --doctests --no-report

    # **The addon half does not run on Windows.** `2c1c9e1` died there with a
    # segmentation fault -- `just ci` exit 139 -- on the `--pool=threads` line,
    # after nine test files had passed, at the point the instrumented addon is
    # loaded in-process. The Rust half above still runs on Windows and its
    # floor still gates there; what Windows does not measure is the 499 regions
    # of `lib.rs`, and that number does not vary by platform, so CI measures it
    # on the other two runners.
    #
    # **What is not known, stated rather than assumed:** whether Windows dies
    # from the instrumentation or from the addon. The addon loads and answers
    # in a `worker_threads` worker on macOS uninstrumented, and no CI job has
    # ever run the JavaScript suite under `--pool=threads` on Windows -- the
    # `test-js` recipe takes vitest's default pool -- so this commit introduced
    # the first such run anywhere. If it is the addon rather than the
    # profiling runtime, then a consumer using `worker_threads` on Windows
    # crashes, which is a shipping defect on a platform this publishes for. The
    # measurement that settles it is one Windows job loading an **ordinary**
    # `.node` under `--pool=threads`; it is not run here, and it is worth
    # running before publish.
    if [[ "{{ os() }}" != "windows" ]]; then

    # The addon, built with the same instrumentation and left where the suite
    # already looks. **Not `MEO_CANVAS_ADDON`**: that variable is the subject
    # of `addon.resolve.test.ts`, and a value in the ambient environment fails
    # 9 of its 12 tests -- the override under test cannot also be the harness.
    #
    # In a subshell, because `show-env` exports a dozen `CARGO_LLVM_COV_*`
    # variables and one of them makes a later `report` exit non-zero after
    # printing a clean result: a floor failure with no floor in it.
    #
    # The addon is instrumented when this finishes. Everything `ci` runs after
    # it works on that binary, only slower; `just addon` puts an ordinary one
    # back.
    (
      eval "$(cargo +{{ fmt_toolchain }} llvm-cov show-env --export-prefix)"
      unset CARGO_LLVM_COV_SHOW_ENV
      cargo +{{ fmt_toolchain }} build -p meo-canvas-node --features "{{ host_features }}"
      cp "${CARGO_LLVM_COV_TARGET_DIR:-target}/debug/{{ lib_name }}" {{ addon_path }}
    )

    # **`--pool=threads` is not a performance choice.** It is the difference
    # between a measurement and a zero: vitest's default `forks` pool loads the
    # instrumented addon in worker processes that never flush a profile, so
    # every one of the 15 test files writes no `.profraw` at all and the run
    # reads exactly like a suite that never touches the addon. Continuous mode
    # (`%c`) is worse -- it writes files whose counters are all zero, which
    # merge cleanly and report 0.00% across the whole workspace. Threads run in
    # one process that exits normally, and its `atexit` writes the profile.
    LLVM_PROFILE_FILE="$PWD/target/llvm-cov-target/js-%p-%14m.profraw" \
      ./node_modules/.bin/vitest run --pool=threads

    # The Rust half, with `lib.rs` still excluded: those 499 regions are 4.81%
    # from any Rust caller and always will be, and averaging them into a
    # workspace floor measures the boundary rather than the code.
    cargo +{{ fmt_toolchain }} llvm-cov report --branch --doctests \
      --ignore-filename-regex 'meo-canvas-node[/\\]src[/\\]lib\.rs$' \
      --fail-under-lines 90 --fail-under-regions 90 \
      --lcov --output-path target/lcov.info

    # `-print -quit` rather than `| head -1`: under `pipefail` the pipe kills
    # `find` with SIGPIPE the moment `head` is satisfied, `set -e` sees that,
    # and the recipe exits right after the Rust half printed a clean report --
    # which reads as the floor failing rather than as the plumbing.
    profdata=$(find target -name '*.profdata' -print -quit)

    # The addon half, read through the toolchain's own `llvm-cov` rather than
    # whatever is first on `PATH`: a stable `llvm-profdata` refuses these files
    # with "raw profile version mismatch", which is at least loud.
    #
    # **A named source, not the whole artefact.** The `.node` links the core
    # and scene crates into itself, so a report over the object measures those
    # too and totals 45% -- a number about layout and Skia rather than about
    # the boundary. `lib.rs` alone is the file this recipe exists for.
    llvm_cov="$(rustc +{{ fmt_toolchain }} --print target-libdir)/../bin/llvm-cov"
    boundary=crates/meo-canvas-node/src/lib.rs
    "$llvm_cov" report {{ addon_path }} -instr-profile="$profdata" "$boundary"

    # A floor of its own, well under the workspace's: 64.13% of regions on 5
    # September 2026, and 60 leaves room for a boundary function landing before
    # the JavaScript that reaches it without turning the number into something
    # to chase. Raising it means writing JavaScript that reaches the error
    # arms, not writing Rust.
    measured=$("$llvm_cov" export {{ addon_path }} -instr-profile="$profdata" \
      -summary-only "$boundary" \
      | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"][0]["totals"]["regions"]["percent"])')
    printf 'the addon boundary is at %.2f%% of regions, floor 60\n' "$measured"
    python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) >= 60.0 else 1)' "$measured"

    # **Put an ordinary addon back.** The instrumented one is the point of
    # everything above, and leaving it is what the comment used to tell a reader
    # to undo by hand -- a recipe describing its own cleanup rather than doing
    # it. Worse, `ci-steps` runs `example` afterwards, so every example wrote a
    # `.profraw` into whatever directory it ran from; nine of them were sitting
    # in `examples/bun` when this was found. Gitignored, so nothing reached a
    # commit, and accumulating anyway.
    just addon

    else
      echo "the addon boundary is not measured on windows; see the note above"
    fi

# The report, opened, with no floor to fail. What to run while writing tests.
[doc("Open the coverage report in a browser.")]
coverage-open:
    cargo +{{ fmt_toolchain }} llvm-cov --workspace --branch --doctests --open

# Run clippy with autofix (modifies working tree).
lint: ensure-deps build-js ensure-example-deps
    cargo clippy --workspace --fix --allow-dirty --allow-staged --all-targets -- -D warnings
    cargo clippy -p meo-canvas-node --fix --allow-dirty --allow-staged --all-targets --features "{{ host_features }}" -- -D warnings
    bun run eslint . --fix

# Run clippy without fixing (CI-safe).
#
# Two passes, because one feature set does not lint the crate. Code reachable
# only with a backend compiled is dead code without one, and `-D warnings`
# refuses it -- so the addon goes unlinted unless its own pass names a backend.
# ESLint is type-aware and `examples/bun` is in its project list, so linting
# needs what the examples' TypeScript program resolves against: their own
# `node_modules`, which only `just example` installed before, and
# `packages/meo-canvas/dist`, which their `meo-canvas` dependency points at
# for types. Locally both existed from earlier commands and the gate passed;
# on a runner neither did, every import resolved to an error type, and
# `no-unsafe-*` reported 102 errors in files with nothing wrong in them.
[doc("Run clippy without fixing (CI-safe).")]
lint-check: ensure-deps build-js ensure-example-deps
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p meo-canvas-node --all-targets --features "{{ host_features }}" -- -D warnings
    cargo clippy --manifest-path examples/rust/Cargo.toml --all-targets --features "{{ host_features }}" -- -D warnings
    # The JavaScript half. `bun run lint` is the same pair for someone not
    # using just: eslint, then prettier's check.
    bun run eslint .

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
    bun run fmt

# Verify formatting without writing.
fmt-check: ensure-deps
    cargo +{{ fmt_toolchain }} fmt --all -- --check
    cargo +{{ fmt_toolchain }} fmt --manifest-path examples/rust/Cargo.toml --all -- --check
    bun run fmt:check

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
    # `tsc` elides a triple-slash type reference from declaration emit, so the
    # one that makes `Buffer` resolve in a consumer has to be put back after it
    # runs. The tool says which declarations carry it, and refuses if none does.
    node packages/meo-canvas/tools/reference-node-types.mjs

# The addon, optimised, where a release takes it from.
#
# `addon` builds debug because that is what a working loop wants; a 51 MB
# binary shipped to anyone is built with optimisation or it is a different
# product. Both write to the same path, so whichever ran last is what the
# TypeScript surface loads -- which is the point of the in-tree path winning
# over an installed platform package.
[doc("Build the native addon in release mode.")]
addon-release:
    cargo build --locked --release -p meo-canvas-node --features "{{ host_features }}"
    @rm -f {{ addon_path }}
    @cp target/release/{{ lib_name }} {{ addon_path }}
    @echo "built {{ addon_path }} (release)"

# The Linux addon, built inside the image the release builds it in.
#
# **A Linux artefact is a property of its build base, not of the source.** Built
# on `ubuntu-latest` the addon demanded glibc 2.35 and failed to load on five of
# the six images a release claims; built in `containers/Dockerfile.glibc` it
# demands 2.28 and loads on all six. Nothing in the tree changed between those
# two runs. So the base belongs in the recipe, where a person can run it, rather
# than only in a workflow nobody executes locally.
#
# The `.node` lands where `addon-release` leaves it, so everything downstream --
# packing, the floor check, the acceptance harness -- reads one path and does
# not care which of the two built it.
#
# `target/container/<suffix>/` rather than `target/`, because the container
# builds as root and three different libcs would otherwise write
# `libmeo_canvas_node.so` to the same place: a host build, a glibc build and a
# musl build are three artefacts with one filename.
#
# **`--target` is deliberately NOT passed, and passing it broke musl.** With an
# explicit `--target`, cargo builds build scripts for the HOST and does not
# apply `RUSTFLAGS` to them -- so `-C target-feature=-crt-static` never reached
# `skia-bindings`' build script, which on musl is then a static binary that
# cannot `dlopen`:
#
#   Unable to find libclang: the `libclang` shared library at
#   /usr/lib/llvm20/lib/libclang.so.20.1.8 could not be opened:
#   Dynamic loading not supported
#
# Without it, host and target are one build and the flag reaches everything.
# The image is chosen for the triple, so host IS target here -- which the step
# below asserts rather than assumes, since that is the whole of what `--target`
# was buying and it is worth keeping without the cost.
#
# `vulkan` is written here rather than taken from `host_features`, which is
# `metal` on macOS: this recipe builds a Linux artefact whichever machine drives
# it, and inheriting the driving host's feature would be wrong exactly when it
# is convenient.
[doc("Build a Linux addon inside its release container.")]
addon-container suffix:
    #!/usr/bin/env bash
    set -euo pipefail

    triple=$(node -e "
      import('./packages/meo-canvas/tools/stage-platform-package.mjs').then(module => {
        const target = module.TARGETS['{{ suffix }}']
        if (target === undefined) {
          process.stderr.write('no target named {{ suffix }}\n')
          process.exit(1)
        }
        process.stdout.write(target.rust)
      })
    ")

    # The base image is chosen here and passed in, because the two families
    # differ by libc and the two architectures differ only by the tag. A
    # `Dockerfile` per architecture would be four files agreeing about
    # everything except one word.
    case "{{ suffix }}" in
        *-gnu)  family=glibc; base=quay.io/pypa/manylinux_2_28 ;;
        *-musl) family=musl;  base=quay.io/pypa/musllinux_1_2 ;;
        *) echo "error: {{ suffix }} is not a Linux target, so it has no build image" >&2; exit 1 ;;
    esac
    # libaom assembles its hot paths on x86 and uses NEON intrinsics on aarch64,
    # so the assembler is a build argument rather than a line in the image.
    case "{{ suffix }}" in
        linux-x64-*)   base="${base}_x86_64";  assembler=nasm ;;
        linux-arm64-*) base="${base}_aarch64"; assembler= ;;
    esac

    # A pinned `FROM` would build an x86_64 image on the arm64 runner and fail
    # somewhere far from the cause, so this refuses rather than guesses. It is a
    # gate and not a warning: the arm64 half of the matrix cannot be built until
    # the image takes its base as an argument.
    dockerfile="containers/Dockerfile.${family}"
    grep -q '^ARG BASE' "$dockerfile" || {
        echo "error: ${dockerfile} pins its base image, so ${base} cannot be built from it" >&2
        echo "       The matrix carries both architectures; the image needs 'ARG BASE' and 'FROM \${BASE}'." >&2
        exit 1
    }

    tag="meo-canvas-build:{{ suffix }}"
    docker build \
        --build-arg BASE="$base" \
        --build-arg ASSEMBLER="$assembler" \
        -f "$dockerfile" -t "$tag" containers/

    # The registry is mounted rather than re-downloaded, so a cache on the host
    # -- the runner's, or a person's own -- reaches the build inside.
    # What `--target` used to guarantee, asserted instead. A `manylinux` image
    # under an `aarch64` suffix would otherwise build an x86_64 artefact and
    # stage it under a name npm installs on machines that cannot load it.
    host=$(docker run --rm "$tag" rustc -vV | sed -n 's/^host: //p')
    if [[ "$host" != "$triple" ]]; then
        echo "error: ${tag} builds for ${host}, but {{ suffix }} is ${triple}" >&2
        exit 1
    fi

    mkdir -p "$HOME/.cargo/registry"
    docker run --rm \
        -v "$PWD":/src -w /src \
        -v "$HOME/.cargo/registry":/root/.cargo/registry \
        -e CARGO_TARGET_DIR=/src/target/container/{{ suffix }} \
        "$tag" \
        cargo build --locked --release -p meo-canvas-node --features vulkan

    # The container ran as root, so everything it wrote under `target/container`
    # is root-owned. On a GitHub runner the cache action's `tar` then cannot
    # read it and the post-job save fails -- `Failed to save: /usr/bin/tar
    # failed with exit code 2` on every container-built target -- which is why
    # the musl builds recompiled Skia from source on every run. Handing the
    # tree back to the invoking user is what lets the next run start from the
    # cache. Skipped where there is no `sudo` or no Linux, which is a Mac with
    # Docker Desktop, where bind mounts are already the host user's.
    #
    # **The registry is mounted into the same root container and was not
    # covered**, which is the same defect one layer along. `cargo metadata`
    # runs in the post-job save and reads `~/.cargo/registry`; with the crates
    # the container downloaded left root-owned it fails
    # `Permission denied (os error 13)`, the action reports `failed with exit
    # code 101`, and it then saves a cache without the metadata that tells it
    # what to keep. Measured on the `linux-x64-musl` leg of run 33961676649.
    if [[ "$(uname)" == Linux ]] && command -v sudo >/dev/null; then
        sudo chown -R "$(id -u):$(id -g)" target/container "$HOME/.cargo/registry"
    fi

    # Into the target's own path, never over `addon_path`. See `container_addon`.
    out="{{ container_addon }}/{{ suffix }}/meo-canvas.node"
    cp "target/container/{{ suffix }}/release/libmeo_canvas_node.so" "$out"
    echo "built $out in ${tag}"

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
# right, and one target answers it. `pack-container` is the Linux spelling,
# where the build base is part of the answer.
#
# The suffix is derived from `TARGETS` by matching this host's os, cpu and libc,
# never written down. It was a two-branch ternary on `os()` that ignored
# architecture and had no Windows branch, so packing on an arm64 Linux box
# staged an arm64 binary into a package named `linux-x64-gnu` declaring
# `cpu: ["x64"]` -- and packed it cleanly, for npm to install on machines that
# cannot load it. A wrong artefact from a green command.
[doc("Pack the installable tarballs for this host into release/.")]
pack: ensure-deps build-js addon-release
    #!/usr/bin/env bash
    set -euo pipefail
    just _pack-tarballs "$(node packages/meo-canvas/tools/stage-platform-package.mjs --host)" {{ addon_path }}

# The same pack, from an addon built in its release container.
#
# The suffix is named rather than derived, and that is the whole difference:
# `--host` reports the machine running the command, which on an x64 glibc runner
# is `linux-x64-gnu` **whichever image built the binary**. Deriving it here would
# stage a musl artefact into a package declaring `libc: ["glibc"]`, pack it
# cleanly, and hand npm a binary it installs onto machines that cannot load it
# -- the arm64 mistake above, in a second dimension.
[doc("Build a Linux addon in its release container and pack it.")]
pack-container suffix: ensure-deps build-js (addon-container suffix)
    just _pack-tarballs "{{ suffix }}" "{{ container_addon }}/{{ suffix }}/meo-canvas.node"

# The packing itself, for whatever addon is at `addon_path` under whatever name
# it is given. Both spellings above end here, so there is one description of
# what a release artefact is.
[private]
_pack-tarballs suffix addon:
    #!/usr/bin/env bash
    set -euo pipefail

    # **The path a build wrote must be the path this reads.** `pack` derives its
    # suffix from the host and `pack-container` is handed one, so the two sides
    # can disagree about how a target is spelled -- the same shape as a
    # `glibc`/`gnu` mismatch, where the wrong spelling matches no key and every
    # host is told nothing is published for it. A missing file here means those
    # two disagreed, and saying so beats `npm pack` failing three lines later
    # about something else.
    if [[ ! -s "{{ addon }}" ]]; then
        echo "error: no addon at {{ addon }} for target {{ suffix }}" >&2
        echo "       A build writes that path and this reads it; if they disagree" >&2
        echo "       about how the target is spelled, that is the defect rather" >&2
        echo "       than a missing build." >&2
        exit 1
    fi

    rm -rf release
    mkdir -p release/npm
    node packages/meo-canvas/tools/stage-platform-package.mjs \
        "{{ suffix }}" "{{ addon }}" release/npm
    npm pack --pack-destination "$PWD/release" ./release/npm/"{{ suffix }}" >/dev/null
    npm pack --pack-destination "$PWD/release" ./packages/meo-canvas >/dev/null
    echo ""
    ls -lh release/*.tgz | awk '{print $9, $5}'
    echo ""
    echo "Install both, platform package first:"
    echo "  npm install $PWD/release/meo-canvas-{{ suffix }}-$(node -p "require('./packages/meo-canvas/package.json').version").tgz"
    echo "  npm install $PWD/release/meo-canvas-$(node -p "require('./packages/meo-canvas/package.json').version").tgz"

# Install what `pack` produced into a throwaway project and render with it.
#
# Packing is not installing. `npm pack` lists what is in the tarball and says
# nothing about whether a consumer can reach it -- `exports` can name a path the
# `files` allowlist dropped, and a platform package's `main` can name a binary
# that is not there. Both pack cleanly and fail at the first import.
[doc("Pack for this host, then install the tarballs elsewhere and render.")]
verify-pack: pack verify-packed

# The same check against tarballs that are already in `release/`.
#
# Separate because a container build must not be followed by a host rebuild:
# `verify-pack` would re-run `pack`, which builds the addon here and overwrites
# the artefact the container produced -- so the thing verified would not be the
# thing published.
#
# **This runs the addon on the machine driving it**, so it answers for a target
# whose libc is the host's and for no other. A musl artefact cannot be loaded on
# a glibc runner at all; `acceptance` is what decides for those.
[doc("Install the tarballs already in release/ and render with them.")]
verify-packed:
    node packages/meo-canvas/tools/verify-package.mjs release

# Render the golden fixtures on `linux-x86_64`, in the container a release is
# built in.
#
# **The goldens are architecture-dependent and this is how the second
# architecture's are made.** 15 of the 23 are byte-identical to the reference
# and 8 are not, and the 8 are the ones with a curve, a gradient, a blend or a
# glyph in them. `tests/fixtures.rs` carries the reasoning and the evidence that
# it is rasterisation rather than a fault.
#
# With no argument this reports which fixtures differ here. With one, it accepts
# that fixture's Linux render into `expected.linux-x86_64.png` -- one name at a
# time, deliberately, for the reason `MEO_FIXTURE_ACCEPT` gives: accepting
# everything at once is how a regression becomes a commit.
#
# The container is the release image rather than any Linux box, so the goldens
# it produces are made by the toolchain that builds the published binary. A
# render from a different Skia build would pin a picture nothing ships.
#
# **`--features vulkan` is not about the GPU here**, which `fixtures.rs` pins
# off regardless. It is what makes `skia-bindings` find a prebuilt Skia for this
# feature set: without it there is no match, Skia is compiled from source, and
# the release image carries no `clang++` to do it with. `just test` runs the
# fixtures both ways -- once in the bare `--workspace` pass and once with the
# feature -- so the two builds are already required to agree, and this takes the
# cheaper of the two.
[doc("Render or accept the golden fixtures on linux-x86_64, in the release container.")]
fixtures-linux name="":
    #!/usr/bin/env bash
    set -euo pipefail
    tag=meo-canvas-build:linux-x64-gnu
    docker image inspect "$tag" >/dev/null 2>&1 || \
        docker build --build-arg BASE=quay.io/pypa/manylinux_2_28_x86_64 \
            --build-arg ASSEMBLER=nasm \
            -f containers/Dockerfile.glibc -t "$tag" containers/
    # The variable is passed only when a name was given. `MEO_FIXTURE_ACCEPT`
    # is read with `env::var().ok()`, so an empty value is still Some -- and the
    # run then fails with "no fixture named ``" rather than reporting.
    accept=()
    if [[ -n "{{ name }}" ]]; then accept=(-e MEO_FIXTURE_ACCEPT="{{ name }}"); fi

    mkdir -p "$HOME/.cargo/registry"
    docker run --rm \
        -v "$PWD":/src -w /src \
        -v "$HOME/.cargo/registry":/root/.cargo/registry \
        -e CARGO_TARGET_DIR=/src/target/container/fixtures-linux \
        "${accept[@]}" \
        "$tag" \
        cargo test -p meo-canvas-core --features vulkan --test fixtures -- --nocapture

    # The container ran as root, so everything it wrote under `target/container`
    # is root-owned. On a GitHub runner the cache action's `tar` then cannot
    # read it and the post-job save fails -- `Failed to save: /usr/bin/tar
    # failed with exit code 2` on every container-built target -- which is why
    # a run after it starts cold. Handing the
    # tree back to the invoking user is what lets the next run start from the
    # cache. Skipped where there is no `sudo` or no Linux, which is a Mac with
    # Docker Desktop, where bind mounts are already the host user's.
    #
    # **The registry is mounted into the same root container and was not
    # covered**, which is the same defect one layer along. `cargo metadata`
    # runs in the post-job save and reads `~/.cargo/registry`; with the crates
    # the container downloaded left root-owned it fails
    # `Permission denied (os error 13)`, the action reports `failed with exit
    # code 101`, and it then saves a cache without the metadata that tells it
    # what to keep. Measured on the `linux-x64-musl` leg of run 33961676649.
    if [[ "$(uname)" == Linux ]] && command -v sudo >/dev/null; then
        sudo chown -R "$(id -u):$(id -g)" target/container "$HOME/.cargo/registry"
    fi

# What the built addon demands of a machine, against what its target promises.
#
# A diagnostic and not a gate -- an unversioned symbol has no version to
# compare, which is how a binary under every ceiling still failed to load on
# `_M_replace_cold`. `acceptance` is the gate.
[doc("Check the built addon demands no more than its target declares.")]
abi-floor suffix:
    node packages/meo-canvas/tools/check-abi-floor.mjs {{ suffix }} "{{ container_addon }}/{{ suffix }}/meo-canvas.node"

# Load the built addon on the images its target claims, with nothing installed.
#
# The gate for the Linux targets, and for the musl pair the only evidence there
# is: no glibc floor exists to check, so `abi-floor` has nothing to say about
# them.
[doc("Load the built addon on the images its target claims.")]
acceptance suffix:
    node packages/meo-canvas/tools/acceptance.mjs {{ suffix }} "{{ container_addon }}/{{ suffix }}/meo-canvas.node"

# Bump the npm package's version, and the platform packages it pins with it.
#
# `bump` is handed to `npm version`, so anything it accepts works:
#
#   just bump-npm prerelease     10.0.0-alpha.3 -> 10.0.0-alpha.4
#   just bump-npm minor          10.0.0-alpha.3 -> 10.1.0
#   just bump-npm premajor --preid rc
#
# Variadic, because `just` splits on whitespace and a single-parameter recipe
# takes only the first word -- `--preid` would then be read as another recipe.
#
# Separate from publishing on purpose. A bump is a commit and a publish is a
# workflow; joining them means a publish that fails for any reason leaves a
# version bumped in the history with nothing on the registry under it, and the
# next attempt has to decide whether to bump again.
#
# `optionalDependencies` is rewritten alongside, because the main package pins
# each platform package at its exact version and `src/addon.test.ts` asserts
# that they agree. A bump that moved only one of them fails that test rather
# than shipping a package whose binaries cannot be resolved.
[doc("Bump the npm version and the platform pins with it.")]
bump-npm *bump="prerelease": ensure-deps
    #!/usr/bin/env bash
    set -euo pipefail
    cd packages/meo-canvas
    npm version --no-git-tag-version {{ bump }} >/dev/null
    node -e '
      const fs = require("fs")
      const p = "./package.json"
      const d = JSON.parse(fs.readFileSync(p, "utf8"))
      for (const name of Object.keys(d.optionalDependencies ?? {})) d.optionalDependencies[name] = d.version
      fs.writeFileSync(p, JSON.stringify(d, null, 2) + "\n")
      process.stderr.write(`${d.name}@${d.version}\n`)
    '

# The repository the release workflow runs in.
#
# Named rather than left to `gh`'s default, because a clone with more than one
# remote has no default repository and `gh` fails on it. meo-skia-canvas learnt
# that the expensive way: a release failed *after* its tag was pushed, leaving
# the tag up with no release under it.
release_repo := "l7aromeo/meo-canvas"

# The branch a release is cut from. Not `main`: `main` on that remote is v1's,
# and it is v1's semantic-release branch, so a push there publishes
# `meo-canvas@latest`. This line is the difference between shipping v10 and
# replacing v9.
release_branch := "v10"

# Rehearse a release without publishing anything.
#
# Runs the whole workflow -- the target matrix, an addon per platform, the pack,
# and the install-and-render check -- and stops short of the registry. This is
# what to run after any change to the workflow, and it is not optional: the
# first run of `release.yml` failed both builds on a YAML quoting mistake that
# no local check could see, because the workflow is the only thing that reads
# it.
[doc("Rehearse an npm release. Builds and validates; publishes nothing.")]
release-npm-dry: (_release_npm "true")

# Publish to npm.
#
# A version carrying a hyphen goes to the `next` dist-tag and a bare one goes to
# `latest`; the recipe prints which before it starts, because that is the
# difference between a prerelease nobody resolves by accident and the version
# every `npm install` picks up.
[doc("Publish to npm. A prerelease goes to `next`, a release to `latest`.")]
release-npm: (_release_npm "false")

# The body both spellings share.
[private]
_release_npm dry:
    #!/usr/bin/env bash
    set -euo pipefail

    if [[ -n "$(git status --porcelain)" ]]; then
        echo "error: the working tree is not clean; a release is cut from a commit" >&2
        exit 1
    fi

    branch=$(git branch --show-current)
    if [[ "${branch}" != "{{ release_branch }}" ]]; then
        echo "error: on branch ${branch}, and a release is cut from {{ release_branch }}" >&2
        exit 1
    fi

    # An unpushed commit means the workflow would build a tree nobody can see,
    # and the version it publishes would not be the version in this checkout.
    if [[ -n "$(git log --oneline "origin/{{ release_branch }}..HEAD" 2>/dev/null)" ]]; then
        echo "error: unpushed commits; the workflow builds what the remote has" >&2
        git --no-pager log --oneline "origin/{{ release_branch }}..HEAD" >&2
        exit 1
    fi

    version=$(node -p "require('./packages/meo-canvas/package.json').version")
    case "${version}" in
        *-*) tag=next ;;
        *)   tag=latest ;;
    esac

    if [[ "{{ dry }}" == "true" ]]; then
        echo "==> rehearsing ${version} (would go to dist-tag ${tag}); nothing is published"
    else
        echo "==> publishing ${version} to dist-tag ${tag}"
        if [[ "${tag}" == "latest" ]]; then
            echo "    this is the version every \`npm install\` resolves"
        fi
    fi

    gh workflow run release.yml -R "{{ release_repo }}" --ref "{{ release_branch }}" -f dry_run={{ dry }}
    sleep 10
    run=$(gh run list -R "{{ release_repo }}" --workflow=release.yml --limit 1 --json databaseId --jq '.[0].databaseId')
    echo "==> https://github.com/{{ release_repo }}/actions/runs/${run}"
    gh run watch "${run}" -R "{{ release_repo }}" --exit-status --interval 20

# Rehearse a crates.io release without publishing anything.
#
# Runs the whole workflow -- the toolchain, the system libraries the
# verification build needs, and `cargo publish --workspace --dry-run` -- and
# stops short of the registry. Worth running after any change to the workflow
# for the reason the npm rehearsal exists: the workflow is the only thing that
# reads its own YAML, and the first run of `release.yml` failed on a quoting
# mistake no local check could see.
[doc("Rehearse a crates.io release. Packages and verifies; publishes nothing.")]
release-crate-dry: (_release_crate "true")

# Publish the four crates to crates.io.
#
# `meo-canvas-scene`, `meo-canvas-core`, `meo-canvas` and `meo-canvas-cli`, in
# the order cargo derives from the dependency graph. `meo-canvas-node` carries
# `publish = false` and is skipped without being named.
#
# **This is the irreversible one.** crates.io has no unpublish: a version can be
# yanked, but a yank still resolves for any lockfile that already names it, so a
# version number is spent the moment it is accepted. The recipe prints what it
# is about to do and the workflow verifies every crate before it uploads any.
[doc("Publish every publishable crate to crates.io. Not reversible.")]
release-crate: (_release_crate "false")

# The body both spellings share.
#
# The same four guards as `_release_npm`, deliberately: a release is cut from a
# commit that the remote has, on the release branch, from a clean tree. The
# version is read from cargo's own metadata rather than from a manifest by hand,
# because the four crates inherit `version.workspace` and the workspace root is
# a virtual manifest with no package of its own to read.
[private]
_release_crate dry:
    #!/usr/bin/env bash
    set -euo pipefail

    if [[ -n "$(git status --porcelain)" ]]; then
        echo "error: the working tree is not clean; a release is cut from a commit" >&2
        exit 1
    fi

    branch=$(git branch --show-current)
    if [[ "${branch}" != "{{ release_branch }}" ]]; then
        echo "error: on branch ${branch}, and a release is cut from {{ release_branch }}" >&2
        exit 1
    fi

    # An unpushed commit means the workflow would build a tree nobody can see,
    # and the version it publishes would not be the version in this checkout.
    if [[ -n "$(git log --oneline "origin/{{ release_branch }}..HEAD" 2>/dev/null)" ]]; then
        echo "error: unpushed commits; the workflow builds what the remote has" >&2
        git --no-pager log --oneline "origin/{{ release_branch }}..HEAD" >&2
        exit 1
    fi

    version=$(cargo metadata --format-version 1 --no-deps --manifest-path crates/meo-canvas/Cargo.toml         | node -p "JSON.parse(require('node:fs').readFileSync(0, 'utf8')).packages.find(p => p.name === 'meo-canvas').version")

    if [[ "{{ dry }}" == "true" ]]; then
        echo "==> rehearsing ${version}; nothing is published"
    else
        echo "==> publishing ${version} to crates.io"
        echo "    meo-canvas-scene, meo-canvas-core, meo-canvas, meo-canvas-cli"
        # Said out loud because it is the one difference from npm that matters:
        # npm lets a version be unpublished within 72 hours, crates.io never
        # does. A wrong number here is spent permanently.
        echo "    crates.io has no unpublish; this version number is spent either way"
    fi

    gh workflow run crates-io.yml -R "{{ release_repo }}" --ref "{{ release_branch }}" -f dry_run={{ dry }}
    sleep 10
    run=$(gh run list -R "{{ release_repo }}" --workflow=crates-io.yml --limit 1 --json databaseId --jq '.[0].databaseId')
    echo "==> https://github.com/{{ release_repo }}/actions/runs/${run}"
    gh run watch "${run}" -R "{{ release_repo }}" --exit-status --interval 20

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
conformance: ensure-deps ensure-browser
    WRITE=1 node packages/meo-canvas/tools/conformance/ellipsis.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/gradients.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/flex.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/borders.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/dotted.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/blend.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/boxshadow.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/shadowextent.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/objectfit.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/objectfit-overflow.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/grid.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/mincontent.mjs
    WRITE=1 node packages/meo-canvas/tools/conformance/overflowposition.mjs

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

# Rewrite the two places a target is named that are not `TARGETS`.
[doc("Regenerate PLATFORM_PACKAGES and optionalDependencies from TARGETS.")]
platform-packages:
    node packages/meo-canvas/tools/generate-platform-packages.mjs

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

# **This replaces the test that asserted the three lists agreed.** A generated
# file plus an equality assertion between it and its source is one mechanism
# written twice, and a reader cannot tell which is authoritative.
[doc("Fail when the generated platform package list is out of date.")]
platform-packages-check:
    @mkdir -p target/platform-packages
    @node packages/meo-canvas/tools/generate-platform-packages.mjs target/platform-packages
    @diff -u packages/meo-canvas/src/generated/platform-packages.ts target/platform-packages/platform-packages.ts \
      || { echo "error: PLATFORM_PACKAGES is stale; run \`just platform-packages\` and commit the result"; exit 1; }
    @diff -u packages/meo-canvas/package.json target/platform-packages/package.json \
      || { echo "error: optionalDependencies is stale; run \`just platform-packages\` and commit the result"; exit 1; }

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
[doc("Regenerate the arena property cases the encoder is checked against.")]
arena-cases:
    cargo test -p meo-canvas-node --lib -- --ignored --exact \
      arena::cases::tests::emit_arena_cases

# Fails when the checked-in cases no longer match the Rust.
#
# Regenerates to a disposable path and diffs, for the same reason
# `arena-tables-check` does: `git status` reports a file as changed whether it
# is untracked, written or staged, so a check built on it refuses the workflow
# it exists to support.
[doc("Fail when the arena property cases are out of date.")]
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

# The JavaScript reference, built and gated the way `docs` gates rustdoc.
#
# TypeDoc reads the declarations `build-js` emits into `dist/`, so a signature
# that names a type nothing exports, or a `{@link}` to nothing, fails here
# rather than reaching a reader as a dead end. The number of undocumented
# members ratchets: it may hold or fall, never rise, and the baseline file is
# what holds the line. `tools/typedoc/build.mjs` says how the two kinds of
# finding are told apart.
#
# The tool pins its own TypeDoc and TypeScript in `tools/typedoc/package.json`
# rather than sharing the root's, because TypeDoc is compiled against one
# TypeScript minor and refuses to load another -- the root can move on its own
# schedule without breaking the reference.
[doc("Build the JavaScript API reference and fail on a dead link or a new undocumented member.")]
docs-js: build-js
    #!/usr/bin/env bash
    set -euo pipefail
    tool=packages/meo-canvas/tools/typedoc
    # Presence by the package, not the `.bin` shim, whose filename differs per platform.
    test -f "$tool/node_modules/typedoc/package.json" || bun install --cwd "$tool" --frozen-lockfile
    node "$tool/build.mjs"

# The half of the reference `docs-js` cannot see.
#
# TypeDoc's model is the exported surface, so a doc comment separated from a
# module-private declaration is not undercounted there -- it is absent. This
# asserts the **set** of private declarations carrying no doc, not its size,
# which is what lets it catch a doc *moved* from one to another: the total is
# unchanged and the set is not.
#
# The baseline is a named list rather than a number, so a reader sees which
# declarations are exceptions -- mostly easing coefficients, where a sentence
# would be noise -- and a new one is a visible edit rather than a count moving.
[doc("Fail if a module-private declaration lost its doc comment.")]
private-docs:
    node packages/meo-canvas/tools/private-docs.mjs

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
