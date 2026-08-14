import { Canvas } from 'meo-skia-canvas'

/** A colour resolved to sRGB. Channels are 0–255; alpha is 0–1. */
export interface Rgba {
  r: number
  g: number
  b: number
  a: number
}

const CHANNEL_MAX = 255

/**
 * Two sentinels used to tell a rejected colour from an accepted one.
 *
 * Assigning an unparseable value to `fillStyle` leaves the previous value in place instead of
 * raising — the behaviour the HTML canvas spec requires. A single probe therefore cannot tell
 * "invalid" from "valid and equal to whatever was already set". Probing twice from two different
 * starting points can: a colour the engine accepts produces the same answer from both, while a
 * rejected one leaves each sentinel untouched and the two answers differ.
 */
const PROBE_A = '#000000'
const PROBE_B = '#ffffff'

/**
 * Scratch canvas used to make the engine do the parsing.
 *
 * Created once and kept: it is a single pixel, and building one per call would dominate the cost
 * of interpolating a colour sixty times a second.
 */
let scratch: ReturnType<Canvas['getContext']> | null = null

function context(): NonNullable<typeof scratch> {
  if (!scratch) {
    scratch = new Canvas(1, 1).getContext('2d')
  }
  return scratch!
}

/**
 * Parsed colours, keyed by the exact input string.
 *
 * An animation asks for the same two endpoints on every page, so this turns a per-page parse into
 * a map lookup. Entries are frozen, and callers get a copy — a mutable cached object would let one
 * caller corrupt every later reader.
 */
const cache = new Map<string, Readonly<Rgba>>()

/**
 * Resolves any colour the rendering engine accepts into sRGB.
 *
 * The engine does the work: the colour is painted onto a one-pixel canvas and read back. That is
 * deliberately not a CSS parser — it means every syntax the engine supports is supported here for
 * free, today and after any upgrade, including `lab()`, `oklch()` and `color(display-p3 …)`, which
 * a hand-written parser would either miss or approximate.
 *
 * Wide-gamut inputs are converted by the engine to the canvas's own sRGB space, so a colour outside
 * sRGB arrives clipped — the same clipping it would get when drawn.
 * @throws if the engine does not recognise the colour, rather than returning a misleading black.
 */
export function parseColor(css: string): Rgba {
  const hit = cache.get(css)
  if (hit) return { ...hit }

  const ctx = context()

  ctx.fillStyle = PROBE_A
  ctx.fillStyle = css
  const fromA = String(ctx.fillStyle)

  ctx.fillStyle = PROBE_B
  ctx.fillStyle = css
  const fromB = String(ctx.fillStyle)

  if (fromA !== fromB) {
    throw new Error(`[canvas] "${css}" is not a colour the renderer recognises`)
  }

  const parsed = Object.freeze(fromNormalized(fromA, ctx))
  cache.set(css, parsed)
  return { ...parsed }
}

/** `#rrggbb` — what the engine returns for any opaque colour already inside sRGB. */
const HEX_RE = /^#([0-9a-f]{6})$/i
/** `rgba(r, g, b, a)` — what it returns once alpha is involved. */
const RGBA_RE = /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+)\s*)?\)$/i
/** `color(space c1 c2 c3 [/ a])` — what it returns for wide-gamut and out-of-gamut colours. */
const COLOR_FN_RE = /^color\((\S+)\s+([\d.eE+-]+)\s+([\d.eE+-]+)\s+([\d.eE+-]+)\s*(?:\/\s*([\d.eE+-]+)\s*)?\)$/

/**
 * Turns the engine's normalised colour string into sRGB channels.
 *
 * Read from the string wherever possible rather than from a painted pixel, because painting is
 * lossy in one specific and common case: the canvas stores premultiplied alpha, so a fully
 * transparent colour comes back as transparent black and loses its hue. Fading in from
 * `rgba(255, 0, 0, 0)` would then start from black instead of red. The string keeps the channels.
 *
 * Only a `color()` result needs the engine's help, since converting a space like `display-p3` into
 * sRGB is exactly the arithmetic worth not reimplementing. Even then alpha comes from the string,
 * and the colour is painted opaque so premultiplication cannot touch the channels.
 */
function fromNormalized(normalized: string, ctx: NonNullable<typeof scratch>): Rgba {
  const hex = HEX_RE.exec(normalized)
  if (hex) {
    const value = parseInt(hex[1], 16)
    return { r: (value >> 16) & 0xff, g: (value >> 8) & 0xff, b: value & 0xff, a: 1 }
  }

  const rgba = RGBA_RE.exec(normalized)
  if (rgba) {
    return {
      r: Number(rgba[1]),
      g: Number(rgba[2]),
      b: Number(rgba[3]),
      a: rgba[4] === undefined ? 1 : Number(rgba[4]),
    }
  }

  const fn = COLOR_FN_RE.exec(normalized)
  if (fn) {
    const [, space, c1, c2, c3, alpha] = fn
    const opaque = `color(${space} ${c1} ${c2} ${c3})`

    ctx.clearRect(0, 0, 1, 1)
    ctx.fillStyle = opaque
    ctx.fillRect(0, 0, 1, 1)
    const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data

    return { r, g, b, a: alpha === undefined ? 1 : Number(alpha) }
  }

  throw new Error(`[canvas] the renderer returned a colour this does not understand: "${normalized}"`)
}

/** Whether the engine recognises this colour. Never throws. */
export function isColor(css: string): boolean {
  try {
    parseColor(css)
    return true
  } catch {
    return false
  }
}

const clampChannel = (value: number): number => Math.min(CHANNEL_MAX, Math.max(0, Math.round(value)))

/**
 * Writes a colour back as a string the engine accepts.
 *
 * Opaque colours become hex, which is what most of this library's props are written as; anything
 * translucent becomes `rgba()`, since hex alpha is less widely readable at a glance.
 */
export function formatColor({ r, g, b, a }: Rgba): string {
  const alpha = Math.min(1, Math.max(0, a))
  const [red, green, blue] = [r, g, b].map(clampChannel)

  if (alpha >= 1) {
    return `#${[red, green, blue].map(c => c.toString(16).padStart(2, '0')).join('')}`
  }
  // Trimmed so a half-way alpha reads as 0.5 rather than 0.5000000000000001.
  return `rgba(${red}, ${green}, ${blue}, ${Number(alpha.toFixed(3))})`
}

/**
 * Blends two colours, in any accepted format, and returns one string.
 *
 * Interpolation happens in sRGB with a straight alpha, which is what the canvas compositing model
 * expects. `t` is clamped, so a track that overshoots cannot produce an impossible colour.
 */
export function mixColor(from: string, to: string, t: number): string {
  const clamped = Math.min(1, Math.max(0, t))
  const a = parseColor(from)
  const b = parseColor(to)

  return formatColor({
    r: a.r + (b.r - a.r) * clamped,
    g: a.g + (b.g - a.g) * clamped,
    b: a.b + (b.b - a.b) * clamped,
    a: a.a + (b.a - a.a) * clamped,
  })
}
