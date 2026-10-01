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

**Fix what let the defect happen, not its symptom.** Change the order, the data
structure or the contract, rather than adding a cap, a retry or a special case
around it. A safety net may sit beside a structural fix, never in its place;
where only a band-aid is practical, say so.

---

## Working agreements

**Nothing is pushed and nothing is released without an explicit instruction.**
Commit freely, gate the work, then stop and report. Pushing, opening a pull
request, publishing and tagging are maintainer decisions taken one at a time.
Approval for one does not carry to the next.

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

**This file changes only with the maintainer's approval.** Propose a change to
it; do not make one unasked.

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

**How the work was organised stays out of the project.** Code, comments, commit
messages, branch names, pull requests, release notes and issue comments read as
the project's own work, and none of them mentions:

- agents, sessions, workers or a supervisor, by role or by name;
- task assignments, hand-offs, review rounds or instructions received;
- internal reasoning or drafts beyond what a reviewer needs to understand the
  change;
- AI tools or models, attribution lines included.

**That covers the commit removing such a mention.** Say what the code states
now; quoting what was taken out puts it back into the history.

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

**Targeted checks are the signal while another build is running**; the full
gate runs when the machine is quiet.

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

The scene contract, its two wire formats, the pipeline and the two surfaces are
in [`docs/design.md`](docs/design.md).

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
   **Nor a comparison with v9**: a comment says what this code does, not how it
   differs from the predecessor. v9 is named only where the code itself reads
   it -- vectors recorded from it, a tool that diffs its surface. A runtime
   message is not a comment: one written for someone porting from v9 may name
   v9's spelling, since that is the guidance the reader needs.
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
7. **Four lines of text at most, and one where one will do.** A `/**` or `*/`
   line on its own is not text. This binds every comment no caller reads:
   `//`, a private item's doc, a private module's `//!`, tests, tools,
   workflows, the `justfile` and configuration files. A doc that rustdoc or
   TypeDoc renders for a caller may run longer and carry an example, and so
   may a wire-format specification beside the decoder that reads it, since
   another implementation is written from that text. What does not fit goes
   in the commit message, which is where history belongs anyway.

**A reason written in two places gets corrected in one.** Write it once, at the
place a reader lands, and point at it from the other.

**Re-read the comments around everything you changed**, including the ones the
diff did not show you -- the doc block above the function, the header of the
file, and any comment elsewhere describing the behaviour you moved. A comment
that is now wrong is a defect in this commit, not a task for later.

### Working around an upstream defect

A compensation for a dependency's defect carries a `[WORKAROUND]` tag and a
probe that fails the day the dependency is fixed, beside a `[FOUNDATION]` test
pinning the property the compensation rests on. The convention in full, and
what `just workaround-probes` checks, is in
[`docs/upstream-workarounds.md`](docs/upstream-workarounds.md).

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

### Stacking and layout semantics

How `z_index`, stacking contexts and the layout defaults behave, with the
Chrome measurements behind them, is in [`docs/design.md`](docs/design.md).

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

### Public enums and statements

The `#[non_exhaustive]` policy and the question "what is this a statement
about" are in [`docs/design.md`](docs/design.md).

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

**`just ci` is not everything CI runs.** A change in these places needs the
recipe beside it as well, because nothing in `ci` would see it go wrong:

| If you touched                                       | Also run                            | Where CI runs it     |
| ---------------------------------------------------- | ----------------------------------- | -------------------- |
| anything behind a `net` feature, or URL fetching     | `just net-check`                    | `ci.yml`, Linux only |
| a dependency or the lockfile                         | `just audit`                        | `ci.yml`, Linux only |
| how the addon loads, or a worker thread loading it   | `just threads-probe`                | `ci.yml`             |
| what the package ships: `files`, `exports`, platform | `just verify-pack`                  | `release.yml` only   |
| the Linux build: `containers/`, link flags           | `just abi-floor`, `just acceptance` | `release.yml` only   |

**The packaging and Linux rows run nowhere before a release**, so a defect there
surfaces only once a release is under way.

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

The benchmark baseline and how to read it are in
[`docs/design.md`](docs/design.md#performance-baseline).

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

### The reference and the coverage floors

How the reference site publishes is in [`docs/releasing.md`](docs/releasing.md).

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

Six places where a v9 component's text means something different here, most of
them silent at runtime, and the method that keeps a port faithful, are in
[`docs/porting-v9.md`](docs/porting-v9.md). Read it before carrying a component
across.

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

Every dependency is on its latest stable release, and the exceptions say why.

| crate             |        |                                                           |
| ----------------- | ------ | --------------------------------------------------------- |
| `meo-skia-canvas` | 0.16.3 | Skia, text shaping, encoding. `default-features = false`. |
| `taffy`           | 0.14   | Flexbox, CSS grid, block layout. Without `calc`.          |
| `csscolorparser`  | 0.8    | CSS colour syntax. Holds channels as `f32` -- see below.  |
| `neon`            | 1.1    | Node addon.                                               |
| `clap`            | 4.6    | CLI.                                                      |
| `thiserror`       | 2.0    | Error types.                                              |
| `ureq`            | 3.4    | Remote images, behind the optional `net` feature.         |
| `rustls`          | 0.23   | A floor, not a use -- see `Cargo.toml`. Under `net`.      |
| `rayon`           | 1.11   | The addon's asynchronous encode. Not an async runtime.    |

| tool       |       |                                                                |
| ---------- | ----- | -------------------------------------------------------------- |
| bun        | 1.4.1 | Package manager and the JavaScript examples' runtime.          |
| typescript | 6.0.3 | Not 7. Moves when typescript-eslint and TypeDoc do.            |
| eslint     | 10    | typescript-eslint 8, `eslint-config-prettier` last.            |
| prettier   | 3.9   | Whole tree; `.prettierignore` names the machine-written files. |
| vitest     | 5     | Tests and the JavaScript coverage floor.                       |
| typedoc    | 0.28  | Its own package, so it pins the TypeScript it loads.           |
| playwright | 1.63  | Drives Chrome for the conformance tables.                      |

**The core requires no async runtime and performs no network I/O unless built
with `net`.** `just runtime-free` fails if a runtime enters the tree.

The notes on taffy's threading and `csscolorparser`'s `f32` channels are in
[`docs/design.md`](docs/design.md#dependency-notes).

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
