# Test fonts

`Roboto-Regular.ttf` — Roboto Regular, version 2.001047 (2015). Copyright 2015 Google Inc., licensed
under the Apache License 2.0. The full text is in `LICENSE` beside it, which is what redistributing
the file obliges us to include; the font's own metadata states the same licence.

It is here so the integration suite does not depend on whatever fonts a machine happens to have.
The pixel fixtures in `../renders` were rendered with it, and a different font would change every
one of them.

Not published to npm — `files` in package.json ships `dist` and the documentation, so nothing under
`tests/` reaches the tarball.
