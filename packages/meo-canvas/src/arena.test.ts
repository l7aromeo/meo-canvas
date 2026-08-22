import { createRequire } from 'node:module'

import { describe, expect, it } from 'vitest'

import cases from '../../../fixtures/arena-cases.json' with { type: 'json' }

import { ArenaWriter, PROPERTY_TABLES, encodeScene, type SideValue } from './arena.js'
import { ENUMS } from './generated/arena-enums.js'
import { EFFECTS, LAYOUT, MAGIC, MASK_BITS, PAINT, TEXT, VERSION, type ArenaProperty } from './generated/arena-tables.js'
import { Box, Image, Path, RichText, Text, type SceneNode } from './node.js'
import type { Style } from './style.js'

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
  // Only the solid arm. A gradient has no spelling on this surface yet, so one
  // reaching here would mean the writer invented something, and throwing says
  // so rather than decoding it into a shape nothing checks.
  PathPaint: input => {
    const tag = ['solid', 'gradient'][slot(input)]
    if (tag !== 'solid') throw new TypeError(`this reader reads a solid path paint, not ${String(tag)}`)
    return { tag, value: color(input) }
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

/** Reads the payload the node's kind carries. */
function readPayload(input: Cursor, kind: string): unknown {
  if (kind === 'Box') return null

  if (kind === 'Text') {
    const maxLines = read(input, 'Option<u32>')
    const ellipsis = read(input, 'Option<String>')
    const count = slot(input)
    const segments = Array.from({ length: count }, () => {
      const text = side(input)
      const mask = Array.from({ length: slotsFor(TEXT.length) }, () => slot(input))
      return { text, style: readGroup(input, TEXT, mask) }
    })
    return { maxLines, ellipsis, segments }
  }

  if (kind === 'Image') {
    // A source is a tag and then a side value, not a tag and a number: the
    // bytes of an image cannot live in a `Float64Array` any more than a string
    // can.
    const tags = ['path', 'url', 'bytes']
    const tag = tags[slot(input)]
    if (tag === undefined) throw new RangeError('ImageSource has no such tag')
    return {
      source: { tag, value: side(input) },
      fit: read(input, 'ObjectFit'),
      position: [read(input, 'Length'), read(input, 'Length')],
      frame: read(input, 'Option<u32>'),
    }
  }

  return {
    data: side(input),
    fill: read(input, 'Option<PathPaint>'),
    stroke: read(input, 'Option<PathPaint>'),
    lineWidth: f32(input),
    fillRule: read(input, 'FillRule'),
    lineCap: read(input, 'LineCap'),
    lineJoin: read(input, 'LineJoin'),
    lineDash: read(input, 'Vec<f32>'),
    lineDashOffset: f32(input),
  }
}

/** A whole arena, decoded. */
interface DecodedArena {
  /** The page size and the scale it was written at. */
  readonly size: readonly [number, number]
  /** The device pixel ratio. */
  readonly scale: number
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
  const count = slot(input)
  const pages = Array.from({ length: count }, () => readNode(input))

  // The check that turns a writer emitting the wrong number of slots from a
  // comparison failure into a structural one.
  expect(input.at, 'the arena has slots past the end of the scene').toBe(slots.length)
  return { size, scale, pages }
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
 */
const PROBES: Readonly<Record<string, Style>> = {
  align_content: { alignContent: 'flex-end' },
  align_items: { alignItems: 'flex-end' },
  align_self: { alignSelf: 'flex-end' },
  aspect_ratio: { aspectRatio: 1 },
  backdrop_filter: { backdropFilter: 'probe' },
  background_color: { backgroundColor: '#00000001' },
  blend_mode: { mixBlendMode: 'multiply' },
  border: { border: 1 },
  border_color: { borderColor: { top: '#00000001', right: '#00000001', bottom: '#00000001', left: '#00000001' } },
  border_color_all: { borderColor: '#00000001' },
  border_radius: { borderRadius: 1 },
  border_style: { borderStyle: 'dashed' },
  box_sizing: { boxSizing: 'content-box' },
  color: { color: '#00000001' },
  direction: { direction: 'rtl' },
  display: { display: 'grid' },
  dither: { dither: true },
  filter: { filter: 'probe' },
  flex_basis: { flexBasis: 1 },
  flex_direction: { flexDirection: 'row-reverse' },
  flex_grow: { flexGrow: 1 },
  flex_shrink: { flexShrink: 2 },
  flex_wrap: { flexWrap: 'wrap' },
  font_family: { fontFamily: 'probe' },
  font_size: { fontSize: 1 },
  font_style: { fontStyle: 'italic' },
  font_weight: { fontWeight: 1 },
  gap: { gap: '1%' },
  grid_auto_columns: { gridAutoColumns: 1 },
  grid_auto_flow: { gridAutoFlow: 'column' },
  grid_auto_rows: { gridAutoRows: 1 },
  grid_column: { gridColumn: { start: 1, span: 1 } },
  grid_row: { gridRow: { start: 1, span: 1 } },
  grid_template_columns: { gridTemplateColumns: [1] },
  grid_template_rows: { gridTemplateRows: [1] },
  inset: { position: '1%' },
  justify_content: { justifyContent: 'flex-end' },
  letter_spacing: { letterSpacing: 1 },
  line_gap: { lineGap: 1 },
  line_height: { lineHeight: 1 },
  margin: { margin: 1 },
  max_size: { maxWidth: 1, maxHeight: 1 },
  min_size: { minWidth: 1, minHeight: 1 },
  opacity: { opacity: 2 },
  overflow: { overflow: 'hidden' },
  padding: { padding: '1%' },
  paint_order: { paintOrder: 'stroke' },
  position_type: { positionType: 'absolute' },
  size: { width: 1, height: 1 },
  text_align: { textAlign: 'end' },
  text_decoration: { textDecoration: 'underline' },
  vertical_align: { verticalAlign: 'middle' },
  word_spacing: { wordSpacing: 1 },
  z_index: { zIndex: 1 },
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
  gradient: 'gradients have no surface yet; the shape of the authoring API is not settled',
  background_image: 'waits on the gradient surface, which it shares a vocabulary with',
  font_variant: 'the thirty-five OpenType features need a spelling of their own',
  text_stroke: 'waits on the paint surface that also gives a path its fill',
  transform: 'waits on a decision about whether it takes a CSS string or a struct',
  box_shadows: 'waits on the same decision as `transform`',
  text_shadows: 'waits on the same decision as `transform`',
  mask: 'waits on the gradient surface and on the image source vocabulary',
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
  it('carries one segment for a plain string', () => {
    const decoded = page(Text('Ukasyah'))

    expect(decoded.kind).toBe('Text')
    expect(decoded.payload).toEqual({
      maxLines: null,
      ellipsis: null,
      segments: [{ text: 'Ukasyah', style: {} }],
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
      maxLines: null,
      ellipsis: null,
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
        { tag: 'percent', value: 50 },
        { tag: 'percent', value: 50 },
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
        { tag: 'percent', value: 50 },
        { tag: 'percent', value: 50 },
      ],
      frame: 3,
    })
  })

  it('carries bytes through the side values rather than the slots', () => {
    const bytes = new Uint8Array([1, 2, 3])
    const arena = encodeScene([Image({ src: { bytes } })], SIZE[0], SIZE[1], SCALE)

    expect(arena.values).toEqual([bytes])
    expect(decode(arena.slots, arena.values).pages[0]?.payload).toMatchObject({ source: { tag: 'bytes', value: bytes } })
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
      lineWidth: 1,
      fillRule: 'NonZero',
      lineCap: 'Butt',
      lineJoin: 'Miter',
      lineDash: [],
      lineDashOffset: 0,
    })
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
      size: [{ tag: 'percent', value: 50 }, { tag: 'auto' }],
    })
  })

  it('take a track list in each of its spellings', () => {
    expect(page(Box({ gridTemplateColumns: [1, '2px', '30%', '4fr', 'auto'] })).groups.layout).toEqual({
      grid_template_columns: [
        { tag: 'points', value: 1 },
        { tag: 'points', value: 2 },
        { tag: 'percent', value: 30 },
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

/** The bytes the addon writes for a scene carrying `style`. */
function throughTheAddon(style: Style): string {
  const arena = encodeScene([Box(style)], SIZE[0], SIZE[1], SCALE)
  return addon().sceneBytes(arena.slots, arena.values.map(sideValue)).toString('base64')
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
})

/**
 * Every keyword union on the surface, against the enum it crosses as.
 *
 * The one copy of a list this file keeps, and it is checked in both directions:
 * a keyword with no variant behind it fails, and a variant with no keyword in
 * front of it fails. The second direction is the one that rots silently — a
 * variant added upstream is a value the scene can carry and this surface cannot
 * name, and nothing else here would notice. It has already happened once:
 * `PositionType::Static` arrived while this was being written.
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
  ['BoxSizing', ['border-box', 'content-box']],
  ['Direction', ['ltr', 'rtl']],
  ['Display', ['flex', 'grid', 'block', 'none']],
  ['FlexDirection', ['row', 'row-reverse', 'column', 'column-reverse']],
  ['FlexWrap', ['nowrap', 'wrap', 'wrap-reverse']],
  ['FontStyle', ['normal', 'italic']],
  ['GridAutoFlow', ['row', 'column', 'row-dense', 'column-dense']],
  ['Justify', ['flex-start', 'flex-end', 'center', 'space-between', 'space-around', 'space-evenly']],
  ['ObjectFit', ['fill', 'contain', 'cover', 'none', 'scale-down']],
  ['Overflow', ['visible', 'hidden', 'scroll']],
  ['PaintOrder', ['fill', 'stroke']],
  ['PositionType', ['static', 'relative', 'absolute']],
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
      const named = keywords.map(keyword =>
        keyword
          .split('-')
          .map(part => part.charAt(0).toUpperCase() + part.slice(1))
          .join(''),
      )
      // `nowrap` is the one keyword CSS spells as a single word where the scene
      // spells the concept `NoWrap`, so the derivation cannot reach it.
      const derived = named.map(variant => (variant === 'Nowrap' ? 'NoWrap' : variant))

      expect(derived.slice().sort(), `${name}`).toEqual(variants.slice().sort())
    }
  })
})
