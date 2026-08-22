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
  BORDER_STYLE,
  BOX_SIZING,
  DIRECTION,
  DISPLAY,
  FILL_RULE,
  FLEX_DIRECTION,
  FLEX_WRAP,
  FONT_STYLE,
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
import type { TrackSize } from './index.js'
import type { ImageSource, PathPaint, PathProps, SceneNode, TextSegment } from './node.js'
import type { Color, Corners, Dimension, FontWeight, GridPlacement, Length, Sides, Spacing, Style } from './style.js'

/** A value the arena cannot carry itself, held beside it. */
export type SideValue = string | Uint8Array

/** A written scene: the slots, and the values they index. */
export interface Arena {
  /** Every slot, in the order the reader consumes them. */
  readonly slots: Float64Array
  /** The strings and buffers the slots index into. */
  readonly values: readonly SideValue[]
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
  finish(): Arena {
    return { slots: Float64Array.from(this.#slots), values: this.#values }
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

/** Writes the five slots every arena opens with, and the page count. */
function writeHeader(out: ArenaWriter, width: number, height: number, scale: number, pages: number): void {
  out.slot(MAGIC)
  out.slot(VERSION)
  out.f32(width)
  out.f32(height)
  out.f32(scale)
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
 * This map holds the exceptions, which is one. CSS spells `nowrap` as a single
 * word and the scene spells the concept `NoWrap`.
 */
const SPELLINGS: Readonly<Record<string, string>> = { nowrap: 'NoWrap' }

/** The Rust variant name a keyword means. */
function variantName(keyword: string): string {
  const exception = SPELLINGS[keyword]
  if (exception !== undefined) return exception
  return keyword
    .split('-')
    .map(part => part.charAt(0).toUpperCase() + part.slice(1))
    .join('')
}

/** The keyword a Rust variant name is written as, for an error message. */
function keywordFor(name: string): string {
  const spelt = Object.entries(SPELLINGS).find(([, variant]) => variant === name)
  if (spelt !== undefined) return spelt[0]
  return name.replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase()
}

/**
 * The number a keyword crosses as.
 *
 * Throws rather than defaults. A keyword the scene has no variant for is a
 * property this package accepts and cannot carry, and quietly writing the
 * zeroth variant would make it arrive as a different value.
 */
function variant(table: Readonly<Record<string, number>>, keyword: string, what: string): number {
  const found = table[variantName(keyword)]
  if (found === undefined) {
    const taken = Object.keys(table).map(keywordFor).join(', ')
    throw new TypeError(`${what} has no value ${JSON.stringify(keyword)}; it takes ${taken}`)
  }
  return found
}

/** How many hexadecimal digits each accepted `#` form has. */
const HEX_FORMS = new Set([3, 4, 6, 8])

/**
 * A colour packed as `r<<24 | g<<16 | b<<8 | a`.
 *
 * One slot rather than four is three fewer stores per colour on this side,
 * which is the side the format exists to make cheap.
 *
 * Hex and `'transparent'` only, for now. A CSS colour name would need the
 * hundred-and-fifty-entry table in this package rather than in Skia, because
 * the arena carries channels rather than a string — v1 could pass the name
 * through and this cannot. Refusing is better than approximating: a name that
 * silently became black would be a wrong picture rather than an error.
 */
function packColor(color: Color): number {
  if (color === 'transparent') return 0

  const digits = color.startsWith('#') ? color.slice(1) : ''
  if (!HEX_FORMS.has(digits.length) || !/^[0-9a-fA-F]+$/.test(digits)) {
    throw new TypeError(`${JSON.stringify(color)} is not a colour this package reads; write #rgb, #rgba, #rrggbb, #rrggbbaa or 'transparent'`)
  }

  const short = digits.length < 6
  const channel = (index: number): number => {
    const at = short ? index : index * 2
    const text = short ? `${digits[at]}${digits[at]}` : digits.slice(at, at + 2)
    return Number.parseInt(text, 16)
  }

  const alpha = digits.length === 4 || digits.length === 8 ? channel(3) : 0xff
  // `* 2 ** 24` rather than `<< 24`: a shift is a signed 32-bit operation, so a
  // red channel above 127 would make the packed value negative and the reader
  // would refuse it as out of range.
  return channel(0) * 2 ** 24 + channel(1) * 2 ** 16 + channel(2) * 2 ** 8 + alpha
}

/** The number a `'…%'` string names, or `undefined` if it is not one. */
function percentage(value: string): number | undefined {
  if (!value.endsWith('%')) return undefined
  const number = Number(value.slice(0, -1))
  return Number.isFinite(number) ? number : undefined
}

/** The number a string ending in `unit` names, or `undefined`. */
function suffixed(value: string, unit: string): number | undefined {
  if (!value.endsWith(unit)) return undefined
  const number = Number(value.slice(0, -unit.length))
  return Number.isFinite(number) ? number : undefined
}

/** Writes a length: a tag, then the value. */
function writeLength(out: ArenaWriter, value: Length): void {
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
function writeDimension(out: ArenaWriter, value: Dimension): void {
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
function writeTrack(out: ArenaWriter, value: TrackSize): void {
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
function writeSpacing(out: ArenaWriter, value: Spacing): void {
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
 * Writes the four edges, in `top right bottom left` order.
 *
 * An edge the caller did not name takes `fallback`, which is that property's
 * own default in the scene rather than a shared zero — `margin` defaults to
 * zero points and `inset` to nothing at all, and writing one where the other
 * belongs would change what the layout does.
 */
function writeSides<T>(value: Sides<T>, fallback: T, write: (value: T) => void): void {
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
function writeCorners(out: ArenaWriter, value: Corners): void {
  if (typeof value === 'number') {
    for (let corner = 0; corner < 4; corner += 1) out.f32(value)
    return
  }
  out.f32(value.topLeft ?? 0)
  out.f32(value.topRight ?? 0)
  out.f32(value.bottomRight ?? 0)
  out.f32(value.bottomLeft ?? 0)
}

/** The number a font weight names: the two keywords are the numbers CSS gives them. */
function packWeight(weight: FontWeight): number {
  if (weight === 'normal') return 400
  if (weight === 'bold') return 700
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
      writeSides(style.position as Sides<Length>, undefined as Length | undefined, edge => out.optional(edge, length => writeLength(out, length))),
  },
  {
    index: 3,
    rust: 'size',
    keys: ['width', 'height'],
    write: (out, style) => {
      writeDimension(out, style.width ?? 'auto')
      writeDimension(out, style.height ?? 'auto')
    },
  },
  {
    index: 4,
    rust: 'min_size',
    keys: ['minWidth', 'minHeight'],
    write: (out, style) => {
      writeDimension(out, style.minWidth ?? 'auto')
      writeDimension(out, style.minHeight ?? 'auto')
    },
  },
  {
    index: 5,
    rust: 'max_size',
    keys: ['maxWidth', 'maxHeight'],
    write: (out, style) => {
      writeDimension(out, style.maxWidth ?? 'auto')
      writeDimension(out, style.maxHeight ?? 'auto')
    },
  },
  { index: 6, rust: 'aspect_ratio', keys: ['aspectRatio'], write: (out, style) => out.optional(style.aspectRatio, ratio => out.f32(ratio)) },
  {
    index: 7,
    rust: 'margin',
    keys: ['margin'],
    write: (out, style) => writeSides(style.margin as Sides<Dimension>, 0 as Dimension, edge => writeDimension(out, edge)),
  },
  {
    index: 8,
    rust: 'padding',
    keys: ['padding'],
    write: (out, style) => writeSides(style.padding as Sides<Length>, 0 as Length, edge => writeLength(out, edge)),
  },
  { index: 9, rust: 'border', keys: ['border'], write: (out, style) => writeSides(style.border as Sides<number>, 0, edge => out.f32(edge)) },
  {
    index: 10,
    rust: 'flex_direction',
    keys: ['flexDirection'],
    write: (out, style) => out.enum(variant(FLEX_DIRECTION, style.flexDirection as string, 'flexDirection')),
  },
  { index: 11, rust: 'flex_wrap', keys: ['flexWrap'], write: (out, style) => out.enum(variant(FLEX_WRAP, style.flexWrap as string, 'flexWrap')) },
  { index: 12, rust: 'flex_grow', keys: ['flexGrow'], write: (out, style) => out.f32(style.flexGrow as number) },
  { index: 13, rust: 'flex_shrink', keys: ['flexShrink'], write: (out, style) => out.f32(style.flexShrink as number) },
  { index: 14, rust: 'flex_basis', keys: ['flexBasis'], write: (out, style) => writeDimension(out, style.flexBasis as Dimension) },
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
      if (typeof gap === 'object') {
        writeLength(out, gap.row ?? 0)
        writeLength(out, gap.column ?? 0)
        return
      }
      writeLength(out, gap)
      writeLength(out, gap)
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
    keys: ['gridTemplateColumns'],
    write: (out, style) => writeTracks(out, style.gridTemplateColumns as readonly TrackSize[]),
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
    write: (out, style) => out.optional(style.gridAutoRows, track => writeTrack(out, track)),
  },
  {
    index: 26,
    rust: 'grid_auto_columns',
    keys: ['gridAutoColumns'],
    write: (out, style) => out.optional(style.gridAutoColumns, track => writeTrack(out, track)),
  },
  {
    index: 27,
    rust: 'grid_auto_flow',
    keys: ['gridAutoFlow'],
    write: (out, style) => out.enum(variant(GRID_AUTO_FLOW, style.gridAutoFlow as string, 'gridAutoFlow')),
  },
  { index: 28, rust: 'grid_column', keys: ['gridColumn'], write: (out, style) => writePlacement(out, style.gridColumn) },
  { index: 29, rust: 'grid_row', keys: ['gridRow'], write: (out, style) => writePlacement(out, style.gridRow) },
]

/** Writes a track list: the count, then each track. */
function writeTracks(out: ArenaWriter, tracks: readonly TrackSize[]): void {
  out.count(tracks.length)
  for (const track of tracks) writeTrack(out, track)
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
  { index: 0, rust: 'background_color', keys: ['backgroundColor'], write: (out, style) => out.integer(packColor(style.backgroundColor as Color)) },
  {
    index: 3,
    rust: 'border_color',
    keys: ['borderColor'],
    present: style => perEdge(style.borderColor),
    write: (out, style) =>
      writeSides(style.borderColor as Sides<Color>, undefined as Color | undefined, edge => out.optional(edge, color => out.integer(packColor(color)))),
  },
  {
    index: 4,
    rust: 'border_color_all',
    keys: ['borderColor'],
    present: style => style.borderColor !== undefined && !perEdge(style.borderColor),
    write: (out, style) => out.integer(packColor(style.borderColor as Color)),
  },
  { index: 5, rust: 'border_style', keys: ['borderStyle'], write: (out, style) => out.enum(variant(BORDER_STYLE, style.borderStyle as string, 'borderStyle')) },
  { index: 6, rust: 'border_radius', keys: ['borderRadius'], write: (out, style) => writeCorners(out, style.borderRadius as Corners) },
  { index: 7, rust: 'opacity', keys: ['opacity'], write: (out, style) => out.f32(style.opacity as number) },
  { index: 8, rust: 'blend_mode', keys: ['mixBlendMode'], write: (out, style) => out.enum(variant(BLEND_MODE, style.mixBlendMode as string, 'mixBlendMode')) },
  { index: 9, rust: 'dither', keys: ['dither'], write: (out, style) => out.bool(style.dither as boolean) },
  { index: 10, rust: 'z_index', keys: ['zIndex'], write: (out, style) => out.integer(style.zIndex as number) },
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
  { index: 0, rust: 'font_family', keys: ['fontFamily'], write: (out, style) => out.optional(style.fontFamily, family => out.text(family)) },
  { index: 1, rust: 'font_size', keys: ['fontSize'], write: (out, style) => out.optional(style.fontSize, size => out.f32(size)) },
  { index: 2, rust: 'font_weight', keys: ['fontWeight'], write: (out, style) => out.optional(style.fontWeight, weight => out.integer(packWeight(weight))) },
  {
    index: 3,
    rust: 'font_style',
    keys: ['fontStyle'],
    write: (out, style) => out.optional(style.fontStyle, value => out.enum(variant(FONT_STYLE, value, 'fontStyle'))),
  },
  { index: 4, rust: 'color', keys: ['color'], write: (out, style) => out.optional(style.color, color => out.integer(packColor(color))) },
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
  { index: 9, rust: 'line_height', keys: ['lineHeight'], write: (out, style) => out.optional(style.lineHeight, height => out.f32(height)) },
  { index: 10, rust: 'line_gap', keys: ['lineGap'], write: (out, style) => out.optional(style.lineGap, gap => out.f32(gap)) },
  { index: 11, rust: 'letter_spacing', keys: ['letterSpacing'], write: (out, style) => out.optional(style.letterSpacing, value => writeSpacing(out, value)) },
  { index: 12, rust: 'word_spacing', keys: ['wordSpacing'], write: (out, style) => out.optional(style.wordSpacing, value => writeSpacing(out, value)) },
]

/** The effects group, in ascending index order. */
const EFFECTS_PROPERTIES: readonly Property[] = [
  { index: 4, rust: 'filter', keys: ['filter'], write: (out, style) => out.optional(style.filter, value => out.text(value)) },
  { index: 5, rust: 'backdrop_filter', keys: ['backdropFilter'], write: (out, style) => out.optional(style.backdropFilter, value => out.text(value)) },
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

/** Black, packed: the fill a path takes when nothing names one. */
const BLACK = 0xff

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
  if ('path' in src) {
    out.enum(0)
    out.text(src.path)
  } else if ('url' in src) {
    out.enum(1)
    out.text(src.url)
  } else {
    out.enum(2)
    out.bytes(src.bytes)
  }

  // `objectFit` and `frame` sit in the payload rather than in a style group,
  // because they are meaningless on anything but an image and the scene puts
  // them where the node kind is. The surface stays flat over that: a caller
  // should not have to know that `objectFit` is payload and `opacity` is a mask
  // bit. This is the seam, and it belongs here rather than in the surface.
  out.enum(variant(OBJECT_FIT, style?.objectFit ?? 'fill', 'objectFit'))
  const position = style?.objectPosition ?? CENTRED
  writeLength(out, position[0])
  writeLength(out, position[1])
  out.optional(style?.frame, frame => out.integer(frame))
}

/**
 * Writes one of a path's two paints.
 *
 * The scene's `PathPaint` is a two-armed tag — solid or gradient — inside an
 * option. This surface writes only the solid arm, because it has no spelling
 * for a gradient anywhere: `gradient` and `backgroundImage` are absent from its
 * style table for the same reason. `'none'` is the absent option rather than a
 * transparent colour, which would be a paint that draws nothing rather than no
 * paint at all.
 */
function writePathPaint(out: ArenaWriter, paint: PathPaint | undefined, fallback: number | undefined): void {
  if (paint === 'none' || (paint === undefined && fallback === undefined)) {
    out.absent()
    return
  }

  out.present()
  out.enum(0)
  out.integer(paint === undefined ? (fallback as number) : packColor(paint))
}

/** Writes the payload only a path node has. */
function writePathPayload(out: ArenaWriter, props: PathProps): void {
  out.text(props.d)

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
export function encodeScene(pages: readonly SceneNode[], width: number, height: number, scale: number): Arena {
  const out = new ArenaWriter()
  writeHeader(out, width, height, scale, pages.length)
  for (const page of pages) writeNode(out, page)
  return out.finish()
}
