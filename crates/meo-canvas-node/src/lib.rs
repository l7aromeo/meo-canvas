//! The Node.js addon: one `.node` binary, one module entry point.
//!
//! JavaScript builds a scene into an `f64` arena and passes it across with a
//! side array holding the strings and buffers a `Float64Array` cannot carry.
//! This crate decodes that arena, runs [`meo_canvas_core`], and hands back the
//! encoded image. One typed array per render rather than a property read per
//! field: walking a `JsObject` tree costs a V8 lookup for every field of every
//! node, and a scene has thousands.
//!
//! **The arena is not the byte format.** [`meo_canvas_scene::codec`] is the
//! persistence format -- self-contained, with its strings inside it, which is
//! what a file on disk needs. The arena is the boundary format, shaped for a
//! side that stores into a `Float64Array` in one operation and would write
//! varint bytes in several. Both decode to the same `Scene`, so a scene
//! captured here and written to disk round-trips without loss, and neither is
//! a version of the other. [`arena`] carries the specification.
//!
//! # Why this crate holds the entry point
//!
//! A `.node` addon exposes exactly one module-init symbol. `meo-skia-canvas`
//! defines its own behind its `node-addon` feature, so the workspace pins that
//! feature off (`default-features = false`, stated in the root manifest) and
//! declares the only [`neon::main`] here. Two crates in one binary both
//! registering a module is a link error at best and the wrong module at worst.
//!
//! # Renders do not run on the event loop
//!
//! The `render` export returns a Promise and the work runs on Node's worker
//! pool. A
//! scene is CPU-bound from resolve through encode, so running it inline
//! would stall every other request in the process for the whole render.
//!
//! Every V8 read happens once, up front, before the task starts: the arena is
//! copied out of its typed array and the side array is walked before the
//! task is spawned. Nothing touches V8 on the worker, which is what makes the
//! pool safe to use at all.
//!
//! # What this crate deliberately excludes
//!
//! No rendering logic. Every function here converts, calls into
//! `meo-canvas-core`, and converts back. Logic that lived here would be logic
//! the CLI and the Rust surface could not reach and the test suite could not
//! run without building a `.node` file.
//!
//! No panics across the boundary. A panic unwinding into Node takes the
//! process down. A failure before the task starts -- an argument of the wrong
//! type -- throws synchronously; a failure inside the render **rejects the
//! Promise** instead, because by then there is no call left to throw from.
//! Those are different things to a JavaScript caller, and only a test in that
//! language can tell them apart.

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
use meo_canvas_core::{EncodeOptions, Renderer, Surface};
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
    let renderer = Renderer::new();
    renderer
        .render_to_buffer(&scene, format, &EncodeOptions::default())
        .map_err(|error| error.to_string())
}

/// Reports which rasteriser a render would use, and which one it asks for.
///
/// Two questions, not one, and they can disagree: `requested` is what the
/// renderer asks for and `active` is what asking got. A build with no GPU
/// backend compiled rasterises on the CPU whatever is requested, which is the
/// distinction `Canvas::gpu`'s own documentation draws — "the request, not the
/// outcome".
///
/// Answered by making a one-pixel canvas and asking it, rather than by
/// reasoning about which features were compiled: the compiled feature set is
/// what a caller would have to reason from, and this reports what actually
/// happens instead.
///
/// Returns an object rather than a JSON string. A string would make every
/// caller parse what this already knows, and a field added later reaches a
/// JavaScript caller as a property rather than as a schema change.
fn backend(mut cx: FunctionContext<'_>) -> JsResult<'_, JsObject> {
    let requested = Renderer::new().gpu();
    let probe = Surface::new(PROBE_SIZE, PROBE_SCALE, requested)
        .or_else(|error| cx.throw_error(error.to_string()))?;

    let object = cx.empty_object();
    let active = cx.string(probe.engine());
    object.set(&mut cx, "active", active)?;
    let requested = cx.boolean(requested);
    object.set(&mut cx, "requestsGpu", requested)?;
    Ok(object)
}

/// The surface [`backend`] asks. One pixel, because nothing is drawn on it.
const PROBE_SIZE: meo_canvas_scene::Size = meo_canvas_scene::Size {
    width: 1.0,
    height: 1.0,
};

/// The scale [`backend`]'s probe surface uses. One, so its pixel is its pixel.
const PROBE_SCALE: f32 = 1.0;

/// Re-encodes an arena through the byte format.
///
/// Takes the arena and its side values, decodes the scene, and returns what
/// [`meo_canvas_scene::codec`] writes for it. The two representations
/// producing one `Scene` is the property the TypeScript round trip asserts,
/// and this is what makes the assertion literally that claim: comparing
/// rendered images instead would let two different scenes pass as one, and a
/// property the encoder forgot that happens to change nothing visible would go
/// unnoticed.
///
/// **Throws on a malformed arena rather than returning short bytes.** A
/// half-written encoder should fail at the boundary naming the slot, not
/// produce a buffer that compares unequal for a reason the test cannot
/// attribute.
///
/// Synchronous, unlike [`render`]: decoding an arena and writing bytes is
/// microseconds of work with no rasteriser in it, so a Promise would cost a
/// tick to save nothing.
fn scene_bytes(mut cx: FunctionContext<'_>) -> JsResult<'_, JsBuffer> {
    let (slots, values) = arguments(&mut cx)?;
    let scene = match arena::decode(&slots, &values) {
        Ok(scene) => scene,
        Err(error) => return cx.throw_error(error.to_string()),
    };
    let bytes = meo_canvas_scene::codec::encode(&scene);
    JsBuffer::from_slice(&mut cx, &bytes)
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
    cx.export_function("sceneBytes", scene_bytes)?;
    Ok(())
}
