# `meo-canvas-node`

The Node.js addon. One `.node` binary, one Neon module entry point.

JavaScript encodes a scene into a single buffer; this crate decodes it, renders,
and returns the bytes. It holds the only `#[neon::main]` in the binary, which is
why `meo-skia-canvas` is built with its `node-addon` feature off.
