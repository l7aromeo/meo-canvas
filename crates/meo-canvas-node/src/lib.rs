//! The Node.js addon: the one `.node` binary and its only [`neon::main`],
//! which is why `meo-skia-canvas` is built with its own entry point off. It
//! decodes the `f64` arena ([`arena`] carries the specification), calls
//! `meo-canvas-core` and converts back; no rendering logic lives here.

// No source in this workspace writes `unsafe`, and this makes adding one a
// deliberate decision rather than a line that passes review. Integration tests
// are separate crates and not covered; their one `unsafe` is the
// `GlobalAlloc` that measures `codec::decode`'s reservation.
#![forbid(unsafe_code)]
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
    EncodeOptions, FetchFailure, ImageFormat, PreparedEncode, RenderedCanvas,
    Renderer, Surface, SurfaceOptions, chained, diagnostic::Diagnostic,
};
use neon::{prelude::*, types::buffer::TypedArray};

/// Reads the arena and its side array out of the call's arguments: every V8
/// read happens here, once, before the render starts.
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

/// Renders a scene given as an `f64` arena, side values, a format name and an
/// options object, resolving to a Buffer. The work runs on Node's worker pool
/// through `cx.task`, since a render is CPU-bound from resolve through encode
/// and would stall the event loop.
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

/// The render itself, with no V8 in reach, so the whole pipeline is callable
/// from a test without a Node process.
fn render_off_thread(
    slots: &[f64],
    values: &Values,
    format: &str,
) -> Result<Vec<u8>, String> {
    // Dropped: this path returns bytes to a worker thread and has no object
    // to hang them on. The render path below surfaces them.
    let (scene, _) =
        arena::decode(slots, values).map_err(|error| chained(&error))?;
    let format = ImageFormat::from_extension(format)
        .ok_or_else(|| format!("no image format is called {format:?}"))?;
    let renderer = Renderer::new();
    renderer
        .render_to_buffer(&scene, format, &EncodeOptions::default())
        .map_err(|error| chained(&error))
}

/// The encode itself, on a rayon worker. A panic is caught because rayon aborts
/// the process when one escapes a `spawn` -- a `SIGABRT` no `catch` reaches --
/// where the same panic on the event loop is a catchable error.
/// `AssertUnwindSafe` holds: the handle is moved in and dropped here.
fn encode_off_thread(prepared: PreparedEncode) -> Result<Vec<u8>, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        prepared
            .encode()
            .map(|image| image.bytes)
            .map_err(|error| chained(&error))
    }))
    .unwrap_or_else(|panic| {
        // Whatever `panic!` was given: a `&str` for a literal, a `String` for
        // a formatted message, and neither for a panic raised with something
        // else.
        let detail = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("no message");
        Err(format!("internal error while encoding: {detail}"))
    })
}

/// The write itself, on a rayon worker, catching a panic as
/// [`encode_off_thread`] does. Not an encode followed by a write: a format that
/// gathers every page streams into the file here, so a long animation is
/// bounded by disk rather than by RAM.
fn write_off_thread(
    prepared: PreparedEncode,
    path: std::path::PathBuf,
) -> Result<(), String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        prepared.write(path).map_err(|error| chained(&error))
    }))
    .unwrap_or_else(|panic| {
        let detail = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("no message");
        Err(format!("internal error while writing: {detail}"))
    })
}

/// Reports which rasteriser a render asks for (`requested`) and which one
/// asking got (`active`), by making a one-pixel canvas rather than reasoning
/// from compiled features. An object rather than a JSON string, so a field
/// added later reaches JavaScript as a property.
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
    .or_else(|error| cx.throw_error(chained(&error)))?;

    let object = cx.empty_object();
    let active = cx.string(probe.engine());
    object.set(&mut cx, "active", active)?;
    let requested = cx.boolean(requested);
    object.set(&mut cx, "requestsGpu", requested)?;
    Ok(object)
}

/// A CSS colour string as `{ r, g, b, a }` -- channels 0 to 255 and alpha 0 to
/// 1, unclamped so an animation can pass outside the gamut -- or `null`. Parsed
/// here so both surfaces share the renderer's parser, including the
/// `color(srgb ...)` pre-pass `csscolorparser` lacks.
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

/// Whether a string is a colour this renderer understands, defined as
/// [`parse_color`] returning something so the two cannot disagree.
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

/// Re-encodes an arena through the byte format, so the TypeScript round trip
/// compares scenes rather than pictures. Throws on a malformed arena, naming
/// the slot. Synchronous, unlike [`render`]: microseconds of work with no
/// rasteriser in it.
fn scene_bytes(mut cx: FunctionContext<'_>) -> JsResult<'_, JsBuffer> {
    let (slots, values) = arguments(&mut cx)?;
    // Dropped: this returns the encoded scene, not a canvas, so there is
    // nowhere for a report to go.
    let (scene, _) = match arena::decode(&slots, &values) {
        Ok(decoded) => decoded,
        Err(error) => return cx.throw_error(chained(&error)),
    };
    let bytes = meo_canvas_scene::codec::encode(&scene);
    JsBuffer::from_slice(&mut cx, &bytes)
}

/// The painted surface, shared by the methods that reach it. `Rc` rather than
/// `JsBox`, which needs `this` bound at every call and breaks a destructured
/// `const { encode } = canvas`; `RefCell` for `to_buffer`'s `&mut self`, and
/// `Option` so `release` can drop the surface early.
type Painted = Rc<RefCell<Option<RenderedCanvas>>>;

/// Reads the `{ fonts }` object `paint` is given, before anything is drawn.
/// `gpu` is not read here: it rides in the arena header beside `scale`, and a
/// second place to say it could disagree with the first.
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
                return cx.throw_error(chained(&error));
            }
        }
    }
    Ok(renderer)
}

/// The [`ImageFormat`] a JavaScript format tag names: a name the caller wrote,
/// which is [`ImageFormat::from_named`]'s question rather than
/// `from_extension`'s.
fn format_from_tag(tag: &str) -> Option<ImageFormat> {
    ImageFormat::from_named(tag)
}

/// Reads the `(path, format, options)` the two writing exports take, so they
/// cannot disagree. The format is a tag rather than inferred from the path: the
/// TypeScript surface resolves the extension, and its error names the file.
fn write_arguments(
    cx: &mut FunctionContext<'_>,
) -> NeonResult<(std::path::PathBuf, ImageFormat, EncodeOptions)> {
    let path = cx.argument::<JsString>(0)?.value(cx);
    let tag = cx.argument::<JsString>(1)?.value(cx);
    let Some(format) = format_from_tag(&tag) else {
        return cx
            .throw_type_error(format!("no image format is called {tag:?}"));
    };
    let options = encode_options(cx, 2)?;
    Ok((std::path::PathBuf::from(path), format, options))
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

/// Hangs `write` and `writeAsync` on the painted surface: two exports that
/// differ only in which thread the encode runs on, split out of [`paint`].
fn attach_writers<'a>(
    cx: &mut FunctionContext<'a>,
    surface: Handle<'a, JsObject>,
    painted: &Painted,
) -> NeonResult<()> {
    let held = Rc::clone(painted);
    let write = JsFunction::new(cx, move |mut cx| {
        let (path, format, options) = write_arguments(&mut cx)?;
        let mut held = held.borrow_mut();
        let Some(canvas) = held.as_mut() else {
            return cx.throw_error(
                "this canvas has been released; write before calling release()",
            );
        };
        match canvas
            .prepare_encode(format, &options)
            .and_then(|prepared| prepared.write(&path))
        {
            Ok(()) => Ok(cx.undefined()),
            Err(error) => cx.throw_error(chained(&error)),
        }
    })?;
    surface.set(cx, "write", write)?;

    let held = Rc::clone(painted);
    let write_async = JsFunction::new(cx, move |mut cx| {
        let (path, format, options) = write_arguments(&mut cx)?;

        // The half that needs the canvas, on the thread that owns it. A family
        // is registered per thread and painting is lazy, so the worker gets
        // pages with text already shaped and never consults a font.
        let prepared = {
            let mut held = held.borrow_mut();
            let Some(canvas) = held.as_mut() else {
                return cx.throw_error(
                    "this canvas has been released; write before calling release()",
                );
            };
            match canvas.prepare_encode(format, &options) {
                Ok(prepared) => prepared,
                Err(error) => return cx.throw_error(chained(&error)),
            }
        };

        let channel = cx.channel();
        let (deferred, promise) = cx.promise();
        rayon::spawn_fifo(move || {
            let result = write_off_thread(prepared, path);
            deferred.settle_with(&channel, move |mut cx| match result {
                Ok(()) => Ok(cx.undefined()),
                Err(message) => cx.throw_error(message),
            });
        });
        Ok(promise)
    })?;
    surface.set(cx, "writeAsync", write_async)?;
    Ok(())
}

/// The diagnostics as an array of `{ path, detail }`.
fn diagnostics_array<'cx>(
    cx: &mut FunctionContext<'cx>,
    found: &[Diagnostic],
) -> JsResult<'cx, JsArray> {
    let out = cx.empty_array();
    for (index, one) in found.iter().enumerate() {
        let entry = cx.empty_object();
        let path = cx.string(&one.path);
        entry.set(cx, "path", path)?;
        let detail = cx.string(&one.detail);
        entry.set(cx, "detail", detail)?;
        // **Absent rather than `null`.** The property is optional on the
        // TypeScript side, and a diagnostic that did not come from markup has
        // no place to point at -- `'offset' in d` and `d.offset !== undefined`
        // then agree, where a `null` would make one of them lie.
        if let Some(offset) = one.offset {
            let offset = cx.number(offset as f64);
            entry.set(cx, "offset", offset)?;
        }
        out.set(cx, u32::try_from(index).unwrap_or(u32::MAX), entry)?;
    }
    Ok(out)
}

/// Every unresolved image source, as an array of plain objects -- always an
/// array, so `warnings.length === 0` needs no guard -- with the classification
/// as a keyword and any HTTP code beside it.
fn warnings_array<'cx>(
    cx: &mut FunctionContext<'cx>,
    canvas: &RenderedCanvas,
) -> JsResult<'cx, JsArray> {
    let warnings = cx.empty_array();
    for (index, warning) in canvas.warnings().iter().enumerate() {
        let entry = cx.empty_object();
        let url = cx.string(&warning.url);
        entry.set(cx, "url", url)?;
        let detail = cx.string(&warning.detail);
        entry.set(cx, "detail", detail)?;
        let nodes = cx.number(warning.nodes as f64);
        entry.set(cx, "nodes", nodes)?;
        let (tag, status) = match warning.failure {
            FetchFailure::Status(code) => ("status", Some(code)),
            FetchFailure::HostNotFound => ("host-not-found", None),
            FetchFailure::BadUrl => ("bad-url", None),
            FetchFailure::Transport => ("transport", None),
            FetchFailure::TooLarge => ("too-large", None),
            _ => ("other", None),
        };
        let failure = cx.string(tag);
        entry.set(cx, "failure", failure)?;
        if let Some(code) = status {
            let code = cx.number(f64::from(code));
            entry.set(cx, "status", code)?;
        }
        warnings.set(cx, index as u32, entry)?;
    }
    Ok(warnings)
}

/// Paints a scene and returns a surface that encodes more than once: `encode`,
/// `release`, and properties for `gpu`, `engine`, `pageCount` and `scale`. The
/// paint runs on the event loop because [`RenderedCanvas`] is not `Send`, which
/// `cx.task` requires; [`render`] is the export that keeps a paint off it.
fn paint(mut cx: FunctionContext<'_>) -> JsResult<'_, JsObject> {
    let (slots, values) = arguments(&mut cx)?;
    let renderer = paint_options(&mut cx, 2)?;

    let (scene, diagnostics) = match arena::decode(&slots, &values) {
        Ok(decoded) => decoded,
        Err(error) => return cx.throw_error(chained(&error)),
    };
    let canvas = match renderer.render(&scene) {
        Ok(canvas) => canvas,
        Err(error) => return cx.throw_error(chained(&error)),
    };

    let surface = cx.empty_object();

    // Read off the canvas before it is boxed, as plain properties: all four are
    // facts about a paint already over, and a method would go stale once
    // `release` freed the surface.
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

    // A fifth such fact, built now for the same reason, and always an array so
    // `warnings.length === 0` needs no guard.
    let warnings = warnings_array(&mut cx, &canvas)?;
    surface.set(&mut cx, "warnings", warnings)?;
    // Beside the warnings and not merged with them: a warning says the world
    // did not answer, a diagnostic says the input did not say what it meant,
    // and a caller acts on the two differently.
    let reported = diagnostics_array(&mut cx, &diagnostics)?;
    surface.set(&mut cx, "diagnostics", reported)?;

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
            Err(error) => cx.throw_error(chained(&error)),
        }
    })?;
    surface.set(&mut cx, "encode", encode)?;

    let held = Rc::clone(&painted);
    let encode_async = JsFunction::new(&mut cx, move |mut cx| {
        let tag = cx.argument::<JsString>(0)?.value(&mut cx);
        let Some(format) = format_from_tag(&tag) else {
            return cx.throw_type_error(format!(
                "no image format is called {tag:?}"
            ));
        };
        let options = encode_options(&mut cx, 1)?;

        // The half that needs the canvas, on the thread that owns it, so the
        // caller keeps drawing while this encode runs. A family is registered
        // per thread and painting is lazy, so the worker gets pages with text
        // already shaped and never consults a font.
        let prepared = {
            let mut held = held.borrow_mut();
            let Some(canvas) = held.as_mut() else {
                return cx.throw_error(
                    "this canvas has been released; encode before calling release()",
                );
            };
            match canvas.prepare_encode(format, &options) {
                Ok(prepared) => prepared,
                Err(error) => return cx.throw_error(chained(&error)),
            }
        };

        let channel = cx.channel();
        let (deferred, promise) = cx.promise();
        rayon::spawn_fifo(move || {
            let result = encode_off_thread(prepared);
            deferred.settle_with(&channel, move |mut cx| match result {
                Ok(bytes) => JsBuffer::from_slice(&mut cx, &bytes),
                Err(message) => cx.throw_error(message),
            });
        });
        Ok(promise)
    })?;
    surface.set(&mut cx, "encodeAsync", encode_async)?;

    attach_writers(&mut cx, surface, &painted)?;

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

/// The module's single registration point. A name that cannot be exported is
/// a Neon error, which Node reports as a failure to load the addon.
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
