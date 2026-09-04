# Contributing

Thanks for looking. This document is about running the checks and reading their output; if
something here is wrong or missing, that is worth an issue on its own.

## What this repository is

Two public surfaces that are siblings rather than layers: a **Rust crate** and an **npm package**.
Both build the same scene and hand it to the same core, so neither can grow a capability the other
cannot reach. A change that adds something to one usually has to add it to the other.

```
crates/            the Rust workspace — scene, core, the facade crate, the CLI, the Node addon
packages/meo-canvas/  the npm package: TypeScript, plus the tools that check it
examples/          the same nine scenes written twice, once per surface
```

`AGENTS.md` is the long-form argument for why things are the way they are. You do not need to read
it to send a patch, but it is where the reasoning lives when a review says "because".

## Getting set up

- **Rust** — the toolchain is pinned in `rust-toolchain.toml` (1.98.0 with `rustfmt`, `clippy` and
  `llvm-tools-preview`). With `rustup` installed, it is fetched for you the first time you build.
- **Node 22 or newer**, and **[bun](https://bun.sh)** — `bun.lock` is the lockfile and
  `packageManager` names the version. `npm` is used deliberately in two places (packing and the
  consumer-side install check) and nowhere else.
- **[just](https://just.systems)** — every workflow in this repository is a `just` recipe, and CI
  runs the same ones you do.
- **Docker**, only if you are building Linux artefacts or running the container checks.

`just --list` shows the recipes with a line each.

> **The first build is slow, and it is not a hang.** Skia's static libraries are fetched or
> compiled depending on your target and which features are on, and `target/` grows to gigabytes —
> a tree that has built debug, release and the Linux containers reached 17 GB here. After the first
> one the cache makes it ordinary.

```bash
just build     # the workspace and the native addon for this platform
just ci        # everything CI runs, in one go
```

## The loop

```bash
just addon        # build the native addon (debug) into packages/meo-canvas/
just build-js     # compile the TypeScript package into dist/
just test         # the Rust tests
just test-js      # the JavaScript tests
just fmt          # format Rust, JavaScript, TypeScript and Markdown (rewrites the tree)
just lint         # clippy with autofix
```

`just test-js` needs the addon, so build it first. `vitest` does not typecheck — a change whose
whole content is a type needs `just typecheck` as well, and a green suite says nothing about it.

## The gates

`just ci` runs all of these and takes a lock, so two of them cannot run in one tree at once. Run it
before opening a pull request. Each is also a recipe you can run alone while you work.

| Recipe                                                                                    | What it is for                                                                 |
| ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `fmt-check`                                                                               | rustfmt on the pinned nightly, then prettier over JS, TS and Markdown          |
| `lint-check`                                                                              | clippy with `-D warnings`, across the workspace, the addon and `examples/rust` |
| `typecheck`                                                                               | the shipped TypeScript surface and its tests                                   |
| `test` / `test-js`                                                                        | the two test suites                                                            |
| `coverage` / `coverage-js`                                                                | a 90% floor on each side                                                       |
| `docs` / `docs-js`                                                                        | a rustdoc warning fails; so does a dead link or a newly undocumented member    |
| `doc-examples-check`                                                                      | the `ts` fences in both READMEs are lifted into a module and compiled          |
| `example`                                                                                 | runs all nine examples on both surfaces and compares every byte                |
| `arena-tables-check`, `arena-enums-check`, `media-types-check`, `platform-packages-check` | generated files must match their sources                                       |
| `layout-check`                                                                            | no `mod.rs` anywhere under `crates/`                                           |
| `runtime-free`                                                                            | fails if an async runtime is anywhere in the dependency tree                   |
| `unused`                                                                                  | `cargo machete` — dependencies declared in a `Cargo.toml` that nothing imports |
| `audit`                                                                                   | `cargo audit` over the lockfile — not in `ci`, run once per push by CI         |

Two are deliberately outside `just ci`: `audit`, because an advisory is a fact about the lockfile
rather than the platform and three runners would buy three copies of one answer; and `conformance`,
which re-measures Chrome with Playwright and rewrites the comparison tables. A re-measurement should
arrive as a diff a person reads, so a clone that never runs it never downloads a browser.

## Generated files are checked in

Several files are emitted from a source of truth and committed: the arena tables and wire enums come
from the Rust declarations, the doc examples come from the comments and the READMEs, the platform
package list comes from one declaration of the targets. Edit the source, run the generator
(`just arena-tables`, `just doc-examples`, `just platform-packages`), and commit the result. The
matching `*-check` recipe is what fails when you forget.

## The golden fixtures

`just fixtures` renders every fixture and compares it against a committed PNG. **There is no
tolerance.** A fixture that differs by one pixel fails, and that is the point: the comparison is
against `expected.<os>-<arch>.png` where a platform is measurably different and `expected.png`
otherwise, so a variant existing is a claim about that platform and a variant missing is a claim too.

So **a diff to a fixture is a diff to the picture.** If one fails, look at the render before you
look at the test. `just fixtures-accept <name>` rewrites one fixture's expected image from what it
currently draws — that is how a deliberate rendering change lands, and it belongs in the same commit
as the change that caused it, with the reason in the message.

Do not accept a fixture to make a build green. If you cannot say what changed in the picture and
why, the fixture is telling you something.

## Both surfaces have to agree

`just example` runs the same nine scenes through the Rust crate and the npm package and compares
every byte they wrote. It builds the addon first on purpose: without that, a change reaches one
surface and not the other and the comparison reports a divergence between a stale binary and a fresh
one, which reads exactly like a real defect.

If you add a capability to one surface, the honest question is what the other one now lacks.

## Style

Comments here say **why**, not what. The code says what it does; a comment earns its place by
recording the thing that is not visible — what was measured, what was tried and did not work, what
the alternative cost. A comment that restates the line below it will be asked about in review.

Commit subjects are sentences describing what the commit does, not `feat:` or `fix:` prefixes.
`git log` is the reference.

## Reporting things

- A bug or a wrong picture — open an issue with the smallest scene that shows it. A rendered PNG
  helps more than a description of one.
- A security issue — **do not open a public issue.** See [SECURITY.md](SECURITY.md).
- Anything about conduct — see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md), which applies here.
