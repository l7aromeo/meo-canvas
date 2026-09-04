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

use std::{cell::RefCell, rc::Rc};

use arena::{SideValue, Values};
use meo_canvas_core::{
    EncodeOptions, ImageFormat, RenderedCanvas, Renderer, Surface,
    SurfaceOptions,
};
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
    let format = ImageFormat::from_extension(format)
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
    let probe = Surface::new(
        PROBE_SIZE,
        PROBE_SCALE,
        SurfaceOptions {
            gpu: requested,
            ..SurfaceOptions::default()
        },
    )
    .or_else(|error| cx.throw_error(error.to_string()))?;

    let object = cx.empty_object();
    let active = cx.string(probe.engine());
    object.set(&mut cx, "active", active)?;
    let requested = cx.boolean(requested);
    object.set(&mut cx, "requestsGpu", requested)?;
    Ok(object)
}

/// A CSS colour string as four channels, or `null`.
///
/// # Why the addon and not the surface
///
/// **The renderer already parses colour, and a second implementation would
/// drift from it.** The first thing it would disagree about is
/// `color(srgb ...)`, which `csscolorparser` does not implement at all and
/// which this crate handles in a pre-pass -- so a JavaScript parser written to
/// the same specification would refuse a string the renderer accepts, and the
/// disagreement would surface as a colour that draws but cannot be animated.
///
/// So the string boundary is here: one parser, both surfaces.
///
/// # The shape
///
/// `{ r, g, b, a }` with `r`, `g` and `b` in 0 to 255 and `a` in 0 to 1, which
/// is v1's convention and what `animate.ts` carries. **Unclamped**: a
/// `color(srgb 1.25 ...)` comes back above 255 rather than flattened, because
/// an animation needs somewhere to be outside the gamut and the clamp belongs
/// where a colour becomes paint.
fn parse_color(mut cx: FunctionContext<'_>) -> JsResult<'_, JsValue> {
    let css = cx.argument::<JsString>(0)?.value(&mut cx);
    let Some([red, green, blue, alpha]) =
        meo_canvas_core::color::parse_channels(&css)
    else {
        return Ok(cx.null().upcast());
    };
    let object = cx.empty_object();
    for (name, channel) in [("r", red), ("g", green), ("b", blue), ("a", alpha)]
    {
        let value = cx.number(channel);
        object.set(&mut cx, name, value)?;
    }
    Ok(object.upcast())
}

/// Whether a string is a colour this renderer understands.
///
/// **Defined as [`parse_color`] returning something**, rather than as its own
/// check: two functions that can disagree about one string are a defect
/// waiting for the first caller who uses both, and a caller who asks this
/// before parsing is entitled to the same answer.
fn is_color(mut cx: FunctionContext<'_>) -> JsResult<'_, JsBoolean> {
    let css = cx.argument::<JsString>(0)?.value(&mut cx);
    Ok(cx.boolean(meo_canvas_core::color::parse_channels(&css).is_some()))
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

/// The painted surface, shared by the two methods that reach it.
///
/// `Rc` rather than `JsBox`: [`RenderedCanvas`] is `!Send` -- Skia's
/// `SkPictureRecorder` is, and a `CanvasGradient` holds an `Rc<RefCell<_>>` --
/// and a `JsBox` would need `this` to be bound at every call site, which a
/// destructured `const { encode } = canvas` would silently break. Two closures
/// each holding a clone give the JavaScript side the plain object its
/// `NativeCanvas` interface declares, and napi frees the captured data when
/// both are collected.
///
/// `RefCell` because [`RenderedCanvas::to_buffer`] takes `&mut self`, and
/// `Option` so [`paint`]'s `release` can drop the surface early and leave a
/// later `encode` something to refuse rather than a surface that is gone.
type Painted = Rc<RefCell<Option<RenderedCanvas>>>;

/// Reads the `{ fonts }` object `paint` is given.
///
/// Every V8 read happens here, before anything is drawn, for the reason
/// [`arguments`] gives.
///
/// **`gpu` is not read here, and that is the change rather than an omission.**
/// It rides in the arena's header beside `scale`, because a caller writes it on
/// `Root` next to the size and the scale and there is no reason two of the four
/// should reach the renderer by a different road. A `gpu` on this object would
/// be a second place to say it, and the two could disagree.
fn paint_options(
    cx: &mut FunctionContext<'_>,
    index: usize,
) -> NeonResult<Renderer> {
    let mut renderer = Renderer::new();
    let Some(options) = cx.argument_opt(index) else {
        return Ok(renderer);
    };
    let Ok(options) = options.downcast::<JsObject, _>(cx) else {
        return cx.throw_type_error("paint options must be an object");
    };

    let Some(fonts) = options.get_opt::<JsArray, _, _>(cx, "fonts")? else {
        return Ok(renderer);
    };
    for index in 0..fonts.len(cx) {
        let entry: Handle<'_, JsObject> = fonts.get(cx, index)?;
        let family = entry.get::<JsString, _, _>(cx, "family")?.value(cx);
        let paths = entry.get::<JsArray, _, _>(cx, "paths")?;
        for path in 0..paths.len(cx) {
            let path = paths.get::<JsString, _, _>(cx, path)?.value(cx);
            // Registration is I/O and can fail on a path that does not exist,
            // which is an argument error rather than a render error: the call
            // that named the file is still on the stack.
            if let Err(error) = renderer.register_font(&family, &path) {
                return cx.throw_error(error.to_string());
            }
        }
    }
    Ok(renderer)
}

/// The [`ImageFormat`] a JavaScript format tag names.
///
/// A tag is a name the caller wrote, not a filename to infer from, which is
/// [`ImageFormat::from_named`]'s question rather than `from_extension`'s. This
/// held its own copy of that distinction until the Rust surface's `to_file`
/// turned out to need the same one and answer differently.
fn format_from_tag(tag: &str) -> Option<ImageFormat> {
    ImageFormat::from_named(tag)
}

/// Reads the encode options object, which may be absent or empty.
fn encode_options(
    cx: &mut FunctionContext<'_>,
    index: usize,
) -> NeonResult<EncodeOptions> {
    let mut options = EncodeOptions::default();
    let Some(given) = cx.argument_opt(index) else {
        return Ok(options);
    };
    if given.is_a::<JsUndefined, _>(cx) || given.is_a::<JsNull, _>(cx) {
        return Ok(options);
    }
    let Ok(given) = given.downcast::<JsObject, _>(cx) else {
        return cx.throw_type_error("encode options must be an object");
    };

    if let Some(quality) = given.get_opt::<JsNumber, _, _>(cx, "quality")? {
        options.quality = Some(quality.value(cx) as f32);
    }
    if let Some(lossless) = given.get_opt::<JsBoolean, _, _>(cx, "lossless")? {
        options.lossless = Some(lossless.value(cx));
    }
    if let Some(matte) = given.get_opt::<JsString, _, _>(cx, "matte")? {
        let css = matte.value(cx);
        let Some(colour) = meo_canvas_core::parse_color(&css) else {
            return cx.throw_type_error(format!(
                "matte {css:?} is not a CSS colour"
            ));
        };
        // Packed `0xRRGGBB`. The alpha is dropped rather than carried: a matte
        // is what an opaque format flattens transparency *against*, so a
        // translucent one describes nothing.
        options.matte = Some(
            (u32::from(colour.r) << 16)
                | (u32::from(colour.g) << 8)
                | u32::from(colour.b),
        );
    }
    if let Some(page) = given.get_opt::<JsNumber, _, _>(cx, "page")? {
        options.page = Some(page.value(cx) as usize);
    }
    if let Some(fps) = given.get_opt::<JsNumber, _, _>(cx, "fps")? {
        options.fps = Some(fps.value(cx) as f32);
    }
    if let Some(delays) = given.get_opt::<JsArray, _, _>(cx, "frameDelays")? {
        let length = delays.len(cx);
        options.frame_delays = Vec::with_capacity(length as usize);
        for index in 0..length {
            let delay = delays.get::<JsNumber, _, _>(cx, index)?.value(cx);
            options.frame_delays.push(delay as u32);
        }
    }
    if let Some(loops) = given.get_opt::<JsNumber, _, _>(cx, "loop")? {
        options.loops = Some(loops.value(cx) as u32);
    }
    Ok(options)
}

/// Paints a scene and hands back a surface that can be encoded more than once.
///
/// Takes the arena, the side values array and a `{ fonts }` object, and returns
/// an object with `encode(format, options)`, `release()`, and four readings of
/// the paint that already happened: `gpu`, `engine`, `pageCount` and `scale`.
///
/// # Why the readings are properties and not methods
///
/// None of them can change, none can fail, and all four describe a paint that
/// is already over. A caller reading `engine` after `release` should still
/// learn which rasteriser drew the bytes it is holding, and a method would
/// have to answer from a surface that is gone.
///
/// `gpu` and `engine` are both reported because **they disagree**: `gpu` is
/// what was asked for and `engine` is what asking got. A build with no GPU
/// backend compiled, a driver that declines, and a float `colorType` all
/// rasterise on the CPU whatever the request said. v1's canvas reports the pair
/// for that reason (`canvas.type.ts:1190`), and until now v2 reported neither
/// per canvas -- `backend()` answers for the build, which is a different
/// question. `gpu` is not among
/// them: it rides in the arena's header, beside the size and the scale a caller
/// writes it next to.
///
/// # Why this is not [`render`], and does not replace it
///
/// `render` folds the encode in and returns bytes, so two formats of one
/// picture cost two of everything. This is the retained form: one resolve, one
/// measure, one layout, one paint, and an encode per format asked for. It is
/// also the only shape in which `gpu` and `fonts` mean anything -- `render`
/// builds a default [`Renderer`], because it has no object to read them from --
/// and the only one that can offer a synchronous encode, which is what
/// `toBufferSync` and its siblings are. `render` still builds a default
/// [`Renderer`], but a scene reaching it now carries its own `gpu`, so the one
/// that mattered is no longer dropped.
///
/// # Why the paint runs on the event loop, unlike [`render`]
///
/// Because it cannot run anywhere else. `cx.task` requires its result to be
/// `Send`, and [`RenderedCanvas`] is not: it holds a Skia `PageRecorder` around
/// an `SkPictureRecorder`, and a `CanvasGradient` behind an `Rc<RefCell<_>>`.
/// Neither is a type this workspace defines, so the paint stays here and
/// `render` remains the export that keeps a paint off the loop.
///
/// The encodes are synchronous on purpose rather than by that constraint:
/// encoding is CPU work with no I/O in it, so a Promise per format would cost a
/// tick and defer nothing.
fn paint(mut cx: FunctionContext<'_>) -> JsResult<'_, JsObject> {
    let (slots, values) = arguments(&mut cx)?;
    let renderer = paint_options(&mut cx, 2)?;

    let scene = match arena::decode(&slots, &values) {
        Ok(scene) => scene,
        Err(error) => return cx.throw_error(error.to_string()),
    };
    let canvas = match renderer.render(&scene) {
        Ok(canvas) => canvas,
        Err(error) => return cx.throw_error(error.to_string()),
    };

    let surface = cx.empty_object();

    // Read off the canvas before it is boxed, and set as plain properties
    // rather than as methods. All four are facts about a paint that has already
    // happened: none can change, none can fail, and a caller reading `engine`
    // after `release` should still learn which rasteriser drew the bytes it is
    // holding. A method would go stale the moment the surface was freed.
    let gpu = cx.boolean(canvas.gpu());
    surface.set(&mut cx, "gpu", gpu)?;
    // `gpu` is the request and `engine` is the outcome, and they disagree
    // whenever a build has no GPU backend, a driver declines, or a float
    // `colorType` forces the CPU. Reporting only the request is what left a
    // caller unable to find out which they got.
    let engine = cx.string(canvas.engine());
    surface.set(&mut cx, "engine", engine)?;
    let pages = cx.number(canvas.page_count() as f64);
    surface.set(&mut cx, "pageCount", pages)?;
    let scale = cx.number(f64::from(canvas.scale()));
    surface.set(&mut cx, "scale", scale)?;

    let painted: Painted = Rc::new(RefCell::new(Some(canvas)));

    let held = Rc::clone(&painted);
    let encode = JsFunction::new(&mut cx, move |mut cx| {
        let tag = cx.argument::<JsString>(0)?.value(&mut cx);
        let Some(format) = format_from_tag(&tag) else {
            return cx.throw_type_error(format!(
                "no image format is called {tag:?}"
            ));
        };
        let options = encode_options(&mut cx, 1)?;

        let mut held = held.borrow_mut();
        let Some(canvas) = held.as_mut() else {
            return cx.throw_error(
                "this canvas has been released; encode before calling release()",
            );
        };
        match canvas.to_buffer(format, &options) {
            Ok(bytes) => JsBuffer::from_slice(&mut cx, &bytes),
            Err(error) => cx.throw_error(error.to_string()),
        }
    })?;
    surface.set(&mut cx, "encode", encode)?;

    let release = JsFunction::new(&mut cx, move |mut cx| {
        // Dropping the surface, not marking it dropped: the point of `release`
        // is that a caller who will not wait for a collection can free the
        // Skia allocation now. Calling it twice takes `None` and does nothing,
        // which is what the interface promises.
        painted.borrow_mut().take();
        Ok(cx.undefined())
    })?;
    surface.set(&mut cx, "release", release)?;

    Ok(surface)
}

/// The module's single registration point.
///
/// # Errors
///
/// Returns a Neon error if a name cannot be exported, which Node reports as a
/// failure to load the addon.
#[neon::main]
fn main(mut cx: ModuleContext<'_>) -> NeonResult<()> {
    cx.export_function("paint", paint)?;
    cx.export_function("render", render)?;
    cx.export_function("backend", backend)?;
    cx.export_function("sceneBytes", scene_bytes)?;
    cx.export_function("parseColor", parse_color)?;
    cx.export_function("isColor", is_color)?;
    Ok(())
}
