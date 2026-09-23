/**
 * The `f64` arena: one `Float64Array` carries a scene into the addon, with strings
 * and buffers beside it. `crates/meo-canvas-node/src/arena.rs` is the definition.
 * Reading values out of V8 is the cost it avoids: 39 of a `lineTo`'s 82 ns, to 17 crossing.
 * @packageDocumentation
 */

import {
  ALIGN,
  BLEND_MODE,
  BACKGROUND_REPEAT,
  BORDER_STYLE,
  BOX_SIZING,
  DIRECTION,
  DISPLAY,
  FILL_RULE,
  FLEX_DIRECTION,
  FLEX_WRAP,
  FONT_STYLE,
  FONT_VARIANT,
  GRADIENT_KIND,
  MASK_SHAPE,
  GRID_AUTO_FLOW,
  JUSTIFY,
  LINE_CAP,
  LINE_JOIN,
  NODE_TAG,
  OBJECT_FIT,
  OVERFLOW,
  PAINT_ORDER,
  POSITION_TYPE,
  TEXT_ALIGN,
  TEXT_DECORATION,
  VERTICAL_ALIGN,
} from './generated/arena-enums.js'
import { EFFECTS, LAYOUT, MAGIC, MASK_BITS, PAINT, TEXT, VERSION } from './generated/arena-tables.js'
import { ON_IMAGE_ERROR } from './generated/arena-enums.js'
import { COLOR_SPACE, COLOR_TYPE } from './generated/arena-enums.js'
import type { ColorSpace, ColorType, OnImageError, TrackSize } from './index.js'
import type { ImageSource, PathPaint, PathProps, SceneNode } from './node.js'
import type {
  BackgroundImage,
  BackgroundSize,
  BoxShadow,
  Color,
  Corners,
  Dimension,
  FontWeight,
  Gradient,
  GradientDirection,
  GradientRamp,
  GradientStop,
  GridPlacement,
  Length,
  LineHeight,
  Mask,
  Sides,
  Spacing,
  Style,
  TextShadow,
  Transform,
} from './style.js'

/** A value the arena cannot carry itself, held beside it. */
export type SideValue = string | Uint8Array

/** A written scene: the slots, and the values they index. */
export interface Arena {
  /** Every slot, in the order the reader consumes them. */
  readonly slots: Float64Array
  /** The strings and buffers the slots index into. */
  readonly values: readonly SideValue[]
  /**
   * Every URL source the scene named, with the options it resolved to. {@link Root}
   * fetches them and encodes again with the bytes, so a URL reaches the wire only
   * where its fetch failed.
   */
  readonly requests: readonly ImageRequest[]
}

/** One URL the scene named, and what a fetch of it would send. */
export interface ImageRequest {
  /** The address as the caller wrote it. */
  readonly url: string
  /**
   * What identifies this fetch, and what fetched bytes are keyed by: the URL with
   * its resolved headers, since two nodes at one address with different
   * `Authorization` are two fetches.
   */
  readonly key: string
  /** The scene-wide options with this source's merged over them. */
  readonly init: RequestInit | undefined
}

/**
 * `over` merged onto `base` per key, with `headers` merged per name, so a source
 * setting one header keeps the scene's credentials. `new Headers` reads every
 * `HeadersInit` spelling and lower-cases names. An undefined value is not copied,
 * since `exactOptionalPropertyTypes` tells it apart from an absent key.
 */
export function mergeHttpOptions(base: RequestInit | undefined, over: RequestInit | undefined): RequestInit | undefined {
  if (base === undefined) return over
  if (over === undefined) return base

  const merged: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(base)) if (value !== undefined) merged[key] = value
  for (const [key, value] of Object.entries(over)) if (value !== undefined) merged[key] = value

  if (base.headers !== undefined || over.headers !== undefined) {
    const headers = new Headers(base.headers)
    for (const [name, value] of new Headers(over.headers)) headers.set(name, value)
    merged['headers'] = headers
  }

  // `signal` is composed rather than overridden: it is the caller's kill switch,
  // not a value a source restates. First to fire wins, so a per-source signal can
  // only tighten the scene's.
  const signals = [base.signal, over.signal].filter((signal): signal is AbortSignal => signal !== undefined && signal !== null)
  if (signals.length === 1) merged['signal'] = signals[0]
  else if (signals.length > 1) merged['signal'] = AbortSignal.any(signals)
  return merged
}

/**
 * What identifies a fetch: the address and its headers, sorted so the same two
 * headers in either order are one request. Nothing else in `RequestInit` changes
 * which bytes come back.
 */
export function requestKey(url: string, init: RequestInit | undefined): string {
  const headers = [...new Headers(init?.headers)].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
  return headers.length === 0 ? url : `${url}\n${headers.map(([name, value]) => `${name}: ${value}`).join('\n')}`
}

/**
 * The tag an absent optional is written as. Named because `0` and `1` are also
 * booleans and enum discriminants in this format.
 */
const ABSENT = 0

/** The tag a present optional is written as. */
const PRESENT = 1

/** The largest value one mask slot holds: 53 bits all set. */
const LARGEST_MASK_SLOT = 2 ** MASK_BITS - 1

/**
 * Accumulates the slots of one scene. Slot counts vary, since a list writes its
 * length and an optional its flag, so every method appends and nothing writes at
 * an offset computed by arithmetic.
 */
export class ArenaWriter {
  /** The slots written so far. */
  readonly #slots: number[] = []

  /** The strings and buffers the slots index. */
  readonly #values: SideValue[] = []

  /**
   * Where each string already written sits in {@link ArenaWriter.#values}. Strings
   * repeat heavily and each copy is another read out of V8; buffers are not
   * deduplicated, since comparing contents costs more than the copy.
   */
  readonly #strings = new Map<string, number>()

  /** Every URL source written so far, with the options it resolved to. */
  readonly requests: ImageRequest[] = []

  /** The scene-wide options a per-source `httpOptions` merges over. */
  httpOptions: RequestInit | undefined = undefined

  /**
   * Bytes already fetched for a URL source, keyed by request. Set on the second
   * encode of a render: the first discovers the URLs, since this writer is the only
   * thing that knows every position a source can occupy.
   */
  fetched: ReadonlyMap<string, Uint8Array> | undefined = undefined

  /** The scene, ready to hand across. */
  finish(): Arena {
    return { slots: Float64Array.from(this.#slots), values: this.#values, requests: this.requests }
  }

  /** Writes one slot exactly as given. */
  slot(value: number): void {
    this.#slots.push(value)
  }

  /**
   * Writes a number the reader narrows to `f32`. Nothing is rounded here, so the
   * narrowing happens once, on the reading side.
   */
  f32(value: number): void {
    this.#slots.push(value)
  }

  /** Writes a boolean as `0` or `1`, which is the only thing the reader takes. */
  bool(value: boolean): void {
    this.#slots.push(value ? 1 : 0)
  }

  /** Writes an integer, which must already be one. */
  integer(value: number): void {
    this.#slots.push(value)
  }

  /** Writes an enum's discriminant, the same byte the codec writes. */
  enum(discriminant: number): void {
    this.#slots.push(discriminant)
  }

  /** Writes a count, then leaves the caller to write that many items. */
  count(items: number): void {
    this.#slots.push(items)
  }

  /** Writes an absent optional: one slot, and nothing follows. */
  absent(): void {
    this.#slots.push(ABSENT)
  }

  /** Writes the flag of a present optional. The value follows. */
  present(): void {
    this.#slots.push(PRESENT)
  }

  /** Writes an optional, calling `write` only when there is something to write. */
  optional<T>(value: T | undefined, write: (value: T) => void): void {
    if (value === undefined) {
      this.absent()
      return
    }
    this.present()
    write(value)
  }

  /** Writes a string as an index into the side values. */
  text(value: string): void {
    const seen = this.#strings.get(value)
    if (seen !== undefined) {
      this.#slots.push(seen)
      return
    }
    const index = this.#values.length
    this.#values.push(value)
    this.#strings.set(value, index)
    this.#slots.push(index)
  }

  /** Writes a buffer as an index into the side values. */
  bytes(value: Uint8Array): void {
    const index = this.#values.length
    this.#values.push(value)
    this.#slots.push(index)
  }

  /**
   * Reserves a group's mask slots and returns where they sit. The mask says which
   * properties follow and is not known until they are written, so
   * {@link ArenaWriter.patchMask} fills it in afterwards.
   */
  reserveMask(slots: number): number {
    const at = this.#slots.length
    for (let index = 0; index < slots; index += 1) this.#slots.push(0)
    return at
  }

  /**
   * Fills a reservation with the bits of the properties written. A slot holds at
   * most {@link MASK_BITS} bits, 53, because a double is exact on integers only to
   * 2^53.
   */
  patchMask(at: number, bits: readonly number[]): void {
    bits.forEach((slot, offset) => {
      if (slot < 0 || slot > LARGEST_MASK_SLOT || !Number.isInteger(slot)) {
        throw new RangeError(`a mask slot holds ${MASK_BITS} bits; ${slot} is not one of them`)
      }
      this.#slots[at + offset] = slot
    })
  }
}

/**
 * What a scene says about its surface beyond its size. Absent leaves the choice to
 * the renderer, which is not the same as asking for its default.
 */
export interface SurfaceOptions {
  /** Whether to rasterise on the GPU. */
  readonly gpu?: boolean
  /** The pixel layout the surface composites in. */
  readonly colorType?: ColorType
  /** The colour space the surface composites in. */
  readonly colorSpace?: ColorSpace
  /**
   * What a render does when an image source cannot be resolved. The policy is the
   * scene's alone, so absent means the default rather than the renderer's choice.
   */
  readonly onImageError?: OnImageError
}

/**
 * A fetch this surface attempted and could not complete, with the reason, so the
 * renderer reports a 404 as a crate consumer would see it.
 */
export type FetchAttempt = {
  readonly url: string
  readonly detail: string
} & (
  | {
      /** The server answered, and this is what it answered with. */
      readonly failure: 'status'
      readonly status: number
    }
  | {
      /** No response to have a status: the fetch never produced one. */
      readonly failure: 'host-not-found' | 'bad-url' | 'transport' | 'too-large' | 'other'
      readonly status?: undefined
    }
)

/**
 * The wire tag for each failure, by hand: `ImageFetchFailure` carries a payload on
 * `Status`, so it is not a `wire_enum!` and has no generated table. The Rust reader
 * names them in the same order.
 */
const FAILURE_TAG = {
  status: 0,
  'host-not-found': 1,
  'bad-url': 2,
  transport: 3,
  'too-large': 4,
  other: 5,
} as const

/** Writes the header every arena opens with, and the page count. */
function writeHeader(
  out: ArenaWriter,
  width: number,
  height: number,
  contentHeight: boolean,
  scale: number,
  surface: SurfaceOptions,
  pages: number,
  attempts: readonly FetchAttempt[] = [],
): void {
  out.slot(MAGIC)
  out.slot(VERSION)
  out.f32(width)
  out.f32(height)
  // Beside the height it qualifies: set, the height above is a floor and the
  // page is as tall as what is in it.
  out.slot(contentHeight ? 1 : 0)
  out.f32(scale)

  // The surface block, between the geometry and the pages.
  out.optional(surface.gpu, gpu => out.slot(gpu ? 1 : 0))
  out.optional(surface.colorType, type => out.enum(variant(COLOR_TYPE, type, 'colorType')))
  out.optional(surface.colorSpace, space => out.enum(variant(COLOR_SPACE, space, 'colorSpace')))
  // Written unconditionally, because the field is not optional on the other
  // side. `'placeholder'` is the default the scene would take anyway; naming
  // it here keeps the arena's shape fixed rather than making the reader's
  // offset depend on whether the caller said anything.
  out.enum(variant(ON_IMAGE_ERROR, surface.onImageError ?? 'placeholder', 'onImageError'))

  // What this surface already tried to fetch and could not, with the reason it
  // measured. Empty unless a URL failed, and it is written unconditionally so
  // the reader's offsets do not depend on whether anything went wrong.
  out.count(attempts.length)
  for (const attempt of attempts) {
    out.text(attempt.url)
    // The status is written behind the tag rather than beside it, so a `'status'`
    // with no code cannot be encoded.
    if (attempt.failure === 'status') {
      out.slot(FAILURE_TAG.status)
      out.slot(attempt.status)
    } else {
      out.slot(FAILURE_TAG[attempt.failure])
    }
    out.text(attempt.detail)
  }

  out.count(pages)
}

/**
 * The exceptions to deriving a Rust variant from a keyword: words CSS runs
 * together where the scene spells the concept in parts. Every other keyword is the
 * kebab-case of its variant, so one with no variant behind it throws.
 */
const SPELLINGS: Readonly<Record<string, string>> = {
  nowrap: 'NoWrap',
  // CSS spells a fill rule as one word too.
  nonzero: 'NonZero',
  evenodd: 'EvenOdd',
}

/**
 * Upstream's alternate names for `ColorType` and `ColorSpace`, whose keywords are
 * upstream's spellings, the ones callers already have written down. A table because
 * no rule turns `'RGBA8888'` into `Uint8`; the keyword test walks every union
 * member and every variant, so a name in neither place fails.
 */
const ALIASES: Readonly<Record<string, string>> = {
  // ColorType: upstream's channel-order names against the scene's layout names.
  ARGB4444: 'Argb4444',
  RGB565: 'Rgb565',
  rgb: 'Rgb888x',
  RGB888x: 'Rgb888x',
  rgba: 'Uint8',
  RGBA8888: 'Uint8',
  bgra: 'Bgra8888',
  BGRA8888: 'Bgra8888',
  BGR101010x: 'Bgr101010x',
  BGRA1010102: 'Bgra1010102',
  RGB101010x: 'Rgb101010x',
  RGBA1010102: 'Rgba1010102',
  SRGBA8888: 'Srgba8888',
  RGBAF16: 'F16',
  RGBAF16Norm: 'F16Norm',
  RGBAF32: 'F32',
  // ColorSpace: the short forms, which are aliases of the long ones.
  linear: 'SrgbLinear',
  p3: 'DisplayP3',
  'p3-linear': 'DisplayP3Linear',
  bt2020: 'Rec2020',
  'bt2020-linear': 'Rec2020Linear',
  hdr10: 'Rec2020Pq',
  hlg: 'Rec2020Hlg',
}

/**
 * The Rust variant name a keyword means. A keyword that is already a variant name
 * is taken as written: a pixel layout like `R8G8UNorm` has no kebab form anyone
 * would type.
 */
function variantName(keyword: string, table: Readonly<Record<string, number>>): string {
  if (table[keyword] !== undefined) return keyword

  const exception = SPELLINGS[keyword]
  if (exception !== undefined) return exception

  // The derivation before the aliases, and the order is load-bearing: `'linear'` is
  // `SrgbLinear` to a colour space and `Linear` to a gradient, and deriving first
  // lets each enum keep its own reading.
  const derived = keyword
    .split('-')
    .map(part => part.charAt(0).toUpperCase() + part.slice(1))
    .join('')
  if (table[derived] !== undefined) return derived

  return ALIASES[keyword] ?? derived
}

/**
 * The keyword a Rust variant name is written as, for an error message. It errs
 * wide: a variant name resolves as written, so this may name one the union omits.
 */
function keywordFor(name: string, table: Readonly<Record<string, number>>): string {
  const spelt = Object.entries(SPELLINGS).find(([, variant]) => variant === name)
  if (spelt !== undefined) return spelt[0]

  // An alias first, since where one exists it is the spelling offered: `Uint8` is
  // `'rgba'`. Then the kebab form, where this surface takes one; the rest of
  // `ColorType` is offered as upstream spells it.
  const alias = Object.entries(ALIASES).find(([, variant]) => variant === name)
  if (alias !== undefined) return alias[0]

  const kebab = name.replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase()
  return variantName(kebab, table) === name ? kebab : name
}

/**
 * Renders a rejected value so a caller can recognise it: `JSON.stringify` for a
 * string so `""` shows, the literal for a number, and the kind for the rest.
 */
export function render(value: unknown): string {
  if (typeof value === 'string') return JSON.stringify(value)
  if (typeof value === 'number') return String(value)
  if (typeof value === 'function') return 'a function'
  if (value === null) return 'null'
  // A list before the object it technically is: a caller writing CSS's shorthand as
  // `padding: [8, 4]` learns nothing from "an object".
  if (Array.isArray(value)) return 'a list'
  if (typeof value === 'object') return 'an object'
  // What is left is `boolean | symbol | bigint | undefined`, spelled out rather
  // than reached through `String` on an `unknown`: a symbol has no string
  // conversion worth printing, and the lint that refuses the general case is
  // right that a bare `String(value)` here would one day meet an object.
  if (typeof value === 'symbol') return 'a symbol'
  if (typeof value === 'boolean') return value ? 'true' : 'false'
  if (typeof value === 'bigint') return `${value}n`
  return 'undefined'
}

/**
 * Refuses anything but a whole number, naming the property. The reader would report
 * `slot 33 holds NaN`, an offset the caller never saw; the writer is the last place
 * that knows the caller wrote `zIndex`, and the only door such a value comes
 * through, since Rust refuses it at compile time.
 */
function whole(value: unknown, what: string): number {
  if (typeof value !== 'number' || !Number.isInteger(value)) {
    throw new TypeError(`${what} is ${render(value)}; it takes a whole number`)
  }
  return value
}

/**
 * Refuses anything but a number, naming the property. `NaN` is refused and
 * `Infinity` is not: an infinity means *as large as possible* and is bounded on the
 * other side, where a `NaN` means nothing. `!Number.isFinite` would refuse both.
 */
function decimal(value: unknown, what: string): number {
  if (typeof value !== 'number' || Number.isNaN(value)) {
    throw new TypeError(`${what} is ${render(value)}; it takes a number`)
  }
  return value
}

/** Refuses anything but a string, naming the property and what it spells. */
function words(value: unknown, what: string, takes: string): string {
  if (typeof value !== 'string') {
    throw new TypeError(`${what} is ${render(value)}; it takes ${takes}`)
  }
  return value
}

/**
 * Refuses a value that is neither a number nor a string, naming the property,
 * before a `write*` helper calls `value.endsWith` on it. A bad *string* keeps each
 * helper's own message, which lists the spellings that exist.
 */
function measured(value: unknown, what: string, takes: string): number | string {
  if ((typeof value !== 'number' && typeof value !== 'string') || Number.isNaN(value)) {
    throw new TypeError(`${what} is ${render(value)}; it takes ${takes}`)
  }
  return value
}

/**
 * The value, or `fallback` where the caller wrote nothing. Not `??`, which would
 * turn `null` into the fallback; a `null` is refused by name downstream instead.
 */
function defaulted<T>(value: T | undefined, fallback: T): T {
  return value === undefined ? fallback : value
}

/**
 * The number a keyword crosses as. Throws rather than defaults: writing the zeroth
 * variant would make an unknown keyword arrive as a different value.
 */
export function variant(table: Readonly<Record<string, number>>, keyword: string, what: string): number {
  const taken = (): string =>
    Object.keys(table)
      .map(name => keywordFor(name, table))
      .join(', ')
  // Refused before the lookup, which would otherwise fail on `keyword.split`. The
  // keyword list is built only in the failing branches, since every enum write
  // crosses this line.
  if (typeof keyword !== 'string') {
    throw new TypeError(`${what} is ${render(keyword)}; it takes ${taken()}`)
  }
  const found = table[variantName(keyword, table)]
  if (found === undefined) {
    throw new TypeError(`${what} has no value ${JSON.stringify(keyword)}; it takes ${taken()}`)
  }
  return found
}

/**
 * The fraction a `'…%'` string names, or `undefined`: `'50%'` is `0.5`, as the scene
 * stores a percentage. `'1%'` cannot probe this, since `1` equals `Percent(1.0)`
 * either way; `root.test.ts` checks it in rendered pixels.
 */
function percentage(value: string): number | undefined {
  if (!value.endsWith('%')) return undefined
  const number = Number(value.slice(0, -1))
  return Number.isFinite(number) ? number / 100 : undefined
}

/** The number a string ending in `unit` names, or `undefined`. */
function suffixed(value: string, unit: string): number | undefined {
  if (!value.endsWith(unit)) return undefined
  const number = Number(value.slice(0, -unit.length))
  return Number.isFinite(number) ? number : undefined
}

/** Writes a length: a tag, then the value. */
function writeLength(out: ArenaWriter, value: Length, what: string): void {
  measured(value, what, "a number of pixels or a '…%' string")
  if (typeof value === 'number') {
    out.enum(0)
    out.f32(value)
    return
  }
  const percent = percentage(value)
  if (percent === undefined) {
    throw new TypeError(`${JSON.stringify(value)} is not a length; write a number of pixels or a '…%' string`)
  }
  out.enum(1)
  out.f32(percent)
}

/**
 * Writes a dimension: a tag, then the value, even for `auto`. A fixed two-slot
 * width is what lets the reader skip a property it does not recognise.
 */
function writeDimension(out: ArenaWriter, value: Dimension, what: string): void {
  measured(value, what, "a number of pixels, a '…%' string, or 'auto'")
  if (value === 'auto') {
    out.enum(0)
    out.f32(0)
    return
  }
  if (typeof value === 'number') {
    out.enum(1)
    out.f32(value)
    return
  }
  const percent = percentage(value)
  if (percent === undefined) {
    throw new TypeError(`${JSON.stringify(value)} is not a size; write a number of pixels, a '…%' string, or 'auto'`)
  }
  out.enum(2)
  out.f32(percent)
}

/** Writes a grid track size: a tag, then the value. */
function writeTrack(out: ArenaWriter, value: TrackSize, what: string): void {
  measured(value, what, "a number, 'auto', '…px', '…%' or '…fr'")
  if (value === 'auto') {
    out.enum(0)
    out.f32(0)
    return
  }
  if (typeof value === 'number') {
    out.enum(1)
    out.f32(value)
    return
  }

  const pixels = suffixed(value, 'px')
  if (pixels !== undefined) {
    out.enum(1)
    out.f32(pixels)
    return
  }
  const percent = percentage(value)
  if (percent !== undefined) {
    out.enum(2)
    out.f32(percent)
    return
  }
  const fraction = suffixed(value, 'fr')
  if (fraction === undefined) {
    throw new TypeError(`${JSON.stringify(value)} is not a track size; write a number, 'auto', '…px', '…%' or '…fr'`)
  }
  out.enum(3)
  out.f32(fraction)
}

/** Writes letter or word spacing: a tag, then the value. */
function writeSpacing(out: ArenaWriter, value: Spacing, what: string): void {
  measured(value, what, "a number, '…px', '…em' or 'normal'")
  if (value === 'normal') {
    out.enum(0)
    out.f32(0)
    return
  }
  if (typeof value === 'number') {
    out.enum(1)
    out.f32(value)
    return
  }

  const pixels = suffixed(value, 'px')
  if (pixels !== undefined) {
    out.enum(1)
    out.f32(pixels)
    return
  }
  const em = suffixed(value, 'em')
  if (em === undefined) {
    throw new TypeError(`${JSON.stringify(value)} is not a spacing; write a number, '…px', '…em' or 'normal'`)
  }
  out.enum(2)
  out.f32(em)
}

/**
 * Writes a line height: a tag, then the value; `normal` is the absent option. A
 * percentage resolves where it is declared and a number at each inheritor, as
 * Chrome does: over a 32px child of a 16px parent, `1.5` gives 48 and `150%` 24.
 */
function writeLineHeight(out: ArenaWriter, value: LineHeight, what: string): void {
  measured(value, what, "a number, '…px' or '…%'")
  if (typeof value === 'number') {
    out.enum(0)
    out.f32(value)
    return
  }
  const pixels = suffixed(value, 'px')
  if (pixels !== undefined) {
    out.enum(1)
    out.f32(pixels)
    return
  }
  const share = percentage(value)
  if (share === undefined) {
    throw new TypeError(`${JSON.stringify(value)} is not a line height; write a number, '…px' or '…%', or leave it out for 'normal'`)
  }
  out.enum(2)
  out.f32(share)
}

/**
 * Writes the four edges, top right bottom left. An unnamed edge takes the
 * property's own default rather than a shared zero: `margin` defaults to zero and
 * `inset` to nothing.
 */
function writeSides<T>(value: Sides<T>, fallback: T, what: string, write: (value: T) => void): void {
  // A list is refused rather than read as named edges with none named: CSS's
  // shorthand is not a spelling here.
  if (Array.isArray(value)) {
    throw new TypeError(`${what} is a list; it takes one value or named edges`)
  }
  if (typeof value !== 'object' || value === null) {
    for (let edge = 0; edge < 4; edge += 1) write(value)
    return
  }
  const named = value as { top?: T; right?: T; bottom?: T; left?: T }
  write(named.top ?? fallback)
  write(named.right ?? fallback)
  write(named.bottom ?? fallback)
  write(named.left ?? fallback)
}

/** Writes the four corners, in `top-left top-right bottom-right bottom-left` order. */
function writeCorners(out: ArenaWriter, value: Corners, what: string): void {
  if (typeof value === 'number') {
    for (let corner = 0; corner < 4; corner += 1) out.f32(value)
    return
  }
  // Named before it is read. A value that is not a number and not an object of
  // corners reached `value.topLeft` and gave either a null dereference or four
  // silent zeroes, which is a radius the caller did not write.
  if (typeof value !== 'object' || value === null) {
    throw new TypeError(`${what} is ${render(value)}; it takes a number or named corners`)
  }
  out.f32(decimal(value.topLeft ?? 0, `${what} topLeft`))
  out.f32(decimal(value.topRight ?? 0, `${what} topRight`))
  out.f32(decimal(value.bottomRight ?? 0, `${what} bottomRight`))
  out.f32(decimal(value.bottomLeft ?? 0, `${what} bottomLeft`))
}

/** Black, which is what a shadow with no colour is. */
const SHADOW_BLACK = '#000000'

/**
 * Writes a transform with the scene's defaults for what was not named. The wire
 * shape is fixed, and the defaults are stated because a `scale` of zero is not no
 * scale.
 */
function writeTransform(out: ArenaWriter, value: Transform): void {
  writeLength(out, value.translateX ?? 0, 'transform translateX')
  writeLength(out, value.translateY ?? 0, 'transform translateY')
  out.f32(value.rotate ?? 0)
  // `scale` sets both axes, and a per-axis value beside it wins.
  out.f32(value.scaleX ?? value.scale ?? 1)
  out.f32(value.scaleY ?? value.scale ?? 1)
  writeLength(out, value.originX ?? '50%', 'transform originX')
  writeLength(out, value.originY ?? '50%', 'transform originY')
}

/** Writes one box shadow. */
function writeBoxShadow(out: ArenaWriter, value: BoxShadow): void {
  out.bool(value.inset ?? false)
  out.f32(value.offsetX ?? 0)
  out.f32(value.offsetY ?? 0)
  out.f32(value.blur ?? 0)
  out.f32(value.spread ?? 0)
  out.text(value.color ?? SHADOW_BLACK)
}

/** Writes one text shadow, which has no spread and no inset. */
function writeTextShadow(out: ArenaWriter, value: TextShadow): void {
  out.f32(value.offsetX ?? 0)
  out.f32(value.offsetY ?? 0)
  out.f32(value.blur ?? 0)
  out.text(value.color ?? SHADOW_BLACK)
}

/**
 * The angle each named direction resolves to, clockwise from twelve o'clock. A
 * keyword is a direction whose angle is known before the box is.
 */
const DIRECTIONS: Readonly<Record<string, number>> = {
  'to-top': 0,
  'to-top-right': 45,
  'to-right': 90,
  'to-bottom-right': 135,
  'to-bottom': 180,
  'to-bottom-left': 225,
  'to-left': 270,
  'to-top-left': 315,
}

/** Writes a linear direction: an angle, or the two points it runs between. */
function writeDirection(out: ArenaWriter, value: GradientDirection): void {
  if (typeof value === 'number') {
    out.enum(0)
    out.f32(value)
    return
  }
  if (typeof value === 'string') {
    const angle = DIRECTIONS[value]
    if (angle === undefined) {
      throw new TypeError(
        `a gradient has no direction ${JSON.stringify(value)}; it takes ${Object.keys(DIRECTIONS).join(', ')}, an angle in degrees, or [x0, y0, x1, y1]`,
      )
    }
    out.enum(0)
    out.f32(angle)
    return
  }
  out.enum(1)
  for (const point of value) writeLength(out, point, 'a gradient direction')
}

/**
 * The stops a ramp names: `colors` spread evenly from first to last, and a single
 * colour at the midpoint.
 */
function rampStops(ramp: GradientRamp): readonly GradientStop[] {
  if (ramp.stops !== undefined) return ramp.stops
  const colors = ramp.colors
  if (colors.length === 1) return [{ offset: 0.5, color: colors[0] as Color }]
  return colors.map((color, index) => ({ offset: index / (colors.length - 1), color }))
}

/** Writes a gradient: its kind, the geometry that kind reads, then its stops. */
function writeGradient(out: ArenaWriter, value: Gradient): void {
  out.enum(variant(GRADIENT_KIND, value.type, 'gradient type'))

  if (value.type === 'linear') {
    writeDirection(out, value.direction ?? 'to-bottom')
  } else {
    // The middle of the box, which is what CSS defaults to and what the scene
    // documents `(0.5, 0.5)` as.
    writeLength(out, value.at?.x ?? '50%', 'gradient at.x')
    writeLength(out, value.at?.y ?? '50%', 'gradient at.y')
    if (value.type === 'conic') out.f32(value.from ?? 0)
  }

  const stops = rampStops(value)
  out.count(stops.length)
  for (const stop of stops) {
    out.f32(stop.offset)
    out.text(stop.color)
  }
}

/** Writes an image source: a tag, then the side value it names. */
function writeSource(out: ArenaWriter, src: string | ImageSource): void {
  const source = typeof src === 'string' ? { path: src } : src
  if ('path' in source) {
    out.enum(0)
    out.text(source.path)
    return
  }
  if ('url' in source) {
    // **Resolved here and nowhere else.** Both encode passes and the fetch pass
    // between them have to agree about what a source's options are and about
    // which sources share a fetch; computing that in one place is what makes
    // agreement structural rather than a thing to keep in step.
    const init = mergeHttpOptions(out.httpOptions, source.httpOptions)
    const key = requestKey(source.url, init)

    // **Bytes cross the wire, never a URL**, where this render already fetched it,
    // so nothing downstream needs a `net` feature. Keyed by request, so two sources
    // at one URL with different headers each take their own bytes.
    const bytes = out.fetched?.get(key)
    if (bytes !== undefined) {
      out.enum(2)
      out.bytes(bytes)
      return
    }

    // Otherwise written, and counted: `Root` reads the count to decide whether to
    // fetch. The headers travel sorted by name, the wire contract the Rust writer
    // keeps too; `Headers` already iterates in order and the `sort` states it.
    // `writes them sorted by name` fails if a plain object replaces `Headers`.
    out.requests.push({ url: source.url, key, init })
    out.enum(1)
    out.text(source.url)
    const headers = [...new Headers(init?.headers)].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    out.count(headers.length)
    for (const [name, value] of headers) {
      out.text(name)
      out.text(value)
    }
    return
  }
  out.enum(2)
  out.bytes(source.bytes)
}

/** Writes a background size: a tag, and the pair only the per-axis arm carries. */
function writeBackgroundSize(out: ArenaWriter, value: BackgroundSize): void {
  if (value === 'cover') {
    out.enum(1)
    return
  }
  if (value === 'contain') {
    out.enum(2)
    return
  }

  out.enum(0)
  if (typeof value === 'object') {
    writeDimension(out, value.width ?? 'auto', 'backgroundImage size.width')
    writeDimension(out, value.height ?? 'auto', 'backgroundImage size.height')
    return
  }
  // A bare value sizes the width and leaves the height to the picture's
  // proportions, as CSS's one-value form does.
  writeDimension(out, value, 'backgroundImage size')
  writeDimension(out, 'auto', 'backgroundImage size')
}

/** Writes a background image: its source, how it tiles, how big, and where. */
function writeBackgroundImage(out: ArenaWriter, value: BackgroundImage): void {
  writeSource(out, value.src)
  out.enum(variant(BACKGROUND_REPEAT, value.repeat ?? 'repeat', 'backgroundImage repeat'))
  writeBackgroundSize(out, value.size ?? {})
  writeLength(out, value.position?.x ?? 0, 'backgroundImage position.x')
  writeLength(out, value.position?.y ?? 0, 'backgroundImage position.y')
}

/** Writes a mask: a tag, then whatever that arm carries. */
function writeMask(out: ArenaWriter, value: Mask): void {
  // A bare string is path data, shorthand for `{ path }`.
  if (typeof value === 'string') {
    out.enum(2)
    out.text(value)
    out.enum(FILL_RULE.NonZero)
    return
  }
  if ('shape' in value) {
    out.enum(1)
    out.enum(variant(MASK_SHAPE, value.shape, 'mask shape'))
    return
  }
  if ('path' in value) {
    out.enum(2)
    out.text(value.path)
    out.enum(variant(FILL_RULE, value.fillRule ?? 'nonzero', 'mask fill rule'))
    return
  }
  out.enum(3)
  writeGradient(out, value.gradient)
}

/** Writes a list, counted. */
function writeList<T>(out: ArenaWriter, values: T | readonly T[], write: (value: T) => void): void {
  const many = Array.isArray(values) ? (values as readonly T[]) : [values as T]
  out.count(many.length)
  for (const value of many) write(value)
}

/** The number a font weight names: the two keywords are the numbers CSS gives them. */
function packWeight(weight: FontWeight, what: string): number {
  if (weight === 'normal') return 400
  if (weight === 'bold') return 700
  // A union, not an enum: `variant` would list only the two keywords and leave out
  // the numeric arm most callers use.
  if (typeof weight !== 'number') {
    throw new TypeError(`${what} is ${render(weight)}; it takes a number from 1 to 1000, or normal or bold`)
  }
  // The range and integer-ness, refused here by name rather than clamped in the
  // codec or reported as a slot offset. An infinity is refused rather than bounded:
  // a weight's range is part of the property, so it is `1500`'s mistake with a
  // larger number.
  if (!Number.isInteger(weight) || weight < 1 || weight > 1000) {
    throw new TypeError(`${what} is ${render(weight)}; it takes a whole number from 1 to 1000, or normal or bold`)
  }
  return weight
}

/**
 * One property of one group, and how a style becomes its slots. It is written when
 * any of its keys is set, which is how `width` and `height` are two properties on
 * the surface and one pair in the format.
 */
interface Property {
  /** Its bit in the group's mask, from the generated table. */
  readonly index: number
  /** The scene's field name, which is what the case fixture is keyed by. */
  readonly rust: string
  /** The style properties that feed it. */
  readonly keys: readonly (keyof Style)[]
  /**
   * Whether this style carries it, when one key being set is not the answer: two
   * properties share `borderColor`, the scalar fallback and the per-edge override,
   * and exactly one is written.
   */
  readonly present?: (style: Style) => boolean
  /** Writes its slots. Called only when the style carries it. */
  readonly write: (out: ArenaWriter, style: Style) => void
}

/** Whether a per-edge value was written as one value or as named edges. */
function perEdge(value: unknown): boolean {
  return typeof value === 'object' && value !== null
}

/**
 * The layout group, in ascending index order: the format's order, which no length
 * check would catch being wrong. The indices are asserted against the generated
 * table.
 */
const LAYOUT_PROPERTIES: readonly Property[] = [
  { index: 0, rust: 'display', keys: ['display'], write: (out, style) => out.enum(variant(DISPLAY, style.display as string, 'display')) },
  {
    index: 1,
    rust: 'position_type',
    keys: ['positionType'],
    write: (out, style) => out.enum(variant(POSITION_TYPE, style.positionType as string, 'positionType')),
  },
  {
    index: 2,
    rust: 'inset',
    keys: ['position'],
    // Every edge is optional here, and an edge the caller did not name is
    // absent rather than zero: an inset of zero pins that edge to the
    // container's, which is a different thing from leaving it to the flow.
    write: (out, style) =>
      writeSides(style.position as Sides<Length>, undefined as Length | undefined, 'an inset', edge =>
        out.optional(edge, length => writeLength(out, length, 'an inset')),
      ),
  },
  {
    index: 3,
    rust: 'size',
    keys: ['width', 'height'],
    write: (out, style) => {
      writeDimension(out, defaulted(style.width, 'auto'), 'width')
      writeDimension(out, defaulted(style.height, 'auto'), 'height')
    },
  },
  {
    index: 4,
    rust: 'min_size',
    keys: ['minWidth', 'minHeight'],
    write: (out, style) => {
      writeDimension(out, defaulted(style.minWidth, 'auto'), 'minWidth')
      writeDimension(out, defaulted(style.minHeight, 'auto'), 'minHeight')
    },
  },
  {
    index: 5,
    rust: 'max_size',
    keys: ['maxWidth', 'maxHeight'],
    write: (out, style) => {
      writeDimension(out, defaulted(style.maxWidth, 'auto'), 'maxWidth')
      writeDimension(out, defaulted(style.maxHeight, 'auto'), 'maxHeight')
    },
  },
  {
    index: 6,
    rust: 'aspect_ratio',
    keys: ['aspectRatio'],
    write: (out, style) => out.optional(style.aspectRatio, ratio => out.f32(decimal(ratio, 'aspectRatio'))),
  },
  {
    index: 7,
    rust: 'margin',
    keys: ['margin'],
    write: (out, style) => writeSides(style.margin as Sides<Dimension>, 0, 'margin', edge => writeDimension(out, edge, 'margin')),
  },
  {
    index: 8,
    rust: 'padding',
    keys: ['padding'],
    write: (out, style) => writeSides(style.padding as Sides<Length>, 0, 'padding', edge => writeLength(out, edge, 'padding')),
  },
  {
    index: 9,
    rust: 'border',
    keys: ['border'],
    write: (out, style) => writeSides(style.border as Sides<number>, 0, 'border', edge => out.f32(decimal(edge, 'border'))),
  },
  {
    index: 10,
    rust: 'flex_direction',
    keys: ['flexDirection'],
    write: (out, style) => out.enum(variant(FLEX_DIRECTION, style.flexDirection as string, 'flexDirection')),
  },
  { index: 11, rust: 'flex_wrap', keys: ['flexWrap'], write: (out, style) => out.enum(variant(FLEX_WRAP, style.flexWrap as string, 'flexWrap')) },
  { index: 12, rust: 'flex_grow', keys: ['flexGrow'], write: (out, style) => out.f32(decimal(style.flexGrow, 'flexGrow')) },
  { index: 13, rust: 'flex_shrink', keys: ['flexShrink'], write: (out, style) => out.f32(decimal(style.flexShrink, 'flexShrink')) },
  { index: 14, rust: 'flex_basis', keys: ['flexBasis'], write: (out, style) => writeDimension(out, style.flexBasis as Dimension, 'flexBasis') },
  {
    index: 15,
    rust: 'justify_content',
    keys: ['justifyContent'],
    write: (out, style) => out.optional(style.justifyContent, value => out.enum(variant(JUSTIFY, value, 'justifyContent'))),
  },
  {
    index: 16,
    rust: 'align_items',
    keys: ['alignItems'],
    write: (out, style) => out.optional(style.alignItems, value => out.enum(variant(ALIGN, value, 'alignItems'))),
  },
  {
    index: 17,
    rust: 'align_self',
    keys: ['alignSelf'],
    write: (out, style) => out.optional(style.alignSelf, value => out.enum(variant(ALIGN, value, 'alignSelf'))),
  },
  {
    index: 18,
    rust: 'align_content',
    keys: ['alignContent'],
    write: (out, style) => out.optional(style.alignContent, value => out.enum(variant(ALIGN, value, 'alignContent'))),
  },
  {
    index: 19,
    rust: 'gap',
    keys: ['gap'],
    // `(row, column)`, following CSS's shorthand. taffy spells the same pair
    // the other way round and `meo-canvas-core` swaps it at that crossing; the
    // scene's order is the one this side writes.
    write: (out, style) => {
      const gap = style.gap as Length | { readonly row?: Length; readonly column?: Length }
      // `null` and an array are both objects to `typeof`: refused before the shape
      // test, which would read `.row` off either.
      if (gap === null || Array.isArray(gap)) {
        throw new TypeError(`gap is ${render(gap)}; it takes a number of pixels, a '…%' string, or named row and column`)
      }
      if (typeof gap === 'object') {
        writeLength(out, defaulted(gap.row, 0), 'rowGap')
        writeLength(out, defaulted(gap.column, 0), 'columnGap')
        return
      }
      writeLength(out, gap, 'gap')
      writeLength(out, gap, 'gap')
    },
  },
  {
    index: 20,
    rust: 'overflow',
    keys: ['overflow'],
    // One keyword on the surface, both axes in the scene, as CSS's `overflow`
    // shorthand sets them.
    write: (out, style) => {
      const value = variant(OVERFLOW, style.overflow as string, 'overflow')
      out.enum(value)
      out.enum(value)
    },
  },
  { index: 21, rust: 'box_sizing', keys: ['boxSizing'], write: (out, style) => out.enum(variant(BOX_SIZING, style.boxSizing as string, 'boxSizing')) },
  { index: 22, rust: 'direction', keys: ['direction'], write: (out, style) => out.enum(variant(DIRECTION, style.direction as string, 'direction')) },
  {
    index: 23,
    rust: 'grid_template_columns',
    keys: ['gridTemplateColumns', 'columns'],
    present: style => columnTracks(style) !== undefined,
    write: (out, style) => writeTracks(out, columnTracks(style) as readonly TrackSize[]),
  },
  {
    index: 24,
    rust: 'grid_template_rows',
    keys: ['gridTemplateRows'],
    write: (out, style) => writeTracks(out, style.gridTemplateRows as readonly TrackSize[]),
  },
  {
    index: 25,
    rust: 'grid_auto_rows',
    keys: ['gridAutoRows'],
    write: (out, style) => out.optional(style.gridAutoRows, track => writeTrack(out, track, 'gridAutoRows')),
  },
  {
    index: 26,
    rust: 'grid_auto_columns',
    keys: ['gridAutoColumns'],
    write: (out, style) => out.optional(style.gridAutoColumns, track => writeTrack(out, track, 'gridAutoColumns')),
  },
  {
    index: 27,
    rust: 'grid_auto_flow',
    keys: ['gridAutoFlow'],
    write: (out, style) => out.enum(variant(GRID_AUTO_FLOW, style.gridAutoFlow as string, 'gridAutoFlow')),
  },
  {
    index: 28,
    rust: 'grid_column',
    keys: ['gridColumn', 'gridArea'],
    present: style => placement(style, 'column') !== undefined,
    write: (out, style) => writePlacement(out, placement(style, 'column')),
  },
  {
    index: 29,
    rust: 'grid_row',
    keys: ['gridRow', 'gridArea'],
    present: style => placement(style, 'row') !== undefined,
    write: (out, style) => writePlacement(out, placement(style, 'row')),
  },
]

/**
 * The tracks a style asks for, written out or as `columns: n`, which is `n` equal
 * fractions. Both at once is refused: nothing can tell which the caller meant.
 */
function columnTracks(style: Style): readonly TrackSize[] | undefined {
  if (style.columns === undefined) return style.gridTemplateColumns
  if (style.gridTemplateColumns !== undefined) {
    throw new TypeError('name `columns` or `gridTemplateColumns`, not both; they are two spellings of one track list')
  }
  if (!Number.isInteger(style.columns) || style.columns < 1) {
    throw new TypeError(`\`columns\` is a whole number of columns, not ${JSON.stringify(style.columns)}`)
  }
  return Array.from({ length: style.columns }, () => '1fr')
}

/**
 * The placement on one axis, from either spelling. `gridArea` is `[rowStart,
 * columnStart, rowEnd, columnEnd]` with exclusive ends, as CSS reads it, and is
 * refused beside the long form as `columns` is.
 */
function placement(style: Style, axis: 'row' | 'column'): GridPlacement | undefined {
  const long = axis === 'row' ? style.gridRow : style.gridColumn
  if (style.gridArea === undefined) return long
  if (long !== undefined) {
    throw new TypeError(`name \`gridArea\` or \`grid${axis === 'row' ? 'Row' : 'Column'}\`, not both; they are two spellings of one placement`)
  }
  const [rowStart, columnStart, rowEnd, columnEnd] = style.gridArea
  const [start, end] = axis === 'row' ? [rowStart, rowEnd] : [columnStart, columnEnd]
  if (![start, end].every(line => Number.isInteger(line)) || end <= start) {
    throw new TypeError(
      `\`gridArea\` takes four whole lines as [rowStart, columnStart, rowEnd, columnEnd], each end past its start, not ${JSON.stringify(style.gridArea)}`,
    )
  }
  return { start, span: end - start }
}

/** Writes a track list: the count, then each track. */
function writeTracks(out: ArenaWriter, tracks: readonly TrackSize[]): void {
  out.count(tracks.length)
  for (const track of tracks) writeTrack(out, track, 'a grid track')
}

/** Writes a grid placement: an optional line, then an optional span. */
function writePlacement(out: ArenaWriter, placement: GridPlacement | undefined): void {
  out.optional(placement?.start, start => out.integer(start))
  out.optional(placement?.span, span => out.integer(span))
}

/**
 * The paint group, in ascending index order. `borderColor` is one property here and
 * two in the scene: the scalar form writes `border_color_all` and the edge form
 * `border_color`.
 */
const PAINT_PROPERTIES: readonly Property[] = [
  {
    index: 0,
    rust: 'background_color',
    keys: ['backgroundColor'],
    write: (out, style) => out.text(words(style.backgroundColor, 'backgroundColor', 'a colour')),
  },
  { index: 1, rust: 'gradient', keys: ['gradient'], write: (out, style) => out.optional(style.gradient, value => writeGradient(out, value)) },
  {
    index: 2,
    rust: 'background_image',
    keys: ['backgroundImage'],
    write: (out, style) => out.optional(style.backgroundImage, value => writeBackgroundImage(out, value)),
  },
  {
    index: 3,
    rust: 'border_color',
    keys: ['borderColor'],
    present: style => perEdge(style.borderColor),
    write: (out, style) =>
      writeSides(style.borderColor as Sides<Color>, undefined as Color | undefined, 'borderColor', edge =>
        out.optional(edge, color => out.text(words(color, 'borderColor', 'a colour'))),
      ),
  },
  {
    index: 4,
    rust: 'border_color_all',
    keys: ['borderColor'],
    present: style => style.borderColor !== undefined && !perEdge(style.borderColor),
    write: (out, style) => out.text(words(style.borderColor, 'borderColor', 'a colour')),
  },
  { index: 5, rust: 'border_style', keys: ['borderStyle'], write: (out, style) => out.enum(variant(BORDER_STYLE, style.borderStyle as string, 'borderStyle')) },
  { index: 6, rust: 'border_radius', keys: ['borderRadius'], write: (out, style) => writeCorners(out, style.borderRadius as Corners, 'borderRadius') },
  { index: 7, rust: 'opacity', keys: ['opacity'], write: (out, style) => out.f32(decimal(style.opacity, 'opacity')) },
  { index: 8, rust: 'blend_mode', keys: ['mixBlendMode'], write: (out, style) => out.enum(variant(BLEND_MODE, style.mixBlendMode as string, 'mixBlendMode')) },
  { index: 9, rust: 'dither', keys: ['dither'], write: (out, style) => out.bool(style.dither as boolean) },
  // Optional on the wire because CSS's `auto` is not a number: `Some(0)` and
  // absent sort the same and differ in whether the node establishes a stacking
  // context. A caller who writes `zIndex` means a number, so the surface has no
  // spelling for `auto` — leaving it unset is what says it.
  {
    index: 10,
    rust: 'z_index',
    keys: ['zIndex'],
    write: (out, style) => out.optional(style.zIndex, index => out.integer(whole(index, 'zIndex'))),
  },
]

/**
 * The text group, in ascending index order. Every field is optional in the scene,
 * since absent means inherit, so each writes a presence flag as well as a value.
 */
const TEXT_PROPERTIES: readonly Property[] = [
  {
    index: 0,
    rust: 'font_family',
    keys: ['fontFamily'],
    write: (out, style) => out.optional(style.fontFamily, family => out.text(words(family, 'fontFamily', 'a registered font name'))),
  },
  { index: 1, rust: 'font_size', keys: ['fontSize'], write: (out, style) => out.optional(style.fontSize, size => out.f32(decimal(size, 'fontSize'))) },
  {
    index: 2,
    rust: 'font_weight',
    keys: ['fontWeight'],
    write: (out, style) => out.optional(style.fontWeight, weight => out.integer(packWeight(weight, 'fontWeight'))),
  },
  {
    index: 3,
    rust: 'font_style',
    keys: ['fontStyle'],
    write: (out, style) => out.optional(style.fontStyle, value => out.enum(variant(FONT_STYLE, value, 'fontStyle'))),
  },
  { index: 4, rust: 'color', keys: ['color'], write: (out, style) => out.optional(style.color, color => out.text(words(color, 'color', 'a colour'))) },
  {
    index: 5,
    rust: 'text_align',
    keys: ['textAlign'],
    write: (out, style) => out.optional(style.textAlign, value => out.enum(variant(TEXT_ALIGN, value, 'textAlign'))),
  },
  {
    index: 6,
    rust: 'text_decoration',
    keys: ['textDecoration'],
    write: (out, style) => out.optional(style.textDecoration, value => out.enum(variant(TEXT_DECORATION, value, 'textDecoration'))),
  },
  {
    index: 7,
    rust: 'vertical_align',
    keys: ['verticalAlign'],
    write: (out, style) => out.optional(style.verticalAlign, value => out.enum(variant(VERTICAL_ALIGN, value, 'verticalAlign'))),
  },
  {
    index: 8,
    rust: 'paint_order',
    keys: ['paintOrder'],
    write: (out, style) => out.optional(style.paintOrder, value => out.enum(variant(PAINT_ORDER, value, 'paintOrder'))),
  },
  {
    index: 9,
    rust: 'line_height',
    keys: ['lineHeight'],
    write: (out, style) => out.optional(style.lineHeight, height => writeLineHeight(out, height, 'lineHeight')),
  },
  { index: 10, rust: 'line_gap', keys: ['lineGap'], write: (out, style) => out.optional(style.lineGap, gap => out.f32(decimal(gap, 'lineGap'))) },
  {
    index: 11,
    rust: 'letter_spacing',
    keys: ['letterSpacing'],
    write: (out, style) => out.optional(style.letterSpacing, value => writeSpacing(out, value, 'letterSpacing')),
  },
  {
    index: 12,
    rust: 'word_spacing',
    keys: ['wordSpacing'],
    write: (out, style) => out.optional(style.wordSpacing, value => writeSpacing(out, value, 'wordSpacing')),
  },
  {
    index: 13,
    rust: 'font_variant',
    keys: ['fontVariant'],
    write: (out, style) =>
      out.optional(style.fontVariant, features => {
        out.count(features.length)
        for (const feature of features) out.enum(variant(FONT_VARIANT, feature, 'fontVariant'))
      }),
  },
  {
    index: 14,
    rust: 'text_stroke',
    keys: ['textStroke'],
    write: (out, style) =>
      out.optional(style.textStroke, stroke => {
        out.f32(stroke.width ?? 0)
        // Black rather than the text's own colour: the scene's `TextStroke` carries
        // a colour and has no way to say "whatever the glyphs are".
        out.text(stroke.color ?? SHADOW_BLACK)
      }),
  },
]

/** The effects group, in ascending index order. */
const EFFECTS_PROPERTIES: readonly Property[] = [
  { index: 0, rust: 'transform', keys: ['transform'], write: (out, style) => out.optional(style.transform, value => writeTransform(out, value)) },
  {
    index: 1,
    rust: 'box_shadows',
    keys: ['boxShadow'],
    write: (out, style) => writeList(out, style.boxShadow as BoxShadow | readonly BoxShadow[], shadow => writeBoxShadow(out, shadow)),
  },
  {
    index: 2,
    rust: 'text_shadows',
    keys: ['textShadow'],
    write: (out, style) => writeList(out, style.textShadow as TextShadow | readonly TextShadow[], shadow => writeTextShadow(out, shadow)),
  },
  { index: 3, rust: 'mask', keys: ['mask'], write: (out, style) => out.optional(style.mask, value => writeMask(out, value)) },
  { index: 4, rust: 'filter', keys: ['filter'], write: (out, style) => out.optional(style.filter, value => out.text(words(value, 'filter', 'a CSS filter'))) },
  {
    index: 5,
    rust: 'backdrop_filter',
    keys: ['backdropFilter'],
    write: (out, style) => out.optional(style.backdropFilter, value => out.text(words(value, 'backdropFilter', 'a CSS filter'))),
  },
]

/** How many mask slots a group of this many properties needs. */
function slotsFor(properties: number): number {
  return Math.ceil(properties / MASK_BITS)
}

/** The four groups, in the order a node writes them. */
const GROUPS = [
  { properties: LAYOUT_PROPERTIES, slots: slotsFor(LAYOUT.length) },
  { properties: PAINT_PROPERTIES, slots: slotsFor(PAINT.length) },
  { properties: TEXT_PROPERTIES, slots: slotsFor(TEXT.length) },
  { properties: EFFECTS_PROPERTIES, slots: slotsFor(EFFECTS.length) },
] as const

/** Whether a property is carried by this style. */
function carries(property: Property, style: Style): boolean {
  if (property.present !== undefined) return property.present(style)
  return property.keys.some(key => style[key] !== undefined)
}

/**
 * Writes one group's values and reports which bits to set. A node's masks sit
 * together before any values, so the caller reserves them and patches them in.
 */
function writeValues(out: ArenaWriter, properties: readonly Property[], slots: number, style: Style | undefined): readonly number[] {
  // A slot at a time, accumulating into a local rather than into a subscript.
  // The order is unaffected — a group's properties ascend and so do its slots,
  // so slot-major is still index-major — and the accumulator needs no `?? 0`
  // for an element that is always there.
  if (style === undefined) return new Array<number>(slots).fill(0)

  const bits: number[] = []
  for (let at = 0; at < slots; at += 1) {
    let carried = 0
    for (const property of properties) {
      if (Math.floor(property.index / MASK_BITS) !== at) continue
      if (!carries(property, style)) continue
      // `+=` rather than `|=`: a bitwise or is a signed 32-bit operation and
      // would lose every bit above the 31st, where a mask slot holds 53.
      carried += 2 ** (property.index % MASK_BITS)
      property.write(out, style)
    }
    bits.push(carried)
  }
  return bits
}

/**
 * Each group's table, by the name the case fixture groups by. Exported so a test
 * can check every `arena_group!` property is written here or named as unspelled.
 */
export const PROPERTY_TABLES: Readonly<Record<string, readonly Property[]>> = {
  layout: LAYOUT_PROPERTIES,
  paint: PAINT_PROPERTIES,
  text: TEXT_PROPERTIES,
  effects: EFFECTS_PROPERTIES,
}

/**
 * Writes the four style groups of one node: all four masks, then the values, the
 * order the reader consumes them in.
 */
function writeStyle(out: ArenaWriter, style: Style | undefined): void {
  const reserved = GROUPS.map(group => ({ group, at: out.reserveMask(group.slots) }))
  for (const { group, at } of reserved) {
    out.patchMask(at, writeValues(out, group.properties, group.slots, style))
  }
}

/** Black: the fill a path takes when nothing names one. */
const BLACK = '#000000'

/** Where an image sits when nothing says otherwise: the centre, as the Rust surface writes. */
const CENTRED: readonly [Length, Length] = ['50%', '50%']

/**
 * Writes the payload only a text node has. It says whether the content is markup
 * the renderer parses or segments it leaves alone; without that, `RichText` of one
 * run and `Text` would write the same bytes.
 */
function writeTextPayload(out: ArenaWriter, node: SceneNode): void {
  out.optional(node.paragraph?.maxLines, lines => out.integer(lines))
  out.optional(node.paragraph?.ellipsis, ellipsis => out.text(ellipsis))

  if (node.markup !== undefined) {
    out.present()
    out.text(node.markup)
    return
  }

  out.absent()
  const segments = node.segments ?? []
  out.count(segments.length)
  for (const segment of segments) {
    out.text(segment.text)
    const at = out.reserveMask(GROUPS[2].slots)
    out.patchMask(at, writeValues(out, TEXT_PROPERTIES, GROUPS[2].slots, segment.style))
  }
}

/** Writes the payload only an image node has. */
function writeImagePayload(out: ArenaWriter, src: ImageSource, style: Style | undefined): void {
  // The image node's own source, written by the same rules as one in a style, so
  // its URL is counted too.
  writeSource(out, src)

  // `objectFit` and `frame` sit in the payload, where the scene keeps image-only
  // fields; the surface keeps them in the style, and this is where the two meet.
  out.enum(variant(OBJECT_FIT, style?.objectFit ?? 'fill', 'objectFit'))
  const position = style?.objectPosition ?? CENTRED
  writeLength(out, position[0], 'objectPosition x')
  writeLength(out, position[1], 'objectPosition y')
  out.optional(style?.frame, frame => out.integer(frame))
}

/**
 * Writes one of a path's two paints: solid or gradient, inside an option. `'none'`
 * is the absent option, not a transparent colour. A gradient is the object form;
 * every string is a colour or `'none'`.
 */
function writePathPaint(out: ArenaWriter, paint: PathPaint | undefined, fallback: Color | undefined): void {
  if (paint === 'none' || (paint === undefined && fallback === undefined)) {
    out.absent()
    return
  }

  out.present()
  if (typeof paint === 'object') {
    out.enum(1)
    writeGradient(out, paint)
    return
  }
  out.enum(0)
  out.text(paint === undefined ? (fallback as Color) : paint)
}

/** Writes the payload only a path node has. */
function writePathPayload(out: ArenaWriter, props: PathProps): void {
  out.text(props.d)

  // Four floats behind a flag, matching the byte codec.
  const view = props.viewBox
  out.bool(view !== undefined)
  if (view !== undefined) for (const number of view) out.f32(number)
  out.bool(props.preserveAspectRatio === 'none')

  // Black and unstroked when the caller says nothing, which is SVG's default
  // and what the Rust surface writes.
  writePathPaint(out, props.fill, BLACK)
  writePathPaint(out, props.stroke, undefined)

  out.f32(props.lineWidth ?? 1)
  out.enum(props.fillRule === 'evenodd' ? FILL_RULE.EvenOdd : FILL_RULE.NonZero)
  out.enum(variant(LINE_CAP, props.lineCap ?? 'butt', 'lineCap'))
  out.enum(variant(LINE_JOIN, props.lineJoin ?? 'miter', 'lineJoin'))

  const dash = props.lineDash ?? []
  out.count(dash.length)
  for (const length of dash) out.f32(length)
  out.f32(props.lineDashOffset ?? 0)
}

/** Writes one node, its payload, and its subtree. */
function writeNode(out: ArenaWriter, node: SceneNode): void {
  out.enum(variant(NODE_TAG, node.kind, 'kind'))
  writeStyle(out, node.style)

  if (node.kind === 'text') writeTextPayload(out, node)
  else if (node.kind === 'image' && node.src !== undefined) writeImagePayload(out, node.src, node.style)
  else if (node.kind === 'path') writePathPayload(out, (node.style ?? {}) as PathProps)

  out.optional(node.name, name => out.text(name))

  const children = node.children ?? []
  out.count(children.length)
  for (const child of children) writeNode(out, child)
}

/**
 * Writes a scene into an arena in one pass, after the whole tree is built: the
 * factories build plain objects, and writing as each ran would land nodes
 * post-order where the arena is pre-order.
 */
export function encodeScene(
  pages: readonly SceneNode[],
  width: number,
  height: number,
  contentHeight: boolean,
  scale: number,
  surface: SurfaceOptions = {},
  fetched?: ReadonlyMap<string, Uint8Array>,
  attempts: readonly FetchAttempt[] = [],
  httpOptions?: RequestInit,
): Arena {
  const out = new ArenaWriter()
  out.fetched = fetched
  out.httpOptions = httpOptions
  writeHeader(out, width, height, contentHeight, scale, surface, pages.length, attempts)
  for (const page of pages) writeNode(out, page)
  return out.finish()
}
