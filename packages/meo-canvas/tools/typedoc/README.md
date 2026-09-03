# The JavaScript reference

`just docs-js` builds the API reference from the declarations `build-js` emits
into `dist/`, and fails on a dead link, a type that reaches a signature without
being exported, or a rise in the number of undocumented members.

## Why this is its own package

TypeDoc is compiled against one TypeScript minor and refuses to load under
another. Sharing the root's TypeScript would mean every root upgrade is also a
TypeDoc upgrade, or a broken reference. Pinning both here lets the root move on
its own schedule.

## The baseline

`undocumented-baseline.txt` holds one number: how many exported members have no
doc comment. `build.mjs` fails if the count rises and rewrites the file if it
falls. Commit the lowered file with the change that lowered it. Nothing here
fails on the gaps that already exist, because a gate that fails on day one
teaches everyone to turn it off.

## Where it publishes

`.github/workflows/docs.yml` runs this on every pull request that touches the
surface, and publishes to GitHub Pages when a release is published — one
directory per version, and `latest/` following the newest stable version the
way npm's `latest` dist-tag does. The deploy waits until the version resolves
from the registry, so `latest/` never describes a version `npm install` cannot
give you yet.
