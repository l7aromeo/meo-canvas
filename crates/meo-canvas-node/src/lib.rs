//! The Node.js addon: one `.node` binary, one module entry point.
//!
//! JavaScript builds a scene, encodes it with the same wire format
//! [`meo_canvas_scene::codec`] defines, and passes one buffer across. This
//! crate decodes it, runs [`meo_canvas_core`], and hands back the encoded
//! image. One buffer per render rather than a property read per field: walking
//! a `JsObject` tree costs a V8 lookup for every field of every node, and a
//! scene has thousands.
//!
//! # Why this crate holds the entry point
//!
//! A `.node` addon exposes exactly one module-init symbol. `meo-skia-canvas`
//! defines its own behind its `node-addon` feature, so the workspace pins that
//! feature off (`default-features = false`, stated in the root manifest) and
//! declares the only [`neon::main`] here. Two crates in one binary both
//! registering a module is a link error at best and the wrong module at worst.
//!
//! # What this crate deliberately excludes
//!
//! No rendering logic. Every function here converts, calls into
//! `meo-canvas-core`, and converts back. Logic that lived here would be logic
//! the CLI and the Rust surface could not reach and the test suite could not
//! run without building a `.node` file.
//!
//! No panics across the boundary. A panic unwinding into Node takes the process
//! down, so every entry point turns a [`meo_canvas_core::Error`] into a thrown
//! JavaScript exception rather than letting it escape.

use neon::prelude::*;

/// Renders a scene given as an encoded buffer and returns the image bytes.
///
/// Takes one argument, the buffer written by the JavaScript encoder, and throws
/// if it is not a scene this revision reads or if any pass fails.
fn render(mut _cx: FunctionContext<'_>) -> JsResult<'_, JsBuffer> {
    unimplemented!()
}

/// Reports which backend the addon resolved, as a JSON string.
///
/// A string rather than an object because the shape is diagnostic output that
/// JavaScript logs or parses, and a stable JSON document survives fields being
/// added where a hand-built object's property order does not.
fn backend(mut _cx: FunctionContext<'_>) -> JsResult<'_, JsString> {
    unimplemented!()
}

/// The module's single registration point.
///
/// # Errors
///
/// Returns a Neon error if a name cannot be exported, which Node reports as a
/// failure to load the addon.
#[neon::main]
fn main(mut cx: ModuleContext<'_>) -> NeonResult<()> {
    cx.export_function("render", render)?;
    cx.export_function("backend", backend)?;
    Ok(())
}
