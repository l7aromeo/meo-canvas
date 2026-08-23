import { describe, expect, it } from 'vitest'

import { Chart, type ChartType } from './chart.js'
import { Box, Root } from './index.js'

/**
 * What a chart actually draws, as opposed to what tree it builds.
 *
 * # Why this file exists
 *
 * `chart.test.ts` has eighty-odd assertions and **not one of them goes through
 * a render.** They assert the tree — that a node exists, that a prop is set —
 * and a builder emitting a perfectly shaped tree that draws the wrong picture
 * passes every one of them.
 *
 * Every geometry number in `chart.ts` was in fact verified by rendering, and
 * **none of those renders was kept.** That is the `borders-per-edge` shape
 * with the roles reversed: geometry certified from a render and then
 * discarded, so nothing could notice it changing. **A measurement that lives
 * outside the repository is indistinguishable from one never taken.**
 *
 * # Why it matters more here than elsewhere
 *
 * **Chrome has no charts.** Nothing external adjudicates any of this, so the
 * arithmetic in `chart.ts` *is* the specification — and the Rust port is being
 * derived from that file rather than from v1. If a number there is wrong, the
 * port reproduces it faithfully and a byte comparison confirms two surfaces
 * agreeing on a wrong picture. **A shared reference with no check is a single
 * point of failure that looks like agreement.**
 *
 * # Per kind, not per behaviour
 *
 * The gridlines, the gutter and the label strip each serve several kinds
 * through one helper. Covering one caller is not covering the helper's
 * callers: an hour before this file was written, the line chart's label strip
 * was untested while the bar's identical code was covered, and a formatter
 * that worked on bars and threw on lines would have passed everything.
 */

/** The test font, so a label's width is the same on every machine. */
const FONT = new URL('../../../crates/meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf', import.meta.url).pathname

/** One rendered page, as `[r, g, b, a]` rows. */
interface Shot {
  readonly width: number
  readonly height: number
  at(x: number, y: number): [number, number, number, number]
}

/**
 * Renders a chart on a white page.
 *
 * **Throws rather than skips when the addon is missing.** A render test that
 * quietly does not run is the same silence this file exists to remove, and it
 * reads as coverage.
 */
async function shot(width: number, height: number, chart: ReturnType<typeof Chart>): Promise<Shot> {
  let raw: Buffer
  try {
    const canvas = await Root({
      width,
      height,
      backgroundColor: '#ffffff',
      fonts: [{ family: 'Fixture', paths: [FONT] }],
      children: chart,
    })
    raw = (await canvas.toBuffer('raw')) as Buffer
  } catch (cause) {
    throw new Error('the addon is not built; run `just addon`. These are the only chart checks that go through the renderer.', { cause })
  }
  return {
    width,
    height,
    at(x, y) {
      const at = (y * width + x) * 4
      return [raw[at] as number, raw[at + 1] as number, raw[at + 2] as number, raw[at + 3] as number]
    },
  }
}

/** Every run of columns whose column contains a pixel passing `ink`. */
function columnRuns(page: Shot, ink: (r: number, g: number, b: number) => boolean): [number, number][] {
  const runs: [number, number][] = []
  let start: number | null = null
  for (let x = 0; x <= page.width; x += 1) {
    let hit = false
    for (let y = 0; y < page.height && !hit; y += 1) {
      const [r, g, b, a] = page.at(Math.min(x, page.width - 1), y)
      hit = x < page.width && a === 255 && ink(r, g, b)
    }
    if (hit && start === null) start = x
    if (!hit && start !== null) {
      runs.push([start, x - 1])
      start = null
    }
  }
  return runs
}

/** The rows in one column that pass `ink`. */
function rowsIn(page: Shot, x: number, ink: (r: number, g: number, b: number) => boolean): number[] {
  const out: number[] = []
  for (let y = 0; y < page.height; y += 1) {
    const [r, g, b, a] = page.at(x, y)
    if (a === 255 && ink(r, g, b)) out.push(y)
  }
  return out
}

const BLUE = (r: number, g: number, b: number) => r < 110 && g > 70 && g < 140 && b > 160
const GREY = (r: number, g: number, b: number) => Math.abs(r - 224) < 14 && Math.abs(g - 224) < 14 && Math.abs(b - 224) < 14

const cartesian = { labels: ['a', 'b'], datasets: [{ data: [1, 2], color: '#3366cc' }] }

describe('a bar chart draws where the arithmetic says', () => {
  // groupWidth = 1/2, spacing = 0.2 * groupWidth = 0.1, barWidth = 0.4.
  // On a 200-wide plot: bar 0 at 0.05 -> x 10, width 80; bar 1 at 0.55 -> 110.
  it('puts each bar at its computed x, with its computed width', async () => {
    const page = await shot(200, 120, Chart({ type: 'bar', data: cartesian, fontFamily: 'Fixture' }))
    expect(columnRuns(page, BLUE)).toEqual([
      [10, 89],
      [110, 189],
    ])
  })

  // height = value / maxValue, anchored to the bottom. Values 1 and 2 against
  // a maximum of 2 give half the plot and all of it.
  it('scales each bar by the largest value across every series', async () => {
    const page = await shot(200, 120, Chart({ type: 'bar', data: cartesian, fontFamily: 'Fixture' }))
    const short = rowsIn(page, 40, BLUE)
    const tall = rowsIn(page, 140, BLUE)
    expect(tall.length).toBe(page.height)
    // Half, to within the pixel a boundary can fall either side of.
    expect(Math.abs(short.length - tall.length / 2)).toBeLessThanOrEqual(1)
    // Anchored to the bottom: the shorter bar reaches the last row.
    expect(short[short.length - 1]).toBe(page.height - 1)
  })
})

describe('gridlines, in every kind that draws them', () => {
  // gridLines() is `i / 5` for i in 0..5 — six lines, five equal bands. The
  // helper serves bar and line, so both are asked.
  it.each([
    ['bar', { type: 'bar' as ChartType, data: cartesian }],
    ['line', { type: 'line' as ChartType, data: cartesian }],
  ])('divides a %s plot into five equal bands', async (_kind, props) => {
    const page = await shot(200, 120, Chart({ ...props, fontFamily: 'Fixture', options: { grid: { show: true } } } as never))
    const rows = rowsIn(page, 4, GREY)
    // **Five, not six.** `gridLines()` returns six fractions and the last is
    // `1.0`, which puts a one-pixel rule with its top on the plot's bottom
    // edge — one row past the last row there is. v1 does the same: it strokes
    // at `chartY + finalChartHeight`, which is equally outside. So the bottom
    // line is drawn and never seen, in both engines, and this asserts what is
    // visible rather than what was emitted.
    expect(rows).toHaveLength(5)
    const gaps = rows.slice(1).map((row, index) => row - (rows[index] as number))
    // Even to within a pixel: 120 does not divide by five into whole numbers.
    expect(Math.max(...gaps) - Math.min(...gaps)).toBeLessThanOrEqual(1)
  })
})

describe('a pie is solid and a doughnut is not', () => {
  const data = [
    { label: 'a', value: 1, color: '#cc3333' },
    { label: 'b', value: 3, color: '#3366cc' },
  ]

  // **A single pixel cannot tell these apart.** v1 strokes every slice white
  // and two slices meet at the centre, so the exact centre is white in both.
  // The discriminator is the coloured share of a disc.
  it.each([
    ['pie', 'pie' as ChartType, 0.9],
    ['doughnut', 'doughnut' as ChartType, 0],
  ])('%s fills its centre', async (_kind, type, atLeast) => {
    const page = await shot(200, 120, Chart({ type, data, fontFamily: 'Fixture' } as never))
    const ink = (r: number, g: number, b: number) => (r > 150 && g < 90 && b < 90) || (r < 90 && g > 60 && g < 130 && b > 150)
    let coloured = 0
    let seen = 0
    for (let dy = -20; dy <= 20; dy += 1) {
      for (let dx = -20; dx <= 20; dx += 1) {
        if (dx * dx + dy * dy > 400) continue
        seen += 1
        const [r, g, b, a] = page.at(100 + dx, 60 + dy)
        if (a === 255 && ink(r, g, b)) coloured += 1
      }
    }
    const share = coloured / seen
    if (atLeast > 0) expect(share).toBeGreaterThan(atLeast)
    else expect(share).toBe(0)
  })

  it('draws a circle rather than an ellipse in a wide box', async () => {
    const page = await shot(240, 120, Chart({ type: 'pie', data, fontFamily: 'Fixture' }))
    const ink = (r: number, g: number, b: number) => (r > 150 && g < 90 && b < 90) || (r < 90 && g > 60 && g < 130 && b > 150)
    // The extremes of the ink in both directions, rather than one column and
    // one row: a column through the centre crosses the white stroke where two
    // slices meet, so a single column understates the height.
    let left = page.width
    let right = -1
    let top = page.height
    let bottom = -1
    for (let y = 0; y < page.height; y += 1) {
      for (let x = 0; x < page.width; x += 1) {
        const [r, g, b, a] = page.at(x, y)
        if (a === 255 && ink(r, g, b)) {
          if (x < left) left = x
          if (x > right) right = x
          if (y < top) top = y
          if (y > bottom) bottom = y
        }
      }
    }
    // `min(w, h)` keeps it round: as wide as it is tall, within a pixel.
    expect(Math.abs(right - left - (bottom - top))).toBeLessThanOrEqual(1)
  })
})

describe('a line chart fills its box and keeps its pen', () => {
  const rising = { labels: ['a', 'b', 'c'], datasets: [{ data: [0, 2, 1], color: '#3366cc' }] }

  // `preserveAspectRatio: 'none'` is the one place a chart needs it: a plot
  // must fill its box and `meet` would letterbox it.
  it('reaches every edge of a box that is not square', async () => {
    const page = await shot(240, 100, Chart({ type: 'line', data: rising, fontFamily: 'Fixture' }))
    const runs = columnRuns(page, BLUE)
    expect(runs[0]?.[0]).toBe(0)
    expect(runs[runs.length - 1]?.[1]).toBe(page.width - 1)
    // And vertically: the series peaks at its maximum and rests at zero, so
    // the ink has to reach both the first row and the last. Not at the edges —
    // the peak is in the middle — which is why this asks the whole page.
    let top = page.height
    let bottom = -1
    for (let y = 0; y < page.height; y += 1) {
      for (let x = 0; x < page.width; x += 1) {
        const [r, g, b, a] = page.at(x, y)
        if (a === 255 && BLUE(r, g, b)) {
          if (y < top) top = y
          if (y > bottom) bottom = y
        }
      }
    }
    expect(top).toBeLessThanOrEqual(1)
    expect(bottom).toBeGreaterThanOrEqual(page.height - 2)
  })

  // The stretch is 2.4:1 here. A pen scaled with the geometry would come out
  // wider on one axis than the other; ours is transformed as a path and
  // stroked afterwards, so it stays the width the caller asked for.
  it('does not thicken its stroke under a non-uniform stretch', async () => {
    const page = await shot(240, 100, Chart({ type: 'line', data: rising, fontFamily: 'Fixture' }))
    // Read across the steep rise, away from the point markers.
    const width = rowsIn(page, 60, BLUE).length
    expect(width).toBeGreaterThan(0)
    expect(width).toBeLessThanOrEqual(4)
  })
})

describe('the y-axis gutter measures its widest label', () => {
  // Both cartesian kinds share the gutter, so both are asked. A gutter that
  // stated a width would inset the plot by the same amount for both.
  it.each([['bar' as ChartType], ['line' as ChartType]])('insets a %s plot by what the labels take', async type => {
    const narrow = { labels: ['a', 'b'], datasets: [{ data: [1, 2], color: '#3366cc' }] }
    const wide = { labels: ['a', 'b'], datasets: [{ data: [7500, 10000], color: '#3366cc' }] }
    const options = { showYAxis: true }
    const small = await shot(240, 120, Chart({ type, data: narrow, fontFamily: 'Fixture', options } as never))
    const large = await shot(240, 120, Chart({ type, data: wide, fontFamily: 'Fixture', options } as never))
    const leftOf = (page: Shot) => (columnRuns(page, BLUE)[0] as [number, number])[0]
    expect(leftOf(large)).toBeGreaterThan(leftOf(small))
  })
})

describe('these measurements can fail', () => {
  // **A control, and the reason it is here rather than in a mutation run.**
  // Every assertion above passed the first time it was correct, which says
  // nothing about whether it would notice a wrong picture. Mutating a constant
  // in `chart.ts` would prove it and would move a file someone else is reading,
  // so the discrimination is proved against a hand-built tree instead: the same
  // shapes at deliberately wrong positions, measured by the same helpers.
  //
  // If this ever passes without the wrong tree differing from the right one,
  // the helpers have stopped measuring and every assertion above is decoration.
  it('reports different geometry for a deliberately wrong bar layout', async () => {
    const right = await shot(200, 120, Chart({ type: 'bar', data: cartesian, fontFamily: 'Fixture' }))
    const wrong = await shot(
      200,
      120,
      Box({
        width: '100%',
        height: '100%',
        positionType: 'relative',
        children: [
          // The same two bars, moved a quarter of the plot to the right and
          // made half as wide — a chart that is still plausibly a chart.
          Box({ positionType: 'absolute', position: { left: '30%', bottom: 0 }, width: '20%', height: '50%', backgroundColor: '#3366cc' }),
          Box({ positionType: 'absolute', position: { left: '75%', bottom: 0 }, width: '20%', height: '100%', backgroundColor: '#3366cc' }),
        ],
      }) as never,
    )
    expect(columnRuns(wrong, BLUE)).not.toEqual(columnRuns(right, BLUE))
  })

  it('reports a solid centre as different from a hollow one', async () => {
    // The pie and doughnut assertion rests on a disc's coloured share. If that
    // measure could not separate them, both would read the same here.
    const data = [
      { label: 'a', value: 1, color: '#cc3333' },
      { label: 'b', value: 3, color: '#3366cc' },
    ]
    const ink = (r: number, g: number, b: number) => (r > 150 && g < 90 && b < 90) || (r < 90 && g > 60 && g < 130 && b > 150)
    const share = async (type: ChartType) => {
      const page = await shot(200, 120, Chart({ type, data, fontFamily: 'Fixture' } as never))
      let coloured = 0
      for (let dy = -20; dy <= 20; dy += 1) {
        for (let dx = -20; dx <= 20; dx += 1) {
          if (dx * dx + dy * dy > 400) continue
          const [r, g, b, a] = page.at(100 + dx, 60 + dy)
          if (a === 255 && ink(r, g, b)) coloured += 1
        }
      }
      return coloured
    }
    expect(await share('pie')).toBeGreaterThan(await share('doughnut'))
  })
})

describe('the label strip centres each label in its slot', () => {
  // **The case that justifies this whole file.** Both surfaces set
  // `alignItems: 'center'` on a row, where `align-items` is the cross axis —
  // so the labels centred vertically and sat against their slots' left edges.
  // Measured before the fix on a 200-wide chart: ink at x 2 and x 102 where
  // the slot centres are 50 and 150.
  //
  // **No byte comparison could see it**, because both surfaces were wrong in
  // the same way, and no geometry row covers it. A pixel is the only
  // instrument that could ever have caught it.
  it.each([['bar' as ChartType], ['line' as ChartType]])('centres a %s chart s labels', async type => {
    const page = await shot(200, 120, Chart({ type, fontFamily: 'Fixture', data: cartesian, options: { showLabels: true } } as never))
    // The strip's own band, below the plot, so bar ink cannot be mistaken for
    // label ink.
    const dark = (r: number, g: number, b: number) => r < 90 && g < 90 && b < 90
    const runs: [number, number][] = []
    let start: number | null = null
    for (let x = 0; x <= page.width; x += 1) {
      let ink = false
      for (let y = page.height - 12; y < page.height && !ink; y += 1) {
        const [r, g, b, a] = page.at(Math.min(x, page.width - 1), y)
        ink = x < page.width && a === 255 && dark(r, g, b)
      }
      if (ink && start === null) start = x
      if (!ink && start !== null) {
        runs.push([start, x - 1])
        start = null
      }
    }
    expect(runs).toHaveLength(2)
    // Two labels across 200 put the slot centres at 50 and 150.
    for (const [slot, [from, to]] of [50, 150].map((centre, index) => [centre, runs[index] as [number, number]] as const)) {
      expect(Math.abs((from + to) / 2 - slot)).toBeLessThanOrEqual(3)
    }
  })
})
