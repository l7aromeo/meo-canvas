<!--
The title of this pull request becomes the commit subject when it is squashed, and semantic-release
reads that to decide the next version. So it has to be a conventional commit: `fix(Image): …`,
`feat(animate): …`, `docs: …`. CONTRIBUTING.md has the full type → release mapping.
-->

## What this changes

<!-- What it does, and why. The diff already says what; say why. -->

## How it was verified

<!--
Which tests cover it, and what you ran. Rendering bugs usually need an integration test — the unit
suite mocks the renderer, so a mock can agree with code that draws the wrong thing.
-->

- [ ] `bun run lint`
- [ ] `bun run test`
- [ ] `bun run build && bun run test:integration`
- [ ] `bun run docs`

## Checklist

- [ ] The title is a conventional commit, and its type matches the release this should cut
- [ ] Public API changes carry doc comments — `bun run docs` fails without them, and they are what the published API reference is built from
- [ ] README updated, if this adds or changes a prop
- [ ] Breaking changes are marked `!` and explained in the body, with what a caller has to do instead
