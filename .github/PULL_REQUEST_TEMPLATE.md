<!--
Commit subjects here are sentences, not conventional-commit prefixes: "Bound what a decode reserves
by what the bytes could hold", not "fix(codec): bound reservation". Match what is already in
`git log`. The body says why, since the diff already says what.
-->

## What this changes, and why

<!--
The reasoning, not the summary. If a number decided it, give the number and say where it came
from — this repository's convention is that a measured claim names its measurement.
-->

## How it was verified

<!--
What you ran and what it said. A test that has never failed is not known to work: if you added a
guard, say how you saw it fire.
-->

- [ ] `just fmt-check`
- [ ] `just lint-check`
- [ ] `just typecheck`
- [ ] `just test`
- [ ] `just test-js`
- [ ] `just doc-examples-check`, `just docs` and `just docs-js`

<!--
Run them separately rather than chained. `just test-js` passing while `just typecheck` fails is a
thing that has happened here, and one `;` instead of `&&` is all it takes.
-->

## Goldens

- [ ] This moves no golden fixture
- [ ] This moves goldens, and they are regenerated for **every** platform below

<!--
Goldens are per-architecture and compared with no tolerance, so a fixture accepted on one platform
does not cover the others: darwin-arm64, linux-x64-gnu, linux-arm64-gnu, linux-x64-musl,
linux-arm64-musl, win32-x64, win32-arm64. If a golden moved, say in the body which ones and why the
new picture is more correct than the old one — a moved golden is either a fix or a regression, and
the diff cannot tell you which.
-->

## Both surfaces

- [ ] Public API changed on one surface only, and that is deliberate — say why in the body
- [ ] Public API changed on both, in this commit

<!--
Neither surface ships a capability the other lacks. `just example` renders the same scenes through
both and compares the bytes, so a surface left behind fails the command rather than being noticed
later.
-->

## Checklist

- [ ] New public items carry doc comments, and `just docs` and `just docs-js` pass — the two published references are built from them, one per surface
- [ ] A behaviour change that a caller could not guess is written down where they would look, not only in the commit
- [ ] Breaking changes explain what a caller has to do instead
