import { Canvas } from 'meo-skia-canvas'

/**
 * A colour resolved to sRGB. Channels are 0–255 and alpha is 0–1.
 *
 * Channels are floats and may fall outside 0–255: that is how a colour sRGB cannot reach is
 * carried. Display P3's red is `r` above 255 with `g` and `b` slightly negative — extended sRGB,
 * the same colour named in sRGB's coordinates. Clamping here would quietly substitute the duller
 * sRGB red for the one that was asked for.
 */
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
let scratch: Canvas | null = null

function surface(): Canvas {
  if (!scratch) {
    // Float, not the default eight bits a channel. An eight-bit surface clamps on the way in, so a
    // Display P3 red would land on plain sRGB red before anything could read it back — the two
    // become the same colour, and the gamut is gone before the parse finishes.
    scratch = new Canvas(1, 1, { colorType: 'RGBAF32' })
  }
  return scratch
}

function context(): ReturnType<Canvas['getContext']> {
  return surface().getContext('2d')
}

/**
 * How many parsed colours are kept.
 *
 * Bounded because the cache outlives every render: it is module state, so without a limit a
 * long-running server would accumulate one permanent entry per distinct colour it ever drew. A
 * render that computes a colour per page — which is what a chained `mix()` produces — makes that a
 * steady leak rather than a theoretical one. A few thousand entries is far more than any scene's
 * palette and costs a few hundred kilobytes.
 */
export const COLOR_CACHE_LIMIT = 4096

/**
 * Parsed colours, keyed by the exact input string, most recently used last.
 *
 * An animation asks for the same two endpoints on every page, so this turns a per-page parse into
 * a map lookup. Entries are frozen, and callers get a copy — a mutable cached object would let one
 * caller corrupt every later reader.
 *
 * Eviction is least-recently-used rather than oldest-first, because age says nothing about whether
 * a colour is still in play: an animation's endpoints are the oldest entries it has and also the
 * two it reads on every single page.
 */
const cache = new Map<string, Readonly<Rgba>>()

/** Number of colours currently cached. Exposed for tests, which is why it is a function. */
export function colorCacheSize(): number {
  return cache.size
}

/**
 * Resolves any colour the rendering engine accepts into sRGB.
 *
 * The engine does the work: the colour is painted onto a one-pixel canvas and read back. That is
 * deliberately not a CSS parser — it means every syntax the engine supports is supported here for
 * free, today and after any upgrade, including `lab()`, `oklch()` and `color(display-p3 …)`, which
 * a hand-written parser would either miss or approximate.
 *
 * Wide-gamut inputs survive: the scratch surface holds floats, so a colour beyond sRGB comes back
 * as extended sRGB rather than clipped to the edge of it.
 * @throws if the engine does not recognise the colour, rather than returning a misleading black.
 */
export function parseColor(css: string): Rgba {
  const hit = cache.get(css)
  if (hit) {
    // Re-inserted so it moves to the end: a Map iterates in insertion order, which is what makes
    // "the first key" the least recently used one.
    cache.delete(css)
    cache.set(css, hit)
    return { ...hit }
  }

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

  if (cache.size >= COLOR_CACHE_LIMIT) {
    const oldest = cache.keys().next().value
    if (oldest !== undefined) cache.delete(oldest)
  }
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
function fromNormalized(normalized: string, ctx: ReturnType<Canvas['getContext']>): Rgba {
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

    // The only form that needs the surface. `#rrggbb` and `rgba()` are already sRGB and already
    // exact in the string; reading those back through float32 would return 102.0000015 for 102 and
    // buy nothing. A `color()` result is the one that may name a colour sRGB cannot reach, and
    // converting a space like display-p3 is the arithmetic worth not reimplementing.
    //
    // Painted opaque so premultiplied storage cannot touch the channels: at alpha 0 the pixel would
    // otherwise come back transparent black with the hue gone. Alpha comes from the string, which
    // has it regardless.
    ctx.clearRect(0, 0, 1, 1)
    ctx.fillStyle = `color(${space} ${c1} ${c2} ${c3})`
    ctx.fillRect(0, 0, 1, 1)

    const raw = surface().toBufferSync('raw', { colorType: 'RGBAF32' })

    return {
      r: raw.readFloatLE(0) * CHANNEL_MAX,
      g: raw.readFloatLE(4) * CHANNEL_MAX,
      b: raw.readFloatLE(8) * CHANNEL_MAX,
      a: alpha === undefined ? 1 : Number(alpha),
    }
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

/** Whether every channel sits inside what sRGB can express, and so inside what hex can write. */
const inGamut = ({ r, g, b }: Rgba): boolean => [r, g, b].every(c => c >= 0 && c <= CHANNEL_MAX)

/** Digits kept for an out-of-gamut channel: enough that a float32 round-trips visually intact. */
const EXTENDED_PRECISION = 6

/**
 * Writes a colour back as a string the engine accepts.
 *
 * Ordinary colours become hex, or `rgba()` once alpha is involved, because that is how this
 * library's props are written everywhere else and it stays readable.
 *
 * A colour outside sRGB cannot be written either way — hex has no room for a channel above 255 or
 * below 0 — so it becomes `color(srgb …)`, which carries the extended values verbatim and is read
 * back exactly. Clamping instead would substitute a duller colour without saying so.
 */
export function formatColor(color: Rgba): string {
  const alpha = Math.min(1, Math.max(0, color.a))

  if (!inGamut(color)) {
    const [red, green, blue] = [color.r, color.g, color.b].map(c => Number((c / CHANNEL_MAX).toFixed(EXTENDED_PRECISION)))
    const tail = alpha >= 1 ? '' : ` / ${Number(alpha.toFixed(EXTENDED_PRECISION))}`
    return `color(srgb ${red} ${green} ${blue}${tail})`
  }

  const [red, green, blue] = [color.r, color.g, color.b].map(clampChannel)

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
