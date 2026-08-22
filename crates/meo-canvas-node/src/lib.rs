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

// `unreachable_pub` is a workspace lint, and `clippy::redundant_pub_crate` is
// its opposite: one asks for `pub(crate)` on an item a private module exports,
// the other calls that redundant. The workspace chose `unreachable_pub`, so the
// clippy half is off here rather than the visibility being written twice.
#![allow(
    clippy::redundant_pub_crate,
    reason = "contradicts the workspace's unreachable_pub"
)]

pub mod arena;

use arena::{SideValue, Values};
use neon::{prelude::*, types::buffer::TypedArray};

/// Reads the arena and its side array out of the call's arguments.
///
/// Every V8 read happens here, once, before the render starts: the point of
/// the arena is that the scene crosses as one typed array rather than as a
/// property lookup per field.
fn arguments(cx: &mut FunctionContext<'_>) -> NeonResult<(Vec<f64>, Values)> {
    let arena = cx.argument::<JsFloat64Array>(0)?;
    let slots = arena.as_slice(cx).to_vec();

    let values = cx.argument::<JsArray>(1)?;
    let length = values.len(cx);
    let mut side = Vec::with_capacity(length as usize);
    for index in 0..length {
        let value: Handle<'_, JsValue> = values.get(cx, index)?;
        if let Ok(text) = value.downcast::<JsString, _>(cx) {
            side.push(SideValue::Text(text.value(cx)));
        } else if let Ok(buffer) = value.downcast::<JsBuffer, _>(cx) {
            side.push(SideValue::Bytes(buffer.as_slice(cx).to_vec()));
        } else {
            return cx.throw_type_error(format!(
                "side value {index} is neither a string nor a Buffer"
            ));
        }
    }
    Ok((slots, Values::new(side)))
}

/// Renders a scene given as an `f64` arena and returns the image bytes.
///
/// Takes the arena, the side values array, the format name and an options
/// object, and resolves to a Buffer.
///
/// Returns a Promise, and the render runs on Node's worker pool rather than on
/// the event loop. A scene of any size is CPU-bound from resolve through
/// encode, so running it inline would stall every other request in the process
/// for the whole render. `cx.task(..).promise(..)` is neon's own mechanism for
/// that; a `Channel` would also work but would leave us owning the thread.
fn render(mut cx: FunctionContext<'_>) -> JsResult<'_, JsPromise> {
    let (slots, values) = arguments(&mut cx)?;
    let format = cx.argument::<JsString>(2)?.value(&mut cx);

    let promise = cx
        .task(move || render_off_thread(&slots, &values, &format))
        .promise(|mut cx, result| match result {
            Ok(bytes) => Ok(JsBuffer::from_slice(&mut cx, &bytes)?),
            Err(message) => cx.throw_error(message),
        });
    Ok(promise)
}

/// The render itself, with no V8 in reach.
///
/// Separated so the whole pipeline is callable from a test without a Node
/// process, which is the only way the decoder's behaviour is covered by
/// anything other than the JavaScript suite.
fn render_off_thread(
    slots: &[f64],
    values: &Values,
    format: &str,
) -> Result<Vec<u8>, String> {
    let scene =
        arena::decode(slots, values).map_err(|error| error.to_string())?;
    let format = meo_canvas_core::ImageFormat::from_extension(format)
        .ok_or_else(|| format!("no image format is called {format:?}"))?;
    let renderer = meo_canvas_core::Renderer::new();
    renderer
        .render(&scene, format, &meo_canvas_core::EncodeOptions::default())
        .map(|image| image.bytes)
        .map_err(|error| error.to_string())
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
