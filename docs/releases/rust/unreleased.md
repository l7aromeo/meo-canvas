### Fixed

- **`--features net` builds of `meo-canvas-cli` still refused URL images.**
  The feature compiled an HTTP client the binary never called and left
  fetching off in `meo-canvas-core`, so every `ImageSource::Url` failed as
  though the feature were absent, and in exactly that build the message
  dropped its hint to build with `--features net`. The feature now enables the
  core's fetch, so a URL image is fetched. A fetch that fails, under
  `on_image_error: Throw`, exits 6, source unobtainable, and names the URL.

- With the `net` feature, an image URL whose host does not resolve is reported as `FetchFailure::HostNotFound` (do not retry) rather than `FetchFailure::Transport` (retry), in `Error::SourceFetch` and in `ImageWarning` alike. A lookup that could not finish, because the resolver was unreachable or said to try again, stays `Transport`.
