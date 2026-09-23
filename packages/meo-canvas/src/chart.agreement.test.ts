import { createRequire } from 'node:module'
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Chart } from './chart.js'
import { Box } from './node.js'
import { encodeScene } from './arena.js'

/**
 * The same chart, built on both surfaces, compared as bytes: two independent
 * implementations agreeing is the check a chart has, since no browser draws one.
 * It closes the port, not the geometry (`chart.render.test.ts`). A new case's first
 * mismatch is usually two option bags differing; compare them before a defect.
 */
// `fileURLToPath`, not `.pathname`: on Windows the pathname is `/D:/a/...`, and a
// path resolver prepends the drive again, giving `D:\D:\a\...`.
const asset = (kind: string) => fileURLToPath(new URL(`../../../crates/meo-canvas/tests/assets/chart/${kind}-bytes.txt`, import.meta.url))

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

/**
 * The options every case switches on, since an option left at its default is a
 * branch neither surface takes. The five function-valued options have no bytes to
 * compare; the `hatches` case compares their effect instead.
 */
const EVERY_OPTION = {
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
  yAxisColor: '#778899',
} as const

/**
 * Whole numbers throughout: a pie legend spells its value unrounded, and whole
 * numbers keep the two languages' number-to-string rules, which part at `>= 1e21`
 * and `< 1e-6`, out of the comparison.
 */
const CARTESIAN = {
  labels: ['a', 'b', 'c'],
  datasets: [{ data: [1, 3, 2], label: 'Sales', color: '#3366cc' }, { data: [3, 1, 2] }],
}

/** Colours on the first and third only, so the palette fallback for the second
 * is inside the comparison rather than beside it. */
const SLICES = [
  { label: 'a', value: 3, color: '#3366cc' },
  { label: 'b', value: 2 },
  { label: 'c', value: 1, color: '#cc6633' },
]

/**
 * The cases, and the node each must contain to have had a subject: an agreement
 * between two empty scenes is still an agreement. Each carries a legend position,
 * since the frame branches on it; bar carries none.
 */
const CASES = [
  {
    kind: 'bar',
    mark: 'bar 0.0',
    // Bar keeps the options it was first pinned with and no legend, so its
    // committed bytes do not move for a change that is about the other three.
    chart: () =>
      Chart({
        type: 'bar',
        fontFamily: 'Fixture',
        data: {
          labels: ['a', 'b'],
          datasets: [{ data: [1, 2], label: 'Sales', color: '#3366cc' }, { data: [2, 1] }],
        },
        options: {
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
      }),
  },
  {
    kind: 'line',
    mark: 'point 0.0',
    chart: () =>
      Chart({
        type: 'line',
        data: CARTESIAN,
        options: { ...EVERY_OPTION, legendPosition: 'left' },
      }),
  },
  {
    // The `right` frame branch, on the line case's chart with one property changed,
    // so a disagreement is the branch and not the data. With a side legend the plot
    // spans 176px either way, against 216 without one.
    kind: 'line-legend-right',
    mark: 'point 0.0',
    chart: () =>
      Chart({
        type: 'line',
        data: CARTESIAN,
        options: { ...EVERY_OPTION, legendPosition: 'right' },
      }),
  },
  {
    // The five function-valued options, compared by effect: the same formatter and
    // hatch on both surfaces must build the same tree. The formatters round before
    // stringifying, and each hatch puts its index in its node, so a wrong call
    // order encodes differently.
    kind: 'hatches',
    mark: 'bar 0.0',
    chart: () =>
      Chart({
        type: 'bar',
        data: CARTESIAN,
        options: {
          ...EVERY_OPTION,
          legendPosition: 'bottom',
          xAxisLabelFormatter: (label: string, index: number) => `${label}#${index}`,
          yAxisLabelFormatter: (value: number) => `$${Math.round(value)}`,
          renderLabelItem: ({ index }: { item: string; index: number }) =>
            Box({ width: 4 + index, height: 4, backgroundColor: '#ff0000', name: `hatch label ${index}` }),
          renderValueItem: ({ index, datasetIndex }: { item: number; index: number; datasetIndex: number }) =>
            Box({ width: 3, height: 3, backgroundColor: '#00ff00', name: `hatch value ${index}.${datasetIndex}` }),
          renderLegendItem: ({ index, color }: { index: number; color: string }) =>
            Box({ width: 6, height: 6, backgroundColor: color, name: `hatch legend ${index}` }),
        } as never,
      }),
  },
  {
    // Path data is compared at `toFixed(4)`, where JavaScript rounds ties away from
    // zero and Rust to even; a tie needs an odd multiple of 1/20000, and no binary
    // float is one.
    kind: 'pie',
    mark: 'slice 0',
    chart: () =>
      Chart({
        type: 'pie',
        data: SLICES,
        options: { ...EVERY_OPTION, legendPosition: 'top' },
      }),
  },
  {
    // `axisColor`, reached only when `yAxisColor` is absent, which `EVERY_OPTION`
    // never is. The bag is written out, as the Rust side's is, and the colour is
    // neither `#778899` nor the chain's `#000000`, so a wrong arm encodes differently.
    kind: 'axis-fallback',
    mark: 'bar 0.0',
    chart: () =>
      Chart({
        type: 'bar',
        data: CARTESIAN,
        options: {
          showLabels: true,
          showValues: true,
          showYAxis: true,
          showLegend: true,
          legendPosition: 'bottom',
          grid: { show: true, color: '#e0e0e0' },
          labelFontSize: 11,
          valueFontSize: 10,
          yAxisFontSize: 9,
          labelColor: '#112233',
          valueColor: '#445566',
          axisColor: '#22cc88',
        },
      }),
  },
  {
    // **`innerRadius` at a value the caller chose.** The doughnut case below
    // passes `0.6`, which is this surface's own fallback and the Rust
    // builder's default -- so the two agree about a number neither was told.
    // `0.35` is told to both.
    kind: 'doughnut-inner',
    mark: 'slice 0',
    chart: () =>
      Chart({
        type: 'doughnut',
        data: SLICES,
        options: { ...EVERY_OPTION, legendPosition: 'bottom', innerRadius: 0.35 },
      }),
  },
  {
    // 0.6 is the default on both surfaces; written out so the two agree about a
    // stated number rather than about two defaults.
    kind: 'doughnut',
    mark: 'slice 0',
    chart: () =>
      Chart({
        type: 'doughnut',
        data: SLICES,
        options: { ...EVERY_OPTION, legendPosition: 'bottom', innerRadius: 0.6 },
      }),
  },
] as const

/** Encodes one chart the way the page would, and reports what it named. */
function encode(chart: ReturnType<typeof Chart>): { hex: string; names: readonly string[] } {
  // Wrapped, because Rust's `Root::new(200, 120)` adds a page root of its own; a
  // chart used directly as the page would be one node short and shift every byte.
  const arena = encodeScene([Box({ children: chart })], 200, 120, false, 1)
  const values = arena.values.map(value => (typeof value === 'string' ? value : Buffer.from(value)))
  return {
    hex: addon().sceneBytes(arena.slots, values).toString('hex'),
    names: values.filter((value): value is string => typeof value === 'string'),
  }
}

describe('the two chart implementations agree', () => {
  it.each(CASES)('produces the bytes the Rust side checks a $kind against', ({ kind, mark, chart }) => {
    const { hex, names } = encode(chart())

    // Before the agreement, the subject. A chart that built nothing encodes to
    // bytes that would match another chart that built nothing.
    expect(names).toContain(mark)

    if (process.env['UPDATE_CHART_BYTES'] === '1') {
      writeFileSync(asset(kind), `${hex}\n`)
    }

    const committed = readFileSync(asset(kind), 'utf8').trim()
    expect(committed).not.toBe('')
    expect(hex).toBe(committed)
  })

  // The control this file needs: bytes that differ when the chart differs.
  // Without it, a comparison against a constant would pass for a scene that
  // encoded to nothing.
  it('produces different bytes for a different chart', () => {
    const other = Chart({ type: 'bar', data: { labels: ['a'], datasets: [{ data: [1] }] } })
    expect(encode(other).hex).not.toBe(encode(CASES[0].chart()).hex)
  })

  // And the cases are that many different pictures rather than one repeated,
  // which is what a copied case would look like from here. Counted from
  // `CASES` rather than written as a number -- the name said "four" while six
  // were running, which is the shape of claim this file exists to refuse.
  it('gives every case its own byte string', () => {
    const encoded = CASES.map(one => encode(one.chart()).hex)
    expect(new Set(encoded).size).toBe(CASES.length)
  })
})
