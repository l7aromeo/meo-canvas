# Working around an upstream defect

This document is the authority on how code here compensates for a defect in a
dependency, and on how that compensation stays findable and retirable. The
check that enforces it is `just workaround-probes`; this is what it enforces.

## Two halves, and neither substitutes for the other

A workaround carries a **`[WORKAROUND]` tag** and a **probe**. The tag makes it
greppable from the code. The probe makes it impossible to leave behind: it is a
test that fails the day the dependency is fixed, and that failure is the
notification.

```rust
// [WORKAROUND] <what the dependency does wrong, and what this does instead>.
// `owner/repo#N`, tracked as `l7aromeo/meo-canvas#M`. Retires when <the
// observable that will become true>, which <the test that will say so>
// asserts.
```

Four parts, and each earns its place:

- **The tag** is a fixed string, so a grep cannot go loose or tight by accident.
  Prove the pattern against a site it must match and one it must not, as with
  any other search.
- **The short description** says what the compensation does, so a reader need
  not reconstruct it.
- **The qualified references** name the upstream defect and our tracking issue.
  `just issue-refs` enforces the `owner/repo#N` form.
- **The retirement observable** is a fact that will become true, not an event
  someone has to notice.

**It is not a TODO.** TODO, FIXME and XXX are refused because they describe work
that ought to happen later. `[WORKAROUND]` describes what the code is:
compensation is its present nature, which is what a comment is required to
state. The tag is a category, not a deferral.

**Every site the compensation reaches carries the tag**, so a grep returns the
whole of it rather than its entry point. The symbol names carry the upstream
reference too: a tag is findable and a name survives a refactor, and the two
fail against different mistakes.

## The probe asserts two things

**First, that the dependency still gets it wrong.** Pin the wrong numbers it
produces, name the right ones beside them, and say in the failure message that
the compensation can be deleted. A test asserting the browser's values would
fail today and could not be committed; a test asserting the dependency's
values fails exactly when the dependency is fixed.
`crates/meo-canvas-core/tests/taffy_negative_margin.rs` is in this form.

**Second, the property the compensation depends on.** A workaround rests on the
dependency being right about something, and nothing else in the tree tests that
something. If it stopped holding, every conformance row could stay green and
the first sign would be a golden moving with nothing saying why. A probe that
asserts only the defect is half a probe.

That second test carries its own tag, **`[FOUNDATION]`, on the test rather than
on the code.** A defect row announces itself -- it asserts wrong numbers and
says so -- while a foundation row looks like any other passing test, which makes
it the one a tidy-up deletes. The tag is what makes it findable from the
compensation.

```rust
// [FOUNDATION] the property <which compensation> rests on, and what a change
// here would cost.
#[test]
fn the_dependency_still_gets_this_right() {
```

**A probe that compensates nothing carries no marker.** It pins a defect nothing
here works around, so no code rests on it and there is no property to name. The
marker belongs to the pair of compensation and probe.

**No file here is named as the example of a marked probe.** Which probes carry a
marker changes as the tree changes, so naming one freezes a fact that is meant
to move. `git grep` for the marker answers the question against the tree of the
day.

## What the check sees

`just workaround-probes` runs in `portable`, beside `issue-refs` and
`gate-lists-check`. For every tagged site it reads that the site names a probe,
or defers to a site in the same file that does; that the probe is a real test
target with a test carrying no `#[ignore]`; and that the target marks at least
one test `[FOUNDATION]`.

**It sees markers, not properties.** Whether the marked row pins the thing the
compensation rests on is not a question a program here can ask, and the tool
says so in its own header. What the check buys is that the question was
answered once, deliberately, where a reader lands.
