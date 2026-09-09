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
reference below it, so start at what changed:

```markdown
## Changes

- `fillText` no longer clears a paragraph whose colour tag was unusable.
- `Diagnostic` carries the offset of the tag it is about.

## Breaking

- `Text::new_reporting` is gone; `Element::into_scene` returns the
  diagnostics instead.
```

Nothing here is generated, and nothing checks the prose. The one mechanical
rule is the filename: it is the version exactly as `package.json` or
`cargo metadata` reports it, with no `v` and no channel prefix.
