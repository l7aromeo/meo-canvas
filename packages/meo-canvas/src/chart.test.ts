import { describe, expect, it } from 'vitest'

import { BAR_GROUP_SPACING, barLayout, Chart, GRID_DIVISIONS, gridLines, linePath, linePoints, seriesColor, sliceAngles, slicePath } from './chart.js'

describe('the bar geometry, which is the only reference there is', () => {
  // Chrome has no charts, so nothing external adjudicates these. The numbers
  // are v1's arithmetic expressed as fractions of the plot area, and this
  // block is where they are checked against the formula rather than against a
  // picture.
  it('divides the width the way v1 divides it', () => {
    // v1: groupWidth = chartWidth / labels.length; barSpacing = groupWidth * 0.2
    //     barWidth = (groupWidth - barSpacing) / datasets.length
    // Two labels, two series: groupWidth 0.5, spacing 0.1, barWidth 0.2.
    const placed = barLayout(
      2,
      2,
      [
        [1, 2],
        [2, 4],
      ],
      4,
    )
    expect(placed).toHaveLength(2)
    expect(placed[0]?.[0]?.width).toBeCloseTo(0.2, 12)
    // First bar of the first group starts half a spacing in.
    expect(placed[0]?.[0]?.x).toBeCloseTo(0.05, 12)
    // Second series sits one bar-width along.
    expect(placed[0]?.[1]?.x).toBeCloseTo(0.25, 12)
    // Second group starts a whole groupWidth along.
    expect(placed[1]?.[0]?.x).toBeCloseTo(0.55, 12)
  })

  it('scales height by the largest value across every series', () => {
    const placed = barLayout(
      2,
      2,
      [
        [1, 2],
        [2, 4],
      ],
      4,
    )
    expect(placed[0]?.[0]?.height).toBeCloseTo(0.25, 12)
    expect(placed[1]?.[1]?.height).toBeCloseTo(1, 12)
  })

  // A chart of all zeroes divides by zero in v1's `(value / maxValue)`. NaN
  // reaches layout as an absent height and draws nothing, which reads as a
  // chart that failed rather than one with no data to show.
  it('survives a maximum of zero without producing NaN', () => {
    const placed = barLayout(2, 1, [[0, 0]], 0)
    expect(placed[0]?.[0]?.height).toBe(0)
    expect(Number.isNaN(placed[0]?.[0]?.height)).toBe(false)
  })

  it('the bars of one group occupy the middle 80% of their slot', () => {
    const placed = barLayout(1, 1, [[1]], 1)
    const bar = placed[0]?.[0]
    expect(bar?.x).toBeCloseTo(BAR_GROUP_SPACING / 2, 12)
    expect(bar?.width).toBeCloseTo(1 - BAR_GROUP_SPACING, 12)
  })

  it('draws both edges of the plot, so five bands need six lines', () => {
    const lines = gridLines()
    expect(lines).toHaveLength(GRID_DIVISIONS + 1)
    expect(lines[0]).toBe(0)
    expect(lines[lines.length - 1]).toBe(1)
  })

  it('gives each series a colour and repeats the palette rather than running out', () => {
    expect(seriesColor(0)).toBe(seriesColor(8))
    expect(seriesColor(0, '#123456')).toBe('#123456')
  })
})

describe('the tree a bar chart expands to', () => {
  const data = { labels: ['a', 'b'], datasets: [{ data: [1, 2] }] }

  it('is a plot area and nothing else when nothing is asked for', () => {
    const chart = Chart({ type: 'bar', data })
    expect(chart.children?.map(child => child.name)).toEqual(['plot'])
  })

  it('adds a label strip only when labels are asked for', () => {
    const chart = Chart({ type: 'bar', data, options: { showLabels: true } })
    expect(chart.children?.map(child => child.name)).toEqual(['plot', 'labels'])
  })

  it('puts a gridline at every division, both edges included', () => {
    const chart = Chart({ type: 'bar', data, options: { grid: { show: true } } })
    const plot = chart.children?.[0]
    const lines = plot?.children?.filter(child => child.name?.startsWith('gridline')) ?? []
    expect(lines).toHaveLength(GRID_DIVISIONS + 1)
  })

  // The hatch's node is PLACED, not measured and drawn — so it must appear in
  // the tree. A hatch whose node never reached the scene would be v1's
  // contract silently surviving behind an unchanged signature.
  it('places a value hatch node in the tree rather than measuring it away', () => {
    const marker = { kind: 'box' as const, name: 'mine' }
    const chart = Chart({
      type: 'bar',
      data,
      options: { showValues: true, renderValueItem: () => marker as never },
    })
    const plot = chart.children?.[0]
    const bar = plot?.children?.find(child => child.name === 'bar 0.0')
    const holder = bar?.children?.[0]
    expect(holder?.children?.[0]?.name).toBe('mine')
  })

  // The unbuilt-kind pin is gone because every kind is built. It moved from
  // `pie` to `line` as each landed and has nothing left to point at, which is
  // a pinned list emptying rather than a test deleted: `Chart` still throws
  // for an unknown type, and the four `ChartType` values are now exhaustive.
  it('names every kind it can draw', () => {
    for (const type of ['bar', 'line'] as const) {
      expect(() => Chart({ type, data: { labels: ['a'], datasets: [{ data: [1] }] } })).not.toThrow()
    }
    for (const type of ['pie', 'doughnut'] as const) {
      expect(() => Chart({ type, data: [{ label: 'a', value: 1 }] })).not.toThrow()
    }
  })
})

describe('a chart with no size given', () => {
  // Percentages resolve against the containing block, so a chart that shrank
  // to its content would lay every bar out against nothing. Measured: without
  // this the two bars of a 200px chart came out 9 pixels wide in total.
  it('fills its parent rather than shrinking to nothing', () => {
    const chart = Chart({ type: 'bar', data: { labels: ['a'], datasets: [{ data: [1] }] } })
    expect(chart.style?.width).toBe('100%')
    expect(chart.style?.height).toBe('100%')
  })

  it('still takes an explicit size when one is given', () => {
    const chart = Chart({ type: 'bar', data: { labels: ['a'], datasets: [{ data: [1] }] }, width: 320, height: 200 })
    expect(chart.style?.width).toBe(320)
    expect(chart.style?.height).toBe(200)
  })
})

describe('data v1 mis-draws', () => {
  // v1 has no zero baseline, so a negative value produces a negative height
  // drawn below the plot, and an all-negative series makes the MOST negative
  // value five times the plot tall while the least negative fills it. Three
  // silent wrong pictures; refusing is the honest answer.
  it('refuses a negative value rather than reproducing the mis-draw', () => {
    expect(() => barLayout(2, 1, [[-5, 10]], 10)).toThrow(/cannot draw a negative value/)
    expect(() => barLayout(2, 1, [[-5, -1]], -1)).toThrow(/cannot draw a negative value/)
  })

  it('draws an empty chart rather than nothing at all', () => {
    // v1 divides zero by zero here and lays out a NaN height.
    const placed = barLayout(1, 1, [[0]], 0)
    expect(placed[0]?.[0]?.height).toBe(0)
  })
})

describe('the pie geometry', () => {
  it('starts at twelve o clock and sweeps clockwise, as v1 does', () => {
    const [first] = sliceAngles([1, 1])
    expect(first?.start).toBeCloseTo(-Math.PI / 2, 12)
    expect(first?.end).toBeCloseTo(-Math.PI / 2 + Math.PI, 12)
  })

  it('divides the turn in proportion and closes it exactly', () => {
    const angles = sliceAngles([1, 3])
    expect(angles[0]!.end - angles[0]!.start).toBeCloseTo(Math.PI / 2, 12)
    expect(angles[1]!.end - angles[1]!.start).toBeCloseTo((3 / 4) * Math.PI * 2, 12)
    // The last slice ends exactly one turn from where the first began.
    expect(angles[1]!.end - angles[0]!.start).toBeCloseTo(Math.PI * 2, 12)
  })

  // The same divergence as the bar chart's zero maximum, for the same reason:
  // v1 divides by a zero total and produces NaN angles.
  it('gives empty slices for a zero total rather than NaN', () => {
    const angles = sliceAngles([0, 0])
    expect(angles.every(angle => angle.start === angle.end)).toBe(true)
    expect(angles.some(angle => Number.isNaN(angle.start))).toBe(false)
  })

  // Without the flag SVG draws the short way round, so a 270-degree slice
  // comes out as 90 — a wrong picture that looks like a legitimate chart.
  it('sets the large-arc flag once a slice passes half a turn', () => {
    const small = slicePath(0, Math.PI / 2, 50, 0)
    const big = slicePath(0, (3 / 2) * Math.PI, 50, 0)
    expect(small).toMatch(/A 50 50 0 0 1/)
    expect(big).toMatch(/A 50 50 0 1 1/)
  })

  it('draws a pie from the centre and a doughnut as a ring', () => {
    // A pie's path starts with a move to the centre and a line out; a
    // doughnut's starts on the outer arc and returns along the inner one.
    expect(slicePath(0, 1, 50, 0)).toMatch(/^M 50 50 L /)
    expect(slicePath(0, 1, 50, 30)).toMatch(/A 30 30 0 0 0/)
  })
})

describe('the tree a pie expands to', () => {
  const data = [
    { label: 'a', value: 1 },
    { label: 'b', value: 3 },
  ]

  it('is one path per slice, each in the whole drawing space', () => {
    const chart = Chart({ type: 'pie', data })
    const slices = chart.children?.[0]?.children ?? []
    expect(slices).toHaveLength(2)
    // Concentric because every slice shares one viewBox rather than being
    // scaled to its own bounds.
    expect(slices.every(slice => (slice.style as { viewBox?: readonly number[] })?.viewBox?.[2] === 100)).toBe(true)
  })

  it('names itself for the kind asked for', () => {
    expect(Chart({ type: 'pie', data }).name).toBe('pie chart')
    expect(Chart({ type: 'doughnut', data }).name).toBe('doughnut chart')
  })

  it('refuses a negative slice, as the bar geometry refuses a negative bar', () => {
    expect(() => Chart({ type: 'pie', data: [{ label: 'a', value: -1 }] })).toThrow(/negative value/)
  })
})

describe('the line geometry', () => {
  // v1 divides by `labels - 1` here and by `labels` for bars, so a line's
  // first and last points sit ON the plot's edges where a bar is inset in its
  // slot. Getting this wrong insets the whole series by half a slot and looks
  // plausible.
  it('spans edge to edge, unlike the bars', () => {
    const points = linePoints(3, [0, 2, 1], 2)
    expect(points[0]?.x).toBeCloseTo(0, 12)
    expect(points[1]?.x).toBeCloseTo(0.5, 12)
    expect(points[2]?.x).toBeCloseTo(1, 12)
  })

  it('measures y downward from the top, as a plot does', () => {
    const points = linePoints(3, [0, 2, 1], 2)
    // The largest value is at the top, zero at the bottom.
    expect(points[0]?.y).toBeCloseTo(1, 12)
    expect(points[1]?.y).toBeCloseTo(0, 12)
    expect(points[2]?.y).toBeCloseTo(0.5, 12)
  })

  // v1's `labels.length > 1 ? labels.length - 1 : 1` — a single label has no
  // span to divide, and dividing by zero would put the point at infinity.
  it('survives a single label rather than dividing by zero', () => {
    const points = linePoints(1, [5], 5)
    expect(points[0]?.x).toBe(0)
    expect(Number.isFinite(points[0]?.x)).toBe(true)
  })

  it('starts with a move and continues with lines', () => {
    const d = linePath(linePoints(3, [0, 2, 1], 2))
    expect(d.startsWith('M ')).toBe(true)
    expect(d.split('L')).toHaveLength(3)
  })
})

describe('the tree a line chart expands to', () => {
  const data = { labels: ['a', 'b', 'c'], datasets: [{ data: [0, 2, 1] }] }

  // The one place a chart needs `none`: a plot must fill its box, and `meet`
  // would letterbox it. Verified by render at 240x100 — the ink spans
  // x 0..239 and y 0..99 exactly, and the stroke stays two pixels under a
  // 2.4:1 scale.
  it('stretches to fill its box rather than fitting inside it', () => {
    const chart = Chart({ type: 'line', data })
    const series = chart.children?.[0]?.children?.find(child => child.name === 'series 0')
    expect((series?.style as { preserveAspectRatio?: string })?.preserveAspectRatio).toBe('none')
  })

  it('draws one path per series', () => {
    const chart = Chart({
      type: 'line',
      data: { labels: ['a', 'b'], datasets: [{ data: [1, 2] }, { data: [2, 1] }] },
    })
    const paths = chart.children?.[0]?.children?.filter(child => child.name?.startsWith('series')) ?? []
    expect(paths).toHaveLength(2)
  })

  it('refuses a negative value, as the other kinds do', () => {
    expect(() => Chart({ type: 'line', data: { labels: ['a'], datasets: [{ data: [-1] }] } })).toThrow(/negative value/)
  })
})
