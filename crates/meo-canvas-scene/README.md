# `meo-canvas-scene`

The scene description three surfaces produce and one renderer consumes, plus the
binary wire format that carries it.

Dependency-free by design: no Skia, no taffy, no Neon, no serde. A scene is
`Send`, buildable without a renderer, and testable without a Skia build. See the
crate documentation for why each of those exclusions is load-bearing.

## Usage

Build a scene, then encode it. The wire format is what the Node addon and the
CLI both hand to the renderer, so anything that can produce these bytes can
drive it.

```rust
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::Dimension,
};

let mut scene = Scene::new(Size::new(200.0, 120.0));
let box_id = scene.push(NodeId::ROOT, Node::new(NodeKind::Box))?;
if let Some(node) = scene.get_mut(box_id) {
    node.layout.size = (Dimension::Points(80.0), Dimension::Points(40.0));
    node.layout.margin.left = Dimension::Points(12.0);
}

let bytes = meo_canvas_scene::codec::encode(&scene);
let round_tripped = meo_canvas_scene::codec::decode(&bytes)?;
assert_eq!(round_tripped.size, scene.size);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## System libraries

None. This crate links nothing, which is the point of it — it is the one part
of the workspace you can depend on without a Skia build. Rendering a scene
needs [`meo-canvas-core`], and that has its own requirements.

[`meo-canvas-core`]: https://crates.io/crates/meo-canvas-core
