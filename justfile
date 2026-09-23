set shell := ["bash", "-euo", "pipefail", "-c"]

# The formatting toolchain, pinned by date because `rustfmt.toml` uses options
# only nightly reads and nightly drifts daily. `.github/workflows/ci.yml`
# installs this exact one, reading it from here.
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

# Where a cross-build lands: one path per target under `target/`, never
# `addon_path`, since a Linux `.so` there fails every check needing the native
# addon with a format error. The suffix is `TARGETS`' spelling.
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

# The browser `conformance` measures with, checked here rather than in
# `ensure-deps`, which every gate pays for. It asks Playwright where the binary
# is and looks: `install --dry-run` exits 0 either way.
[private]
ensure-browser:
    @node -e 'import("playwright").then(async p => { const { accessSync } = await import("node:fs"); accessSync(p.chromium.executablePath()) })' > /dev/null 2>&1 \
      || { echo "error: no chromium for Playwright -- run \`just setup\`, or \`npx playwright install chromium\`"; exit 1; }

# The examples take the package as a `file:` dependency, which bun installs by
# copying, so the copy keeps `dist/` as it was. This reinstalls when the copy's
# `index.d.ts` is missing or older, so every caller lists `build-js` first.
[private]
ensure-example-deps:
    #!/usr/bin/env bash
    set -euo pipefail
    real=packages/meo-canvas/dist/index.d.ts
    copy=examples/bun/node_modules/meo-canvas/dist/index.d.ts
    if [[ ! -f "$copy" || "$real" -nt "$copy" ]]; then
        (cd examples/bun && bun install --frozen-lockfile)
    fi

# Aggregate: what CI runs, with non-fixing variants. Refuses to start beside
# another gate in this tree: two share `target/llvm-cov-target`, and a relink
# under a running doctest reads as SIGKILL. `CARGO_TARGET_DIR` is the cold-build
# escape hatch.
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

# The checks that read the repository and nothing else: a recipe belongs here if
# nothing in its dependency closure names `cargo`, which `gate-lists-check`
# asserts. They run once, not per host; `typecheck` alone was 221s of a 959s
# Windows run.
[doc("The half of `ci` that reads the tree and runs no cargo, once per run.")]
portable: gate-lists-check cache-budget-check release-tags-check typecheck docs-js private-docs issue-refs fixture-notes workaround-probes conformance-writes doc-examples-check arena-tables-check arena-enums-check platform-packages-check layout-check

# The checks that compile, link or run the addon, per host. `addon` precedes
# `test-js`, and `test-js` precedes `test`, so a red Rust suite cannot hide the
# JavaScript result; `gate-lists-check` asserts both.
[doc("The half of `ci` that compiles, links or runs the addon, once per host.")]
native: fmt-check arena-cases-check media-types-check lint-check docs addon test-js test coverage coverage-js example runtime-free unused

# The gate itself. Run `ci`, which takes the lock first.
[private]
ci-steps: portable native

# First-time setup, idempotent. The nightly carries `llvm-tools-preview`, since
# `coverage` needs `-Z coverage-options=branch`. `CLAUDE.md` is a local symlink
# to `AGENTS.md`, so one text answers to both names.
[doc("Install the toolchain, the cargo tools, and the local symlink.")]
setup:
    rustup component add rustfmt clippy llvm-tools-preview
    rustup toolchain install {{ fmt_toolchain }} --component rustfmt --component llvm-tools-preview
    cargo install --locked cargo-llvm-cov cargo-machete
    npx playwright install chromium
    @test -L CLAUDE.md || ln -s AGENTS.md CLAUDE.md
    @echo "ready -- run \`just ci\`"

# Two invocations, since only the node crate has a backend and a workspace-wide
# `--features` names it on every member.
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
    # A fresh inode every build: `cp` over the old file keeps its inode, and a
    # marked inode here had node SIGKILLed on load, exit 137, while the same
    # bytes loaded anywhere else. `rm -f` first.
    @rm -f {{ addon_path }}
    @cp target/debug/{{ lib_name }} {{ addon_path }}
    @echo "built {{ addon_path }}"

# Twice for the crates naming a GPU backend: without one, a test asserting the
# two rasterisers differ passes vacuously.
[doc("Run the Rust tests, the doctests and the golden fixtures.")]
test:
    # `--no-fail-fast`, so this line names every failing target rather than the
    # first. The lines below still stop at the first failure.
    cargo test --workspace --no-fail-fast
    cargo test -p meo-canvas-node --features "{{ host_features }}"
    cargo test -p meo-canvas --features "{{ host_features }}"
    # The golden fixtures pin `gpu` to false, and a build with no backend
    # compiled cannot tell whether the pin holds -- the two rasterisers differ on
    # eight of the ten scenes. This is the run that reads the pin.
    cargo test -p meo-canvas-core --features "{{ host_features }}"
# An advisory is a fact about the lockfile, so this runs once, on Linux.
# Vulnerabilities fail; unmaintained notices report. RUSTSEC-2026-0194/-0195
# (`quick-xml` 0.37.5) are ignored because little_exif 0.6.23 needs `^0.37.5`;
# drop both when it requires 0.41.
[doc("Fail on a known vulnerability in the dependency tree.")]
audit:
    cargo audit --ignore RUSTSEC-2026-0194 --ignore RUSTSEC-2026-0195

# Compiles and tests `net`, which no other recipe builds, once on Linux: whether
# it compiles is a fact about the code. A local `just ci` does not build it.
# Measured at 10s cold and 17 more crates.
[doc("Compile and test the `net` feature, which no other recipe builds.")]
net-check:
    cargo clippy -p meo-canvas -p meo-canvas-core --all-targets --features net -- -D warnings
    cargo test -p meo-canvas-core --features net --test fetch_policy
    cargo test -p meo-canvas --features net --test net_feature
# Not in `ci`: it regenerates four images rather than checking, so it sits with
# `conformance`. Wants `dist` and the addon built first.
[doc("Redraw the README banners with the library itself.")]
brand: build-js addon
    node tools/brand/banner.mjs


# Whether an ordinary addon survives vitest's threads pool, where the
# instrumented one segfaults on Windows. It passes there, so that crash is the
# profiling runtime's; a crash here would be a shipping defect, since
# `worker_threads` is how a server keeps a render off its loop. Gates on Windows.
[doc("Check an ordinary addon survives a worker-thread pool.")]
threads-probe: ensure-deps addon
    ./node_modules/.bin/vitest run --pool=threads


# Coverage fails below 90% of regions and lines -- no tool fails on branches --
# on the pinned nightly so branches are measured; `--doctests` counts what
# `test` runs. `meo-canvas-node/src/lib.rs`, which no Rust calls, is excluded
# here and floored below.
[doc("Measure coverage and fail below the 90% floor.")]
coverage: ensure-deps
    #!/usr/bin/env bash
    set -euo pipefail
    # One profile directory, two reports: `cargo llvm-cov report` cannot name
    # the addon's `cdylib`, putting `lib.rs` at 4.81%, so the second report runs
    # `llvm-cov` on the `.node` itself, 64.13%.
    cargo +{{ fmt_toolchain }} llvm-cov clean --workspace
    cargo +{{ fmt_toolchain }} llvm-cov --workspace --branch --doctests --no-report

    # Not on Windows, where the instrumented addon segfaults under
    # `--pool=threads`; the Rust floor above still gates there, and `lib.rs`'s
    # regions are measured on the other two runners.
    if [[ "{{ os() }}" != "windows" ]]; then

    # The instrumented addon goes where the suite already looks, not
    # `MEO_CANVAS_ADDON`, which is what `addon.resolve.test.ts` tests. In a
    # subshell, since one `CARGO_LLVM_COV_*` variable fails a later `report`.
    (
      eval "$(cargo +{{ fmt_toolchain }} llvm-cov show-env --export-prefix)"
      unset CARGO_LLVM_COV_SHOW_ENV
      cargo +{{ fmt_toolchain }} build -p meo-canvas-node --features "{{ host_features }}"
      cp "${CARGO_LLVM_COV_TARGET_DIR:-target}/debug/{{ lib_name }}" {{ addon_path }}
    )

    # `--pool=threads` is what makes a measurement: the default `forks` pool
    # never flushes a profile, and continuous mode writes all-zero counters.
    # Threads run in one process whose `atexit` writes it.
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

    # Through the toolchain's own `llvm-cov`, which reads these profiles, and
    # over `lib.rs` alone: the `.node` links the core and scene crates, and a
    # whole-object report measures those too.
    llvm_cov="$(rustc +{{ fmt_toolchain }} --print target-libdir)/../bin/llvm-cov"
    boundary=crates/meo-canvas-node/src/lib.rs
    "$llvm_cov" report {{ addon_path }} -instr-profile="$profdata" "$boundary"

    # A floor of its own under the workspace's: 64.13% of regions measured, 60
    # to leave room. Raising it means JavaScript that reaches the error arms.
    measured=$("$llvm_cov" export {{ addon_path }} -instr-profile="$profdata" \
      -summary-only "$boundary" \
      | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"][0]["totals"]["regions"]["percent"])')
    printf 'the addon boundary is at %.2f%% of regions, floor 60\n' "$measured"
    python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) >= 60.0 else 1)' "$measured"

    # Put an ordinary addon back, since `ci-steps` runs `example` next and every
    # example would write a `.profraw` wherever it ran.
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
    # The same boundary `fmt` names below, with the check's feature set, so this
    # fixes what `lint-check` goes on to lint.
    cargo clippy --manifest-path examples/rust/Cargo.toml --fix --allow-dirty --allow-staged --all-targets --features "{{ host_features }}" -- -D warnings
    bun run eslint . --fix

# Two passes, since addon code reachable only with a backend is dead code
# without one and `-D warnings` refuses it. ESLint is type-aware over
# `examples/bun`, so it needs that project's `node_modules` and `dist` first.
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

# `exports` names `dist`, so no consumer can resolve the package until this has
# run.
[doc("Build the TypeScript package into dist.")]
build-js: ensure-deps
    ./node_modules/.bin/tsc -p packages/meo-canvas/tsconfig.build.json
    # `tsc` elides a triple-slash type reference from declaration emit, so the
    # one that makes `Buffer` resolve in a consumer has to be put back after it
    # runs. The tool says which declarations carry it, and refuses if none does.
    node packages/meo-canvas/tools/reference-node-types.mjs

# The addon, optimised, where a release takes it from; `addon` builds debug for
# the working loop. Both write one path, so whichever ran last is what the
# TypeScript surface loads.
[doc("Build the native addon in release mode.")]
addon-release:
    cargo build --locked --release -p meo-canvas-node --features "{{ host_features }}"
    @rm -f {{ addon_path }}
    @cp target/release/{{ lib_name }} {{ addon_path }}
    @echo "built {{ addon_path }} (release)"

# A Linux artefact is a property of its build base: built on `ubuntu-latest` it
# needed glibc 2.35 and loaded on one of six images, in the release container
# 2.28 and all six. No `--target`, which keeps `RUSTFLAGS` off build scripts and
# broke musl; `vulkan` is written, not inherited.
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

    # The image must build for the suffix's triple: under the wrong suffix it
    # would build an x86_64 artefact and stage it under a name npm installs where
    # it cannot load.
    host=$(docker run --rm "$tag" rustc -vV | sed -n 's/^host: //p')
    if [[ "$host" != "$triple" ]]; then
        echo "error: ${tag} builds for ${host}, but {{ suffix }} is ${triple}" >&2
        exit 1
    fi

    # The registry is mounted rather than re-downloaded, so a host cache -- the
    # runner's or a person's -- reaches the build inside.
    mkdir -p "$HOME/.cargo/registry"
    docker run --rm \
        -v "$PWD":/src -w /src \
        -v "$HOME/.cargo/registry":/root/.cargo/registry \
        -e CARGO_TARGET_DIR=/src/target/container/{{ suffix }} \
        "$tag" \
        cargo build --locked --release -p meo-canvas-node --features vulkan

    # The container ran as root, so hand `target/container` and the cargo
    # registry back to the user: root-owned, the cache action's post-job save
    # fails on both. Skipped without Linux or `sudo`, where mounts are already
    # the user's.
    if [[ "$(uname)" == Linux ]] && command -v sudo >/dev/null; then
        sudo chown -R "$(id -u):$(id -g)" target/container "$HOME/.cargo/registry"
    fi

    # Into the target's own path, never over `addon_path`. See `container_addon`.
    out="{{ container_addon }}/{{ suffix }}/meo-canvas.node"
    cp "target/container/{{ suffix }}/release/libmeo_canvas_node.so" "$out"
    echo "built $out in ${tag}"

# Everything a release publishes, packed for this host only: the platform
# package holding the binary and the main package pinning it. The suffix is
# derived from `TARGETS` by this host's os, cpu and libc, never written down.
[doc("Pack the installable tarballs for this host into release/.")]
pack: ensure-deps build-js addon-release
    #!/usr/bin/env bash
    set -euo pipefail
    just _pack-tarballs "$(node packages/meo-canvas/tools/stage-platform-package.mjs --host)" {{ addon_path }}

# The same pack, from an addon built in its release container, with the suffix
# named: `--host` reports the runner, so deriving it would stage a musl binary
# into a glibc package.
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

    # The path a build wrote must be the path this reads: `pack-container` is
    # handed a suffix and can disagree with how a target is spelled, and saying
    # so beats `npm pack` failing later about something else.
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

# Packing is not installing: `exports` can name a path `files` dropped, and a
# platform package's `main` a binary that is not there. Both pack cleanly and
# fail at the first import.
[doc("Pack for this host, then install the tarballs elsewhere and render.")]
verify-pack: pack verify-packed

# The same check against tarballs already in `release/`, without `verify-pack`'s
# rebuild, which would overwrite a container build. It loads the addon here, so
# it answers only for a target sharing the host's libc.
[doc("Install the tarballs already in release/ and render with them.")]
verify-packed:
    node packages/meo-canvas/tools/verify-package.mjs release

# Renders the goldens on `linux-x86_64` in the release container: 8 of 23 differ
# there, the ones with a curve, gradient, blend or glyph. With a name, accepts
# that one fixture's render. `vulkan` is what finds a prebuilt Skia, not a GPU
# request.
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

    # The container ran as root; hand `target/container` and the cargo registry
    # back to the user, as `addon-container` does, or the cache action's
    # post-job save fails on both.
    if [[ "$(uname)" == Linux ]] && command -v sudo >/dev/null; then
        sudo chown -R "$(id -u):$(id -g)" target/container "$HOME/.cargo/registry"
    fi

# What the built addon demands of a machine against what its target promises: a
# diagnostic, since an unversioned symbol has no version to compare.
# `acceptance` is the gate.
[doc("Check the built addon demands no more than its target declares.")]
abi-floor suffix:
    node packages/meo-canvas/tools/check-abi-floor.mjs {{ suffix }} "{{ container_addon }}/{{ suffix }}/meo-canvas.node"

# Loads the built addon on the images its target claims, with nothing installed:
# the gate for Linux, and for musl, which versions no symbols, the only
# evidence.
[doc("Load the built addon on the images its target claims.")]
acceptance suffix:
    node packages/meo-canvas/tools/acceptance.mjs {{ suffix }} "{{ container_addon }}/{{ suffix }}/meo-canvas.node"

# Bumps the npm version and the platform packages pinned at it (`just bump-npm
# prerelease`, `minor`, `premajor --preid rc`), variadic so `--preid` is not
# read as a recipe. Separate from publishing, so a failed publish leaves no
# stranded bump.
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

# Renames a channel's `unreleased.md` to the version it ships under and leaves a
# fresh one. Bump first: the version is read from the manifest now, the same
# place the release reads it.
[doc("Rename a channel's unreleased.md to the version it ships under.")]
cut-notes channel:
    #!/usr/bin/env bash
    set -euo pipefail

    case "{{ channel }}" in
        npm)  version=$(node -p "require('./packages/meo-canvas/package.json').version") ;;
        rust) version=$(cargo metadata --format-version 1 --no-deps --manifest-path crates/meo-canvas/Cargo.toml | node -p "JSON.parse(require('node:fs').readFileSync(0, 'utf8')).packages.find(p => p.name === 'meo-canvas').version") ;;
        *)    echo "error: channel is npm or rust, not {{ channel }}" >&2; exit 1 ;;
    esac

    dir="docs/releases/{{ channel }}"
    from="${dir}/unreleased.md"
    to="${dir}/${version}.md"

    if [[ ! -f "${from}" ]]; then
        echo "error: ${from} does not exist, so there is nothing to cut" >&2
        exit 1
    fi
    # An empty note and a missing one are the same failure: a release page with
    # nothing on it. Caught here rather than at dispatch, because here there is
    # something to do about it.
    if [[ ! -s "${from}" ]]; then
        echo "error: ${from} is empty; a release needs a hand-written note" >&2
        exit 1
    fi
    if [[ -e "${to}" ]]; then
        echo "error: ${to} already exists; ${version} has been cut before" >&2
        exit 1
    fi

    git mv "${from}" "${to}"
    : > "${from}"
    git add --intent-to-add "${from}"
    echo "==> ${from} -> ${to}, and a fresh ${from} left behind"
    echo "    both are staged; commit them before dispatching the release"

# The repository the release workflow runs in, named because a clone with more
# than one remote has no default for `gh`.
release_repo := "l7aromeo/meo-canvas"

# The branch a release is cut from. Nothing here publishes the frozen `v9`
# branch: npm lineages differ by version and dist-tag, and release channels by
# tag prefix, `npm-v*` against `rust-v*`, which `release-tags-check` pins.
release_branch := "main"

# Rehearse a release without publishing: the whole workflow short of the
# registry. Run it after any workflow change, since only the workflow reads its
# own YAML.
[doc("Rehearse an npm release. Builds and validates; publishes nothing.")]
release-npm-dry: (_release_npm "true")

# A version with a hyphen goes to `next` and a bare one to `latest`, and the
# recipe says which before it starts.
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

    # The note the workflow demands, checked before dispatch: a forgotten rename
    # otherwise fails inside the workflow after seven addons build.
    notes="docs/releases/npm/${version}.md"
    if [[ ! -f "${notes}" ]]; then
        echo "error: ${notes} does not exist; the release workflow reads it and refuses without it" >&2
        echo "       run \`just cut-notes npm\` and commit what it stages" >&2
        exit 1
    fi
    # An empty note is the same failure wearing a different sign: the workflow
    # finds the file, publishes, and the release page is blank. `cut-notes`
    # leaves an empty `unreleased.md` behind by design, so this is the shape a
    # forgotten cycle actually takes.
    if [[ ! -s "${notes}" ]]; then
        echo "error: ${notes} is empty; a release page is the note, and there is nothing in it" >&2
        exit 1
    fi

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

# Rehearse a crates.io release without publishing: toolchain, system libraries
# and `cargo publish --workspace --dry-run`, short of the registry.
[doc("Rehearse a crates.io release. Packages and verifies; publishes nothing.")]
release-crate-dry: (_release_crate "true")

# Publishes `meo-canvas-scene`, `-core`, `meo-canvas` and `-cli` in dependency
# order; `meo-canvas-node` is `publish = false`. Irreversible: a yank still
# resolves for any lockfile naming it.
[doc("Publish every publishable crate to crates.io. Not reversible.")]
release-crate: (_release_crate "false")

# The body both spellings share, with `_release_npm`'s four guards. The version
# comes from cargo's metadata, since the crates inherit `version.workspace` from
# a virtual root.
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

    # The note the workflow demands, checked before dispatch, for the reason the
    # npm recipe gives.
    notes="docs/releases/rust/${version}.md"
    if [[ ! -f "${notes}" ]]; then
        echo "error: ${notes} does not exist; the release workflow reads it and refuses without it" >&2
        echo "       run \`just cut-notes rust\` and commit what it stages" >&2
        exit 1
    fi
    # An empty note is the same failure wearing a different sign: the workflow
    # finds the file, publishes, and the release page is blank. `cut-notes`
    # leaves an empty `unreleased.md` behind by design, so this is the shape a
    # forgotten cycle actually takes.
    if [[ ! -s "${notes}" ]]; then
        echo "error: ${notes} is empty; a release page is the note, and there is nothing in it" >&2
        exit 1
    fi

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

# Typechecks both consumer projects against the built package, runs every
# example on both surfaces and diffs every file written: one input through two
# surfaces, which neither suite can check. `addon` as well, or a painter change
# reaches one side only.
[doc("Run every example on both surfaces and compare every byte they wrote.")]
example: build-js addon
    # The example's `node_modules` must exist before it typechecks, and a fresh
    # clone has none. `--frozen-lockfile`, so it installs what `bun.lock` names.
    cd examples/bun && bun install --frozen-lockfile
    ./node_modules/.bin/tsc --noEmit -p examples/bun/tsconfig.json
    rm -rf examples/bun/out examples/rust/out
    cd examples/bun && for source in src/*.ts; do [ "$source" = "src/write.ts" ] && continue; bun run "$source"; done
    cd examples/rust && for source in src/bin/*.rs; do cargo run --quiet --features "{{ host_features }}" --bin "$(basename "$source" .rs)"; done
    @test -d examples/bun/out || { echo "error: the JavaScript surface wrote nothing to compare"; exit 1; }
    @diff -rq examples/bun/out examples/rust/out \
      || { echo "error: the two surfaces did not write the same bytes; each line above names a file they disagree on"; exit 1; }
    @echo "both surfaces wrote the same bytes in $(find examples/bun/out -type f | wc -l | tr -d ' ') files"

# The conformance tools in measuring order, written out: `browser.mjs` and
# `png.mjs` share the directory and measure nothing.
conformance_tools := "ellipsis gradients flex borders dotted blend boxshadow shadowextent objectfit objectfit-overflow grid mincontent replacedinsets replacedratio overflowposition abspositioned aspectratio textaligndirection boxsizing paintorder flexratiocross flexbasiscollapse ratiostretchmain"

# Not in `ci`: a re-measurement is a diff someone reads, not a suite going red
# on whichever machine updated Chrome first. Named, one tool rewrites one table.
# Every page asserts its font loaded and derives its sample points.
[doc("Re-measure Chrome with Playwright and rewrite the conformance tables.")]
conformance tool="": ensure-deps ensure-browser
    #!/usr/bin/env bash
    set -euo pipefail
    requested="{{ tool }}"
    tools="{{ conformance_tools }}"
    if [ -n "$requested" ]; then
      # A name nobody predicted is not a request to measure nothing. A `for`
      # over an empty list would exit 0 having done nothing, which is the
      # failure a misspelling deserves least: the person is told it worked and
      # goes looking at the table that did not move.
      case " $tools " in
        *" $requested "*) tools="$requested" ;;
        *)
          echo "error: no conformance tool named '$requested'" >&2
          echo "the tools are: $tools" >&2
          exit 1
          ;;
      esac
    fi
    for tool in $tools; do
      WRITE=1 node "packages/meo-canvas/tools/conformance/$tool.mjs"
    done

# What the npm package publishes as its types, which only this reads: prettier
# parses without checking. Not `check`, since `-check` here names the reporting
# variant of a rewriting recipe.
[doc("Type-check the shipped TypeScript surface.")]
typecheck: ensure-deps
    ./node_modules/.bin/tsc --noEmit -p packages/meo-canvas/tsconfig.json
    ./node_modules/.bin/tsc --noEmit -p packages/meo-canvas/tsconfig.test.json

# Lifts the fenced examples out of TypeScript doc comments into a compiled file
# under `src`, which `typecheck` covers, since TypeScript compiles nothing
# inside a comment.
[doc("Emit the TypeScript doc examples as compilable code.")]
doc-examples:
    node packages/meo-canvas/tools/generate-doc-examples.mjs

# Rewrite the two places a target is named that are not `TARGETS`.
[doc("Regenerate PLATFORM_PACKAGES and optionalDependencies from TARGETS.")]
platform-packages:
    node packages/meo-canvas/tools/generate-platform-packages.mjs

# Regenerates to a disposable path and diffs, since git reports a file changed
# whether untracked, written or staged.
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

# vitest from `node_modules` rather than an npm script, as prettier and tsc are:
# this file names every command.
[doc("Run the JavaScript tests.")]
test-js: ensure-deps addon
    # `addon` as well: these read the compiled `.node`, and a stale one reads as
    # an encoder defect.
    ./node_modules/.bin/vitest run

# The JavaScript suite with the Rust half's 90% floor, a separate recipe as
# `coverage` is. The floor and exclusions live in `vitest.config.mts`.
[doc("Measure JavaScript coverage and fail below the 90% floor.")]
coverage-js: ensure-deps addon
    ./node_modules/.bin/vitest run --coverage

# Emits the TypeScript arena tables from `arena_group!` in
# `crates/meo-canvas-node/src/arena.rs`, so there is one table, not two agreeing
# by inspection. Static rather than exported at runtime, since the encoder reads
# it per property per node.
[doc("Emit the TypeScript arena tables from the Rust tables.")]
arena-tables:
    node packages/meo-canvas/tools/generate-arena-tables.mjs

# Regenerates the round trip's expected bytes: one case per arena property plus
# one setting all, keyed by Rust field name.
[doc("Regenerate the arena property cases the encoder is checked against.")]
arena-cases:
    cargo test -p meo-canvas-node --lib -- --ignored --exact \
      arena::cases::tests::emit_arena_cases

# Regenerates to a disposable path and diffs, for `arena-tables-check`'s reason.
[doc("Fail when the arena property cases are out of date.")]
arena-cases-check:
    @mkdir -p target
    @MEO_ARENA_CASES="$PWD/target/arena-cases.check.json" cargo test -q \
      -p meo-canvas-node --lib -- --ignored --exact \
      arena::cases::tests::emit_arena_cases > /dev/null
    @diff -u fixtures/arena-cases.json target/arena-cases.check.json \
      || { echo "error: the arena cases are stale; run \`just arena-cases\` and commit the result"; exit 1; }

# Regenerates to a disposable path and diffs: a stale index writes the right
# number of slots into the wrong field. `diff`, not `git status`, which reports
# untracked, written and staged alike; it also fails on an absent file.
[doc("Fail if the checked-in arena tables have drifted from the Rust.")]
arena-tables-check:
    @mkdir -p target
    @node packages/meo-canvas/tools/generate-arena-tables.mjs target/arena-tables.check.ts
    @diff -u packages/meo-canvas/src/generated/arena-tables.ts target/arena-tables.check.ts \
      || { echo "error: the arena tables are stale; run \`just arena-tables\` and commit the result"; exit 1; }

# Emits the wire-enum tables from the 26 `wire_enum!` blocks in
# `crates/meo-canvas-scene/src`: a hand copy that missed an inserted variant
# would decode as a different variant, not fail.
[doc("Emit the TypeScript wire-enum tables from the Rust declarations.")]
arena-enums:
    node packages/meo-canvas/tools/generate-arena-enums.mjs

# Regenerates to a disposable path and diffs; `$PWD`, since a relative path
# resolves wherever the recipe's shell started.
[doc("Fail if the checked-in wire-enum tables have drifted from the Rust.")]
arena-enums-check:
    @mkdir -p target
    @node packages/meo-canvas/tools/generate-arena-enums.mjs "$PWD/target/arena-enums.check.ts"
    @diff -u packages/meo-canvas/src/generated/arena-enums.ts target/arena-enums.check.ts \
      || { echo "error: the wire-enum tables are stale; run \`just arena-enums\` and commit the result"; exit 1; }

# The TypeScript format table, emitted from the Rust one: browsers accept both
# `image/x-icon` and `image/vnd.microsoft.icon`, so a transcribed table could be
# wrong and still serve. A Rust test, since the values exist only at runtime.
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

# Golden fixtures, byte for byte with no tolerance, with one registered font and
# every other family refused. A failure writes `actual.png` and `diff.png` under
# `target/fixtures/<name>/`. Not in `ci`: `test` already runs the harness.
[doc("Render every fixture and compare it against its committed image.")]
fixtures:
    cargo test -p meo-canvas-core --test fixtures

# Rewrites one fixture's expected image. No bulk form: accepting every
# difference at once is how a regression becomes a commit.
[doc("Accept one fixture's current render as its expected image.")]
fixtures-accept name:
    MEO_FIXTURE_ACCEPT={{ name }} cargo test -p meo-canvas-core --test fixtures

# Rewrites the percentage fixture's scene from its Rust source: the only check
# on what a percentage means, since compared bytes come from one number and
# agree regardless.
[doc("Rewrite the percentage fixture's scene from its source.")]
percentage-fixture:
    @cargo test -q -p meo-canvas --test percentage_fixture -- --ignored --exact emit_percentage_scene > /dev/null
    @echo "wrote fixtures/percentages/scene.mcs; run \`just fixtures-accept percentages\` if the picture should move"

# Rewrites every golden's scene from
# `crates/meo-canvas/tests/fixture_scenes.rs`, where a test asserts each encodes
# to the committed bytes, so a codec change is a re-run.
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

# Builds the TypeScript reference and fails on a dead link, a type exported
# nowhere, or any undocumented member:
# `packages/meo-canvas/tools/typedoc/build.mjs` fails above its baseline of 0.
# TypeDoc pins its own TypeScript.
[doc("Build the JavaScript API reference and fail on a dead link or a new undocumented member.")]
docs-js: build-js
    #!/usr/bin/env bash
    set -euo pipefail
    tool=packages/meo-canvas/tools/typedoc
    # Presence by the package, not the `.bin` shim, whose filename differs per platform.
    test -f "$tool/node_modules/typedoc/package.json" || bun install --cwd "$tool" --frozen-lockfile
    node "$tool/build.mjs"

# A bare `#N` resolves, which is worse than a dead link: a reader lands
# somewhere real and wrong.
[doc("Fail if a comment names an issue without naming its repository.")]
issue-refs:
    node packages/meo-canvas/tools/issue-refs.mjs

# A probe rots silently -- a renamed file, an `#[ignore]`, a deleted foundation
# row -- and leaves a workaround nobody can retire. `AGENTS.md`'s convention,
# enforced.
[doc("Fail if a [WORKAROUND] site has lost its probe.")]
workaround-probes:
    node packages/meo-canvas/tools/workaround-probes.mjs

# `fixtures.rs` compares bytes and never opens a note, so this reads that the
# three fields are there and say something; whether they are true is not
# testable.
[doc("Fail if a fixture has no notes.json, or one missing a field.")]
fixture-notes:
    node packages/meo-canvas/tools/fixture-notes.mjs

# A static check on the conformance tools, which nothing in the gate executes,
# so an unguarded one could rewrite a fixture unnoticed.
[doc("Fail if a conformance tool writes a fixture without a WRITE guard.")]
conformance-writes:
    node packages/meo-canvas/tools/conformance-writes.mjs

# CI runs `portable` and `native` as separate jobs and never `ci-steps`, so this
# is the only check of the composition. It asks `just`, and floors both lists,
# since two empty lists compare equal.
[doc("Fail if ci-steps and the two lists have drifted apart.")]
gate-lists-check:
    node packages/meo-canvas/tools/gate-lists.mjs

# GitHub evicts silently: a dead 3.53 GiB cache once made another leg cold-build
# every run. Needs a token -- `GITHUB_TOKEN` in CI, `gh auth token` locally --
# and fails without one only in CI.
[doc("Fail if the Actions cache is near the limit; name the refs holding it.")]
cache-budget-check:
    node packages/meo-canvas/tools/cache-budget.mjs

# A dry run unless given `--delete`, which only
# `.github/workflows/cache-prune.yml` passes. Not in `ci-steps`: a remedy inside
# a gate changes what it measures.
[doc("Print the superseded cache entries; --delete removes them.")]
cache-prune *args:
    node packages/meo-canvas/tools/cache-prune.mjs {{ args }}

# A fix merged upstream and in no release is invisible to the probes, which go
# red only on a bump. Reads every upstream reference in the tree's comments. Not
# in `ci-steps`: it asks about the world, not the diff.
[doc("Report what upstream has done about the defects this tree works around.")]
upstream-watch:
    node packages/meo-canvas/tools/upstream-watch.mjs

# `docs.yml` publishes no reference for a tag its filter rejects, silently, so
# this runs a tag of each channel through that exact `grep -Eq`, with the
# prefixes read from the workflows.
[doc("Fail if the release tag prefixes and the docs filter disagree.")]
release-tags-check:
    node packages/meo-canvas/tools/release-tags.mjs

# Asserts the set of module-private declarations with no doc, not its size, so a
# doc moved to another declaration is caught. The baseline names each exception.
[doc("Fail if a module-private declaration lost its doc comment.")]
private-docs:
    node packages/meo-canvas/tools/private-docs.mjs

# Compare v9's prop surface against this renderer's. Not in `ci`: it reads
# `../meo-canvas-old`, which a CI machine does not hold.
[doc("Print v9's prop surface against this renderer's, naming v9's tag and commit.")]
surface-report:
    node packages/meo-canvas/tools/surface-report.mjs

# `meo-canvas-core` promises it is runtime-free; `-e normal` checks what a
# consumer links, and `--all-features` any feature someone might enable.
# Runtimes and reactors only, not traits or macros.
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

# An instrument, not a gate: a number that varies with the machine cannot fail a
# build honestly.
[doc("Benchmark both surfaces: criterion, then throughput and memory.")]
bench: bench-rust bench-js

# The pipeline's own timings, in Rust, through criterion.
[doc("Benchmark the core pipeline (criterion).")]
bench-rust:
    cargo bench -p meo-canvas-core

# Whether a long-lived Node process settles back after thousands of renders;
# `--expose-gc` separates retained from uncollected. On the release addon, since
# debug numbers describe nothing shipped.
[doc("Benchmark the Node surface: throughput, rss, heap, peak, idle.")]
bench-js: ensure-deps build-js addon-release
    node --expose-gc packages/meo-canvas/tools/bench.mjs

# Lists test names matching a pattern -- they are sentences -- by compiling,
# never running. No pattern lists all; no match exits 1 and names the pattern.
# Quotes in the pattern break it.
[doc("List test names matching a pattern, without running any. No pattern lists all.")]
tests pattern="":
    #!/usr/bin/env bash
    set -euo pipefail
    pattern='{{ pattern }}'
    # stderr is kept, since `Running tests/<file>.rs` goes there and names each
    # test's file. `|| true`, so an empty match reaches the message below rather
    # than `set -e`.
    found=$(cargo test --workspace -- --list 2>&1 | awk -v pat="$pattern" '
        /^ *(Running|Doc-tests)/ {
            where = $0
            # A pattern naming the file lists everything in it, since test names
            # rarely repeat their file's subject.
            whole = tolower(where) ~ tolower(pat)
            next
        }
        $0 ~ /: test$/ && (whole || tolower($0) ~ tolower(pat)) {
            if (where != shown) { print where; shown = where }
            print "  " $0
        }' || true)
    if [[ -z "$found" ]]; then
        echo "no test name matches: $pattern" >&2
        exit 1
    fi
    echo "$found"

# The rung between one test and the gate, about a minute warm, printing what it
# did not run -- `portable` and `native` minus this, read from `just --dump`.
# `cargo test --workspace` is 88% of the time and the only behavioural check.
[doc("The fast rung: cheap checks plus one test pass, and a list of what it skipped.")]
precheck: layout-check gate-lists-check issue-refs typecheck fmt-check
    # `-- -D warnings` is clippy's whole gate: without it this exits 0 on every
    # lint.
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    @just --dump --dump-format json | node -e ' \
      const dump = JSON.parse(require("fs").readFileSync(0, "utf8")).recipes; \
      const deps = name => dump[name].dependencies.map(one => one.recipe); \
      const gate = new Set([...deps("portable"), ...deps("native")]); \
      for (const ran of deps("precheck")) gate.delete(ran); \
      const partly = { \
        "lint-check": "clippy ran here; ESLint did not", \
        test: "one cargo run here; the three GPU-feature runs did not", \
      }; \
      for (const name of Object.keys(partly)) gate.delete(name); \
      console.log("\nRAN IN PART, so a green here is not a green there:"); \
      for (const [name, what] of Object.entries(partly)) console.log("  " + name + " -- " + what); \
      console.log("\nNOT RUN AT ALL, and each of these can fail on its own:"); \
      console.log("  " + [...gate].join("  ")); \
      console.log("\n`just ci` is the gate. This was not it."); \
    '

# Remove all build output.
clean:
    cargo clean
