import { createRequire } from 'node:module'
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Chart } from './chart.js'
import { Box } from './node.js'
import { encodeScene } from './arena.js'

/**
 * The same chart, built twice, compared as bytes.
 *
 * # Why bytes and why two implementations
 *
 * **For `Chart` there is no external adjudicator.** Chrome has no charts, v1 is
 * both baselines, and the arithmetic is the specification. So the strongest
 * check available is that **two independent implementations produce the same
 * scene** — and if they differ, one is wrong and the comparison says so
 * without either being trusted.
 *
 * # What this closes, and what it does not
 *
 * **It closes the port and not the geometry.** Both surfaces agreeing on a
 * wrong bar edge passes every byte of this file. What guards the numbers is
 * the rendering: `chart.render.test.ts` on this side and the equivalent on the
 * Rust side, which check the arithmetic against pixels rather than against
 * itself. **Three checks, three different questions, and none substitutes for
 * another.**
 *
 * # Why a comparison to four decimals is safe here
 *
 * The pie's path data is formatted with `toFixed(4)`, and the two languages
 * round halfway cases differently — JavaScript away from zero, Rust to even.
 * **A tie at four decimals needs an odd multiple of `1/20000`, and
 * `20000 = 2^5 x 625` is not a power of two, so no binary float lands on
 * one.** The only transcendentals in chart geometry are `sin` and `cos`, which
 * were measured bit-identical between V8 and libm in the animation work;
 * `exp`, the one known to differ, does not appear.
 *
 * # The first disagreement of a new case is usually the harness
 *
 * **Twice now it has been a font family set on one side only.** The first
 * agreement run ever differed by `fontFamily` and by a page node this side
 * added; the three cases added later differed the same way, because bar's
 * options were copied onto cases whose Rust spec has none. Neither was an
 * implementation. **Check the two option bags field by field before reporting
 * a defect** -- a real disagreement survives that check and a harness
 * asymmetry does not.
 *
 * # Why the asset is checked in rather than written here
 *
 * `ci` runs the Rust tests **before** the JavaScript ones. A test that wrote
 * the asset on every run would leave the Rust side comparing against whatever
 * the previous run produced — the stale-artifact trap, with the staleness
 * created by the suite itself. So the bytes are committed, both sides assert
 * against them, and a legitimate change to the chart is a deliberate
 * regeneration: `UPDATE_CHART_BYTES=1 npx vitest run chart.agreement`.
 */
// `fileURLToPath`, not `.pathname`. A file URL's pathname on Windows is
// `/D:/a/...`, and handing that to anything that resolves paths prepends the
// current drive: `D:\D:\a\...`, ENOENT, in the three test files that read an
// asset from disk and nowhere else. Nine other files here already did this
// correctly; these three were the ones that had never run on Windows.
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
 * The options every case switches on.
 *
 * **A default is a branch neither surface takes**, so an option left alone is
 * one the comparison never sees: the two agree about it trivially and the row
 * reads as coverage it is not.
 *
 * Three options on this surface are deliberately absent and cannot be added
 * here: `yAxisLabelFormatter`, `xAxisLabelFormatter` and the three
 * `render*Item` hooks are **functions**, and a function has no counterpart to
 * compare against on the other side. They are guarded by the tree tests
 * instead. Naming them is the point — otherwise "every option switched on"
 * reads as complete.
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

/** Whole numbers throughout. A pie legend entry reads `label (value)` and
 * formats the value **unrounded**, so `1` and `1.0` would be spelled `1` here
 * and `1` there only by luck; whole numbers keep the two languages'
 * number-to-string rules out of the comparison. They part company at `>= 1e21`
 * and `< 1e-6`, and nowhere a chart will go. */
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
 * The four charts, and the node each must contain to have had a subject.
 *
 * **A legend position per case**, because the frame branches on it —
 * `left` stacks and `top`/`bottom` run along, so both directions are byte
 * checked. Bar carries no legend at all, which is the case the other three
 * cannot cover.
 *
 * **`mark` is the empty-scene guard.** A byte comparison that passes says the
 * two ports agree; it says nothing about whether either drew anything, and an
 * agreement between two nothings is an agreement. Each case names a node only
 * a drawn chart has and asserts it is there before asserting the bytes match.
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
    // **The fourth frame branch, and the only one nothing compared.** `left`,
    // `top` and `bottom` each ride on a kind above; `right` rode on nothing,
    // and bar carries no legend at all. Deliberately the **same** chart as the
    // line case with one property changed, so a disagreement here is the
    // branch and cannot be the data.
    //
    // Verified to render before it was pinned, rather than pinned because it
    // was missing: with no legend the plot spans 216px, and with the legend
    // `left` or `right` it spans 176 either way -- symmetric, and the legend
    // takes its side. It was uncovered, not broken.
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
    // **The five function-valued options, which neither suite reached.** A
    // function cannot be encoded, so what is compared is its *effect*: the
    // same formatter and the same hatch on both surfaces must produce the
    // same tree.
    //
    // The two formatters round before they stringify. `3` and `2.4` are not
    // the risk -- **`Display` and JavaScript's number-to-string part company**
    // at the ends of the range, and a y-axis division is exactly the kind of
    // value that arrives as `2.4000000000000004`. Rounding first keeps the
    // languages' spelling rules out of a comparison that is about the hook.
    //
    // Every hatch takes its index into the node it returns, so a case that
    // called them in the wrong order, or called one of them once, would not
    // encode to the same bytes as one that did not.
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
    // v1's `outerRadius * (innerRadius ?? 0.6)`, and 0.6 is what this surface
    // passes when the caller says nothing — **not the Rust `pie()` default,
    // which has none.** Written out rather than left off, so the two sides are
    // agreeing about a stated number instead of about two defaults.
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
  // Wrapped, because `Root::new(200, 120)` on the Rust side contributes a page
  // root of its own -- so a chart used directly as the page would be one node
  // short and every byte after the count would shift. **The first run differed
  // by exactly that node and by a font family set on one side only: both were
  // the harness, neither was an implementation.**
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

  // And the four are four different pictures rather than one repeated, which
  // is what a copied case would look like from here.
  it('gives the four kinds four different byte strings', () => {
    const encoded = CASES.map(one => encode(one.chart()).hex)
    expect(new Set(encoded).size).toBe(CASES.length)
  })
})
