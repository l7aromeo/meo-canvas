import { createRequire } from 'node:module'
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Chart, type BaseChartOptions, type ChartType } from './chart.js'
import { Box, type SceneNode } from './node.js'
import { encodeScene } from './arena.js'

/**
 * Every chart option, swept across every kind, compared by FNV-1a digest and byte
 * length with the Rust surface: field coverage is not combination coverage, and the
 * pinned charts in `chart.agreement.test.ts` each carry one kind and one bag.
 * Regenerate with `UPDATE_CHART_DIFFERENTIAL=1`; the Rust side never writes it.
 */
const asset = () => fileURLToPath(new URL('../../../crates/meo-canvas/tests/assets/chart/differential-digests.txt', import.meta.url))

interface Addon {
  sceneBytes(slots: Float64Array, values: readonly (string | Buffer)[]): Buffer
}

function addon(): Addon {
  try {
    return createRequire(import.meta.url)('../meo-canvas.node') as Addon
  } catch (cause) {
    throw new Error('the addon is not built; run `just addon`. This is the only check here that reaches the byte codec.', { cause })
  }
}

/** The node each kind names, and where its bytes begin. */
const MARK: Record<ChartType, string> = { bar: 'bar chart', line: 'line chart', pie: 'pie chart', doughnut: 'doughnut chart' }

/**
 * Every option but the one overriding the axis-colour fallback, so the cases
 * reaching that fallback start from a bag that does not mask it. The key is
 * omitted rather than `undefined`, as a caller who said nothing omits it.
 */
const EVERY_BUT_Y_AXIS_COLOUR: BaseChartOptions = {
  showLabels: true,
  showValues: true,
  showYAxis: true,
  showLegend: true,
  grid: { show: true, color: '#e0e0e0' },
  labelFontSize: 11,
  valueFontSize: 10,
  yAxisFontSize: 9,
  labelColor: '#112233',
  valueColor: '#445566',
}

/** Every option switched on, so a case varies one thing against a known rest. */
const EVERY: BaseChartOptions = { ...EVERY_BUT_Y_AXIS_COLOUR, yAxisColor: '#778899' }

/** Long enough to wrap and to dwarf its slot, which is where a label's box stops agreeing. */
const LONG = 'l'.repeat(200)

/** A string no CSS syntax spells as a colour. */
const UNREADABLE = 'not-a-colour'

const cartesian = (labels: readonly string[], datasets: readonly { data: readonly number[]; label?: string; color?: string }[]) => ({ labels, datasets })
const series = (data: readonly number[], label?: string, color?: string) => ({
  data,
  ...(label === undefined ? {} : { label }),
  ...(color === undefined ? {} : { color }),
})
const slice = (label: string, value: number, color?: string) => ({ label, value, ...(color === undefined ? {} : { color }) })

/** The chart every option case varies, so a difference is the option. */
const BAR_DATA = cartesian(['a', 'b'], [series([1, 2], 'Sales', '#3366cc'), series([2, 1])])
const PIE_DATA = [slice('a', 3, '#3366cc'), slice('b', 2), slice('c', 1, '#cc6633')]

const CARTESIAN_KINDS = ['bar', 'line'] as const
const RADIAL_KINDS = ['pie', 'doughnut'] as const
const EVERY_KIND = [...CARTESIAN_KINDS, ...RADIAL_KINDS] as const

interface Case {
  readonly name: string
  readonly kind: ChartType
  readonly build: () => SceneNode
}

const chart =
  (kind: ChartType, data: unknown, options?: BaseChartOptions, fontFamily?: string): (() => SceneNode) =>
  () =>
    Chart({
      type: kind,
      data,
      ...(options === undefined ? {} : { options }),
      ...(fontFamily === undefined ? {} : { fontFamily }),
    } as never)

function cases(): Case[] {
  const out: Case[] = []
  const add = (name: string, kind: ChartType, data: unknown, options?: BaseChartOptions, fontFamily?: string) =>
    out.push({ name, kind, build: chart(kind, data, options, fontFamily) })

  // Shapes of data, on the two kinds that take a series. Each is a case the
  // pinned charts cannot reach: they carry one shape between them.
  const shapes: Record<string, ReturnType<typeof cartesian>> = {
    zeros: cartesian(['a', 'b', 'c'], [series([0, 0, 0], 'Zero')]),
    'one-datum': cartesian(['a'], [series([1], 'One')]),
    'all-equal': cartesian(['a', 'b', 'c'], [series([2, 2, 2], 'Flat')]),
    negatives: cartesian(['a', 'b', 'c'], [series([1, -2, 3], 'Signed')]),
    'one-negative': cartesian(['a'], [series([-1], 'Down')]),
    // Nine series against a palette of eight, so the wrap is inside the
    // comparison rather than beside it.
    'palette-wrap': cartesian(
      ['a', 'b'],
      Array.from({ length: 9 }, (_, index) => series([index + 1, 9 - index], `S${index}`)),
    ),
    'empty-label': cartesian(['', 'b'], [series([1, 2], 'E')]),
    'long-label': cartesian([LONG, 'b'], [series([1, 2], 'L')]),
    'non-ascii-label': cartesian(['日本語', 'émoji 🎉'], [series([1, 2], 'U')]),
    'empty-series-label': cartesian(['a', 'b'], [series([1, 2], '')]),
    'no-labels-no-data': cartesian([], []),
    'labels-without-datasets': cartesian(['a', 'b'], []),
    'datasets-without-labels': cartesian([], [series([1, 2], 'X')]),
    'empty-data': cartesian(['a', 'b'], [series([], 'Empty')]),
    // A bar chart iterates the labels and a line chart iterates the data, so
    // the two mismatches are different branches rather than one.
    'more-data-than-labels': cartesian(['a'], [series([1, 2, 3], 'Over')]),
    'fewer-data-than-labels': cartesian(['a', 'b', 'c'], [series([1], 'Under')]),
    // A value with more than two decimals is where a rounded label and an
    // unrounded one part company; whole numbers cannot tell them apart.
    fractional: cartesian(['a', 'b'], [series([1.5, 2.25], 'Frac')]),
    thirds: cartesian(['a', 'b'], [series([1 / 3, 2 / 3], 'Third')]),
    large: cartesian(['a', 'b'], [series([1e6, 3e6], 'Big')]),
    tiny: cartesian(['a', 'b'], [series([1e-3, 2e-3], 'Small')]),
  }
  for (const [shape, data] of Object.entries(shapes)) for (const kind of CARTESIAN_KINDS) add(`data/${shape}/${kind}`, kind, data, EVERY)

  const slices: Record<string, ReturnType<typeof slice>[]> = {
    'one-slice': [slice('only', 5)],
    'zero-value-slice': [slice('a', 3), slice('b', 0), slice('c', 1)],
    'all-zero': [slice('a', 0), slice('b', 0)],
    'negative-slice': [slice('a', 3), slice('b', -1)],
    'palette-wrap': Array.from({ length: 9 }, (_, index) => slice(`s${index}`, index + 1)),
    'empty-label': [slice('', 3), slice('b', 1)],
    'long-label': [slice(LONG, 3), slice('b', 1)],
    'non-ascii-label': [slice('日本語', 3), slice('émoji 🎉', 1)],
    'no-slices': [],
    fractional: [slice('a', 1.5), slice('b', 2.25)],
    'equal-slices': [slice('a', 1), slice('b', 1), slice('c', 1)],
  }
  for (const [shape, data] of Object.entries(slices)) for (const kind of RADIAL_KINDS) add(`data/${shape}/${kind}`, kind, data, EVERY)

  // A label spelled like the node this file slices from, so the mark appears three
  // times; `a label spelling the mark does not move the slice` measures the
  // collision, and this compares the repeated string's encoding.
  for (const kind of CARTESIAN_KINDS) {
    add(`data/mark-collision/${kind}`, kind, cartesian([MARK[kind], 'b'], [series([1, 2], MARK[kind])]), EVERY)
  }
  for (const kind of RADIAL_KINDS) {
    add(`data/mark-collision/${kind}`, kind, [slice(MARK[kind], 3), slice('b', 1)], EVERY)
  }

  // A colour no CSS syntax spells. Both surfaces refuse it rather than
  // drawing a default, and the two that build are the shape of the claim: a
  // pie has no grid, so the option is unused rather than unreadable.
  for (const kind of CARTESIAN_KINDS) {
    add(`data/unreadable-series-colour/${kind}`, kind, cartesian(['a', 'b'], [series([1, 2], 'S', UNREADABLE)]), EVERY)
    add(`option/unreadable-grid-colour/${kind}`, kind, BAR_DATA, { ...EVERY, grid: { show: true, color: UNREADABLE } })
  }
  for (const kind of RADIAL_KINDS) {
    add(`data/unreadable-slice-colour/${kind}`, kind, [slice('a', 3, UNREADABLE), slice('b', 1)], EVERY)
    add(`option/unreadable-grid-colour/${kind}`, kind, PIE_DATA, { ...EVERY, grid: { show: true, color: UNREADABLE } })
  }

  const data = (kind: ChartType) => (kind === 'bar' || kind === 'line' ? BAR_DATA : PIE_DATA)
  const everyKind = (name: string, options?: BaseChartOptions) => {
    for (const kind of EVERY_KIND) add(`option/${name}/${kind}`, kind, data(kind), options)
  }

  everyKind('all-default', undefined)
  everyKind('empty-options', {})
  everyKind('font-size-zero', { ...EVERY, labelFontSize: 0, valueFontSize: 0, yAxisFontSize: 0 })
  everyKind('font-size-negative', { ...EVERY, labelFontSize: -4, valueFontSize: -4, yAxisFontSize: -4 })
  everyKind('font-size-fractional', { ...EVERY, labelFontSize: 11.5, valueFontSize: 10.25, yAxisFontSize: 9.75 })
  everyKind('grid-without-colour', { ...EVERY, grid: { show: true } })
  everyKind('grid-off-with-colour', { ...EVERY, grid: { show: false, color: '#e0e0e0' } })
  everyKind('grid-empty', { ...EVERY, grid: {} })

  // `axisColor` is the fallback `yAxisColor` overrides, so a case that sets
  // both never reaches it. These three separate the two.
  everyKind('axis-colour-alone', { ...EVERY_BUT_Y_AXIS_COLOUR, axisColor: '#334455' })
  everyKind('axis-colour-overridden', { ...EVERY, axisColor: '#334455' })
  everyKind('axis-colour-absent', EVERY_BUT_Y_AXIS_COLOUR)

  // A doughnut's hole, at both ends of its range and in the middle. The other
  // three kinds ignore it, which is itself a byte-checked claim.
  for (const fraction of [0, 0.25, 1]) everyKind(`inner-radius-${fraction}`, { ...EVERY, innerRadius: fraction })

  // The four booleans, every combination, so no pair hides behind another.
  for (let mask = 0; mask < 16; mask += 1) {
    everyKind(`toggles-${mask.toString(2).padStart(4, '0')}`, {
      ...EVERY,
      showLabels: Boolean(mask & 1),
      showValues: Boolean(mask & 2),
      showYAxis: Boolean(mask & 4),
      showLegend: Boolean(mask & 8),
    })
  }

  for (const position of ['top', 'bottom', 'left', 'right'] as const) everyKind(`legend-${position}`, { ...EVERY, legendPosition: position })

  // A legend switched on over series that name nothing for it to list.
  for (const kind of CARTESIAN_KINDS) add(`option/legend-without-labels/${kind}`, kind, cartesian(['a', 'b'], [series([1, 2]), series([2, 1])]), EVERY)

  // The family lives on `ChartProps` here and in `Options` there, so this is
  // also the case that checks the two routes arrive at the same place.
  for (const kind of EVERY_KIND) add(`option/font-family/${kind}`, kind, data(kind), EVERY, 'Fixture')

  return out
}

const CASES = cases()

/** What a case encodes to, or that it refused. */
type Encoded = { readonly refused: true } | { readonly refused: false; readonly digest: string; readonly length: number; readonly hex: string }

function fromTheChart(bytes: Buffer, mark: string): Buffer {
  const at = bytes.indexOf(Buffer.from(mark, 'utf8'))
  if (at < 0) throw new Error(`the scene has no \`${mark}\` node`)
  return bytes.subarray(at)
}

/**
 * One chart, encoded as the page would encode it: wrapped in a `Box`, as Rust's
 * `Root::new(200, 120)` adds a page root, and compared from the chart's own node.
 */
/**
 * FNV-1a, 32-bit, with the offset basis and prime `draft-eastlake-fnv` names.
 * `Math.imul` wraps at thirty-two bits as Rust's `wrapping_mul` does.
 */
const FNV_OFFSET_BASIS = 0x811c_9dc5
const FNV_PRIME = 0x0100_0193

function fnv1a(bytes: Buffer): string {
  let hash = FNV_OFFSET_BASIS
  for (const byte of bytes) hash = Math.imul(hash ^ byte, FNV_PRIME)
  return (hash >>> 0).toString(16).padStart(8, '0')
}

function encode(one: Case): Encoded {
  // The whole route, not just the build: an unreadable colour crosses unparsed and
  // the addon refuses it in `sceneBytes`, after `Chart()` has returned.
  let bytes: Buffer
  try {
    const arena = encodeScene([Box({ children: one.build() })], 200, 120, false, 1)
    const values = arena.values.map(value => (typeof value === 'string' ? value : Buffer.from(value)))
    bytes = fromTheChart(addon().sceneBytes(arena.slots, values), MARK[one.kind])
  } catch {
    return { refused: true }
  }
  return { refused: false, digest: fnv1a(bytes), length: bytes.length, hex: bytes.toString('hex') }
}

const REFUSED = 'refused'
const COLUMNS = '# case\tdigest\tbytes'

function render(): string {
  const rows = CASES.map(one => {
    const encoded = encode(one)
    return encoded.refused ? `${one.name}\t${REFUSED}\t-` : `${one.name}\t${encoded.digest}\t${encoded.length}`
  })
  return `${[COLUMNS, ...rows].join('\n')}\n`
}

/** The committed rows, by case name, with the header asserted rather than skipped. */
function committed(): Map<string, { digest: string; length: string }> {
  const lines = readFileSync(asset(), 'utf8').trim().split('\n')
  expect(lines[0], 'the asset has lost its column line').toBe(COLUMNS)
  const rows = new Map<string, { digest: string; length: string }>()
  for (const line of lines.slice(1)) {
    const [name, digest, length] = line.split('\t')
    if (name === undefined || digest === undefined || length === undefined) throw new Error(`the asset row \`${line}\` is not three columns`)
    rows.set(name, { digest, length })
  }
  return rows
}

if (process.env['UPDATE_CHART_DIFFERENTIAL'] === '1') writeFileSync(asset(), render())

describe('every option, against every kind, agrees across the two surfaces', () => {
  const rows = committed()

  it('names the same cases the asset does', () => {
    expect([...rows.keys()].sort()).toStrictEqual(CASES.map(one => one.name).sort())
    expect(rows.size).toBe(CASES.length)
  })

  it.each(CASES)('encodes $name to the committed digest', one => {
    const row = rows.get(one.name)
    expect(row, `the asset has no row for ${one.name}`).toBeDefined()
    const encoded = encode(one)
    if (encoded.refused) {
      expect(row?.digest, `${one.name} refused here and the asset expects bytes`).toBe(REFUSED)
      return
    }
    expect(Number(row?.length), `${one.name} encodes to a different number of bytes`).toBe(encoded.length)
    expect(row?.digest).toBe(encoded.digest)
  })

  /**
   * The comparison has to be able to fail: each is a real chart one option away
   * from a committed row, and none may match the row it is named after.
   */
  it.each([
    { name: 'option/toggles-1111/bar', changed: { showValues: false } },
    { name: 'option/legend-left/line', changed: { legendPosition: 'right' as const } },
    { name: 'option/font-family/pie', changed: { labelColor: '#010203' } },
    { name: 'option/inner-radius-0.25/doughnut', changed: { innerRadius: 0.75 } },
  ])('reports a difference when $name is perturbed', ({ name, changed }) => {
    const one = CASES.find(candidate => candidate.name === name)
    if (one === undefined) throw new Error(`no case named ${name}`)
    const row = rows.get(name)
    const kind = one.kind
    const options = { ...EVERY, ...changed } as BaseChartOptions
    const perturbed = encode({
      name,
      kind,
      build: chart(kind, kind === 'bar' || kind === 'line' ? BAR_DATA : PIE_DATA, options, name.includes('font-family') ? 'Fixture' : undefined),
    })
    if (perturbed.refused) throw new Error(`the perturbed ${name} refused, so it compares nothing`)
    expect(perturbed.digest, `perturbing ${name} did not change its bytes`).not.toBe(row?.digest)
  })
})

/**
 * Which options a kind can see, stated rather than assumed and pinned both ways:
 * an option that stops being observable fails, and so does one that starts. The
 * inert entries are not gaps: a line chart draws no per-datum value.
 */
const INERT: Record<string, readonly ChartType[]> = {
  showValues: ['line', 'pie', 'doughnut'],
  showYAxis: ['pie', 'doughnut'],
  grid: ['pie', 'doughnut'],
  valueFontSize: ['line', 'pie', 'doughnut'],
  yAxisFontSize: ['pie', 'doughnut'],
  valueColor: ['line', 'pie', 'doughnut'],
  yAxisColor: ['pie', 'doughnut'],
  innerRadius: ['bar', 'line', 'pie'],
}

const VARIED: Record<string, unknown> = {
  showLabels: false,
  showValues: false,
  showYAxis: false,
  showLegend: false,
  legendPosition: 'left',
  grid: { show: false },
  labelFontSize: 22,
  valueFontSize: 20,
  yAxisFontSize: 18,
  labelColor: '#ff0000',
  valueColor: '#00ff00',
  yAxisColor: '#0000ff',
  innerRadius: 0.2,
}

describe('every option this file varies can be seen in the bytes', () => {
  const base = (kind: ChartType) =>
    encode({ name: `base/${kind}`, kind, build: chart(kind, kind === 'bar' || kind === 'line' ? BAR_DATA : PIE_DATA, { ...EVERY, innerRadius: 0.5 }) })

  it.each(EVERY_KIND.flatMap(kind => Object.keys(VARIED).map(option => ({ kind, option }))))('$option on a $kind chart', ({ kind, option }) => {
    const first = base(kind)
    const varied = encode({
      name: `${option}/${kind}`,
      kind,
      build: chart(kind, kind === 'bar' || kind === 'line' ? BAR_DATA : PIE_DATA, { ...EVERY, innerRadius: 0.5, [option]: VARIED[option] }),
    })
    if (first.refused || varied.refused) throw new Error(`the base chart for ${kind} refused, so nothing is compared`)
    const inert = (INERT[option] ?? []).includes(kind)
    if (inert) expect(varied.hex, `${option} changed a ${kind}, which this file records as ignoring it`).toBe(first.hex)
    else expect(varied.hex, `${option} left a ${kind} unchanged, so every case varying it agrees about nothing`).not.toBe(first.hex)
  })

  // The family reaches text through a different route on each surface, and
  // every kind draws text, so no kind may ignore it.
  it.each(EVERY_KIND)('a font family changes a %s chart', kind => {
    const plain = base(kind)
    const named = encode({
      name: `family/${kind}`,
      kind,
      build: chart(kind, kind === 'bar' || kind === 'line' ? BAR_DATA : PIE_DATA, { ...EVERY, innerRadius: 0.5 }, 'Fixture'),
    })
    if (plain.refused || named.refused) throw new Error(`the base chart for ${kind} refused, so nothing is compared`)
    expect(named.hex).not.toBe(plain.hex)
  })

  // `axisColor` is only reachable with `yAxisColor` unset, which is why a
  // sweep that switches every option on at once reports it inert everywhere.
  it.each(EVERY_KIND)('an axis colour is reachable on a %s chart only through the fallback', kind => {
    const without = EVERY_BUT_Y_AXIS_COLOUR
    const data = kind === 'bar' || kind === 'line' ? BAR_DATA : PIE_DATA
    const plain = encode({ name: `axis/${kind}`, kind, build: chart(kind, data, without) })
    const fallback = encode({ name: `axis-set/${kind}`, kind, build: chart(kind, data, { ...without, axisColor: '#ff00ff' }) })
    const overridden = encode({ name: `axis-over/${kind}`, kind, build: chart(kind, data, { ...EVERY, axisColor: '#ff00ff' }) })
    const yOnly = encode({ name: `y-only/${kind}`, kind, build: chart(kind, data, EVERY) })
    if (plain.refused || fallback.refused || overridden.refused || yOnly.refused) throw new Error(`a base chart for ${kind} refused`)
    const drawsAnAxis = kind === 'bar' || kind === 'line'
    if (drawsAnAxis) expect(fallback.hex, 'the axis colour did not reach the y-axis text').not.toBe(plain.hex)
    else expect(fallback.hex, 'a kind with no y axis took an axis colour').toBe(plain.hex)
    expect(overridden.hex, 'an explicit y-axis colour did not override the axis colour').toBe(yOnly.hex)
  })
})

/**
 * The unreadable colour is what makes those cases refuse: each refusing shape is
 * rebuilt with a readable colour and must encode, so a harness producing nothing
 * cannot pass. A pie has no grid, so its grid colour is unused rather than refused.
 */
describe('an unreadable colour is what makes a chart refuse', () => {
  const shapes = (kind: ChartType, colour: string): { role: string; build: () => SceneNode; refuses: boolean }[] =>
    kind === 'bar' || kind === 'line'
      ? [
          { role: 'series colour', build: chart(kind, cartesian(['a', 'b'], [series([1, 2], 'S', colour)]), EVERY), refuses: true },
          { role: 'grid colour', build: chart(kind, BAR_DATA, { ...EVERY, grid: { show: true, color: colour } }), refuses: true },
        ]
      : [
          { role: 'slice colour', build: chart(kind, [slice('a', 3, colour), slice('b', 1)], EVERY), refuses: true },
          { role: 'grid colour', build: chart(kind, PIE_DATA, { ...EVERY, grid: { show: true, color: colour } }), refuses: false },
        ]

  it.each(EVERY_KIND.flatMap(kind => shapes(kind, UNREADABLE).map(({ role, refuses }) => ({ kind, role, refuses }))))(
    'a $kind chart with an unreadable $role',
    ({ kind, role, refuses }) => {
      const unreadable = shapes(kind, UNREADABLE).find(one => one.role === role)
      const readable = shapes(kind, '#336699').find(one => one.role === role)
      if (unreadable === undefined || readable === undefined) throw new Error(`no ${role} shape for a ${kind} chart`)
      expect(
        encode({ name: `readable/${kind}/${role}`, kind, build: readable.build }).refused,
        'the same chart with a colour that reads also refuses, so the colour is not what this measures',
      ).toBe(false)
      expect(encode({ name: `unreadable/${kind}/${role}`, kind, build: unreadable.build }).refused).toBe(refuses)
    },
  )
})

/**
 * A label that spells the mark does not move the slice, since a node's name is
 * encoded before its subtree's strings. Measured: a same-length label that does not
 * spell the mark slices to the same length, where a slice from the label is shorter.
 */
describe('a label spelling the mark does not move the slice', () => {
  const decoy: Record<ChartType, string> = { bar: 'qqq qqqqq', line: 'qqqq qqqqq', pie: 'qqq qqqqq', doughnut: 'qqqqqqqq qqqqx' }

  it.each(EVERY_KIND)('on a %s chart', kind => {
    const text = MARK[kind]
    expect(Buffer.byteLength(decoy[kind]), 'the decoy label must be the same length as the mark').toBe(Buffer.byteLength(text))
    const build = (label: string) =>
      kind === 'bar' || kind === 'line'
        ? chart(kind, cartesian([label, 'b'], [series([1, 2], label)]), EVERY)
        : chart(kind, [slice(label, 3), slice('b', 1)], EVERY)
    const colliding = encode({ name: `collide/${kind}`, kind, build: build(text) })
    const plain = encode({ name: `decoy/${kind}`, kind, build: build(decoy[kind]) })
    if (colliding.refused || plain.refused) throw new Error(`the probe chart for ${kind} refused`)
    expect(colliding.length, 'the slice began at the label rather than at the chart').toBe(plain.length)
  })
})

/**
 * The framing matches the pinned assets. This file's own asset would move with a
 * framing change and still pass, so the bar chart `chart.agreement.test.ts` pins is
 * checked against the bytes that file committed.
 */
describe('the framing agrees with the pinned assets', () => {
  it('encodes the pinned bar chart to the bytes the agreement asset carries', () => {
    const pinned = readFileSync(fileURLToPath(new URL('../../../crates/meo-canvas/tests/assets/chart/bar-bytes.txt', import.meta.url)), 'utf8').trim()
    expect(pinned).not.toBe('')
    const theirs = fromTheChart(Buffer.from(pinned, 'hex'), MARK.bar).toString('hex')
    const mine = encode({
      name: 'framing/bar',
      kind: 'bar',
      build: chart(
        'bar',
        cartesian(['a', 'b'], [series([1, 2], 'Sales', '#3366cc'), series([2, 1])]),
        {
          showLabels: true,
          showValues: true,
          showYAxis: true,
          grid: { show: true, color: '#e0e0e0' },
          labelFontSize: 11,
          valueFontSize: 10,
          yAxisFontSize: 9,
          labelColor: '#112233',
          valueColor: '#445566',
          yAxisColor: '#778899',
        },
        'Fixture',
      ),
    })
    if (mine.refused) throw new Error('the pinned bar chart refused to build')
    expect(mine.hex).toBe(theirs)
  })
})
