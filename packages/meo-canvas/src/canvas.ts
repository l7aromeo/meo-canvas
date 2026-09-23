/**
 * What a rendered canvas exposes: the ways to get the picture out. Painting
 * happens once in `Root`, so each method is only an encode; the asynchronous
 * forms hand it to a worker and the synchronous ones run it on the calling thread.
 * @packageDocumentation
 */

// `Buffer` is a Node global a consumer's compiler may lack: without `"types":
// ["node"]` it becomes `any` and `skipLibCheck` hides the error. Neither `import
// type` from `node:buffer` nor a `/// <reference>` here survives into the `.d.ts`,
// so `tools/reference-node-types.mjs` adds it, and `verify-package.mjs` proves it.
import { MEDIA_TYPES, type Format } from './generated/media-types.js'
import type { Diagnostic, ImageWarning } from './index.js'

export type { Format }

/**
 * Quality and container settings that only some formats read.
 *
 * A format that ignores a field ignores it. The timing fields are the
 * exception: naming a frame rate for a format with no clock is an error rather
 * than something quietly dropped, because a caller who wrote `fps` and got a
 * still image asked for something that did not happen.
 */
export interface EncodeOptions {
  /** Lossy quality from `0` to `1`, read by JPEG, WebP and AVIF. */
  readonly quality?: number
  /** Encode WebP without loss. */
  readonly lossless?: boolean
  /** The colour transparency is flattened against, for a format with no alpha. */
  readonly matte?: string
  /**
   * Which page is written, counting from zero.
   *
   * **Not only for a single-page format.** Naming a page wins over a format
   * that would otherwise gather them all, so a GIF asked for page 1 of three
   * is a one-frame GIF and a PDF asked for one page is a one-sheet PDF. Left
   * out, a spanning format writes every page and every other format writes the
   * last one.
   *
   * An index past the last page is refused rather than clamped.
   */
  readonly page?: number
  /** Frames per second for an animated format. */
  readonly fps?: number
  /** Per-frame durations in milliseconds, one per written page. */
  readonly frameDelays?: readonly number[]
  /** How many times an animation plays. Absent plays it forever. */
  readonly loop?: number
}

/**
 * The retained surface, as the addon hands it over.
 *
 * Declared here rather than imported so this file compiles without the native
 * module, and so the shape the addon has to satisfy is written down in one
 * place.
 *
 * **Two encodes rather than one, and the difference is which thread does the
 * work.** {@link NativeCanvas.encode} runs on the event loop;
 * {@link NativeCanvas.encodeAsync} takes the half of an export that needs the
 * canvas, hands the rest to a worker, and resolves when that worker finishes.
 * Both are required: a surface offering only the synchronous one would make
 * {@link Canvas.toBuffer} a promise that was already settled, which is what
 * this pair exists to stop being true.
 */
export interface NativeCanvas {
  /**
   * Encodes the painted pages and returns the bytes.
   *
   * A Node `Buffer`, which is what the addon hands back: it builds the result
   * with Neon's `JsBuffer::from_slice` in `crates/meo-canvas-node/src/lib.rs`.
   * A caller supplying their own native surface for a test may return any
   * `Buffer`; a plain `Uint8Array` would be a different value from the one this
   * package ships.
   */
  encode(format: Format, options: EncodeOptions): Buffer
  /**
   * Encodes the painted pages on a worker and resolves with the bytes.
   *
   * The same bytes {@link NativeCanvas.encode} returns — the addon defines the
   * two as one path rather than two, so they cannot drift — produced without
   * occupying the event loop for the encode.
   *
   * Rejects rather than throwing once the work has left the calling thread:
   * a format the addon does not know is a synchronous `TypeError`, because
   * there is still a call to throw from, while a failure inside the encode
   * settles the promise.
   */
  encodeAsync(format: Format, options: EncodeOptions): Promise<Buffer>
  /**
   * Encodes the painted pages straight into a file, blocking.
   *
   * Not {@link NativeCanvas.encode} followed by a write, and the difference is
   * the whole reason it exists: a format that gathers every page streams into
   * the file, where encoding first has to hold the entire document in memory
   * to hand it back. A long animation is bounded by disk here and by RAM
   * there.
   *
   * The format is passed rather than inferred from the path. The extension is
   * resolved on this side, because the error for an unrecognised one names the
   * file; inferring it again on the far side would be one question with two
   * places to answer it.
   */
  write(path: string, format: Format, options: EncodeOptions): void
  /**
   * Encodes the painted pages straight into a file, on a worker.
   *
   * {@link NativeCanvas.write} with the encode moved off the event loop, the
   * way {@link NativeCanvas.encodeAsync} moves it for a buffer.
   */
  writeAsync(path: string, format: Format, options: EncodeOptions): Promise<void>
  /** Frees the Skia surface. Calling it twice is not an error. */
  release(): void
  /** Whether the GPU was asked for. */
  readonly gpu: boolean
  /** Which rasteriser drew the pages: `'gpu'` or `'cpu'`. */
  readonly engine: string
  /** How many pages were painted. */
  readonly pageCount: number
  /** The device-pixel multiplier the pages were drawn at. */
  readonly scale: number
  /** Every image source that could not be resolved, in node order. */
  readonly warnings: readonly ImageWarning[]
  /** What the input said that could not be used. */
  readonly diagnostics: readonly Diagnostic[]
}

/**
 * The format a filename's extension names. An extension naming none is an error
 * rather than a default, since writing a PNG unasked turns a typo into a file
 * whose name lies about its contents.
 */
function formatForPath(path: string): Format {
  const dot = path.lastIndexOf('.')
  const extension = dot === -1 ? '' : path.slice(dot + 1).toLowerCase()
  const named: Format | undefined = extension === 'jpeg' ? 'jpg' : (extension as Format)

  if (extension !== '' && named in MEDIA_TYPES) return named
  throw new Error(`cannot tell the format from ${JSON.stringify(path)}; name the file with an extension such as .png`)
}

/**
 * A painted canvas, and the ways to read it back.
 *
 * Constructed by `Root`; a caller never builds one. It holds the native surface
 * and nothing else, so every method here is an encode of work already done.
 */
export class Canvas {
  /** The painted surface. */
  readonly #native: NativeCanvas

  /** Whether {@link Canvas.release} has already run. */
  #released = false

  /**
   * Wraps a native surface. This is the one injection point: a test, or a host
   * without `node:fs`, substitutes a {@link NativeCanvas}.
   */
  constructor(native: NativeCanvas) {
    this.#native = native
  }

  /**
   * Encodes the canvas on a worker thread and resolves with the bytes.
   *
   * **This is where the pixels are allocated**, because painting recorded a
   * drawing rather than a bitmap. So this is what costs time in proportion to
   * the canvas area — about 11 ms at 800×800, 65 ms at 2000×2000 and 256 ms at
   * 4000×4000 on one machine — and this is what throws when the area is more
   * than the host can allocate, however long ago the size was chosen.
   *
   * **That time is not spent on the event loop.** What crosses to the worker is
   * the recorded pages, not the scene — the drawing is already shaped, so the
   * worker consults no font and cannot substitute one. The loop keeps only the
   * half of an export that needs the canvas, which does not grow with area.
   *
   * See {@link Canvas.toBufferSync} for why the type is `Buffer`.
   */
  async toBuffer(format: Format = 'png', options: EncodeOptions = {}): Promise<Buffer> {
    this.#assertLive()
    return this.#native.encodeAsync(format, options)
  }

  /**
   * Encodes the canvas and returns the bytes.
   *
   * The same bytes {@link Canvas.toBuffer} resolves with, produced on the
   * calling thread instead of a worker: this one blocks the event loop for the
   * whole encode, which is what a script wants and what a server does not.
   *
   * A `Buffer`, because that is the value the addon returns; `Buffer` extends
   * `Uint8Array`, so a caller wanting either is satisfied.
   */
  toBufferSync(format: Format = 'png', options: EncodeOptions = {}): Buffer {
    this.#assertLive()
    return this.#native.encode(format, options)
  }

  /**
   * Encodes the canvas on a worker and writes it to `path`.
   *
   * **The bytes never come back through JavaScript.** The file is written where
   * it is encoded, and a spanning format streams into it page by page, so a long
   * animation is bounded by disk rather than by memory.
   */
  async toFile(path: string, options: EncodeOptions = {}): Promise<void> {
    this.#assertLive()
    await this.#native.writeAsync(path, formatForPath(path), options)
  }

  /**
   * Encodes the canvas and writes it to `path`, blocking.
   *
   * The same call on the calling thread. It asks the format question once, on
   * this side, and everything after that is the one decision made in one
   * place — see {@link NativeCanvas.write}.
   */
  toFileSync(path: string, options: EncodeOptions = {}): void {
    this.#assertLive()
    this.#native.write(path, formatForPath(path), options)
  }

  /**
   * Encodes the canvas on a worker and resolves with a `data:` URL.
   *
   * The base64 runs on the calling thread, which is where it has to: it is a
   * string, and a string cannot cross to a worker without being copied twice.
   * It is also the cheap end — a 4000×4000 PNG is tens of kilobytes by the
   * time it is bytes, against the hundred milliseconds that made it.
   */
  async toURL(format: Format = 'png', options: EncodeOptions = {}): Promise<string> {
    const bytes = await this.toBuffer(format, options)
    return `data:${MEDIA_TYPES[format]};base64,${toBase64(bytes)}`
  }

  /** Encodes the canvas and returns a `data:` URL. */
  toURLSync(format: Format = 'png', options: EncodeOptions = {}): string {
    const bytes = this.toBufferSync(format, options)
    return `data:${MEDIA_TYPES[format]};base64,${toBase64(bytes)}`
  }

  /**
   * The `HTMLCanvasElement` spelling of {@link Canvas.toURLSync}.
   *
   * Synchronous and taking a quality rather than an options object, because the
   * DOM method it is named after is both.
   */
  toDataURL(format: Format = 'png', quality?: number): string {
    return this.toURLSync(format, quality === undefined ? {} : { quality })
  }

  // -- Async convenience getters: one per format, each `toBuffer(format)` ------
  // Getters, and a getter on the prototype is neither invoked by a spread nor
  // called by Node's inspector, which prints `[Getter]` — so `console.log(canvas)`
  // does not start twelve encodes.

  /** `toBuffer('png')`. Lossless, and the format to reach for without a reason not to. */
  get png(): Promise<Uint8Array> {
    return this.toBuffer('png')
  }

  /** `toBuffer('jpg')`. Lossy and opaque — no alpha channel. */
  get jpg(): Promise<Uint8Array> {
    return this.toBuffer('jpg')
  }

  /** `toBuffer('webp')`. Smaller than PNG at the same quality, and takes every page as an animation. */
  get webp(): Promise<Uint8Array> {
    return this.toBuffer('webp')
  }

  /** `toBuffer('avif')`. Smaller again, and slower to encode. */
  get avif(): Promise<Uint8Array> {
    return this.toBuffer('avif')
  }

  /** `toBuffer('bmp')`. Uncompressed, and rarely what is wanted. */
  get bmp(): Promise<Uint8Array> {
    return this.toBuffer('bmp')
  }

  /** `toBuffer('ico')`. The Windows icon container. */
  get ico(): Promise<Uint8Array> {
    return this.toBuffer('ico')
  }

  /** `toBuffer('tiff')`. Lossless, and what a print pipeline usually asks for. */
  get tiff(): Promise<Uint8Array> {
    return this.toBuffer('tiff')
  }

  /** `toBuffer('gif')`. Every page as a frame, at 256 colours. */
  get gif(): Promise<Uint8Array> {
    return this.toBuffer('gif')
  }

  /** `toBuffer('apng')`. Every page as a frame, with PNG's colour and alpha. */
  get apng(): Promise<Uint8Array> {
    return this.toBuffer('apng')
  }

  /** `toBuffer('svg')`. Vector, so text stays text. */
  get svg(): Promise<Uint8Array> {
    return this.toBuffer('svg')
  }

  /** `toBuffer('pdf')`. Vector, and every page a page. */
  get pdf(): Promise<Uint8Array> {
    return this.toBuffer('pdf')
  }

  /** `toBuffer('raw')`. The pixels, unencoded. */
  get raw(): Promise<Uint8Array> {
    return this.toBuffer('raw')
  }

  /**
   * Frees the Skia surface now rather than at the next collection.
   *
   * Optional. The surface is freed when this canvas is collected either way;
   * this only makes it sooner, which a server rendering thousands of images
   * wants and a script does not need. Calling it twice is not an error, and
   * every other method throws afterwards rather than reading freed memory.
   */
  release(): void {
    if (this.#released) return
    this.#released = true
    this.#native.release()
  }

  /** Whether the surface is still there. */
  get released(): boolean {
    return this.#released
  }

  // -- What the paint settled on --------------------------------------
  //
  // Readings rather than methods, and readable after {@link Canvas.release}: each
  // is a fact about a paint already done, so none can change and none can fail.

  /**
   * Whether the GPU was asked for.
   *
   * **Asking is not getting** — compare {@link Canvas.engine}. This is what
   * `Root` was told, and it is `true` when nothing was said, because that is
   * the renderer's own default.
   */
  get gpu(): boolean {
    return this.#native.gpu
  }

  /**
   * Which rasteriser drew the pages: `'gpu'` or `'cpu'`.
   *
   * The outcome rather than the request, and they disagree: a build with no GPU
   * backend compiled, a driver that declines, and a float `colorType` all
   * rasterise on the CPU whatever `gpu` says. Without this, a caller who asks
   * for the GPU and gets the CPU has no way to find out.
   */
  get engine(): string {
    return this.#native.engine
  }

  /**
   * How many pages were painted.
   *
   * What a page means is the format's answer: a frame for GIF and APNG, a sheet
   * for PDF and TIFF, one size of the same icon for ICO.
   */
  get pageCount(): number {
    return this.#native.pageCount
  }

  /**
   * The device-pixel multiplier the pages were drawn at.
   *
   * Layout always solves at one, so this is resolution rather than anything
   * about where things sit.
   */
  get scale(): number {
    return this.#native.scale
  }

  /**
   * Every image source that could not be resolved, in node order.
   *
   * **Always an array**, so `canvas.warnings.length === 0` is a check you
   * write once and never guard — empty is the ordinary answer, not a missing
   * one. One entry per distinct source rather than per node, because sixty
   * nodes drawing one dead URL is one thing that went wrong.
   *
   * Populated whatever `onImageError` on `Root` chose, **including
   * `'ignore'`**. That option decides what is drawn, never what is known: a
   * caller who finds the placeholder distracting keeps the diagnostic.
   *
   * A render that finishes with warnings is not one that succeeded quietly.
   * Each entry is a picture that was asked for and did not arrive.
   */
  get warnings(): readonly ImageWarning[] {
    return this.#native.warnings
  }

  /**
   * What this render's input said that could not be used.
   *
   * **Separate from {@link Canvas.warnings}, and the distinction is the
   * point.** A warning says the world did not answer — a fetch that failed. A
   * diagnostic says the input did not say what it meant, which is the
   * caller's to fix and not the network's.
   *
   * The case these exist for is one where the render is *correct*:
   * `<color=#ff00>` is conformant, four-digit `#RGBA`, yellow at alpha zero,
   * and a caller who truncated `#ff0000` sees blank text either way. Nothing
   * in the picture can tell them apart.
   *
   * **Always an array**, as `warnings` is.
   */
  get diagnostics(): readonly Diagnostic[] {
    return this.#native.diagnostics
  }

  /** Refuses a call on a surface that has been freed. */
  #assertLive(): void {
    if (this.#released) {
      throw new Error('this canvas was released; encode before calling release()')
    }
  }
}

/**
 * Base64 without depending on `Buffer`, which would tie this file to Node; a data
 * URL is not on any hot path, so the portable form costs nothing worth naming.
 */
function toBase64(bytes: Uint8Array): string {
  const ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
  let out = ''
  for (let index = 0; index < bytes.length; index += 3) {
    const a = bytes[index] ?? 0
    const b = bytes[index + 1] ?? 0
    const c = bytes[index + 2] ?? 0
    const triple = (a << 16) | (b << 8) | c
    const remaining = bytes.length - index

    // `charAt` rather than an index: an index into a string may be `undefined`
    // as far as the type system knows, and a `?? ''` beside it would be a
    // fallback for something six bits masked to sixty-four cannot do.
    out += ALPHABET.charAt((triple >> 18) & 63)
    out += ALPHABET.charAt((triple >> 12) & 63)
    out += remaining > 1 ? ALPHABET.charAt((triple >> 6) & 63) : '='
    out += remaining > 2 ? ALPHABET.charAt(triple & 63) : '='
  }
  return out
}
