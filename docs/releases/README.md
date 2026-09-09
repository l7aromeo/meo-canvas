# Release notes

One file per release, named by version, under the channel that publishes it:

    docs/releases/npm/10.0.0-alpha.6.md     ->  tag npm-v10.0.0-alpha.6
    docs/releases/rust/0.1.0.md             ->  tag rust-v0.1.0

The release workflow reads the file for the version it is publishing and
refuses to release without one. There is no fallback to `git log`, and adding
one would make the check unable to fail -- every release would pass it whether
or not anyone had written anything.

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
headings, that is the moment. The one mechanical
rule is the filename: it is the version exactly as `package.json` or
`cargo metadata` reports it, with no `v` and no channel prefix.
