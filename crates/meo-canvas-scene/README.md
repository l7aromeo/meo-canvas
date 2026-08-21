# `meo-canvas-scene`

The scene description three surfaces produce and one renderer consumes, plus the
binary wire format that carries it.

Dependency-free by design: no Skia, no taffy, no Neon, no serde. A scene is
`Send`, buildable without a renderer, and testable without a Skia build. See the
crate documentation for why each of those exclusions is load-bearing.
