# `meo-canvas-core`

The rendering pipeline: resolve, measure, layout, paint, encode.

Wraps taffy for layout and `meo-skia-canvas` for drawing, and exposes neither in
a public signature. Fetch-free and runtime-free — an unresolved URL is an error,
not a network call.
