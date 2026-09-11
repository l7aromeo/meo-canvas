# Release notes

One file per release, named by version, under the channel that publishes it:

    docs/releases/npm/10.0.0-alpha.6.md     ->  tag npm-v10.0.0-alpha.6
    docs/releases/rust/0.1.0.md             ->  tag rust-v0.1.0

The release workflow reads the file for the version it is publishing and
refuses to release without one. There is no fallback to `git log`, and adding
one would make the check unable to fail -- every release would pass it whether
or not anyone had written anything.

## Before the version exists: `unreleased.md`

**The note is written while the change is being made, and the version it ships
under is not known then.** So each channel has an `unreleased.md` that
accumulates entries, and cutting a release renames it:

    docs/releases/npm/unreleased.md   ->  docs/releases/npm/<version>.md
    docs/releases/rust/unreleased.md  ->  docs/releases/rust/<version>.md

`just cut-notes npm` and `just cut-notes rust` do it, reading the version from
the same manifest the release recipe reads rather than taking it as an
argument — a typed version is a second source that can disagree, and the
disagreement surfaces as the workflow refusing a note that exists under a name
nobody expected.

Renamed rather than copied, so the next cycle starts from an empty file rather
than from the last release's text with somebody's edits on top. **The recipe
leaves that empty file behind**, because a rename alone makes the path stop
existing the moment a release is cut, and the next contributor is then told by
`CONTRIBUTING.md` to edit a file that is not there.

**Every pull request fills it.** Add the entry in the same commit as the change,
under the channel or channels the change reaches -- a change in
`meo-canvas-core` reaches both, a change in `packages/meo-canvas/src` reaches
npm alone, and a change to a gate, a test or a workflow reaches neither and says
so in the pull request rather than inventing an entry for it.

Writing it then rather than at release time is what makes the entry describe the
defect as the person who hit it met it. A note assembled from `git log` a week
later describes the fix, because that is what the log records, and the reader
looking for their own symptom does not find it.

A change that contradicts an entry already in `unreleased.md` edits that entry
rather than appending a second one beside it: nobody has installed the version
it describes, so there is nothing to correct and nobody to tell.
`CONTRIBUTING.md` has the three shapes this takes and how to resolve the
conflict when two pull requests append at once.

**The version-named file is the released one.** `just release-npm` and
`just release-crate` refuse to dispatch when the note for the version they are
about to publish is missing **or empty**, because the failure otherwise arrives
from inside a workflow after the matrix has already built seven addons. Empty
counts because that is the shape a forgotten cycle actually takes: the stub
`cut-notes` leaves is a real file, so a run that never wrote an entry would
otherwise publish a blank release page rather than stopping.

**Write it against the previous release of the same channel**, not against the
previous commit. The two channels cut from one trunk, so the commits between
two crate releases include every npm-side change and every gate edit; a page
listing all of them tells a crate consumer nothing. Ninety-two non-merge
commits stood between the last two npm releases and roughly twenty of them
meant anything to someone installing the package.

The file is the body of the release page. The workflow adds the install line
above it, the prerelease note where one applies, and the link to the API
reference below it, so start at what changed.

**The headings are Keep a Changelog's**, in its order, and only the ones that
have entries. This repository already writes Conventional Commits; Keep a
Changelog is the counterpart for the release page, and using it means a reader
arriving from another project already knows what each section promises.

| heading      | what belongs under it                                 |
| ------------ | ----------------------------------------------------- |
| `Added`      | a capability that did not exist                       |
| `Changed`    | behaviour that existed and is now different           |
| `Deprecated` | still works, will be removed, name the replacement    |
| `Removed`    | gone in this release                                  |
| `Fixed`      | a defect, described as the caller met it              |
| `Security`   | a vulnerability, and what a caller has to do about it |

```markdown
### Added

- `Diagnostic` carries the byte offset of the markup tag it is about, so three
  `<color>` tags in one string can be told apart.

### Changed

- Gradient stops interpolate in the context's colour space rather than Oklab.

### Fixed

- An unusable colour tag no longer clears the paragraph it is in.

### Removed

- **Breaking.** `Text::new_reporting` is gone; `Element::into_scene` returns the
  diagnostics instead.
```

**A breaking change is marked in the entry, not given a section of its own.**
Keep a Changelog has no `Breaking` heading, and inventing one splits the same
change across two places -- a removal is a removal whether or not it breaks
somebody. Put **Breaking.** at the front of the entry and say what to write
instead.

Headings are `###` because the workflow's own scaffolding owns `#` and `##`.

Nothing here is generated, and nothing checks the prose -- the headings included.
A check could assert the set and the order, and the reason there is not one yet
is that the file has been written twice; if a third release picks its own
headings, that is the moment. The one mechanical rule is the filename: an
accumulating note is `unreleased.md`, and a released one is the version exactly
as `package.json` or `cargo metadata` reports it, with no `v` and no channel
prefix.
