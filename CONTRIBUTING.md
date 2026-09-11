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

`just ci` takes a lock, so two of them cannot run in one tree at once. Run it before opening a pull
request. Each recipe below is also one you can run alone while you work.

No gate reads the release notes, so a green `just ci` is not a finished pull request — see
[Release notes](#release-notes) below, which every pull request fills or says why it does not.

**The table is a selection rather than the list.** `portable` and `native` in the `justfile` are
what `just ci` runs and are the authority on it; they name several recipes this table does not. A
full copy here would go stale with nothing to report it, which is why there is not one.

| Recipe                                                                                    | What it is for                                                                               |
| ----------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `fmt-check`                                                                               | rustfmt on the pinned nightly, then prettier over JS, TS and Markdown                        |
| `lint-check`                                                                              | clippy with `-D warnings`, across the workspace, the addon and `examples/rust`               |
| `typecheck`                                                                               | the shipped TypeScript surface and its tests                                                 |
| `test` / `test-js`                                                                        | the two test suites                                                                          |
| `coverage` / `coverage-js`                                                                | a 90% floor on each side; `coverage` carries a second at 60% of regions on the Neon boundary |
| `docs` / `docs-js`                                                                        | a rustdoc warning fails; so does a dead link or a newly undocumented member                  |
| `doc-examples-check`                                                                      | the `ts` fences in both READMEs are lifted into a module and compiled                        |
| `example`                                                                                 | runs all nine examples on both surfaces and compares every byte                              |
| `arena-tables-check`, `arena-enums-check`, `media-types-check`, `platform-packages-check` | generated files must match their sources                                                     |
| `layout-check`                                                                            | no `mod.rs` anywhere under `crates/`                                                         |
| `runtime-free`                                                                            | fails if an async runtime is anywhere in the dependency tree                                 |
| `unused`                                                                                  | `cargo machete` — dependencies declared in a `Cargo.toml` that nothing imports               |
| `audit`                                                                                   | `cargo audit` over the lockfile — not in `ci`, run once per push by CI                       |

Three are deliberately outside `just ci`. `audit`, because an advisory is a fact about the lockfile
rather than the platform and three runners would buy three copies of one answer. `net-check`,
because the `net` feature is off by default, so no run of the gate compiles the code it covers.
And `conformance`, which re-measures Chrome with Playwright and rewrites the comparison tables — a
re-measurement should arrive as a diff a person reads, so a clone that never runs it never downloads
a browser.

## Where a test goes

Five layers. Pick by the question you are answering, not by which is quickest to write — the
quickest to write is the unit test, and it is the one that proves the least about a renderer.

| Layer           | The question it answers                   | Where it lives                                                                  |
| --------------- | ----------------------------------------- | ------------------------------------------------------------------------------- |
| **unit**        | does this function do what it says        | `mod tests` in the file itself                                                  |
| **integration** | do the stages still agree across a seam   | `crates/<crate>/tests/*.rs`                                                     |
| **golden**      | is the picture right                      | `fixtures/<name>/`, compared byte for byte                                      |
| **conformance** | does this match what a browser does       | a table under `crates/meo-canvas/tests/assets/chrome/`, read by a `chrome_*.rs` |
| **doctest**     | does the documented example still compile | a fenced example in the doc comment                                             |

**Paint is verified by comparison, never by assertion.** Executing a fill proves the line ran, not
that the pixels are right, so anything about drawing belongs in a golden rather than in an `assert`.

**Which crate.** A test in `crates/meo-canvas-core/tests/` uses the core and nothing above it —
that holds for every file there today. A test in `crates/meo-canvas/tests/` may use either, and
several use the core directly because what they are measuring is layout rather than the builder
surface. If your test never mentions `meo_canvas::`, it probably belongs in the core's directory.

**Finding the existing one first.** `cargo test --workspace -- --list` prints every test name, and
the names here are sentences — `a_font_without_a_family_exits_five_and_says_the_shape` — so
grepping that list for a word from your question usually lands on the file. The conformance readers
are split across two crates and their names do not always match the table they read, so grep the
table name too.

**Before you trust a new test, make it fail.** Mutate the thing it guards and watch it go red. A
green result from a check never shown to fail says nothing, and it is the most expensive kind of
test here — it looks like coverage and is not.

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

## Release notes

**Every pull request fills one.** The two channels each keep an accumulating
note — `docs/releases/npm/unreleased.md` and `docs/releases/rust/unreleased.md`
— and the entry lands in the same commit as the change it describes. Cutting a
release renames that file to the version it ships under, so nobody has to know
the version while writing the entry.

**At the start of a cycle the file is empty, and that is not a mistake.**
`just cut-notes <channel>` renames the last one away and leaves an empty file
in its place, so the path always exists and the first entry of a cycle is an
ordinary edit rather than a decision about what a new file should look like.

Append the entry at the end of its Keep a Changelog heading, and cite the issue
it answers in parentheses at the end of it, because the issue is where the
measurement and the argument live and the release page should not repeat them.
**Answers rather than closes**: an issue compensated by a workaround stays open
until the upstream fix ships, and its entry cites it exactly the same way. A
change with no issue cites nothing — do not open one so the entry has a number.

**A breaking change is marked in the entry rather than given a section**, with
**Breaking.** at the front and what to write instead. `docs/releases/README.md`
says why: Keep a Changelog has no such heading, and inventing one splits a
removal across two places.

Which channel is decided by where the change lands, not by which surface you
were thinking about. A change under `crates/meo-canvas-core/` reaches both, so
it gets an entry in each, written in that channel's vocabulary — `aspectRatio`
on the npm side, `aspect_ratio` on the crate side. A change under
`packages/meo-canvas/src/` reaches npm alone. A change to a gate, a test, a
workflow or this file reaches no caller at all. Say so in one line of the pull
request description — "no release note: nothing a caller meets" — rather than
inventing an entry, because a release page listing gate edits tells someone
installing the package nothing, and because a reviewer cannot tell a change
that needed no entry from one that forgot.

**Write it as the person who hit it met it**, not as the fix. "A percentage
height under a ratio parent came out at zero" is the symptom someone searches
for; "resolve the ratio before the percentage rule" is the commit subject and
belongs there. `docs/releases/README.md` has the headings, their order and what
belongs under each — they are Keep a Changelog's.

### When your change contradicts an entry that is already there

**An unreleased entry has no reader.** Nobody has installed the version it
describes, so there is nothing to correct and nobody to tell. Edit the entry.
Do not append a second one that undoes the first, and do not write "previously
announced", "as of this release" or any other sentence that only makes sense to
someone who read a file that was never published.

Read the file before you append. The entry you contradict is almost always the
one nearest your own area, which is also the one you are least likely to reread
because you already know what it says.

Three shapes, and they are decided by what a caller ends up with:

- **Your change completes something an entry called a known limitation.** Edit
  that entry to drop the limitation. **An entry replaces it only where a caller
  is left with something to know**: where the limitation described a defect
  introduced and removed inside the same unreleased cycle nobody ever met it,
  so the page says nothing at all; where something in the same family still
  diverges, the replacement says which, in the caller's terms. This has already
  happened here both ways -- the aspect-ratio entry ended by saying one case was
  "not fixed here" and named the issue, and the next pull request fixed that
  issue before either had shipped; later that entry carried a regression the
  compensation introduced, and the fix for it removed the paragraph rather than
  answering it. Left alone, one release page would have told a caller a thing
  was broken and then, four paragraphs later, that it was not.

  **Read what leaves with the paragraph.** That regression paragraph closed by
  defending the change, and the defence carried the only measured figure on
  either page for what the compensation buys. Deleting it whole would have
  taken that with the limitation it was arguing against -- so a figure the rest
  of the entry still needs is re-homed on its own merits, in a sentence that
  stands without the limitation it was written to answer.

- **Your change supersedes an unreleased one** — a different fix for the same
  defect, a renamed API, a reworked option. Rewrite the original entry to
  describe what actually ships. The release page describes the version, not the
  route the repository took to it.

- **Your change reverts an unreleased one.** Delete the entry. Nothing shipped,
  so there is nothing to announce, and a pair of entries adding and removing the
  same thing is noise a reader has to resolve.

**A released note is never edited**, and the rule above is why: a version-named
file is what somebody already read. A defect found in one is fixed in the next
release's note, naming the version it was wrong in.

**Two pull requests appending at once will conflict**, both at the end of the
same section, and that conflict is doing its job. Resolve it by keeping both
entries in merge order — never by taking one side of the file wholesale, which
is how an entry disappears with nothing red to say so.

## Style

Comments here say **why**, not what. The code says what it does; a comment earns its place by
recording the thing that is not visible — what was measured, what was tried and did not work, what
the alternative cost. A comment that restates the line below it will be asked about in review.

Commit subjects take a Conventional Commits type and a scope naming the part of the tree they touch
— `fix(layout):`, `test(codec):`, `docs(releases):` — and say what changed, in the imperative, under
about seventy characters. The body says why: what was wrong, how it was found, what else was tried,
and what is true now that was not before. Prose, not bullets. `git log` is the reference and
`AGENTS.md` has the long form.

## Reporting things

- A bug or a wrong picture — open an issue with the smallest scene that shows it. A rendered PNG
  helps more than a description of one.
- A security issue — **do not open a public issue.** See [SECURITY.md](SECURITY.md).
- Anything about conduct — see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md), which applies here.
