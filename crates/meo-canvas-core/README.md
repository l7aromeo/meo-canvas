# `meo-canvas-core`

The rendering pipeline: resolve, measure, layout, paint, encode.

Wraps taffy for layout and `meo-skia-canvas` for drawing, and exposes neither in
a public signature.

**Runtime-free always, and fetch-free by default.** Without the `net` feature an
unresolved URL is an error rather than a network call. With it, a URL is fetched
through a blocking client — no async runtime either way, which is the half of
the rule that does not bend.
