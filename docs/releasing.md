# Releasing

This document is the authority on how meo-canvas is released: the two channels,
the targets, the Linux artefacts, the platform packages, what triggers a publish
and how the reference site follows it. The `justfile` recipes and the workflows
are the authority on the exact steps; this explains what they rely on.

**Nothing is released without an explicit instruction**, and the version is the
maintainer's decision. What an agent may and may not run is listed under
"Agents never publish" in [`AGENTS.md`](../AGENTS.md).

## Two channels, numbered independently

The npm package is **`meo-canvas`**, unscoped, continuing v9's npm lineage. The
cargo crate is `meo-canvas` too and versions independently, from `0.1.0`.
**The two numbers are not comparable and are never synchronised for tidiness.**
Tags carry the channel: `npm-v*` and `rust-v*`.

Release notes are hand-written per channel at
`docs/releases/{npm,rust}/<version>.md`, against the previous release _of that
channel_. They accumulate in `docs/releases/{npm,rust}/unreleased.md`, written
in the commit that makes the change, and the release renames that file to the
version. `just release-npm` and `just release-crate` refuse to dispatch when the
version's note is missing. [`releases/README.md`](releases/README.md) is the
authority on the headings and on what belongs under each.

**The `v*` tags from before the channel prefix are not rewritten, and a bare
`v10.0.0` is still a valid npm tag.** `docs.yml`'s filter is `^(npm-)?v...` on
purpose, because `workflow_dispatch` backfills exactly those releases, and
`release-tags-check` asserts it. Do not narrow it to `npm-v`.

**The tag carries the channel prefix and the site directory does not.**
`site-index.mjs` parses directory names as `^v(\d+)\.(\d+)\.(\d+)`, so a
directory named `npm-v10.0.0` would deploy, never be listed and never advance
`latest/`, with nothing red. The asymmetry is deliberate.

**Installing two generations side by side uses an npm alias**, which lets the
consumer choose the local name: `npm install meo-canvas-v9@npm:meo-canvas@9`.
The alias goes on whichever generation is not the bare install.

## The targets

    linux-x64-gnu     linux-arm64-gnu
    linux-x64-musl    linux-arm64-musl
    darwin-arm64      win32-x64         win32-arm64

**`darwin-x64` is excluded**: Apple no longer supports Intel, and building it
needs `macos-13`, which GitHub is retiring. The rule the set comes from is
everything within our control, with anything that is not named as such rather
than left a silent gap.

**Adding a target is one row in `TARGETS`** in
`packages/meo-canvas/tools/stage-platform-package.mjs`, and nothing else. The
chain `TARGETS` -> build matrix -> `optionalDependencies` ->
`PLATFORM_PACKAGES` -> ABI floors exists to provide that; any second edit
needed is a defect in the chain.

**A target is named three times, and a test asserts all three agree.**
`TARGETS` is what a release builds, `optionalDependencies` what an install
fetches, `PLATFORM_PACKAGES` what a process resolves. Any two agreeing without
the third is its own silent failure: unresolved renders nothing with the binary
on disk; unbuilt fails every install; unpinned works only in a checkout, which
is where it would be tested.

## A Linux artefact is a property of its build base

The same source built on `ubuntu-latest` demands glibc 2.35 and fails to load on
five of the six images the package claims; built in
`containers/Dockerfile.glibc` it demands 2.28 and loads on all six. Build Linux
artefacts in the containers.

The musl image is its own piece of work rather than a variant: Rust's musl
targets default to `crt-static`, which cannot produce a `cdylib`; rust-skia
ships no prebuilt Skia for musl, so the image compiles all of it; and an
explicit `--target` must **not** be passed, because cargo then builds build
scripts without `RUSTFLAGS` and `skia-bindings`' script cannot `dlopen`
libclang.

**Two instruments, and they are not substitutes.** `just abi-floor` reads the
built `.node`'s undefined symbols, fails if it demands more than `TARGETS`
declares, and prints measured beside declared, because a floor declared too
high fails nothing. `just acceptance` loads the addon in six images with nothing
installed. A ceiling compares version tags and an unversioned symbol has none,
so **the floor diagnoses and the load decides.** The musl pair declare no
floors, because musl does not version its symbols; the load is the whole of the
evidence there.

## The addon ships beside the package, not inside it

One package per target carries one binary, named in `optionalDependencies` with
its own `os`, `cpu` and `libc`. A postinstall download is the rejected
alternative: it needs the network at install time and breaks offline installs,
locked-down CI and `--ignore-scripts`.

`resolveAddon` looks in three places in order -- `MEO_CANVAS_ADDON`, the
`.node` beside the package in a working tree, then the platform package -- and
a failure names all three. So a checkout tests what it just built, and
`just addon` needs no reinstall to take effect.

**Packing is not installing.** `exports` can name a path the `files` allowlist
dropped, and a platform package's `main` can name a binary that is not there;
both pack cleanly and fail at the first import. `just verify-pack` installs the
tarballs somewhere that is not this repository and renders through them.

## What triggers a publish

`release.yml` runs on **`workflow_dispatch` only**: a push is how code arrives,
and publishing is a decision about code that already arrived. `just release-npm`
refuses on a dirty tree, off the branch, or with unpushed commits. `dry_run`
defaults to **true**, and a dry run is what to dispatch after any change to the
workflow, since only the workflow reads its own YAML.

**A version containing a hyphen goes to the `next` dist-tag**, never `latest`:
a semver range never matches a prerelease, so nobody reaches it without naming
it.

Seven runners build one addon each. The publish job refuses unless all seven
tarballs are present, then publishes them **before** the main package, which
pins them at an exact version.

**The tag and the GitHub release come last, after the registry has accepted
everything.** A tag pushed before a publish that then fails leaves a version
number that can never be reissued.

## A new platform package cannot start on OIDC

The platform packages are scoped: **`@meo-canvas/<suffix>`**.
`optionalDependencies` in `packages/meo-canvas/package.json` is the authority
on the names, and the suffixes come from `TARGETS`.

A trusted publisher is configured on a package's settings page, and a package
never published has none. So the first release of a new `@meo-canvas/<suffix>`
is refused -- correctly, with nothing published and no tag -- and **a dry run
cannot see it, because `--dry-run` never authenticates.**

Before releasing a version that adds a platform package, run
`npm view @meo-canvas/<suffix> version` on the name `optionalDependencies`
gives: an `E404` means bootstrap first, and a version means it is already on
OIDC. A misspelt name answers `E404` too, for packages that exist. Bootstrap by
hand with `npm publish --access public --tag next <tgz>`, **one tarball at a
time with a pause between**: several new names in a few seconds trip npm's spam
gate.

**Every platform package must exist before the main one.** npm and pnpm skip an
optional dependency whose `os`/`cpu`/`libc` excludes the host without asking
the registry, but **yarn resolves all seven before installing any**, and a 404
on one fails the whole install (measured with yarn 4.10.3). The window between
publishing the main package and finishing a bootstrap is one in which every
yarn install fails.

## The reference site

`docs.yml` builds the TypeDoc reference on every pull request that touches the
surface and publishes it when a release is published, which `release.yml` does
only after every package is on npm. **It still polls `npm view` before
deploying**, because published and installable are separated by propagation;
the wait loop is there for that.

One directory per version, prereleases included. **`latest/` follows the newest
_stable_ version**, as npm's dist-tag does: `docs.yml` picks it and
`site-index.mjs` refuses a prerelease stamp.
