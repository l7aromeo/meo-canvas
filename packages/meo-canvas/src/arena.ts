/**
 * The `f64` arena: how a scene crosses into the addon.
 *
 * One `Float64Array` carries the whole tree. The wire specification lives in
 * `crates/meo-canvas-node/src/arena.rs` — this is the writing half of it, and
 * the reader on the other side is the definition. Where the two could disagree
 * the numbers are generated rather than copied: {@link MAGIC}, {@link VERSION}
 * and the property tables come from `./generated/arena-tables.js`.
 *
 * It is shaped this way because **reading a value out of V8 is what costs**,
 * not the crossing. A `lineTo` in `meo-skia-canvas` costs 82 nanoseconds, of
 * which 17 is the crossing and 39 is reading two floats out of the arguments.
 * Decoding from a `&[f64]` skips V8 entirely, and a store into a
 * `Float64Array` is one operation where writing varint bytes from JavaScript
 * is several.
 *
 * Strings and buffers cannot live in a `Float64Array`, so they go into a side
 * array and the arena stores an index into it.
 *
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
   * Every URL source the scene named.
   *
   * Carried out rather than refused in the writer: the arm belongs to the wire
   * and a Rust caller that resolves its own sources may write one. What this
   * surface cannot do is promise to *draw* it — nothing here fetches, and
   * `meo-canvas-core` refuses a URL by design — so {@link Root} reads this and
   * refuses before rendering, at the surface where the promise was made rather
   * than at the far end where it could only fail.
   */
  readonly urls: readonly string[]
}

/**
 * The tag an absent optional is written as.
 *
 * Named because `0` and `1` appear as three different things in this format —
 * a presence flag, a boolean, and an enum discriminant — and a reader of this
 * file should not have to work out which from context.
 */
const ABSENT = 0

/** The tag a present optional is written as. */
const PRESENT = 1

/** The largest value one mask slot holds: 53 bits all set. */
const LARGEST_MASK_SLOT = 2 ** MASK_BITS - 1

/**
 * Accumulates the slots of one scene.
 *
 * A cursor rather than an index calculation, because slot counts are not
 * constant: a list writes its length and then its items, an optional writes a
 * flag and perhaps a value. Every method appends and returns nothing, so a
 * caller cannot write a value at the wrong offset by arithmetic.
 */
export class ArenaWriter {
  /** The slots written so far. */
  readonly #slots: number[] = []

  /** The strings and buffers the slots index. */
  readonly #values: SideValue[] = []

  /**
   * Where each string already written sits in {@link ArenaWriter.#values}.
   *
   * Strings repeat heavily — a font family appears on every text node of a
   * document — and each duplicate is one more value the addon has to read out
   * of V8, which is the cost this format exists to avoid. Buffers are not
   * deduplicated: comparing their contents would cost more than the copy saves,
   * and the same buffer written twice is a caller's own doing.
   */
  readonly #strings = new Map<string, number>()

  /** The scene, ready to hand across. */
  /** Every URL source written so far. */
  readonly urls: string[] = []

  /**
   * Bytes already obtained for a URL source, keyed by the URL.
   *
   * Set on the **second** encode of a render that named one. The first encode
   * is what discovers the URLs — this writer is the only thing that knows every
   * position a source can occupy, so asking it is cheaper than maintaining a
   * second walker that would drift from it the first time a source moves.
   */
  fetched: ReadonlyMap<string, Uint8Array> | undefined = undefined

  finish(): Arena {
    return { slots: Float64Array.from(this.#slots), values: this.#values, urls: this.urls }
  }

  /** Writes one slot exactly as given. */
  slot(value: number): void {
    this.#slots.push(value)
  }

  /**
   * Writes a number the reader narrows to `f32`.
   *
   * Nothing is rounded here. The arena is `f64` because JavaScript has no other
   * number and every geometric quantity in a scene is `f32`, so the narrowing
   * happens once, on the reading side, rather than twice.
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
   * Reserves a group's mask slots and returns where they sit.
   *
   * The mask says which of a group's properties follow it, and that is not
   * known until they have been written. So the slots are reserved, the values
   * are written, and {@link ArenaWriter.patchMask} fills the reservation in —
   * one pass over the style rather than one to decide and another to write.
   */
  reserveMask(slots: number): number {
    const at = this.#slots.length
    for (let index = 0; index < slots; index += 1) this.#slots.push(0)
    return at
  }

  /**
   * Fills in a reservation with the bits of the properties that were written.
   *
   * `bits` is one accumulator per slot, each carrying at most
   * {@link MASK_BITS} bits — 53, because a double is exact on integers only to
   * 2^53 and the 54th bit of a mask packed into one slot is lost with no
   * rounding a reader could detect.
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
 * What a scene says about the surface it is drawn on, beyond its size.
 *
 * Every field optional, and absent is not the same as the renderer's default:
 * absent leaves the choice to the renderer, which is what makes *the
 * renderer's value is the default* true rather than a comment.
 */
export interface SurfaceOptions {
  /** Whether to rasterise on the GPU. */
  readonly gpu?: boolean
  /** The pixel layout the surface composites in. */
  readonly colorType?: ColorType
  /** The colour space the surface composites in. */
  readonly colorSpace?: ColorSpace
  /**
   * What a render does when an image source cannot be resolved.
   *
   * Not optional, unlike the three above: they distinguish "the caller said
   * nothing" from "the caller asked for the default" because the renderer has
   * its own answer for them. This one does not — the policy is the scene's
   * alone — so the default is a value rather than an absence.
   */
  readonly onImageError?: OnImageError
}

/**
 * A fetch this surface attempted and could not complete.
 *
 * **Carries the reason, not only the fact.** Without it the renderer would
 * have to synthesise a vaguer warning than a crate consumer gets for the same
 * real-world 404, and the two public surfaces would describe one event
 * differently.
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
 * The wire tag for each failure, written by hand.
 *
 * Not from the generated enum table: `ImageFetchFailure` carries a payload on
 * `Status` and so is not a `wire_enum!`, which is what that table is generated
 * from. The discriminants here are the same contract, and the Rust reader
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

  // The surface block, between the geometry and the pages. Slots inserted
  // rather than appended, which is why the arena's VERSION moved to 2: a reader
  // of the old revision would take `gpu`'s discriminant for the page count.
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
    // The status is written *behind* the tag rather than in a slot beside it,
    // so a `'status'` with no code cannot be encoded. Two fields and a rule
    // that they agree makes the disagreement representable, and the only thing
    // to do with it downstream is invent a number a consumer is documented to
    // branch on.
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
 * The TypeScript spelling of a Rust variant name.
 *
 * Every keyword this surface takes is the kebab-case of the scene's variant —
 * `'space-between'` is `SpaceBetween`, `'scale-down'` is `ScaleDown` — so the
 * mapping is derived rather than listed. Deriving it is what makes a keyword
 * with no variant behind it **throw** instead of silently writing nothing:
 * finding `'oblique'` and `'baseline'` in this package's own unions, neither of
 * which the scene or v1 has, is what the derivation caught.
 *
 * This map holds the exceptions, which are the words CSS runs together where
 * the scene spells the concept in parts: `nowrap`, `nonzero`, `evenodd`.
 */
const SPELLINGS: Readonly<Record<string, string>> = {
  nowrap: 'NoWrap',
  // CSS spells a fill rule as one word too, and so does v1's `Mask`.
  nonzero: 'NonZero',
  evenodd: 'EvenOdd',
}

/**
 * Upstream's own alternate spellings for the two surface enums.
 *
 * `ColorType` and `ColorSpace` take upstream's names rather than this package's
 * house style, because a v1 caller already has those strings written down — see
 * the unions in `index.ts`. Upstream offers several of them under more than one
 * name, and both names have to work.
 *
 * A table rather than a derivation, and it is the only one here: there is no
 * rule that turns `'RGBA8888'` into `Uint8` or `'hdr10'` into `Rec2020Pq`. What
 * keeps it honest is the keyword test, which walks every member of both unions
 * and every variant of both enums, so a name in neither place fails.
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
 * The Rust variant name a keyword means.
 *
 * A keyword that is already a variant name is taken as written. Two of these
 * enums are not CSS vocabulary at all — a pixel layout is `R8G8UNorm`, and
 * kebab-casing it produces something nobody would type — so those surfaces
 * spell the variant, as v1 does for the same list.
 */
function variantName(keyword: string, table: Readonly<Record<string, number>>): string {
  if (table[keyword] !== undefined) return keyword

  const exception = SPELLINGS[keyword]
  if (exception !== undefined) return exception

  // The derivation before the aliases, and this order is load-bearing:
  // `ALIASES` is one table across every enum, and `'linear'` means
  // `SrgbLinear` to a colour space and `Linear` to a gradient. Deriving first
  // lets each enum keep its own reading of a word another enum has claimed, and
  // an alias is consulted only when nothing in *this* table answers.
  const derived = keyword
    .split('-')
    .map(part => part.charAt(0).toUpperCase() + part.slice(1))
    .join('')
  if (table[derived] !== undefined) return derived

  return ALIASES[keyword] ?? derived
}

/**
 * The keyword a Rust variant name is written as, for an error message.
 *
 * Names what the **encoder** accepts, which for `ColorType` and `ColorSpace` is
 * a little wider than the union: a variant name resolves as written, so
 * `'Alpha8'` and `'alpha8'` both work where the union lists only the first.
 * Erring wide is right for a message whose job is to unstick a caller.
 */
function keywordFor(name: string, table: Readonly<Record<string, number>>): string {
  const spelt = Object.entries(SPELLINGS).find(([, variant]) => variant === name)
  if (spelt !== undefined) return spelt[0]

  // An alias first, because where one exists it is the spelling the surface
  // offers: `Uint8` is written `'rgba'`, not `'uint8'`. Then the kebab form,
  // where it is one this surface takes — it is not for the rest of
  // `ColorType`, whose variants are spelt as upstream spells them, so
  // `R8UNorm` is offered as itself rather than as `r8-unorm`.
  const alias = Object.entries(ALIASES).find(([, variant]) => variant === name)
  if (alias !== undefined) return alias[0]

  const kebab = name.replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase()
  return variantName(kebab, table) === name ? kebab : name
}

/**
 * Renders a rejected value so a caller can recognise what they passed.
 *
 * `JSON.stringify` for a string, so `""` is visible rather than a gap;
 * the literal for a finite number; `NaN` and `Infinity` spell themselves; and
 * the kind of thing where the value itself would say nothing.
 */
export function render(value: unknown): string {
  if (typeof value === 'string') return JSON.stringify(value)
  if (typeof value === 'number') return String(value)
  if (typeof value === 'function') return 'a function'
  if (value === null) return 'null'
  // A list before the object it technically is. An array reported as "an
  // object" is the same unhelpfulness this function exists to replace, and the
  // mistake it comes from is specific: a caller reaching for CSS's four-value
  // shorthand writes `padding: [8, 4]`, and being told that is an object gives
  // them nothing to correct.
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
 * Refuses anything but a whole number, naming the property that was written.
 *
 * **Refused here rather than reported by the reader.** A value of the wrong
 * type reaches the arena as `NaN` and comes back as `slot 33 holds NaN, which
 * is not an integer` -- an offset into a wire format the caller never saw, and
 * one that moves with the rest of the scene, so the same mistake produces
 * different text in different trees. The writer is the last place that still
 * knows the caller wrote `zIndex`.
 *
 * **And the writer is the only door these can arrive through.** The crate
 * surface cannot express a string where a number goes: Rust refuses it at
 * compile time. So refusing here repairs every case, which is not true of a
 * value that is in-type on both surfaces.
 */
function whole(value: unknown, what: string): number {
  if (typeof value !== 'number' || !Number.isInteger(value)) {
    throw new TypeError(`${what} is ${render(value)}; it takes a whole number`)
  }
  return value
}

/**
 * Refuses anything but a number, naming the property that was written.
 *
 * `whole`'s neighbour for the properties that take a fraction. Without it
 * `opacity: null` reached `out.f32`, which pushes whatever it is given into a
 * slot, and `Number(null)` is `0` -- so the element rendered fully transparent
 * and nothing was reported. `fontSize: null` blanked the text the same way.
 *
 * **`NaN` is refused and `Infinity` is not, and the predicate is the whole
 * difference.** `typeof NaN === 'number'`, so it passed every guard here until
 * it was named: a check about the *type* cannot see it. `Number.isNaN(value)`
 * is what sees it, and `!Number.isFinite(value)` -- which reads as the same
 * intent and is one word shorter -- would take `Infinity` with it.
 *
 * **`Infinity` is deliberately not refused here.** It is bounded on the other
 * side, where a finite ceiling gives it a meaning; refusing it here would make
 * that unreachable, and the picture would not change until someone went looking
 * for it. The two values are answered differently because they mean different
 * things: an infinity means *as large as possible*, and a `NaN` means nothing
 * at any door.
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
 * Refuses a value that is neither a number nor a string, naming the property.
 *
 * The guard the `write*` helpers below share. Each of them tries the number
 * first and then reads the tail of a string, so anything else reached
 * `value.endsWith` and came back as `value.endsWith is not a function` -- the
 * name of a parser's internal step, which no caller can search for or act on.
 * The string throw each helper already carries is untouched and still says
 * which spellings exist: a *bad string* is a different mistake from a value of
 * the wrong kind, and only one of them is helped by a list of suffixes.
 */
function measured(value: unknown, what: string, takes: string): number | string {
  if ((typeof value !== 'number' && typeof value !== 'string') || Number.isNaN(value)) {
    throw new TypeError(`${what} is ${render(value)}; it takes ${takes}`)
  }
  return value
}

/**
 * The value, or `fallback` where the caller wrote nothing at all.
 *
 * `??` where this is used instead, because `??` cannot tell *unset* from
 * `null`: `style.width ?? 'auto'` turned `width: null` into `auto` and the
 * declaration vanished with nothing said, while `fontWeight: null` two rows
 * away was refused by name. One of those had to move, and refusing is the
 * older of the two -- `efeb61e` settled it for the properties it reached.
 */
function defaulted<T>(value: T | undefined, fallback: T): T {
  return value === undefined ? fallback : value
}

/**
 * The number a keyword crosses as.
 *
 * Throws rather than defaults. A keyword the scene has no variant for is a
 * property this package accepts and cannot carry, and quietly writing the
 * zeroth variant would make it arrive as a different value.
 */
export function variant(table: Readonly<Record<string, number>>, keyword: string, what: string): number {
  const taken = (): string =>
    Object.keys(table)
      .map(name => keywordFor(name, table))
      .join(', ')
  // Refused before the lookup, because `variantName` reaches for
  // `keyword.split` and a value that is not a string came back carrying that
  // method's name instead of this property's. The list of keywords is built
  // inside the two failing branches rather than above them: every enum in the
  // scene crosses this line on every write, and only the mistakes need it.
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
 * The fraction a `'…%'` string names, or `undefined` if it is not one.
 *
 * **Divided by a hundred**, because the scene stores a percentage as a fraction
 * where `1.0` is 100% — `Length::Percent`, `Dimension::Percent` and
 * `TrackSize::Percent` all say so, and taffy's `percent()` takes the same. A
 * caller writes `'50%'` and the scene holds `0.5`.
 *
 * The probes are `'100%'`, which is the spelling of `Percent(1.0)`. `'1%'` is
 * the one value that cannot check this: written as `1` it equals
 * `Percent(1.0)` whether or not the division happens, so a probe using it
 * leaves the round trip and the byte comparison agreeing either way. The check
 * that does not share the units is in `root.test.ts`, which counts rendered
 * pixels.
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
 * Writes a dimension: a tag, then the value, which is written even for `auto`.
 *
 * Every dimension is two slots wide whatever it holds, so this side emits a
 * pair unconditionally rather than branching — and a fixed width is what lets
 * the reader skip a property it does not recognise.
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
 * Writes a line height: a tag, then the value.
 *
 * CSS's four kinds and the three the scene stores. `normal` is the absence of
 * a value and never reaches here — the caller writes nothing at all, and the
 * `optional` wrapper around this writes the absent marker.
 *
 * **A percentage is written as a percentage and resolved in the core**, at the
 * element that declares it, because that is where its own font size is known.
 * A number is not resolved there: it descends as a number and each inheritor
 * recomputes it against its own size. Measured in Chrome, a 16px parent
 * declaring for a 32px child: `1.5` inherits as 48, `150%` inherits as 24.
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
 * Writes the four edges, in `top right bottom left` order.
 *
 * An edge the caller did not name takes `fallback`, which is that property's
 * own default in the scene rather than a shared zero — `margin` defaults to
 * zero points and `inset` to nothing at all, and writing one where the other
 * belongs would change what the layout does.
 */
function writeSides<T>(value: Sides<T>, fallback: T, what: string, write: (value: T) => void): void {
  // **A list is the one wrong shape this would otherwise swallow.** CSS's
  // shorthand takes up to four values and a caller reaches for `[8, 4]`; an
  // array is an object with no `top`, so every edge took the fallback and the
  // declaration was dropped without a word. A single value and the named form
  // are both real spellings; a list is not one yet.
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
 * Writes a transform, filling in the scene's defaults for what was not named.
 *
 * Every field is written whether or not the caller set it, because the wire
 * shape is fixed — the mask bit says the property is present and the six values
 * follow. Absent means the scene's default, which is why they are stated here
 * rather than left to a zero: a `scale` of zero is not the same thing as no
 * scale at all.
 */
function writeTransform(out: ArenaWriter, value: Transform): void {
  writeLength(out, value.translateX ?? 0, 'transform translateX')
  writeLength(out, value.translateY ?? 0, 'transform translateY')
  out.f32(value.rotate ?? 0)
  // `scale` sets both axes and a per-axis value beside it wins, which is v1's
  // rule and the only place these two spellings meet.
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
 * The angle each named direction resolves to, clockwise from twelve o'clock.
 *
 * The scene holds a linear direction as an angle or as two points, and a
 * keyword is neither — it is a direction whose angle is known before the box
 * is. CSS and v1 name the same eight.
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
 * The stops a ramp names.
 *
 * `colors` is spread evenly from the first to the last, which is v1's rule, and
 * a single colour sits at the midpoint rather than at the start — a one-colour
 * gradient is a flat fill and where it sits does not matter, but the midpoint
 * is what v1 writes and a fixture would notice the difference.
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
    // **Bytes cross the wire, never a URL.** Where this render has already
    // fetched the URL, the source is written as though the caller had passed
    // the bytes — so nothing downstream has to know a network was involved and
    // `meo-canvas-core` needs no `net` feature to draw it.
    const bytes = out.fetched?.get(source.url)
    if (bytes !== undefined) {
      out.enum(2)
      out.bytes(bytes)
      return
    }

    // Otherwise written, and **counted**. The arm is part of the wire and a
    // Rust caller with a resolver of its own can use it. The count is what
    // `Root` reads to decide whether a fetch pass is needed at all, so a scene
    // naming no URL pays nothing for this.
    out.urls.push(source.url)
    out.enum(1)
    out.text(source.url)
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
  // A bare value sizes the width and leaves the height to the picture's own
  // proportions, which is v1's reading and CSS's one-value form.
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
  // A bare string is path data, which is v1's shorthand for `{ path }`.
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
  // **A union, not an enum.** The property takes a number *or* one of two
  // keywords, so listing the keywords alone -- which is what `variant` would
  // do -- would confidently name an accepted set that excludes the numeric
  // arm, and that is most of what callers pass. Worse than the message it
  // replaces.
  if (typeof weight !== 'number') {
    throw new TypeError(`${what} is ${render(weight)}; it takes a number from 1 to 1000, or normal or bold`)
  }
  return weight
}

/**
 * One property of one group, and how a style becomes its slots.
 *
 * The index and the Rust field name come from the generated tables; the keys
 * and the writer are what this file adds. A property is written when **any** of
 * its keys is set, which is what lets `width` and `height` be two properties on
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
   * Whether this style carries it, when one key being set is not the answer.
   *
   * Two properties share `borderColor`, and exactly one of them is written: the
   * scalar form is the fallback colour, the edge form is the per-edge
   * override. Without this both would be written from one key and the reader
   * would take four colours' worth of slots for one.
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
 * The layout group, in ascending index order.
 *
 * The order is the format's: present properties are written in ascending index
 * order, and a table out of order would read the right number of slots into the
 * wrong fields — which no length check catches. The indices are asserted
 * against the generated table rather than trusted.
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
      // `null` is an object to `typeof` and an array is one too, so both read
      // `.row` off something that has none: one threw with the name of the
      // field, the other wrote two zeroes and said nothing. Refused above the
      // shape test rather than inside it, because narrowing an array out of
      // the named form leaves it in the arm that takes a single value.
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
    // One keyword on the surface, both axes in the scene: CSS's `overflow`
    // shorthand sets them together and nothing here sets them apart yet.
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
 * The tracks a style asks for, whether it wrote them out or asked for `n`.
 *
 * `columns: 3` is three equal fractions and nothing else, so the shorthand is
 * resolved here rather than carried: the arena has one way to say a track list
 * and this is where the surface's two spellings become it.
 *
 * Both at once is refused. A caller who wrote `columns` beside
 * `gridTemplateColumns` meant one of them and nothing here can tell which, and
 * a precedence rule would silently drop the other.
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
 * The placement a style asks for on one axis, from either spelling.
 *
 * `gridArea` is `[rowStart, columnStart, rowEnd, columnEnd]` with the ends
 * exclusive, as CSS orders and reads them, so a span is the difference between
 * a pair. Refused beside the long form for the reason `columns` is.
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
 * The paint group, in ascending index order.
 *
 * `borderColor` is one property here and two in the scene: a fallback colour
 * beside per-edge overrides. That split is for the wire format's convenience
 * rather than the caller's, so the scalar form routes to `border_color_all` and
 * the edge form to `border_color`, and no v2-only name reaches the surface.
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
 * The text group, in ascending index order.
 *
 * Every field here is an `Option` in the scene, because a text style inherits:
 * absent means "take the parent's" where a layout property's absence means "take
 * the default". So each writes a presence flag as well as its value, and the
 * mask bit alone would not have said enough.
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
        // Black rather than the text's own colour, which is what v1 documents:
        // the scene's `TextStroke` carries a colour and has nowhere to say
        // "whatever the glyphs are". Naming one is the honest form.
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
 * Writes the values of one group, and reports which bits to set.
 *
 * The mask cannot be written here: a node's four masks sit together, before any
 * of the four groups' values. So the caller reserves all four, calls this, and
 * fills the reservations in — which also means the style is walked once rather
 * than once to decide and once to write.
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
 * Each group's table, by the name the case fixture groups by.
 *
 * Exported so a test can check this file's tables against the generated ones
 * and against the case fixture: every property of every `arena_group!` is
 * either written here or named as one this surface does not spell yet. A
 * property added upstream with neither then fails a test rather than being
 * quietly absent from every scene.
 */
export const PROPERTY_TABLES: Readonly<Record<string, readonly Property[]>> = {
  layout: LAYOUT_PROPERTIES,
  paint: PAINT_PROPERTIES,
  text: TEXT_PROPERTIES,
  effects: EFFECTS_PROPERTIES,
}

/**
 * Writes the four style groups of one node.
 *
 * All four masks, then all four groups' values, which is the order the reader
 * consumes them in: it reads the masks together so it knows what follows before
 * it reads any of it.
 */
function writeStyle(out: ArenaWriter, style: Style | undefined): void {
  const reserved = GROUPS.map(group => ({ group, at: out.reserveMask(group.slots) }))
  for (const { group, at } of reserved) {
    out.patchMask(at, writeValues(out, group.properties, group.slots, style))
  }
}

/** Black: the fill a path takes when nothing names one. */
const BLACK = '#000000'

/**
 * The centre of the box, which is where an image sits when nothing says
 * otherwise.
 *
 * The same default the Rust surface writes, because the two surfaces produce
 * one scene and a picture should not depend on which one built it.
 */
const CENTRED: readonly [Length, Length] = ['50%', '50%']

/**
 * Writes the payload only a text node has.
 *
 * The content is one of two things and the payload says which: `markup` is a
 * string the renderer parses, `segments` are runs the caller built and the
 * renderer leaves alone. Without that discriminant the two are indistinguishable
 * — `RichText` of one run writes the same bytes as `Text` — and the decoder
 * would have to guess, losing either `RichText`'s literal `<` or every
 * caller's rich text.
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
  // The image node's own source, written by the same rules as one in a style.
  // Two writers existed here and only one counted its URLs, which meant the
  // check in `Root` saw a background image's URL and never an `Image`'s -- the
  // node the property is *for*.
  writeSource(out, src)

  // `objectFit` and `frame` sit in the payload rather than in a style group,
  // because they are meaningless on anything but an image and the scene puts
  // them where the node kind is. The surface stays flat over that: a caller
  // should not have to know that `objectFit` is payload and `opacity` is a mask
  // bit. This is the seam, and it belongs here rather than in the surface.
  out.enum(variant(OBJECT_FIT, style?.objectFit ?? 'fill', 'objectFit'))
  const position = style?.objectPosition ?? CENTRED
  writeLength(out, position[0], 'objectPosition x')
  writeLength(out, position[1], 'objectPosition y')
  out.optional(style?.frame, frame => out.integer(frame))
}

/**
 * Writes one of a path's two paints.
 *
 * The scene's `PathPaint` is a two-armed tag — solid or gradient — inside an
 * option, and this writes both arms. `'none'` is the absent option rather than
 * a transparent colour, which would be a paint that draws nothing rather than
 * no paint at all.
 *
 * The arm is chosen by shape, since the surface type carries no tag: a gradient
 * is the object, and every string is a colour or `'none'`.
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
 * Writes a scene into an arena.
 *
 * The whole tree in one pass, and the only place this package produces the
 * format. Nothing crosses into native code until the arena is complete: the
 * factories build plain objects and this walks them, because JavaScript
 * evaluates arguments inside out and writing opcodes as each factory ran would
 * land them post-order where the arena is pre-order.
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
): Arena {
  const out = new ArenaWriter()
  out.fetched = fetched
  writeHeader(out, width, height, contentHeight, scale, surface, pages.length, attempts)
  for (const page of pages) writeNode(out, page)
  return out.finish()
}
