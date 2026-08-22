import { createRequire } from 'node:module'

import { describe, expect, it } from 'vitest'

import cases from '../../../fixtures/arena-cases.json' with { type: 'json' }

import { ArenaWriter, PROPERTY_TABLES, encodeScene, variant, type SideValue } from './arena.js'
import { ENUMS, NODE_TAG } from './generated/arena-enums.js'
import { EFFECTS, LAYOUT, MAGIC, MASK_BITS, PAINT, TEXT, VERSION, type ArenaProperty } from './generated/arena-tables.js'
import { Box, Image, Path, RichText, Text, type SceneNode } from './node.js'
import type { PositionType } from './index.js'
import type { BackgroundImage, Gradient, GradientDirection, Style } from './style.js'

/**
 * The reader this file checks the writer against.
 *
 * Deliberately **not** built from the writer's own table. It walks the `type`
 * column of the generated tables — the Rust type, as `arena_group!` spells it —
 * and decodes from that. So a property whose writer emits the wrong number of
 * slots does not merely compare unequal: the cursor desynchronises and the
 * trailing-slot check at the end fails, which is the failure the format itself
 * would suffer.
 *
 * It lives in a test file because it is not part of the package. Nothing ships
 * that reads an arena; the addon does that, in Rust.
 */
interface Cursor {
  /** The slots being read. */
  readonly slots: Float64Array
  /** The strings and buffers the slots index. */
  readonly values: readonly SideValue[]
  /** How far in the cursor has got. */
  at: number
}

/** Reads one slot. */
function slot(input: Cursor): number {
  const value = input.slots[input.at]
  if (value === undefined) throw new RangeError(`the arena ends at slot ${input.at}`)
  input.at += 1
  return value
}

/** Reads one slot, narrowed the way the Rust reader narrows it. */
function f32(input: Cursor): number {
  return Math.fround(slot(input))
}

/** Reads a side value by the index one slot holds. */
function side(input: Cursor): SideValue {
  const index = slot(input)
  const value = input.values[index]
  if (value === undefined) throw new RangeError(`slot names side value ${index}, which is not there`)
  return value
}

/** A tagged value, as the case fixture writes one. */
interface Tagged {
  readonly tag: string
  readonly value?: number
}

/** Reads a tag and its value, naming the tag from `tags`. */
function tagged(input: Cursor, tags: readonly string[], what: string): Tagged {
  const tag = slot(input)
  const value = f32(input)
  const name = tags[tag]
  if (name === undefined) throw new RangeError(`${what} has no tag ${tag}`)
  // A variant with nothing to hold omits the value rather than writing null,
  // which is how the case fixture spells the same thing.
  return name === 'auto' || name === 'normal' ? { tag: name } : { tag: name, value }
}

/** Reads a colour, unpacked into the channels the fixture names. */
function color(input: Cursor): Record<string, number> {
  const packed = slot(input)
  return {
    r: Math.floor(packed / 2 ** 24) & 0xff,
    g: Math.floor(packed / 2 ** 16) & 0xff,
    b: Math.floor(packed / 2 ** 8) & 0xff,
    a: packed & 0xff,
  }
}

/** The leaf types the tables use, and how each reads. */
const LEAVES: Readonly<Record<string, (input: Cursor) => unknown>> = {
  f32,
  bool: input => slot(input) === 1,
  u16: slot,
  u32: slot,
  i16: slot,
  i32: slot,
  String: input => side(input),
  FontWeight: slot,
  Color: color,
  Length: input => tagged(input, ['points', 'percent'], 'Length'),
  Dimension: input => tagged(input, ['auto', 'points', 'percent'], 'Dimension'),
  TrackSize: input => tagged(input, ['auto', 'points', 'percent', 'fraction'], 'TrackSize'),
  Spacing: input => tagged(input, ['normal', 'points', 'em'], 'Spacing'),
  GridPlacement: input => ({ start: read(input, 'Option<i16>'), span: read(input, 'Option<u16>') }),
  GradientStop: input => ({ offset: f32(input), color: color(input) }),
  BackgroundSize: input => {
    const tag = ['per-axis', 'cover', 'contain'][slot(input)]
    if (tag === 'per-axis') return { tag, value: [read(input, 'Dimension'), read(input, 'Dimension')] }
    if (tag === undefined) throw new RangeError('BackgroundSize has no such tag')
    return { tag }
  },
  BackgroundImage: input => ({
    source: (() => {
      const tag = ['path', 'url', 'bytes'][slot(input)]
      if (tag === undefined) throw new RangeError('ImageSource has no such tag')
      return { tag, value: sourceValue(side(input)) }
    })(),
    repeat: read(input, 'BackgroundRepeat'),
    size: read(input, 'BackgroundSize'),
    position: [read(input, 'Length'), read(input, 'Length')],
  }),
  Gradient: input => ({ geometry: read(input, 'GradientGeometry'), stops: read(input, 'Vec<GradientStop>') }),
  GradientGeometry: input => {
    // The tag is `GradientKind`, a fieldless wire enum beside the data enum —
    // the same split `NodeTag` has, and forced by the same thing: `from_wire`
    // turns a byte back into a value and cannot invent a payload.
    const kind = read(input, 'GradientKind') as string
    if (kind === 'Linear') return { kind, direction: read(input, 'LinearDirection') }
    const at = [read(input, 'Length'), read(input, 'Length')]
    if (kind === 'Radial') return { kind, at }
    return { kind, at, from: f32(input) }
  },
  LinearDirection: input => {
    const tag = slot(input)
    if (tag === 0) return { tag: 'angle', value: f32(input) }
    if (tag !== 1) throw new RangeError(`LinearDirection has no tag ${tag}`)
    return {
      tag: 'between',
      start: [read(input, 'Length'), read(input, 'Length')],
      end: [read(input, 'Length'), read(input, 'Length')],
    }
  },
  Transform: input => ({
    translate_x: read(input, 'Length'),
    translate_y: read(input, 'Length'),
    rotate_degrees: f32(input),
    scale_x: f32(input),
    scale_y: f32(input),
    origin: [read(input, 'Length'), read(input, 'Length')],
  }),
  BoxShadow: input => ({
    inset: slot(input) === 1,
    offset_x: f32(input),
    offset_y: f32(input),
    blur: f32(input),
    spread: f32(input),
    color: color(input),
  }),
  TextShadow: input => ({
    offset_x: f32(input),
    offset_y: f32(input),
    blur: f32(input),
    color: color(input),
  }),
  TextStroke: input => ({ width: f32(input), color: color(input) }),
  Mask: input => {
    const tag = ['image', 'shape', 'path', 'gradient'][slot(input)]
    if (tag === 'shape') return { tag, value: read(input, 'MaskShape') }
    if (tag === 'path') return { tag, data: side(input), fillRule: read(input, 'FillRule') }
    if (tag === 'image') return { tag, value: sourceValue(side(input)) }
    if (tag === 'gradient') return { tag, value: read(input, 'Gradient') }
    throw new RangeError('Mask has no such tag')
  },
  PathPaint: input => {
    const tag = ['solid', 'gradient'][slot(input)]
    if (tag === 'solid') return { tag, value: color(input) }
    if (tag === 'gradient') return { tag, value: read(input, 'Gradient') }
    throw new RangeError('PathPaint has no such tag')
  },
}

/** The last segment of a Rust path, which is the type's own name. */
function leafName(type: string): string {
  const at = type.lastIndexOf('::')
  return at === -1 ? type : type.slice(at + 2)
}

/**
 * Reads one value of the type `type` names.
 *
 * A tiny recursive descent over the type expression rather than a table of
 * whole types: `Sides<Option<Length>>` is three rules meeting, and writing the
 * combinations out would be the list this reads instead.
 */
function read(input: Cursor, type: string): unknown {
  const cleaned = type.replaceAll(/\s+/g, '')

  if (cleaned.startsWith('(') && cleaned.endsWith(')')) {
    const [first, second] = split(cleaned.slice(1, -1))
    return [read(input, first), read(input, second)]
  }

  const generic = /^(?:[\w:]*::)?(Option|Vec|Sides|Corners)<(.*)>$/.exec(cleaned)
  if (generic) {
    const [, container, inner] = generic
    if (container === 'Option') return slot(input) === 0 ? null : read(input, inner as string)
    if (container === 'Vec') {
      const count = slot(input)
      return Array.from({ length: count }, () => read(input, inner as string))
    }
    // `Sides` is top right bottom left and `Corners` is top-left top-right
    // bottom-right bottom-left; both are four of the same thing, and the
    // fixture writes both as an array in that order.
    return Array.from({ length: 4 }, () => read(input, inner as string))
  }

  const name = leafName(cleaned)
  const leaf = LEAVES[name]
  if (leaf !== undefined) return leaf(input)

  const declared = ENUMS[name]
  if (declared === undefined) throw new TypeError(`this reader does not know the type ${type}`)

  const discriminant = slot(input)
  const found = Object.entries(declared.variants).find(([, value]) => value === discriminant)
  if (found === undefined) throw new RangeError(`${name} has no variant ${discriminant}`)
  return found[0]
}

/** Splits a two-element tuple body at its top-level comma. */
function split(body: string): [string, string] {
  let depth = 0
  for (let index = 0; index < body.length; index += 1) {
    const character = body[index]
    if (character === '<' || character === '(') depth += 1
    else if (character === '>' || character === ')') depth -= 1
    else if (character === ',' && depth === 0) return [body.slice(0, index), body.slice(index + 1)]
  }
  throw new TypeError(`${body} is not a pair`)
}

/** Reads one group's mask, and then the properties it names. */
function readGroup(input: Cursor, table: readonly ArenaProperty[], mask: readonly number[]): Record<string, unknown> {
  const carried: Record<string, unknown> = {}
  for (const property of table) {
    const slotOf = mask[Math.floor(property.index / MASK_BITS)] ?? 0
    if (Math.floor(slotOf / 2 ** (property.index % MASK_BITS)) % 2 === 0) continue
    carried[property.name] = read(input, property.type)
  }
  return carried
}

/** How many mask slots a table of this many properties uses. */
function slotsFor(properties: number): number {
  return Math.ceil(properties / MASK_BITS)
}

/** The four groups, in the order a node writes them. */
const GROUPS = [
  { key: 'layout', table: LAYOUT },
  { key: 'paint', table: PAINT },
  { key: 'text', table: TEXT },
  { key: 'effects', table: EFFECTS },
] as const

/** One node, as this reader sees it. */
interface DecodedNode {
  /** Which kind the tag named. */
  readonly kind: string
  /** Each group's carried properties, by the scene's field name. */
  readonly groups: Record<string, Record<string, unknown>>
  /** The payload only this kind has. */
  readonly payload: unknown
  /** Its name, or `null`. */
  readonly name: string | null
  /** Its subtree. */
  readonly children: readonly DecodedNode[]
}

/** Reads one node and its subtree. */
function readNode(input: Cursor): DecodedNode {
  const kind = read(input, 'NodeTag') as string

  const masks = GROUPS.map(group => Array.from({ length: slotsFor(group.table.length) }, () => slot(input)))
  const groups: Record<string, Record<string, unknown>> = {}
  GROUPS.forEach((group, at) => {
    groups[group.key] = readGroup(input, group.table, masks[at] ?? [])
  })

  const payload = readPayload(input, kind)
  const name = read(input, 'Option<String>') as string | null

  const count = slot(input)
  const children = Array.from({ length: count }, () => readNode(input))
  return { kind, groups, payload, name, children }
}

/** A side value in the shape the case fixture writes it. */
function sourceValue(value: SideValue): string | number[] {
  return typeof value === 'string' ? value : [...value]
}

/** Reads the payload the node's kind carries. */
function readPayload(input: Cursor, kind: string): unknown {
  // The shape is the case fixture's, field for field, so a decoded payload can
  // be compared against what Rust wrote without an adapter in between. An
  // adapter here would be a third description of the format, written by the
  // same hand as the other two.
  if (kind === 'Box') return {}

  if (kind === 'Text') {
    const paragraph = {
      max_lines: read(input, 'Option<u32>'),
      ellipsis: read(input, 'Option<String>'),
    }
    // The discriminant: markup present means the renderer parses the string,
    // absent means the runs follow and it leaves them alone. Reading the count
    // unconditionally here would desynchronise the whole rest of the stream,
    // which is why the writer and this reader move together.
    const markup = read(input, 'Option<String>')
    if (markup !== null) return { paragraph, markup }

    const count = slot(input)
    const segments = Array.from({ length: count }, () => {
      const text = side(input)
      const mask = Array.from({ length: slotsFor(TEXT.length) }, () => slot(input))
      return { text, style: readGroup(input, TEXT, mask) }
    })
    return { paragraph, segments }
  }

  if (kind === 'Image') {
    // A source is a tag and then a side value, not a tag and a number: the
    // bytes of an image cannot live in a `Float64Array` any more than a string
    // can.
    const tags = ['path', 'url', 'bytes']
    const tag = tags[slot(input)]
    if (tag === undefined) throw new RangeError('ImageSource has no such tag')
    return {
      // Bytes come back as a plain array, which is how the case fixture writes
      // a buffer. A `Uint8Array` here would compare unequal to Rust's own
      // answer for a difference that is about JavaScript rather than the format.
      source: { tag, value: sourceValue(side(input)) },
      fit: read(input, 'ObjectFit'),
      position: [read(input, 'Length'), read(input, 'Length')],
      frame: read(input, 'Option<u32>'),
    }
  }

  return {
    data: side(input),
    fill: read(input, 'Option<PathPaint>'),
    stroke: read(input, 'Option<PathPaint>'),
    line_width: f32(input),
    fill_rule: read(input, 'FillRule'),
    line_cap: read(input, 'LineCap'),
    line_join: read(input, 'LineJoin'),
    line_dash: read(input, 'Vec<f32>'),
    line_dash_offset: f32(input),
  }
}

/** A whole arena, decoded. */
interface DecodedArena {
  /** The page size and the scale it was written at. */
  readonly size: readonly [number, number]
  /** The device pixel ratio. */
  readonly scale: number
  /** What the scene asked of the surface, where it asked anything. */
  readonly surface: { gpu: unknown; colorType: unknown; colorSpace: unknown }
  /** The pages. */
  readonly pages: readonly DecodedNode[]
}

/** Decodes an arena, refusing one that does not end where it should. */
function decode(slots: Float64Array, values: readonly SideValue[]): DecodedArena {
  const input: Cursor = { slots, values, at: 0 }

  expect(slot(input)).toBe(MAGIC)
  expect(slot(input)).toBe(VERSION)

  const size: [number, number] = [f32(input), f32(input)]
  const scale = f32(input)

  // The surface block, between the geometry and the pages. Three optionals,
  // each absent unless the scene stated one — which is why the arena's version
  // moved to 2: a reader of the old revision takes `gpu`'s presence flag for
  // the page count.
  const surface = {
    gpu: read(input, 'Option<bool>'),
    colorType: read(input, 'Option<ColorType>'),
    colorSpace: read(input, 'Option<ColorSpace>'),
  }

  const count = slot(input)
  const pages = Array.from({ length: count }, () => readNode(input))

  // The check that turns a writer emitting the wrong number of slots from a
  // comparison failure into a structural one.
  expect(input.at, 'the arena has slots past the end of the scene').toBe(slots.length)
  return { size, scale, surface, pages }
}

/** One case of the fixture: where the property sits, and what Rust wrote for it. */
interface Case {
  /** The group it belongs to. */
  readonly group: string
  /** Its bit in that group's mask. */
  readonly index: number
  /** The value Rust set, in the fixture's own JSON shape. */
  readonly value: unknown
  /** What the byte codec wrote for a scene with that property set. */
  readonly bytes: string
}

/** The cases, by the scene's field name. */
const CASES = cases.cases as unknown as Readonly<Record<string, Case>>

/** The page size every case was written at. */
const SIZE = cases.$size as [number, number]

/** The scale every case was written at. */
const SCALE = cases.$scale as number

/**
 * One probe per property this surface spells, keyed by the scene's field name.
 *
 * Hand-written, and deliberately not derived from the fixture's `value`.
 * Deriving them would mean writing the Rust-to-TypeScript adapter here and then
 * checking the encoder's adapter against it — one adapter checking another
 * adapter from the same hand. These are what a caller would write to mean what
 * the fixture says Rust wrote, and the comparison is against Rust's own answer.
 *
 * Every value is the fixture's probe, which is chosen to differ from the
 * property's default. A probe equal to the default proves nothing: the property
 * would compare equal whether it was written or dropped.
 *
 * **Every fractional value is a quarter, and that is load-bearing rather than
 * arbitrary.** These were all `1` — `'100%'` for the percentages — until a
 * hundredfold units bug shipped: `'50%'` covered a whole two-hundred-pixel
 * canvas, and every check here passed. `Length::Percent` is a fraction where
 * `1.0` is 100%, so `'1%'` written without the division is `Percent(1.0)`
 * too — the one value where forgetting to divide encodes identically to
 * remembering. `0` is the other, being a fixed point of any scaling. A quarter
 * is neither, and is exact in an `f32` so no expectation here carries a
 * rounding argument.
 *
 * Raising one back to `1` reopens the blind spot for that property alone, and
 * nothing in this file would fail.
 *
 * The general form, which is why the quarter is worth the churn it cost: **a
 * fixture with one value per type checks the shape of a read and not its
 * kind.** Moving off `1` immediately surfaced a latent one — `Option`'s
 * presence flag was read through the raw slot path rather than the integer
 * one, which is the same read at `1` and a different read at `0.25`. No
 * reviewer would have caught it, because the read matched its type in every
 * case the suite contained; only a value the suite did not contain separates
 * them.
 */
const PROBES: Readonly<Record<string, Style>> = {
  align_content: { alignContent: 'flex-end' },
  align_items: { alignItems: 'flex-end' },
  align_self: { alignSelf: 'flex-end' },
  aspect_ratio: { aspectRatio: 0.25 },
  backdrop_filter: { backdropFilter: 'probe' },
  background_color: { backgroundColor: '#00000001' },
  blend_mode: { mixBlendMode: 'multiply' },
  border: { border: 0.25 },
  border_color: { borderColor: { top: '#00000001', right: '#00000001', bottom: '#00000001', left: '#00000001' } },
  border_color_all: { borderColor: '#00000001' },
  border_radius: { borderRadius: 0.25 },
  border_style: { borderStyle: 'dashed' },
  box_sizing: { boxSizing: 'content-box' },
  color: { color: '#00000001' },
  direction: { direction: 'rtl' },
  display: { display: 'grid' },
  dither: { dither: true },
  filter: { filter: 'probe' },
  flex_basis: { flexBasis: 0.25 },
  flex_direction: { flexDirection: 'row-reverse' },
  flex_grow: { flexGrow: 0.25 },
  flex_shrink: { flexShrink: 0.25 },
  flex_wrap: { flexWrap: 'wrap' },
  font_family: { fontFamily: 'probe' },
  font_size: { fontSize: 0.25 },
  font_style: { fontStyle: 'italic' },
  font_weight: { fontWeight: 1 },
  gap: { gap: '25%' },
  grid_auto_columns: { gridAutoColumns: 0.25 },
  grid_auto_flow: { gridAutoFlow: 'column' },
  grid_auto_rows: { gridAutoRows: 0.25 },
  grid_column: { gridColumn: { start: 1, span: 1 } },
  grid_row: { gridRow: { start: 1, span: 1 } },
  grid_template_columns: { gridTemplateColumns: [0.25] },
  grid_template_rows: { gridTemplateRows: [0.25] },
  inset: { position: '25%' },
  justify_content: { justifyContent: 'flex-end' },
  letter_spacing: { letterSpacing: 0.25 },
  line_gap: { lineGap: 0.25 },
  line_height: { lineHeight: 0.25 },
  margin: { margin: 0.25 },
  max_size: { maxWidth: 0.25, maxHeight: 0.25 },
  min_size: { minWidth: 0.25, minHeight: 0.25 },
  opacity: { opacity: 0.25 },
  overflow: { overflow: 'hidden' },
  padding: { padding: '25%' },
  paint_order: { paintOrder: 'stroke' },
  position_type: { positionType: 'absolute' },
  size: { width: 0.25, height: 0.25 },
  text_align: { textAlign: 'end' },
  text_decoration: { textDecoration: 'underline' },
  vertical_align: { verticalAlign: 'middle' },
  word_spacing: { wordSpacing: 0.25 },
  z_index: { zIndex: 1 },
  // `'100%'`, not the `'25%'` the kind cases use: this case's percentages come
  // from `PROBE_FILL`, which is `1.0` for every property, so the probe has to
  // match it. That is the blind spot #22 closes — a hundredfold units error is
  // invisible at exactly this value — and the probe cannot step out of it
  // alone, because the bytes it is compared against are written from the fill.
  gradient: {
    gradient: { type: 'radial', at: { x: '25%', y: '25%' }, stops: [{ offset: 0.25, color: '#00000001' }] },
  },
  background_image: {
    backgroundImage: {
      src: { url: 'probe' },
      repeat: 'repeat-x',
      size: 'cover',
      position: { x: '25%', y: '25%' },
    },
  },
  transform: {
    transform: { translateX: '25%', translateY: '25%', rotate: 0.25, scaleX: 0.25, scaleY: 0.25, originX: '25%', originY: '25%' },
  },
  box_shadows: { boxShadow: { inset: true, offsetX: 0.25, offsetY: 0.25, blur: 0.25, spread: 0.25, color: '#00000001' } },
  text_shadows: { textShadow: { offsetX: 0.25, offsetY: 0.25, blur: 0.25, color: '#00000001' } },
  mask: { mask: { shape: 'ellipse' } },
  text_stroke: { textStroke: { width: 0.25, color: '#00000001' } },
}

/**
 * The properties the scene carries and this surface does not spell yet, and why.
 *
 * Written down rather than left out. A property with neither a table entry here
 * nor a line in this list fails the partition below, so adding one to an
 * `arena_group!` upstream forces a decision about its TypeScript spelling
 * instead of leaving it silently absent from every scene this package writes.
 */
const UNSPELT: Readonly<Record<string, string>> = {
  font_variant: 'the thirty-five OpenType features need a spelling of their own',
}

describe('the property tables', () => {
  it('agree with the generated ones', () => {
    // The indices are the whole format: a writer reading a stale index writes
    // the right number of slots into the wrong field, and no length check
    // catches that. So they are checked against the table the Rust emits
    // rather than trusted to have been copied correctly.
    for (const { key, table } of GROUPS) {
      for (const property of PROPERTY_TABLES[key] ?? []) {
        const generated = table.find(entry => entry.index === property.index)
        expect(generated, `${key} has no property at index ${property.index}`).toBeDefined()
        expect(generated?.name).toBe(property.rust)
      }
    }
  })

  it('partition every property the scene carries', () => {
    for (const { key, table } of GROUPS) {
      const spelt = new Set((PROPERTY_TABLES[key] ?? []).map(property => property.rust))
      for (const property of table) {
        const written = spelt.has(property.name)
        const named = UNSPELT[property.name] !== undefined
        expect(written || named, `${key}.${property.name} is neither written nor named as unspelt`).toBe(true)
        expect(written && named, `${key}.${property.name} is both written and named as unspelt`).toBe(false)
      }
    }
  })

  it('leave nothing named as unspelt that the scene no longer carries', () => {
    const carried = new Set(GROUPS.flatMap(group => group.table.map(property => property.name)))
    for (const name of Object.keys(UNSPELT)) {
      expect(carried.has(name), `${name} is named as unspelt and is not a property any more`).toBe(true)
    }
  })

  it('have a probe for every property they write', () => {
    const written = GROUPS.flatMap(group => (PROPERTY_TABLES[group.key] ?? []).map(property => property.rust)).sort()
    expect(Object.keys(PROBES).sort()).toEqual(written)
  })

  it('cover exactly the properties the case fixture does', () => {
    const carried = GROUPS.flatMap(group => group.table.map(property => property.name)).sort()
    const fixture = Object.keys(CASES)
      .filter(name => !name.startsWith('__'))
      .sort()
    expect(fixture).toEqual(carried)
  })
})

/** Encodes one box carrying `style`, and reads it back. */
function roundTrip(style: Style): DecodedNode {
  const arena = encodeScene([Box(style)], SIZE[0], SIZE[1], SCALE)
  const decoded = decode(arena.slots, arena.values)

  expect(decoded.size).toEqual(SIZE)
  expect(decoded.scale).toBe(SCALE)
  expect(decoded.pages).toHaveLength(1)

  const page = decoded.pages[0]
  if (page === undefined) throw new Error('the arena decoded to no page')
  return page
}

describe('a property crosses as itself', () => {
  // One test per property rather than one asserting all of them. A single
  // failure then names the property, which is the search that would otherwise
  // be performed by hand at the moment the suite goes red.
  for (const [rust, probe] of Object.entries(PROBES)) {
    it(`carries ${rust}`, () => {
      const expected = CASES[rust]
      if (expected === undefined) throw new Error(`the fixture has no case for ${rust}`)

      const page = roundTrip(probe)
      const carried = page.groups[expected.group] ?? {}

      // Exactly this property, and nothing else. A writer that set a
      // neighbouring bit as well would still compare equal on the value.
      expect(Object.keys(carried)).toEqual([rust])
      expect(carried[rust]).toEqual(expected.value)

      for (const group of GROUPS) {
        if (group.key === expected.group) continue
        expect(page.groups[group.key], `${rust} wrote into ${group.key}`).toEqual({})
      }
    })
  }
})

describe('the arena', () => {
  it('carries nothing for a node that sets nothing', () => {
    const page = roundTrip({})

    for (const group of GROUPS) expect(page.groups[group.key]).toEqual({})
    expect(page.kind).toBe('Box')
    expect(page.name).toBeNull()
    expect(page.children).toEqual([])
  })

  it('writes children in the order they were given', () => {
    const arena = encodeScene([Box({ name: 'root', children: [Box({ name: 'first' }), Box({ name: 'second' })] })], SIZE[0], SIZE[1], SCALE)
    const page = decode(arena.slots, arena.values).pages[0]

    expect(page?.name).toBe('root')
    expect(page?.children.map(child => child.name)).toEqual(['first', 'second'])
  })

  it('writes one side value for a string used twice', () => {
    // A font family repeats on every text node of a document, and each
    // duplicate is one more value the addon reads out of V8 -- which is the
    // cost this format exists to avoid.
    const arena = encodeScene([Box({ children: [Box({ fontFamily: 'Inter' }), Box({ fontFamily: 'Inter' })] })], SIZE[0], SIZE[1], SCALE)

    expect(arena.values).toEqual(['Inter'])
  })
})

describe('a value the format cannot carry', () => {
  it('is refused rather than approximated', () => {
    expect(() => roundTrip({ backgroundColor: 'rebeccapurple' })).toThrow(/not a colour this package reads/)
    expect(() => roundTrip({ padding: 'thin' as unknown as number })).toThrow(/not a length/)
    expect(() => roundTrip({ width: 'wide' as unknown as number })).toThrow(/not a size/)
    expect(() => roundTrip({ gridTemplateColumns: ['1x' as unknown as number] })).toThrow(/not a track size/)
    expect(() => roundTrip({ letterSpacing: '1rem' as unknown as number })).toThrow(/not a spacing/)
  })

  it('names what it does take', () => {
    // The keyword check is what caught this package offering `'oblique'` and
    // `'baseline'`, neither of which the scene or v1 has. A lookup that fell
    // back to the zeroth variant would have written `normal` and `top`.
    expect(() => roundTrip({ display: 'inline' as unknown as 'flex' })).toThrow(/display has no value "inline"; it takes flex, grid, block, none/)
  })
})

describe('a mask slot', () => {
  it('refuses a value wider than the 53 bits a double holds', () => {
    // The 54th bit of a mask packed into one slot is lost with no rounding a
    // reader could detect, so the writer refuses rather than truncates.
    const out = new ArenaWriter()
    const at = out.reserveMask(1)

    expect(() => out.patchMask(at, [2 ** MASK_BITS])).toThrow(/holds 53 bits/)
  })
})

/** Encodes one page and reads it back, without asserting the header. */
function page(node: SceneNode): DecodedNode {
  const arena = encodeScene([node], SIZE[0], SIZE[1], SCALE)
  const decoded = decode(arena.slots, arena.values).pages[0]
  if (decoded === undefined) throw new Error('the arena decoded to no page')
  return decoded
}

describe('a text node', () => {
  it('carries a plain string as markup, for the renderer to parse', () => {
    // Not as a segment. `Text` and `RichText` of one run would otherwise write
    // identical bytes, and the decoder would have to guess which to parse —
    // losing either `RichText`'s literal `<` or every caller's rich text.
    const decoded = page(Text('Ukasyah'))

    expect(decoded.kind).toBe('Text')
    expect(decoded.payload).toEqual({
      paragraph: { max_lines: null, ellipsis: null },
      markup: 'Ukasyah',
    })
  })

  it('carries paragraph properties, which are not style', () => {
    const decoded = page(Text('Ukasyah', { maxLines: 2, ellipsis: '...' }))

    expect(decoded.payload).toEqual({
      paragraph: { max_lines: 2, ellipsis: '...' },
      markup: 'Ukasyah',
    })
  })

  it('writes no markup slot content for runs the caller built', () => {
    // The absent discriminant is what tells the decoder the count follows. A
    // writer that skipped it would leave every later slot read one out.
    const decoded = page(RichText([{ text: 'a <b> b', style: undefined }]))

    expect(decoded.payload).toEqual({
      paragraph: { max_lines: null, ellipsis: null },
      segments: [{ text: 'a <b> b', style: {} }],
    })
  })

  it('carries a style per run, overriding the node’s', () => {
    const decoded = page(
      RichText(
        [
          { text: 'plain ', style: undefined },
          { text: 'bold', style: { fontWeight: 'bold' } },
        ],
        { fontSize: 1 },
      ),
    )

    expect(decoded.groups.text).toEqual({ font_size: 1 })
    expect(decoded.payload).toEqual({
      paragraph: { max_lines: null, ellipsis: null },
      segments: [
        { text: 'plain ', style: {} },
        { text: 'bold', style: { font_weight: 700 } },
      ],
    })
  })

  it('writes the two keywords as the numbers CSS gives them', () => {
    expect(page(Text('x', { fontWeight: 'normal' })).groups.text).toEqual({ font_weight: 400 })
    expect(page(Text('x', { fontWeight: 1 })).groups.text).toEqual({ font_weight: 1 })
  })
})

describe('an image node', () => {
  it('carries a local path', () => {
    expect(page(Image({ src: 'avatar.png' })).payload).toEqual({
      source: { tag: 'path', value: 'avatar.png' },
      fit: 'Fill',
      position: [
        { tag: 'percent', value: 0.5 },
        { tag: 'percent', value: 0.5 },
      ],
      frame: null,
    })
  })

  it('carries a URL, a fit and a frame', () => {
    const decoded = page(Image({ src: { url: 'https://example.invalid/a.png' }, objectFit: 'cover', frame: 3 }))

    expect(decoded.payload).toEqual({
      source: { tag: 'url', value: 'https://example.invalid/a.png' },
      fit: 'Cover',
      position: [
        { tag: 'percent', value: 0.5 },
        { tag: 'percent', value: 0.5 },
      ],
      frame: 3,
    })
  })

  it('carries bytes through the side values rather than the slots', () => {
    const bytes = new Uint8Array([1, 2, 3])
    const arena = encodeScene([Image({ src: { bytes } })], SIZE[0], SIZE[1], SCALE)

    expect(arena.values).toEqual([bytes])
    expect(decode(arena.slots, arena.values).pages[0]?.payload).toMatchObject({ source: { tag: 'bytes', value: [1, 2, 3] } })
  })

  it('keeps `objectFit` and `frame` out of the style groups', () => {
    // They sit in the payload because they are meaningless on anything but an
    // image. The surface stays flat over that seam; this is where it is.
    const decoded = page(Image({ src: 'a.png', objectFit: 'cover', frame: 3, opacity: 2 }))

    expect(decoded.groups.paint).toEqual({ opacity: 2 })
    expect(decoded.groups.layout).toEqual({})
  })
})

describe('a path node', () => {
  it('is filled black and not stroked when nothing says otherwise', () => {
    // The same defaults the Rust surface writes, because the two surfaces
    // produce one scene and a picture should not depend on which built it.
    expect(page(Path({ d: 'M2 8 L6 12 L14 3' })).payload).toEqual({
      data: 'M2 8 L6 12 L14 3',
      fill: { tag: 'solid', value: { r: 0, g: 0, b: 0, a: 255 } },
      stroke: null,
      line_width: 1,
      fill_rule: 'NonZero',
      line_cap: 'Butt',
      line_join: 'Miter',
      line_dash: [],
      line_dash_offset: 0,
    })
  })

  it('carries every part of its paint when the caller names one', () => {
    expect(
      page(
        Path({
          d: 'M0 0 L4 4',
          fill: '#01020304',
          stroke: '#05060708',
          lineWidth: 2.5,
          fillRule: 'evenodd',
          lineCap: 'round',
          lineJoin: 'bevel',
          lineDash: [1, 2],
          lineDashOffset: 0.5,
        }),
      ).payload,
    ).toEqual({
      data: 'M0 0 L4 4',
      fill: { tag: 'solid', value: { r: 1, g: 2, b: 3, a: 4 } },
      stroke: { tag: 'solid', value: { r: 5, g: 6, b: 7, a: 8 } },
      line_width: 2.5,
      fill_rule: 'EvenOdd',
      line_cap: 'Round',
      line_join: 'Bevel',
      line_dash: [1, 2],
      line_dash_offset: 0.5,
    })
  })

  it("reads 'none' as no paint at all, not as a transparent one", () => {
    // A transparent paint is a paint that draws nothing; absent is no paint.
    // The scene distinguishes them and the encoder has to as well.
    const payload = page(Path({ d: 'M0 0', fill: 'none', stroke: 'none' })).payload as Record<string, unknown>
    expect(payload.fill).toBeNull()
    expect(payload.stroke).toBeNull()

    const transparent = page(Path({ d: 'M0 0', fill: 'transparent' })).payload as Record<string, unknown>
    expect(transparent.fill).toEqual({ tag: 'solid', value: { r: 0, g: 0, b: 0, a: 0 } })
  })
})

describe('the effects', () => {
  it('take one shadow or many', () => {
    // v1 takes `BoxShadowProps | BoxShadowProps[]`, so a caller writing one
    // does not wrap it. The scene holds a list either way.
    const one = page(Box({ boxShadow: { offsetY: 2 } })).groups.effects
    const two = page(Box({ boxShadow: [{ offsetY: 2 }, { offsetY: 4 }] })).groups.effects

    expect((one?.box_shadows as unknown[]).length).toBe(1)
    expect((two?.box_shadows as unknown[]).length).toBe(2)
  })

  it('fill a shadow in with the scene’s defaults, not with zeroes', () => {
    // A shadow that names only an offset is black, unblurred, unspread and not
    // inset — the same values the scene's `Default` gives, stated here because
    // the wire shape is fixed and every field is written whatever was said.
    expect(page(Box({ boxShadow: { offsetY: 2 } })).groups.effects).toEqual({
      box_shadows: [{ inset: false, offset_x: 0, offset_y: 2, blur: 0, spread: 0, color: { r: 0, g: 0, b: 0, a: 255 } }],
    })
  })

  it('give a transform the centre of the box to turn about', () => {
    // CSS's `transform-origin` default, and the scene's. A transform naming
    // only a rotation still writes six values, so the defaults have to be the
    // scene's rather than zeroes — a `scale` of zero is not no scale.
    expect(page(Box({ transform: { rotate: 90 } })).groups.effects).toEqual({
      transform: {
        translate_x: { tag: 'points', value: 0 },
        translate_y: { tag: 'points', value: 0 },
        rotate_degrees: 90,
        scale_x: 1,
        scale_y: 1,
        origin: [
          { tag: 'percent', value: 0.5 },
          { tag: 'percent', value: 0.5 },
        ],
      },
    })
  })

  it('let a per-axis scale win over the one that sets both', () => {
    const both = page(Box({ transform: { scale: 2 } })).groups.effects
    const mixed = page(Box({ transform: { scale: 2, scaleY: 3 } })).groups.effects

    expect(both?.transform).toMatchObject({ scale_x: 2, scale_y: 2 })
    expect(mixed?.transform).toMatchObject({ scale_x: 2, scale_y: 3 })
  })

  it('read a bare string mask as path data', () => {
    // v1's shorthand for `{ path }`, and the fill rule CSS starts from.
    expect(page(Box({ mask: 'M0 0 L4 4' })).groups.effects).toEqual({
      mask: { tag: 'path', data: 'M0 0 L4 4', fillRule: 'NonZero' },
    })
    expect(page(Box({ mask: { path: 'M0 0', fillRule: 'evenodd' } })).groups.effects).toEqual({
      mask: { tag: 'path', data: 'M0 0', fillRule: 'EvenOdd' },
    })
  })
})

describe('a gradient', () => {
  const of = (gradient: Gradient): unknown => page(Box({ gradient })).groups.paint?.gradient

  it('resolves a named direction to the angle it means', () => {
    // Eight names, clockwise from twelve. The scene holds an angle or two
    // points, and a keyword is neither — it is a direction whose angle is known
    // before the box is.
    const angle = (direction: GradientDirection): unknown =>
      (of({ type: 'linear', direction, colors: ['#000000ff'] }) as { geometry: { direction: unknown } }).geometry.direction

    expect(angle('to-top')).toEqual({ tag: 'angle', value: 0 })
    expect(angle('to-right')).toEqual({ tag: 'angle', value: 90 })
    expect(angle('to-bottom-left')).toEqual({ tag: 'angle', value: 225 })
    expect(angle(45)).toEqual({ tag: 'angle', value: 45 })
  })

  it('runs to the bottom when a linear gradient says nothing', () => {
    // CSS's default for `linear-gradient`, and the only direction that can be
    // assumed without inventing one.
    expect(of({ type: 'linear', colors: ['#000000ff'] })).toMatchObject({
      geometry: { kind: 'Linear', direction: { tag: 'angle', value: 180 } },
    })
  })

  it('carries two explicit endpoints when it is given them', () => {
    expect(of({ type: 'linear', direction: [0, 0, '25%', '75%'], colors: ['#000000ff'] })).toMatchObject({
      geometry: {
        direction: {
          tag: 'between',
          start: [
            { tag: 'points', value: 0 },
            { tag: 'points', value: 0 },
          ],
          end: [
            { tag: 'percent', value: 0.25 },
            { tag: 'percent', value: 0.75 },
          ],
        },
      },
    })
  })

  it('spreads a colour list evenly and puts one colour at the midpoint', () => {
    // v1's rule for `colors`. A single colour is a flat fill, and the midpoint
    // is where v1 puts it.
    const stops = (colors: readonly string[]): unknown => (of({ type: 'linear', colors }) as { stops: { offset: number }[] }).stops.map(stop => stop.offset)

    expect(stops(['#000000ff'])).toEqual([0.5])
    expect(stops(['#000000ff', '#ffffffff'])).toEqual([0, 1])
    expect(stops(['#000000ff', '#888888ff', '#ffffffff'])).toEqual([0, 0.5, 1])
  })

  it('gives a radial and a conic the middle of the box unless told', () => {
    const centre = [
      { tag: 'percent', value: 0.5 },
      { tag: 'percent', value: 0.5 },
    ]

    expect(of({ type: 'radial', colors: ['#000000ff'] })).toMatchObject({ geometry: { kind: 'Radial', at: centre } })
    expect(of({ type: 'conic', colors: ['#000000ff'] })).toMatchObject({ geometry: { kind: 'Conic', at: centre, from: 0 } })
  })

  it('is refused when the direction names nothing', () => {
    expect(() => of({ type: 'linear', direction: 'to-nowhere' as 'to-top', colors: ['#000000ff'] })).toThrow(/no direction "to-nowhere"/)
  })

  it('can be the alpha of a mask', () => {
    expect(page(Box({ mask: { gradient: { type: 'radial', colors: ['#000000ff', '#00000000'] } } })).groups.effects).toMatchObject({
      mask: { tag: 'gradient', value: { geometry: { kind: 'Radial' } } },
    })
  })
})

describe('a background image', () => {
  const of = (backgroundImage: BackgroundImage): unknown => page(Box({ backgroundImage })).groups.paint?.background_image

  it('reads a bare string as a local path and tiles both ways', () => {
    expect(of({ src: 'texture.png' })).toEqual({
      source: { tag: 'path', value: 'texture.png' },
      repeat: 'Repeat',
      // The picture's own size on both axes, which is CSS's initial value and
      // has exactly one spelling.
      size: { tag: 'per-axis', value: [{ tag: 'auto' }, { tag: 'auto' }] },
      position: [
        { tag: 'points', value: 0 },
        { tag: 'points', value: 0 },
      ],
    })
  })

  it('sizes the width from a bare value and leaves the height to the picture', () => {
    // v1's reading of `size: 12`, and CSS's one-value form.
    expect(of({ src: 'a.png', size: 12 })).toMatchObject({
      size: { tag: 'per-axis', value: [{ tag: 'points', value: 12 }, { tag: 'auto' }] },
    })
    expect(of({ src: 'a.png', size: { height: '50%' } })).toMatchObject({
      size: { tag: 'per-axis', value: [{ tag: 'auto' }, { tag: 'percent', value: 0.5 }] },
    })
  })

  it('carries the two keywords that scale to the box', () => {
    expect(of({ src: 'a.png', size: 'cover' })).toMatchObject({ size: { tag: 'cover' } })
    expect(of({ src: 'a.png', size: 'contain' })).toMatchObject({ size: { tag: 'contain' } })
  })
})

describe('the shorthands', () => {
  it('spread one value across four edges', () => {
    expect(page(Box({ padding: 4 })).groups.layout).toEqual({
      padding: Array.from({ length: 4 }, () => ({ tag: 'points', value: 4 })),
    })
  })

  it('leave an edge the caller did not name at that property’s own default', () => {
    // Not a shared zero: `padding` defaults to zero and `position` to nothing
    // at all, and an inset of zero pins that edge where absence leaves it to
    // the flow.
    expect(page(Box({ padding: { top: 4 } })).groups.layout).toEqual({
      padding: [
        { tag: 'points', value: 4 },
        { tag: 'points', value: 0 },
        { tag: 'points', value: 0 },
        { tag: 'points', value: 0 },
      ],
    })
    expect(page(Box({ position: { top: 4 } })).groups.layout).toEqual({
      inset: [{ tag: 'points', value: 4 }, null, null, null],
    })
  })

  it('take one gap for both axes, and name them apart when asked', () => {
    expect(page(Box({ gap: 4 })).groups.layout).toEqual({
      gap: [
        { tag: 'points', value: 4 },
        { tag: 'points', value: 4 },
      ],
    })
    // `(row, column)`, following CSS's shorthand rather than taffy's order.
    expect(page(Box({ gap: { row: 4, column: 9 } })).groups.layout).toEqual({
      gap: [
        { tag: 'points', value: 4 },
        { tag: 'points', value: 9 },
      ],
    })
  })

  it('name a corner at a time', () => {
    expect(page(Box({ borderRadius: { topLeft: 4, bottomRight: 9 } })).groups.paint).toEqual({
      border_radius: [4, 0, 9, 0],
    })
  })

  it('read a size in either unit, and `auto` as neither', () => {
    expect(page(Box({ width: '50%', height: 'auto' })).groups.layout).toEqual({
      size: [{ tag: 'percent', value: 0.5 }, { tag: 'auto' }],
    })
  })

  it('take a track list in each of its spellings', () => {
    expect(page(Box({ gridTemplateColumns: [1, '2px', '25%', '4fr', 'auto'] })).groups.layout).toEqual({
      grid_template_columns: [
        { tag: 'points', value: 1 },
        { tag: 'points', value: 2 },
        // `25%`, not `30%`: a quarter is exact in an `f32` and three tenths is
        // not, and this test is about spellings rather than about narrowing.
        { tag: 'percent', value: 0.25 },
        { tag: 'fraction', value: 4 },
        { tag: 'auto' },
      ],
    })
  })

  it('take a spacing in each of its spellings', () => {
    expect(page(Box({ letterSpacing: 'normal' })).groups.text).toEqual({ letter_spacing: { tag: 'normal' } })
    expect(page(Box({ letterSpacing: '2px' })).groups.text).toEqual({ letter_spacing: { tag: 'points', value: 2 } })
    expect(page(Box({ letterSpacing: '0.5em' })).groups.text).toEqual({ letter_spacing: { tag: 'em', value: 0.5 } })
  })

  it('read every hex colour form', () => {
    const forms: readonly [string, Record<string, number>][] = [
      ['#f0c', { r: 0xff, g: 0x00, b: 0xcc, a: 0xff }],
      ['#f0c8', { r: 0xff, g: 0x00, b: 0xcc, a: 0x88 }],
      ['#101014', { r: 0x10, g: 0x10, b: 0x14, a: 0xff }],
      ['#10101480', { r: 0x10, g: 0x10, b: 0x14, a: 0x80 }],
      ['transparent', { r: 0, g: 0, b: 0, a: 0 }],
    ]

    for (const [written, channels] of forms) {
      expect(page(Box({ backgroundColor: written })).groups.paint, written).toEqual({ background_color: channels })
    }
  })

  it('route a border colour to whichever field its form means', () => {
    // One property on the surface, two in the scene: a fallback colour beside
    // per-edge overrides. The split is the wire format's convenience, so it
    // does not reach the caller.
    expect(page(Box({ borderColor: '#f0c' })).groups.paint).toEqual({
      border_color_all: { r: 0xff, g: 0, b: 0xcc, a: 0xff },
    })
    expect(page(Box({ borderColor: { top: '#f0c' } })).groups.paint).toEqual({
      border_color: [{ r: 0xff, g: 0, b: 0xcc, a: 0xff }, null, null, null],
    })
  })

  it('write a grid placement’s halves independently', () => {
    expect(page(Box({ gridColumn: { start: 2 } })).groups.layout).toEqual({ grid_column: { start: 2, span: null } })
    expect(page(Box({ gridRow: { span: 3 } })).groups.layout).toEqual({ grid_row: { start: null, span: 3 } })
  })
})

describe('a scene', () => {
  it('carries every page it was given', () => {
    const arena = encodeScene([Box({ name: 'one' }), Box({ name: 'two' })], SIZE[0], SIZE[1], SCALE)
    const decoded = decode(arena.slots, arena.values)

    expect(decoded.pages.map(each => each.name)).toEqual(['one', 'two'])
  })
})

describe('a half-written shorthand', () => {
  it('takes the axis that was named and leaves the other automatic', () => {
    expect(page(Box({ width: 4 })).groups.layout).toEqual({ size: [{ tag: 'points', value: 4 }, { tag: 'auto' }] })
    expect(page(Box({ height: 4 })).groups.layout).toEqual({ size: [{ tag: 'auto' }, { tag: 'points', value: 4 }] })
    expect(page(Box({ minWidth: 4 })).groups.layout).toEqual({ min_size: [{ tag: 'points', value: 4 }, { tag: 'auto' }] })
    expect(page(Box({ minHeight: 4 })).groups.layout).toEqual({ min_size: [{ tag: 'auto' }, { tag: 'points', value: 4 }] })
    expect(page(Box({ maxWidth: 4 })).groups.layout).toEqual({ max_size: [{ tag: 'points', value: 4 }, { tag: 'auto' }] })
    expect(page(Box({ maxHeight: 4 })).groups.layout).toEqual({ max_size: [{ tag: 'auto' }, { tag: 'points', value: 4 }] })
  })

  it('takes the gap axis that was named and leaves the other at nothing', () => {
    expect(page(Box({ gap: { row: 4 } })).groups.layout).toEqual({
      gap: [
        { tag: 'points', value: 4 },
        { tag: 'points', value: 0 },
      ],
    })
    expect(page(Box({ gap: { column: 4 } })).groups.layout).toEqual({
      gap: [
        { tag: 'points', value: 0 },
        { tag: 'points', value: 4 },
      ],
    })
  })

  it('takes the edges and corners that were named', () => {
    expect(page(Box({ padding: { right: 4 } })).groups.layout).toEqual({
      padding: [
        { tag: 'points', value: 0 },
        { tag: 'points', value: 4 },
        { tag: 'points', value: 0 },
        { tag: 'points', value: 0 },
      ],
    })
    expect(page(Box({ borderRadius: { topRight: 4, bottomLeft: 9 } })).groups.paint).toEqual({ border_radius: [0, 4, 0, 9] })
  })
})

describe('the positioned values', () => {
  it('all cross as themselves', () => {
    // Four values that are not `static`, and each has to reach its own variant:
    // they differ in where they resolve rather than in when they paint, so a
    // mix-up between two of them is invisible in paint order and visible only
    // in a rendered position.
    const of = (positionType: PositionType): unknown => page(Box({ positionType })).groups.layout?.position_type

    expect(of('static')).toBe('Static')
    expect(of('relative')).toBe('Relative')
    expect(of('absolute')).toBe('Absolute')
    expect(of('fixed')).toBe('Fixed')
    expect(of('sticky')).toBe('Sticky')
  })
})

describe('an offset with no position type', () => {
  it('crosses faithfully, and the layout is what ignores it', () => {
    // `PositionType` defaults to `Static`, CSS's initial value, which ignores
    // `inset`. So the offsets are encoded exactly as written and the layout
    // does nothing with them — which is what Chrome does, measured across
    // block, flex and grid.
    //
    // Deliberate, and the division is the point: **round-trip fidelity is the
    // codec's contract and dropping the offsets is the layout's.** Refusing the
    // combination here, or quietly writing `positionType: 'relative'` beside
    // it, would make the codec lie to compensate for the layout — and would
    // cost the `inset` case its byte check, since this node would then carry
    // two properties where the case carries one.
    //
    // v1 has no equivalent, because Yoga's default is `Relative`: a ported tree
    // that positioned things stops positioning them. That is a porting note,
    // not a defect in the codec.
    const decoded = page(Box({ position: { top: 4 } }))

    expect(decoded.groups.layout).toEqual({ inset: [{ tag: 'points', value: 4 }, null, null, null] })
    expect(decoded.groups.layout?.position_type).toBeUndefined()
  })

  it('does what the caller meant once the type is named', () => {
    expect(page(Box({ position: { top: 4 }, positionType: 'relative' })).groups.layout).toEqual({
      inset: [{ tag: 'points', value: 4 }, null, null, null],
      position_type: 'Relative',
    })
  })
})

describe('the one keyword CSS and the scene spell differently', () => {
  it('crosses as the variant the scene declares', () => {
    // CSS writes `nowrap` as a single word; the scene writes the concept
    // `NoWrap`. The derivation has one exception and this is it.
    expect(page(Box({ flexWrap: 'nowrap' })).groups.layout).toEqual({ flex_wrap: 'NoWrap' })
  })

  it('is offered back under its CSS spelling when something else is refused', () => {
    expect(() => page(Box({ flexWrap: 'reverse' as 'wrap' }))).toThrow(/it takes nowrap, wrap, wrap-reverse/)
  })
})

describe('a boolean', () => {
  it('crosses as itself either way', () => {
    expect(page(Box({ dither: true })).groups.paint).toEqual({ dither: true })
    expect(page(Box({ dither: false })).groups.paint).toEqual({ dither: false })
  })
})

describe('a number that is not one', () => {
  it('is refused rather than written as NaN', () => {
    // `'abc%'` ends in a percent sign and parses to `NaN`. Writing it would put
    // a value in the arena that has no JSON spelling and no meaning.
    expect(() => page(Box({ padding: 'abc%' as '1%' }))).toThrow(/not a length/)
  })
})

/**
 * One probe per node kind the surface can express, keyed by the case's name.
 *
 * The kind cases pin the **payload**, which the property cases cannot: every
 * one of those is a styled `Box`, so without these a change to how a text,
 * image or path node is written passes every gate in this file and fails first
 * in a rendered example, as a slot the reader cannot make sense of.
 *
 * The probes are hand-written for the reason {@link PROBES} are: deriving them
 * from the fixture's `value` would mean writing the Rust-to-TypeScript adapter
 * here and checking the encoder's against it.
 */
const KIND_PROBES: Readonly<Record<string, SceneNode>> = {
  __kind_box: Box(),
  // From `RichText`, not `Text`: the case pins built runs, and `Text` now sets
  // the markup discriminant instead. The second run styles nothing on purpose —
  // an empty style still writes its mask, and that is the slot a writer skips.
  __kind_text: RichText(
    [
      { text: 'a', style: { fontWeight: 'bold' } },
      { text: 'b', style: {} },
    ],
    { maxLines: 2, ellipsis: '...' },
  ),
  __kind_image_path: Image({ src: 'probe.png', objectFit: 'cover', objectPosition: ['25%', 3], frame: 2 }),
  __kind_image_url: Image({
    src: { url: 'https://probe.invalid/a' },
    objectFit: 'cover',
    objectPosition: ['25%', 3],
    frame: 2,
  }),
  // The markup form of a text node: `Text` sets the discriminant and the string
  // crosses unparsed, because the parser lives in Rust so both surfaces get it.
  __kind_text_markup: Text('one <b>two</b>'),
  __kind_image_bytes: Image({
    src: { bytes: new Uint8Array([1, 2, 3]) },
    objectFit: 'cover',
    objectPosition: ['25%', 3],
    frame: 2,
  }),
  __kind_path: Path({
    d: 'M0 0 L4 4',
    fill: '#01020304',
    stroke: '#05060708',
    lineWidth: 2.5,
    fillRule: 'evenodd',
    lineCap: 'round',
    lineJoin: 'bevel',
    lineDash: [1, 2],
    lineDashOffset: 0.5,
  }),
  // The gradient arm of a path's paint, on the stroke and with no fill: the
  // solid arm is what `__kind_path` pins, so this one is about the tag rather
  // than about paths.
  __kind_path_gradient: Path({
    d: 'M0 0 L4 4',
    fill: 'none',
    stroke: {
      type: 'linear',
      direction: ['25%', 3, '75%', '25%'],
      stops: [{ offset: 0.5, color: '#09080706' }],
    },
  }),
}

/**
 * The kinds this surface cannot describe yet, and why.
 *
 * The same partition the style properties have. A kind case with neither a
 * probe nor a line here fails, so a payload added upstream forces a decision
 * rather than being quietly untested from this side.
 */
const UNSPELT_KINDS: Readonly<Record<string, string>> = {}

describe('the node kinds', () => {
  it('partition every kind case the fixture carries', () => {
    const cases = Object.keys(CASES).filter(name => name.startsWith('__kind_'))

    for (const name of cases) {
      const probed = KIND_PROBES[name] !== undefined
      const named = UNSPELT_KINDS[name] !== undefined
      expect(probed || named, `${name} has neither a probe nor a reason`).toBe(true)
      expect(probed && named, `${name} has both a probe and a reason`).toBe(false)
    }
    for (const name of [...Object.keys(KIND_PROBES), ...Object.keys(UNSPELT_KINDS)]) {
      expect(cases, `${name} is not a case any more`).toContain(name)
    }
  })
})

/**
 * What the arena actually carries for a case, out of what the case records.
 *
 * A markup case records two things: the string the arena holds, and what Rust
 * parses it into. Only the first crosses — the parser runs on the far side,
 * which is the whole reason the discriminant exists — so the round trip
 * compares against the markup and the paragraph, and the `parses_to` segments
 * are Rust documenting itself rather than something this side can check.
 *
 * Derived from the case rather than written out beside the probe, so a case
 * whose parse changes does not need this file edited.
 */
function carried(value: unknown): unknown {
  const payload = value as { markup?: string; parses_to?: { paragraph: unknown } }
  if (payload.markup === undefined || payload.parses_to === undefined) return value
  return { paragraph: payload.parses_to.paragraph, markup: payload.markup }
}

describe('a node kind crosses as itself', () => {
  for (const [name, probe] of Object.entries(KIND_PROBES)) {
    it(`carries the payload of ${name}`, () => {
      const expected = CASES[name]
      if (expected === undefined) throw new Error(`the fixture has no case for ${name}`)

      const decoded = page(probe)
      const tag = Object.entries(NODE_TAG).find(([, value]) => value === expected.index)

      expect(decoded.kind, `${name} is not the tag the case names`).toBe(tag?.[0])
      expect(decoded.payload).toEqual(carried(expected.value))
      // A payload is not a style: a kind case that quietly set one would be
      // checking two things and reporting one.
      for (const group of GROUPS) expect(decoded.groups[group.key]).toEqual({})
    })
  }
})

/**
 * The half of the round trip that can disagree with Rust.
 *
 * Everything above proves this package's writer and a reader built from the
 * generated tables agree. Both are from one hand, and two things from one hand
 * can agree perfectly while being wrong together. This hands the arena to the
 * addon, which decodes it into a `Scene` and writes the **byte** format for it,
 * and compares that against the bytes Rust wrote for the same property. Nothing
 * in the comparison is on this side of the language boundary.
 *
 * Bytes rather than pictures on purpose: two different scenes can render to one
 * image, so a property the encoder forgot that happens to change nothing
 * visible would go unnoticed.
 *
 * **These fail when the addon is absent. They are never skipped.** A boundary
 * check that quietly does not run is a check that passes for the wrong reason,
 * and the addon is the only thing here that checks anything against Rust.
 */

/** What the addon exports, of what this file uses. */
interface Addon {
  /** Decodes an arena and returns what the byte codec writes for that scene. */
  sceneBytes(slots: Float64Array, values: readonly (string | Buffer)[]): Buffer
}

/**
 * The built addon, or an error saying how to build it.
 *
 * Loaded inside a test rather than while the file is being collected. A throw
 * during collection takes the whole file down, so a missing addon would stop
 * the ninety-odd checks above from running at all -- the boundary suite failing
 * is the point, everything else failing with it is collateral.
 */
function addon(): Addon {
  try {
    return createRequire(import.meta.url)('../meo-canvas.node') as Addon
  } catch (cause) {
    throw new Error(
      'the addon is not built; run `just addon`. This is not skipped when it is missing, because it is the only check here that is against Rust rather than against this file.',
      { cause },
    )
  }
}

/** A side value in the form the addon's argument reader takes. */
function sideValue(value: SideValue): string | Buffer {
  return typeof value === 'string' ? value : Buffer.from(value)
}

/** The bytes the addon writes for a scene whose one page is `node`. */
function bytesOf(node: SceneNode): string {
  const arena = encodeScene([node], SIZE[0], SIZE[1], SCALE)
  return addon().sceneBytes(arena.slots, arena.values.map(sideValue)).toString('base64')
}

/** The bytes the addon writes for a scene carrying `style`. */
function throughTheAddon(style: Style): string {
  return bytesOf(Box(style))
}

describe('the bytes Rust writes for the same scene', () => {
  it('come back from the addon for an arena carrying nothing', () => {
    // One check that the two agree at all before the per-property ones. Its
    // failure means the header, the node framing or the empty masks are wrong,
    // which would otherwise fail fifty-four times over and name none of them.
    expect(throughTheAddon({}).length).toBeGreaterThan(0)
  })

  for (const [rust, probe] of Object.entries(PROBES)) {
    it(`agree on ${rust}`, () => {
      const expected = CASES[rust]
      if (expected === undefined) throw new Error(`the fixture has no case for ${rust}`)

      expect(throughTheAddon(probe)).toBe(expected.bytes)
    })
  }

  it('are the same for markup and for the runs it parses into', () => {
    // What the markup discriminant is *for*, stated as a check rather than
    // trusted. The two arenas differ — one carries a string, the other carries
    // built runs — and the scenes the addon decodes them into are identical, so
    // the byte codec writes the same thing for both.
    //
    // **The arena carries the markup unparsed**, because the parser lives on
    // the far side so that both surfaces get it. **The byte comparison is where
    // parsing is checked**, here and in the `__kind_text_markup` case: that
    // case's `bytes` are the *parsed* scene, and the string `one <b>two</b>`
    // appears nowhere in them, so the comparison passes only if the addon
    // produced exactly these runs.
    const markup = Text('one <b>two</b>')
    const runs = RichText([
      { text: 'one ', style: {} },
      { text: 'two', style: { fontWeight: 'bold' } },
    ])

    expect(encodeScene([markup], SIZE[0], SIZE[1], SCALE).slots).not.toEqual(encodeScene([runs], SIZE[0], SIZE[1], SCALE).slots)
    expect(bytesOf(markup)).toBe(bytesOf(runs))
  })

  for (const [name, probe] of Object.entries(KIND_PROBES)) {
    it(`agree on ${name}`, () => {
      // The payload half, and the one that was missing when an arena text
      // payload changed under a green suite.
      const expected = CASES[name]
      if (expected === undefined) throw new Error(`the fixture has no case for ${name}`)

      expect(bytesOf(probe)).toBe(expected.bytes)
    })
  }
})

/**
 * Every keyword union on the surface, against the enum it crosses as.
 *
 * The one copy of a list this file keeps, and it is checked in both directions:
 * a keyword with no variant behind it fails, and a variant with no keyword in
 * front of it fails. The second direction is the one that rots silently — a
 * variant added upstream is a value the scene can carry and this surface cannot
 * name, and nothing else here notices — the encoder throws only for a keyword
 * with no variant, never for a variant with no keyword.
 *
 * The unions themselves are types, so they are gone at runtime and cannot be
 * read. This is that list written down where it can be checked.
 */
const KEYWORDS: readonly (readonly [string, readonly string[]])[] = [
  ['Align', ['flex-start', 'flex-end', 'center', 'stretch', 'baseline', 'space-between', 'space-around', 'space-evenly']],
  [
    'BlendMode',
    [
      'normal',
      'multiply',
      'screen',
      'overlay',
      'darken',
      'lighten',
      'color-dodge',
      'color-burn',
      'hard-light',
      'soft-light',
      'difference',
      'exclusion',
      'hue',
      'saturation',
      'color',
      'luminosity',
    ],
  ],
  ['BorderStyle', ['solid', 'dashed', 'dotted']],
  // The two whose keywords are upstream's rather than this package's, and which
  // upstream spells differently from each other: a pixel layout has no CSS
  // vocabulary to borrow, a colour space does. Both carry aliases, so these two
  // are the only entries here that are not one-to-one with their variants.
  [
    'ColorType',
    [
      'Alpha8',
      'Gray8',
      'R8UNorm',
      'A16Float',
      'A16UNorm',
      'ARGB4444',
      'R8G8UNorm',
      'RGB565',
      'rgb',
      'RGB888x',
      'rgba',
      'RGBA8888',
      'bgra',
      'BGRA8888',
      'BGR101010x',
      'BGRA1010102',
      'R16G16Float',
      'R16G16UNorm',
      'RGB101010x',
      'RGBA1010102',
      'SRGBA8888',
      'N32',
      'R16G16B16A16UNorm',
      'RGBAF16',
      'RGBAF16Norm',
      'RGBAF32',
    ],
  ],
  [
    'ColorSpace',
    [
      'srgb',
      'srgb-linear',
      'linear',
      'display-p3',
      'p3',
      'display-p3-linear',
      'p3-linear',
      'rec2020',
      'bt2020',
      'rec2020-linear',
      'bt2020-linear',
      'rec2020-pq',
      'hdr10',
      'rec2020-hlg',
      'hlg',
    ],
  ],
  ['BoxSizing', ['border-box', 'content-box']],
  ['Direction', ['ltr', 'rtl']],
  ['Display', ['flex', 'grid', 'block', 'none']],
  ['FillRule', ['nonzero', 'evenodd']],
  ['FlexDirection', ['row', 'row-reverse', 'column', 'column-reverse']],
  ['FlexWrap', ['nowrap', 'wrap', 'wrap-reverse']],
  ['FontStyle', ['normal', 'italic']],
  ['GridAutoFlow', ['row', 'column', 'row-dense', 'column-dense']],
  ['MaskShape', ['circle', 'ellipse']],
  ['Justify', ['flex-start', 'flex-end', 'center', 'space-between', 'space-around', 'space-evenly']],
  ['ObjectFit', ['fill', 'contain', 'cover', 'none', 'scale-down']],
  ['Overflow', ['visible', 'hidden', 'scroll']],
  ['PaintOrder', ['fill', 'stroke']],
  ['PositionType', ['static', 'relative', 'absolute', 'fixed', 'sticky']],
  ['TextAlign', ['start', 'end', 'left', 'center', 'right', 'justify']],
  ['TextDecoration', ['none', 'underline', 'overline', 'line-through']],
  ['VerticalAlign', ['top', 'middle', 'bottom']],
]

describe('the keywords this surface offers', () => {
  it('name one variant each, and every variant the scene has', () => {
    for (const [name, keywords] of KEYWORDS) {
      const declared = ENUMS[name]
      expect(declared, `${name} is not a wire enum any more`).toBeDefined()

      const variants = Object.keys(declared?.variants ?? {})
      // Resolved through the encoder's own function rather than through a copy
      // of its rules, so the check and the thing checked cannot drift. A
      // keyword the encoder cannot resolve throws here rather than comparing
      // unequal.
      const table = declared?.variants ?? {}
      const reached = keywords.map(keyword => {
        const discriminant = variant(table, keyword, name)
        return Object.entries(table).find(([, value]) => value === discriminant)?.[0]
      })

      // Every variant reachable from at least one keyword, and every keyword
      // reaching one. Not one-to-one: `ColorType` and `ColorSpace` carry
      // upstream's aliases, so `'rgba'` and `'RGBA8888'` are both `Uint8`.
      expect([...new Set(reached)].sort(), `${name}`).toEqual(variants.slice().sort())
    }
  })
})
