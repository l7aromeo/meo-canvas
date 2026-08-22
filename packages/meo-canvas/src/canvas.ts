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

/** The containers a canvas encodes to. */
export type Format = 'png' | 'jpg' | 'webp' | 'avif' | 'bmp' | 'ico' | 'tiff' | 'gif' | 'apng' | 'svg' | 'pdf' | 'raw'

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
  /** Encodes the painted pages and returns the bytes. */
  encode(format: Format, options: EncodeOptions): Uint8Array
  /** Frees the Skia surface. Calling it twice is not an error. */
  release(): void
}

/** Writes bytes to a path. Supplied by the caller of {@link Canvas}. */
export type WriteFile = (path: string, bytes: Uint8Array) => Promise<void>

/** Writes bytes to a path, blocking. */
export type WriteFileSync = (path: string, bytes: Uint8Array) => void

/** The media type each format is served as in a `data:` URL. */
const MEDIA_TYPES: Readonly<Record<Format, string>> = {
  png: 'image/png',
  jpg: 'image/jpeg',
  webp: 'image/webp',
  avif: 'image/avif',
  bmp: 'image/bmp',
  ico: 'image/x-icon',
  tiff: 'image/tiff',
  gif: 'image/gif',
  apng: 'image/apng',
  svg: 'image/svg+xml',
  pdf: 'application/pdf',
  raw: 'application/octet-stream',
}

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

  /** Encodes the canvas and resolves with the bytes. */
  async toBuffer(format: Format = 'png', options: EncodeOptions = {}): Promise<Uint8Array> {
    return this.toBufferSync(format, options)
  }

  /**
   * Encodes the canvas and returns the bytes.
   *
   * The same call without the `await`. Encoding is CPU work with no I/O, so
   * there is nothing for the asynchronous form to overlap with — it exists
   * because v1's did and a ported script should keep working.
   */
  toBufferSync(format: Format = 'png', options: EncodeOptions = {}): Uint8Array {
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

    out += ALPHABET[(triple >> 18) & 63] ?? ''
    out += ALPHABET[(triple >> 12) & 63] ?? ''
    out += remaining > 1 ? (ALPHABET[(triple >> 6) & 63] ?? '') : '='
    out += remaining > 2 ? (ALPHABET[triple & 63] ?? '') : '='
  }
  return out
}
