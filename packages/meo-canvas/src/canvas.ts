/**
 * What a rendered canvas exposes: the ways to get the picture out.
 *
 * v1's output surface, because v1 is the reference and a ported script should
 * not have to change how it writes a file.
 *
 * A canvas comes back from `Root`, and every method here reads it: `toBuffer`
 * and `toBufferSync` for the bytes, `toFile` and `toFileSync` to write them,
 * `toURL`/`toURLSync` and `toDataURL` for a `data:` URL. The worked example
 * arrives with `Root` — an example naming a function that does not exist yet
 * would be one this package's own gate refuses to compile, which is the point
 * of having it.
 *
 * **Two formats cost one paint.** Resolving, measuring, laying out and painting
 * happen once inside `Root`; each of these methods is only an encode.
 *
 * The sync variants are ordinary functions. v1 needed `Atomics.wait` on a
 * `SharedArrayBuffer` for them because its canvas lived in a worker; this one
 * does not, so `toBufferSync` is the same call without the `await`.
 *
 * @packageDocumentation
 */

// **`Buffer` is a global, and a consumer's compiler does not have it.** Every
// method below answers a Node `Buffer`, which is the value the addon actually
// returns. TypeScript 6 does not auto-include `node_modules/@types`, so a
// consumer who has not written `"types": ["node"]` cannot resolve the name --
// it becomes `any`, and `skipLibCheck`, which `tsc --init` writes as `true`,
// swallows the error that would have said so. The declarations compiled here
// and degraded there, and nothing in this repository could see it: every
// in-tree typecheck sets `types`.
//
// **Two obvious fixes were measured and neither works.**
// `import type { Buffer } from 'node:buffer'` survives into the emitted `.d.ts`
// and still does not resolve: a bare `node:` specifier needs `@types/node`
// already loaded, which is the thing the consumer has not done. And a
// `/// <reference types="node" />` here, which does resolve, is elided from
// declaration emit -- it reaches `dist/canvas.js` and never `dist/canvas.d.ts`,
// with or without `types` in the build config.
//
// So the reference is added to the emitted declaration after `tsc` runs, by
// `tools/reference-node-types.mjs`, and `verify-package.mjs` proves it landed
// by compiling a consumer whose control has to fail. That is also what the
// `@types/node` entry in `dependencies` is for: a reference is followed
// transitively, and the package sits beside this one under both a hoisted and
// an isolated layout only because it is a real dependency.
import { MEDIA_TYPES, type Format } from './generated/media-types.js'

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
  /** Which page a single-page format writes, counting from zero. */
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
 * place. Every method is synchronous: encoding is CPU work with no I/O, and
 * {@link Canvas} is what decides which calls a caller awaits.
 */
export interface NativeCanvas {
  /**
   * Encodes the painted pages and returns the bytes.
   *
   * A Node `Buffer`, which is what the addon hands back: it builds the result
   * with Neon's `JsBuffer::from_slice` (`crates/meo-canvas-node/src/lib.rs:485`).
   * A caller supplying their own native surface for a test may return any
   * `Buffer`; a plain `Uint8Array` would be a different value from the one this
   * package ships.
   */
  encode(format: Format, options: EncodeOptions): Buffer
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
}

/** Writes bytes to a path. Supplied by the caller of {@link Canvas}. */
export type WriteFile = (path: string, bytes: Uint8Array) => Promise<void>

/** Writes bytes to a path, blocking. */
export type WriteFileSync = (path: string, bytes: Uint8Array) => void

/**
 * The format a filename's extension names.
 *
 * `toFile` takes a path rather than a format, as v1 does, so the extension has
 * to say which container to write. An extension naming none is an error rather
 * than a default: writing a PNG because nothing said otherwise turns a typo
 * into a file whose name lies about its contents.
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

  /** How bytes reach the filesystem, awaited. */
  readonly #writeFile: WriteFile

  /** How bytes reach the filesystem, blocking. */
  readonly #writeFileSync: WriteFileSync

  /** Whether {@link Canvas.release} has already run. */
  #released = false

  /**
   * Wraps a native surface.
   *
   * The filesystem is injected rather than imported so this class can be tested
   * without touching a disk, and so a caller in an environment without
   * `node:fs` can supply their own.
   */
  constructor(native: NativeCanvas, writeFile: WriteFile, writeFileSync: WriteFileSync) {
    this.#native = native
    this.#writeFile = writeFile
    this.#writeFileSync = writeFileSync
  }

  /**
   * Encodes the canvas and resolves with the bytes.
   *
   * **This is where the pixels are allocated**, because painting recorded a
   * drawing rather than a bitmap. So this is what costs time in proportion to
   * the canvas area — about 11 ms at 800×800, 65 ms at 2000×2000 and 256 ms at
   * 4000×4000 on one machine — and this is what throws when the area is more
   * than the host can allocate, however long ago the size was chosen.
   *
   * See {@link Canvas.toBufferSync} for why the type is `Buffer`.
   */
  async toBuffer(format: Format = 'png', options: EncodeOptions = {}): Promise<Buffer> {
    return this.toBufferSync(format, options)
  }

  /**
   * Encodes the canvas and returns the bytes.
   *
   * The same call without the `await`. Encoding is CPU work with no I/O, so
   * there is nothing for the asynchronous form to overlap with — it exists
   * because v1's did and a ported script should keep working.
   *
   * # Why `Buffer` and not `Uint8Array`
   *
   * **Because a `Buffer` is what already came back.** The addon returns a
   * Neon `JsBuffer` and always has; the declaration said `Uint8Array`, which
   * was a false statement about the value. `Buffer` extends `Uint8Array`, so
   * this narrows the type without changing a byte, and every caller that
   * wanted either is satisfied.
   *
   * **It is not a fix for sharp.** sharp accepts a plain `Uint8Array` — its
   * `SharpInput` names the type, and 0.34.5, 0.35.3 and 0.35.4 each read one
   * back to `png 410x140` when measured. Whatever a caller hit handing this to
   * sharp, this was not it, and the type being honest is worth having on its
   * own.
   */
  toBufferSync(format: Format = 'png', options: EncodeOptions = {}): Buffer {
    this.#assertLive()
    return this.#native.encode(format, options)
  }

  /** Encodes the canvas and writes it to `path`. */
  async toFile(path: string, options: EncodeOptions = {}): Promise<void> {
    const bytes = this.toBufferSync(formatForPath(path), options)
    await this.#writeFile(path, bytes)
  }

  /** Encodes the canvas and writes it to `path`, blocking. */
  toFileSync(path: string, options: EncodeOptions = {}): void {
    this.#writeFileSync(path, this.toBufferSync(formatForPath(path), options))
  }

  /** Encodes the canvas and resolves with a `data:` URL. */
  async toURL(format: Format = 'png', options: EncodeOptions = {}): Promise<string> {
    return this.toURLSync(format, options)
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
   * DOM method it is named after is both. v1 has it for the same reason.
   */
  toDataURL(format: Format = 'png', quality?: number): string {
    return this.toURLSync(format, quality === undefined ? {} : { quality })
  }

  // -- Async convenience getters --------------------------------------
  //
  // One per format, each `toBuffer(format)` with the name said once instead of
  // twice. v1 has all twelve and none of them is deprecated, so they are live
  // API a ported script may be written against.
  //
  // Getters rather than methods because that is v1's spelling. The cost is not
  // what it looks like: a getter on the prototype is not invoked by a spread,
  // and Node's inspector prints `[Getter]` rather than calling it, so `console
  // .log(canvas)` does not start twelve encodes.

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
  // Four readings rather than four methods, and readable after
  // {@link Canvas.release}: each is a fact about a paint that has already
  // happened, so none can change and none can fail. A caller holding bytes it
  // has released should still be able to say which rasteriser drew them.

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
   * rasterise on the CPU whatever `gpu` says. v1 reports both for this reason,
   * and without it a caller who asks for the GPU and gets the CPU has no way to
   * find out — the same shape of invisibility that hid a missing build feature
   * from this project for a session.
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

  /** Refuses a call on a surface that has been freed. */
  #assertLive(): void {
    if (this.#released) {
      throw new Error('this canvas was released; encode before calling release()')
    }
  }
}

/**
 * Base64 without depending on `Buffer`.
 *
 * `Buffer.toString('base64')` would be shorter and ties this file to Node. The
 * package targets Node today and a data URL is not on any hot path, so the
 * portable form costs nothing worth naming.
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
