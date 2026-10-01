# meo-canvas -- guidance for agents and contributors

Renders declarative scene trees to images. A caller describes boxes, rows, text,
images, paths, grids and charts, and gets back encoded bytes. Layout is flexbox
and CSS grid, drawing is Skia, and text is shaped and broken by Skia's paragraph
engine. Two public surfaces, a Rust crate and a Node addon, build the same
`Scene` and hand it to the same core.

The predecessor is **v9**: frozen at `9.0.4` on the `v9` branch, its source in
the checkout at `../meo-canvas-old`. This renderer ships as npm 10.x and as a
crate from `0.1.0`; both numbers move, so this file calls it "this renderer".

This file is the authority on how work is done here. Read it in full before a
first change. Where a fact lives elsewhere -- a recipe, a workflow, a constant
-- it points there rather than restating it, because a copy goes stale in
silence. The maintained documents it links to are the authority on their own
subjects:

- [`docs/design.md`](docs/design.md): architecture, the two surfaces, stacking
  and layout semantics, the enum and error policies, the performance baseline.
- [`docs/releasing.md`](docs/releasing.md): the release runbook.
- [`docs/porting-v9.md`](docs/porting-v9.md): carrying a v9 component across.
- [`docs/upstream-workarounds.md`](docs/upstream-workarounds.md): the
  `[WORKAROUND]` and `[FOUNDATION]` convention.

---

## The rules that decide everything

**The browser is the baseline for behaviour; v9 is the baseline for the API.**
What a property does is what Chrome does with it; which properties exist is
what `../meo-canvas-old` offers. Neither overrides the other in the other's
half. **Ask whether a v9 choice is a decision or a limitation before copying
it**: v9's radial gradient is a circle because `ctx.createRadialGradient` makes
only circles, so the property exists because v9 has it and behaves as an
ellipse because a browser does.

**Measure the browser rather than arguing from the specification.** A reading of
the prose is a claim about what the words permit, never evidence about what
anything does.

**Neither surface ships a capability the other lacks, and neither is finished
first.** A change to one is not done until the other has it. The rule is about
reach rather than defaults: `net` puts URL fetching behind a flag on the crate
and `RootProps.httpOptions` has it always on npm, and both can reach the same
assets. **A sentence asserting parity is a claim about the code**, so re-check
it rather than trusting it, in both directions.

**A change's two sides land in one commit.** Ownership decides who edits a file,
not what a commit is; a wire change split across two commits leaves `ci` red
between them.

**Adding a feature is a commitment to it.** The edge cases and the error paths
get the attention the example path gets, and the shape still holds when the
next feature lands beside it. A half-finished capability is worse than an
absent one, because a caller builds on it.

**Do not leave a known defect in place to satisfy a weak argument** -- schedule
pressure, a passing test that does not test it, or "no one hits this". If it
cannot be fixed now, say so plainly and record why.

**Fix what let the defect happen, not its symptom.** Change the order, the data
structure or the contract rather than adding a cap, a retry or a special case.
A safety net may sit beside a structural fix, never in its place; where only a
band-aid is practical, say so.

---

## Working agreements

**Nothing is pushed and nothing is released without an explicit instruction.**
Commit freely, gate the work, then stop and report. Pushing, opening a pull
request, publishing and tagging are maintainer decisions taken one at a time;
approval for one does not carry to the next.

**Agents never publish.** Never run `npm publish`, `npm dist-tag`,
`npm deprecate`, `cargo publish` or anything else that writes to a registry;
never dispatch `release.yml` or `crates-io.yml`, which `just release-npm` and
`just release-crate` do, without the maintainer's word for that run, dry runs
included; never push a tag; never enter credentials, tokens or one-time codes.

**Whoever submits a change is its author.** Using AI tools to work here is
allowed, and the person submitting a change answers for its correctness, its
review and everything it ships, as for code written by hand. A change carries
only that person's authorship: no `Co-Authored-By` or other attribution line
naming a tool or a model.

**This file changes only with the maintainer's approval**: propose, never edit
unasked.

**Nothing an agent produces for its own benefit belongs in this repository**:
plans, specs, design notes, task lists, scratch analyses, transcripts, progress
trackers, review write-ups. Work in a scratch directory outside the repository
and let the commit message carry what needs to survive. Such files describe an
intention at one moment and nobody maintains them. What belongs: the commit
message, a comment next to the code, this file and the documents it links to.

**How the work was organised stays out of the project.** Code, comments, commit
messages, branch names, pull requests, release notes and issue comments read as
the project's own work, and none of them mentions:

- agents, sessions, workers or a supervisor, by role or by name;
- task assignments, hand-offs, review rounds or instructions received;
- internal reasoning or drafts beyond what a reviewer needs to understand the
  change;
- AI tools or models, attribution lines included.

That covers the commit removing such a mention: say what the code states now,
since quoting what was taken out puts it back into the history.

### Git safety

**Never run `git reset --hard`, `git checkout --`, `git clean` or any other
destructive git command without stashing first** (`git stash push -m "backup"`,
then `git stash pop` if it went wrong). Prefer the non-destructive form where
one exists: `git branch -f` moves a ref without touching the working tree.

**`.gitignore` denies by default.** Line 2 is `*`, and every tracked path is
re-included by name, so a new file is untracked and invisible to `git status`
until an allow line names it, and a file a workflow requires can be
uncommittable, which reads as the workflow being broken.
`git status --porcelain` reporting nothing is not evidence that a worktree holds
no work, and `git status --ignored=matching` reports a directory rather than its
contents. Before deleting a worktree or trusting "clean", enumerate with
`git ls-files --others --ignored --exclude-standard -- .tmp`.

**Branch names take a conventional type**: `fix/`, `feat/`, `bump/`, `ci/`,
`docs/`, `chore/`, never the author's initial. Renaming a branch that has an
open pull request closes the pull request for good, so get the name right at
branch time.

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

Every door into the renderer -- the Rust element tree, the JavaScript arena, the
CLI reading from disk -- produces a `Scene`, and the core cannot tell which one
it came through. One pass, no re-entry into caller code:
`resolve -> measure -> layout -> paint -> encode`. `resolve` is the only stage
with I/O, and network I/O exists only with the `net` feature.
[`docs/design.md`](docs/design.md) has the rest.

**There is no `mod.rs` at any depth.** `src/lib.rs` declares `mod foo;` for
`src/foo.rs`, whose children are `src/foo/bar.rs`. No lint enforces this, so
`just layout-check` does, inside `just ci`. **Every file carries a `//!` module
doc**, because `missing_docs` is denied: `//!` inside the file, never `///`
above the `mod` declaration.

---

## Commands

`just` drives everything. **`just` alone lists every recipe with its one-line
doc, and the `justfile` is the authority on what each does** -- read the recipe
rather than a description of it, including this one. A bare verb rewrites the
tree; a `-check` suffix reports instead. `just ci` runs only reporting recipes,
refuses to start beside another gate in the same tree, and is split into
`just portable` (reads the tree, no cargo) and `just native` (compiles, links
or runs the addon). **A prose-only change is not gated by `portable`**:
`fmt-check`, which runs prettier over Markdown, YAML and JSON, sits in
`native`. Run `just fmt-check` beside `portable` for a `.md` change, and say
that you did.

**`just ci` is not everything CI runs.** A change in these places needs the
recipe beside it too:

| If you touched                                       | Also run                            | Where CI runs it     |
| ---------------------------------------------------- | ----------------------------------- | -------------------- |
| anything behind a `net` feature, or URL fetching     | `just net-check`                    | `ci.yml`, Linux only |
| a dependency or the lockfile                         | `just audit`                        | `ci.yml`, Linux only |
| how the addon loads, or a worker thread loading it   | `just threads-probe`                | `ci.yml`             |
| what the package ships: `files`, `exports`, platform | `just verify-pack`                  | `release.yml` only   |
| the Linux build: `containers/`, link flags           | `just abi-floor`, `just acceptance` | `release.yml` only   |

The packaging and Linux rows run nowhere before a release.

**Tooling choices that look arbitrary and are not:**

- **bun is the package manager**: `bun install --frozen-lockfile`. npm stays for
  `npm pack` and `npm publish`, whose naming and provenance the release relies
  on, and for the consumer install in `verify-package.mjs`.
- **`fmt` is rustfmt on the nightly named by `fmt_toolchain`**, because
  `rustfmt.toml` uses options stable ignores, then prettier over the tree.
  `.prettierignore` names what prettier must not touch, with a reason each,
  including the machine-written Chrome tables.
- **`lint` is clippy with `-D warnings`, then ESLint.** `require-await` is off
  because `toBuffer` is `async` with no `await` by contract, and
  `no-unused-vars` has no `^_` escape for variables, because `const _ = x` is
  what the rule exists to stop.
- **Iterate against `meo-skia-canvas` through an untracked
  `.cargo/config.toml`** with a `[patch.crates-io]` path entry, never a path
  dependency in `Cargo.toml`: CI clones this repository alone.
- **`CLAUDE.md` is a symlink to this file** that `just setup` creates; a fresh
  `git worktree add` has none until setup runs in it.
- **Before writing a `.mjs` tool or a test that touches the filesystem, grep
  for** `.pathname`, `split('/')`, `startsWith(dir + '/')` and
  `execFileSync('npm'`: each breaks on Windows, where npm is `npm.cmd`.

### Tests and the gates on them

Unit tests cover the scene crate, the codec and each core stage; doctests run
every example in the crate documentation; **golden fixtures** cover paint,
because executing a fill proves the line ran, not that the pixels are right.
`crates/meo-canvas-core/tests/fixtures.rs` renders `fixtures/` and compares
committed images **byte for byte, with no tolerance, ever**. A platform gets an
`expected.<os>-<arch>.png` only where it measurably differs from the macOS
aarch64 reference, and its absence is a claim the run checks. The harness
registers exactly one font, `Fixture`, refuses any other, pins the scale and
turns `gpu` off; `fixtures.yml` uploads artefacts and never commits goldens.

`docs-js` fails on a dead link, a type reaching a signature unexported, or any
undocumented member (`undocumented-baseline.txt` is `0`, a floor). Coverage
fails below 90% on each side -- regions and lines across the Rust workspace on
the pinned nightly, and the JavaScript suite under vitest -- and `coverage`
adds a floor of 60% of regions on the Neon boundary, which Windows skips.

---

## Evidence

The recurring failure here is a claim nobody checked, repeated until it is
load-bearing. **Verify a claim before you act on it or pass it on**: a figure
recalled, quoted or inherited is a claim. Run the command and use what it
printed.

**A check that cannot fail is not a check.** Before trusting a probe, make it
produce the wrong answer: mutate what it guards, add a case it must refuse, run
a version known to be broken. Ask "could this evidence have failed?" before
reporting it.

- A comparison that passes with the change absent is a guard, not evidence, and
  a control the change deletes is worse than no control.
- Pin the **sign** where the defect inverts it; a magnitude cannot tell a defect
  from its fix on the same side of a threshold. A bound satisfied exactly by the
  worst case cannot see it. Include a case that amplifies, with its gain read
  off the formula.
- **A check has to be as wide as the claim it supports.** Byte parity is not
  capability parity, so a property one surface gains needs a parity check other
  than fixture bytes. A gate examines a proxy narrower than the rule it is cited
  for, and a check in the same currency as what it checks agrees by
  construction. Structural coverage is not positional coverage, one value per
  type checks a read's shape and not its arithmetic, and the sampling has to be
  finer than the feature.
- **Prove a search pattern in both directions**: against something it must
  match and something it must not. An absence a pattern reports measures the
  pattern. Where it matters, count the sites first and assert the edit touched
  that many; a scripted replace that matches nothing succeeds silently.
- **Calibrate instruments.** Point one at a known value before trusting it on an
  unknown; when two disagree, run the doubted one over the reference. A
  measurement through a truncating boundary reports the boundary, and a `want`
  column is arithmetic until measured. **Floor a sample point; round a reported
  value.** Two guards fail against different mistakes, so a row wants both.
- **Match on identity, not text**: a check matching text can match the checker,
  so use a pid, `ps -o comm` or an exit status you arranged to read. An exit
  status describes the last thing that ran: a pipe, a trailing `echo` or
  `gh run view` replaces it, `${PIPESTATUS[0]}` expands to nothing in this
  shell, and `git merge-base --is-ancestor` exits 1 for a missing ref too.
- **Reduce honestly.** Reduce from the top when the bottom keeps failing; a
  repro is minimal in what it is not testing, and one built from raw `Node`
  values lays out as block where the factories make flex, so it names its
  display. A failing test is not evidence of which side is wrong, and the most
  available explanation is not evidence either. Measure a defect against the
  fix branch before describing it in the fix's terms, and when a comparison
  changes a container, check the children.

**A shape, a comment and a name are claims about the contents, not evidence
about them.** A field named for a quantity has to hold that quantity, a
well-chosen error shape can hold the wrong cases, and a sentence true of the
design goes false once the code it describes changes. When one of them makes
something look considered, read the contents. Prose has no compiler: a document
can contradict itself across sections, so fixing a defect means finding every
sentence about it, here and in the documents this file links. Ask what a check
is a statement about and which frame its evidence came from;
[`docs/design.md`](docs/design.md#what-is-this-a-statement-about) has the
worked cases.

**Stale state reports something plausible rather than failing.**
`just coverage` leaves an instrumented addon: before trusting a measurement,
check `otool -l <addon> | grep -ciE 'sectname.*(llvm|prf|cov)'` prints 0.
`just addon-container` writes a Linux `.so` over the native path: back the
binary up and confirm with `file` that it is Mach-O again. An artefact replaced
under a running process gives unrelated-looking symptoms, and the second one is
diagnostic. `cargo check --workspace` does not compile `#[cfg(test)]` code, and
`cargo test` stops at the first failing target, so a truncated list is not a
failure count.

**The machine is shared**, so a repository-scoped ownership check clears a
saturated box. **All timeouts and no assertion failures is contention until
proven otherwise**: run `uptime` first, and above about 8 on the fifteen-minute
average, time nothing and trust no timeout-shaped red. Load only slows tests, so
a green under load is strong, a red under load says nothing, and a timing is
worthless. Run targeted checks while another build runs and the full gate when
the machine is quiet.

**Say what you checked and what you did not.** Name negative results and what
you judged rather than verified; a green with no denominator is not a result.
Report the invocation verbatim, its exit status and summary line, and what did
not run -- skipped, filtered, feature-gated or unreachable on this host.

---

## Conventions

### Comments

1. **A comment states what the code is, today.** Never what it was, will be, or
   what changed: no "used to", "changed from", "now", "no longer", and no TODO,
   FIXME or XXX. Nor a comparison with v9; v9 is named only where the code reads
   it, such as recorded vectors or a tool that diffs its surface. A runtime
   message written for someone porting from v9 may name v9's spelling.
2. **A comment answers "why this and not the obvious alternative".** Restating
   the code is worse than silence.
3. **When performance is the reason, cite the measurement**: a number, not an
   adjective.
4. **When a test pins a behaviour, name the test.**
5. **`//!` for module rationale, `///` for item rationale, `//` for a decision
   only a maintainer needs.** If a reader outside the file would act on it, it
   is a doc comment.
6. **Present tense, indicative**: "Rejects a radius below zero, as a browser
   does."
7. **Four lines of text at most, and one where one will do.** A `/**` or `*/`
   line on its own is not text. This binds every comment no caller reads: `//`,
   a private item's doc, a private module's `//!`, tests, tools, workflows, the
   `justfile` and configuration files. A doc that rustdoc or TypeDoc renders for
   a caller may run longer and carry an example, and so may a wire-format
   specification beside the decoder that reads it. What does not fit goes in the
   commit message.

**A reason written in two places gets corrected in one.** Write it once, where a
reader lands, and point at it from elsewhere.

**Re-read the comments around everything you changed**, including the ones the
diff did not show: the doc block above the function, the file header, and any
comment elsewhere describing the behaviour you moved. A comment that is now
wrong is a defect in this commit.

### Working around an upstream defect

A compensation for a dependency's defect carries a `[WORKAROUND]` tag and a
probe that fails the day the dependency is fixed, beside a `[FOUNDATION]` test
pinning the property the compensation rests on. The convention in full, and
what `just workaround-probes` checks, is in
[`docs/upstream-workarounds.md`](docs/upstream-workarounds.md).

### Defaults, constants and errors

**A default that means something different from the same value stated
explicitly is absent**: `z_index` is `Option<i32>` because `auto` and `0`
differ, and not pinned is not the same as pinned to zero.

**Every value that is a judgement gets a named `const` whose doc justifies the
magnitude**, not merely the strategy. No lint checks this; review does.

**`unwrap` is denied; `expect` warns** and is allowed where its message explains
the invariant that makes it unreachable. The core returns
`Result<_, MeoError>` with a variant per failure class, which the addon maps to
exceptions and the CLI to exit codes. **Where a validation repair goes is
decided by whether the other surface can express the bad input.** The writer
refuses what the type forbids; the consumption side clamps what arrives as
bytes.

**A value the renderer cannot use is a diagnostic, not an error**: the render
goes ahead and reports it on both surfaces. **A public enum is marked
`#[non_exhaustive]` when this project will add to it, not when CSS closed the
set.** [`docs/design.md`](docs/design.md) has the shapes and the lists.

---

## Releasing

**Nothing is released without an explicit instruction**, and the version is the
maintainer's decision. Every pull request fills
`docs/releases/{npm,rust}/unreleased.md` in the commit that makes the change,
under the channel or channels it reaches; a change that reaches neither says so
in the pull request rather than inventing an entry.
[`docs/releases/README.md`](docs/releases/README.md) is the authority on the
headings, and `CONTRIBUTING.md` on an entry that contradicts an earlier one.
The runbook -- channels and tags, targets, Linux artefacts, platform packages,
the OIDC bootstrap and the reference site -- is
[`docs/releasing.md`](docs/releasing.md).

---

## Dependencies

Every dependency is on its latest stable release, and the exceptions say why;
the versions and the notes behind them are in
[`docs/design.md`](docs/design.md#dependencies). **The core requires no async
runtime and performs no network I/O unless built with `net`**, and
`just runtime-free` fails if a runtime enters the tree.

---

## Commit messages

**The subject says what, in Conventional Commits form. The body says why.**
Scanning `git log` a year later, the subject is all you get.

```
type(scope): what changed, in the imperative

Why it changed. What was wrong, how that was found, what else was tried,
and what is true now that was not before.
```

**Types:** `feat`, `fix`, `perf` (a _measured_ change), `refactor` (no
behavioural difference), `docs`, `test`, `build`, `ci`, `chore`, `revert`; a
breaking change takes a `!` before the colon and says so in the body. **The
scope names the part of the tree the change touches** and has to match it,
because it is what a later search filters on: `scene`, `codec`, `core`,
`layout`, `paint`, `text`, `arena`, `node`, `cli`, `justfile`, `workflows`.

The subject is imperative, with no trailing period, under about seventy
characters. The body leads with the defect or the gap in the terms someone
hitting it would use; gives the evidence -- a measurement, a decoded byte, an
assertion that failed; says what was rejected and why; and names what is still
not right. Prose, not bullets, wrapped at 72 columns. Its length follows from
the reasoning: most commits here run twenty to fifty lines, and a small change
takes three.

---

## Writing for people

- Be concise. Simple sentences. Technical jargon is fine.
- Do not overexplain. Assume the reader is technically proficient.
- No flattering, corporate or marketing language. No weasel words.
- No vague claims the context does not support.

---

## Before every commit

1. **`just ci` passes, all of it.** The `justfile` is the authority on what it
   runs, and `just fmt-check` covers prose that `portable` does not.
2. **Nothing after `just ci` on the same command line**: a pipe, an `&&` or a
   trailing `echo` replaces the gate's exit status with its own.
3. **No `unwrap()` you added remains, and every `expect()` explains its
   invariant.**
4. **The comments around everything you changed are re-read**, including the
   ones the diff did not show.
5. **Both surfaces have the change**, or you can say why the other does not
   need it.
6. **Report the shape, not a summary**: the invocation, its exit status and
   summary line, and what did not run.
7. **Then stop.** Pushing, opening a pull request, publishing and tagging are
   separate decisions, taken one at a time.
