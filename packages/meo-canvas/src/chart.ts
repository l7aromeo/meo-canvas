/**
 * Charts, as a builder that expands to ordinary nodes.
 *
 * # Why this is a reimplementation and not a port
 *
 * v1's chart is `ChartNode extends BoxNode` with fifty-eight direct `ctx.`
 * calls: it paints by issuing 2D commands during the render pass. **Not one
 * step of that exists here.** Layout runs once over the whole scene before
 * paint, nodes are encoded into an arena before the renderer sees them, and
 * there is no way for a caller's function to produce a node mid-paint.
 *
 * So what transfers is the **arithmetic** — where bars sit, how a pie is swept,
 * where gridlines fall — and what does not is every line that positions by
 * drawing. The API is v1's exactly; the mechanism behind it is not.
 *
 * # v1 is both baselines, so the geometry is written down
 *
 * Chrome has no charts. **Nothing external can adjudicate a chart fixture**,
 * which makes the arithmetic below the only reference there is — so it is
 * stated here rather than left implicit in the tree it builds. A fixture that
 * asserted a picture could never be checked against anything.
 *
 * # Nothing is measured at build time
 *
 * v1 measures text to size the plot area: `measureText(ctx, maxLabel).width`
 * for the y-axis gutter, `measureText(ctx, 'Mg')` for the label strip. **A
 * builder cannot do that and does not need to.** Every quantity in the bar
 * geometry is a *fraction* of the plot area, so it is written as a percentage
 * and resolved by layout; the gutter and the strip become a column and a row
 * whose sizes are their own content's. **The measurement v1 performs by hand
 * is what the layout pass does for free**, which is the same observation that
 * made the hatches tractable.
 *
 * # The hatches keep their signature and change their contract
 *
 * `renderLabelItem`, `renderValueItem` and `renderLegendItem` still take the
 * same props and still return a node. **What changed is what happens to it.**
 * v1 lays the returned node out detached, reads its measured size, and draws it
 * at a position computed from that size — `valueX - layout.width / 2` centres
 * it on the bar. Here it is *placed*: an absolutely positioned box centred on
 * the same point, measured by the ordinary layout pass. **The type is
 * unchanged and the contract is not**, which is exactly the kind of divergence
 * that stays invisible until someone relies on the old behaviour.
 */

import { Box, Column, Path, Row, Text, type SceneNode } from './node.js'

/** The four kinds, as v1 names them. */
export type ChartType = 'pie' | 'doughnut' | 'bar' | 'line'

/** One series of a cartesian chart. */
export interface ChartDataset {
  readonly label?: string
  readonly data: readonly number[]
  readonly color?: string
}

/** Data for a bar or line chart. */
export interface CartesianChartData {
  readonly labels: readonly string[]
  readonly datasets: readonly ChartDataset[]
}

/** One slice of a pie or doughnut. */
export interface PieChartDataPoint {
  readonly label: string
  readonly value: number
  readonly color?: string
}

/**
 * How many gridlines a cartesian plot carries, counting both edges.
 *
 * v1's `for (let i = 0; i <= 5; i++)`, so six lines and five bands.
 */
export const GRID_DIVISIONS = 5

/**
 * The share of a group's width left empty between groups.
 *
 * v1: `barSpacing = groupWidth * 0.2`, half of it at each end of the group, so
 * the bars of one group occupy the middle 80% of their slot.
 */
export const BAR_GROUP_SPACING = 0.2

/**
 * Where every bar of a cartesian chart sits, as fractions of the plot area.
 *
 * **This is the reference.** v1 computes these in pixels against a plot
 * rectangle it measured; the same arithmetic in fractions is independent of
 * how big the plot turns out to be, which is what lets layout resolve it:
 *
 * ```text
 * groupWidth = 1 / labels                      v1: chartWidth / labels.length
 * spacing    = groupWidth * 0.2                v1: groupWidth * 0.2
 * barWidth   = (groupWidth - spacing) / series v1: (groupWidth - barSpacing) / datasets.length
 * x          = index * groupWidth + spacing/2 + series * barWidth
 * height     = value / maxValue                v1: (value / maxValue) * finalChartHeight
 * ```
 *
 * `y` is not returned because a bar is anchored to the bottom of the plot: v1's
 * `barY = chartY + finalChartHeight - barHeight` is what "sits on the axis"
 * means, and a bottom-aligned box says it without arithmetic.
 */
/**
 * Refuses data this geometry cannot draw.
 *
 * **Negative values, and v1 does not support them — it mis-draws them three
 * different ways.** `maxValue` is `Math.max(...)` with no `Math.min` and no
 * zero baseline, so on a 100-pixel plot:
 *
 * ```text
 * [-5, 10]    maxValue 10   the -5 bar gets height -50 and barY 150
 *                           fillRect draws UPWARD from below the plot
 * [-5, -1]    maxValue -1   the -5 bar gets height 500 -- five times the plot --
 *                           and barY -400, while the LEAST negative gets a full
 *                           bar: the chart is inverted and overflows
 * ```
 *
 * **Refused rather than reproduced.** Bug-compatibility was never the rule —
 * v1 is the API baseline, meaning which props exist and what they are called —
 * and a chart that silently mis-draws a profit-and-loss series is worse than
 * one that says it cannot. Supporting them properly needs a zero baseline and
 * bars on both sides of it, which is a feature v1 does not have and nobody has
 * asked for.
 */
function assertDrawable(values: readonly (readonly number[])[]): void {
  for (const series of values) {
    for (const value of series) {
      if (value < 0) {
        throw new Error(
          `[canvas] a chart cannot draw a negative value (got ${value}) — v1 mis-draws these rather than supporting them, so they are refused here instead of reproduced`,
        )
      }
    }
  }
}

export function barLayout(
  labels: number,
  series: number,
  values: readonly (readonly number[])[],
  maxValue: number,
): { x: number; width: number; height: number }[][] {
  assertDrawable(values)

  const groupWidth = 1 / labels
  const spacing = groupWidth * BAR_GROUP_SPACING
  const width = (groupWidth - spacing) / series

  return Array.from({ length: labels }, (_, index) =>
    Array.from({ length: series }, (_, s) => ({
      x: index * groupWidth + spacing / 2 + s * width,
      width,
      // **A deliberate divergence, recorded so nobody restores it while
      // "matching v1".** v1's formula is `(value / maxValue) * finalChartHeight`
      // with `maxValue = Math.max(...)`. An all-zero chart divides zero by
      // zero; NaN reaches layout as an absent height and the chart draws
      // **nothing**, which reads as a broken renderer rather than as an empty
      // chart. Zero is the honest height for a zero value.
      height: maxValue === 0 ? 0 : (values[s]?.[index] ?? 0) / maxValue,
    })),
  )
}

/** Where each gridline falls, as a fraction from the top of the plot. */
export function gridLines(divisions: number = GRID_DIVISIONS): number[] {
  return Array.from({ length: divisions + 1 }, (_, i) => i / divisions)
}

/** v1's default series colours, in order. */
const PALETTE = ['#4e79a7', '#f28e2c', '#e15759', '#76b7b2', '#59a14f', '#edc949', '#af7aa1', '#ff9da7']

/** The colour a series takes when it names none. */
export function seriesColor(index: number, given?: string): string {
  return given ?? (PALETTE[index % PALETTE.length] as string)
}

/**
 * A fraction as the percentage string the style vocabulary takes.
 *
 * Typed as the template literal rather than `string`, because `Length` is
 * `number | \`${number}%\`` — a bare `string` is refused, which is the type
 * system insisting a length say which kind it is.
 */
const percent = (fraction: number): `${number}%` => `${fraction * 100}%`

/** Options every chart understands, as v1 spells them. */
export interface BaseChartOptions {
  readonly showLabels?: boolean
  readonly showValues?: boolean
  readonly showYAxis?: boolean
  readonly showLegend?: boolean
  readonly legendPosition?: LegendPosition
  readonly labelFontSize?: number
  readonly valueFontSize?: number
  readonly yAxisFontSize?: number
  readonly labelColor?: string
  readonly valueColor?: string
  readonly yAxisColor?: string
  readonly axisColor?: string
  readonly grid?: { readonly show?: boolean; readonly color?: string }
  readonly yAxisLabelFormatter?: (value: number) => string
  /**
   * Draw the label beside each value yourself.
   *
   * **The signature is v1's and the contract is not.** v1 lays the returned
   * node out detached during painting and draws it at a position derived from
   * its measured size. Here the node is *placed* — centred on the same point by
   * ordinary layout — so it participates in the scene rather than being
   * measured and discarded.
   */
  readonly renderLabelItem?: (props: { item: string; index: number }) => SceneNode | null | undefined
  /** As {@link renderLabelItem}, for the value drawn against each bar. */
  readonly renderValueItem?: (props: { item: number; index: number; datasetIndex: number }) => SceneNode | null | undefined
  /** As {@link renderLabelItem}, for one legend entry. */
  readonly renderLegendItem?: (props: { item: ChartDataset | PieChartDataPoint; index: number; color: string }) => SceneNode | null | undefined
}

/** What a chart is asked for. */
export interface ChartProps<T extends ChartType> {
  readonly type: T
  readonly data: T extends 'bar' | 'line' ? CartesianChartData : readonly PieChartDataPoint[]
  readonly options?: BaseChartOptions
  readonly width?: number | string
  readonly height?: number | string
  readonly fontFamily?: string
}

/** The gridlines, as absolutely positioned rules across the plot. */
function grid(options: BaseChartOptions | undefined): SceneNode[] {
  if (!options?.grid?.show) return []
  return gridLines().map(fraction =>
    Box({
      positionType: 'absolute',
      position: { top: percent(fraction), left: 0, right: 0 },
      height: 1,
      backgroundColor: options.grid?.color ?? '#e0e0e0',
      name: `gridline ${fraction}`,
    }),
  )
}

/** v1's swatch is `Math.min(15, itemHeight - 2)`; fifteen is its cap. */
const LEGEND_SWATCH = 15
/** v1's gap between a swatch and its label. */
const LEGEND_GAP = 5
/** v1's padding between one legend item and the next. */
const LEGEND_PADDING = 20

/** Where a legend sits, as v1 spells it. */
export type LegendPosition = 'top' | 'bottom' | 'left' | 'right'

/**
 * The legend, as a wrapping row of swatch-and-label pairs.
 *
 * **v1 measures every label and wraps by hand** — `currentX + itemWidth >
 * totalWidth` and a running row count, so it can subtract the legend's height
 * from the plot's. A builder needs none of that: `flexWrap` does the wrapping,
 * the row's height is whatever it wraps to, and the plot takes the rest
 * through `flexGrow`. **The measuring v1 does to size the plot is the thing
 * layout was going to do anyway.**
 */
function legend(
  options: BaseChartOptions | undefined,
  entries: readonly { label: string; color: string }[],
  fontFamily: string | undefined,
): SceneNode | undefined {
  if (!options?.showLegend || entries.length === 0) return undefined

  const upright = options.legendPosition === 'left' || options.legendPosition === 'right'
  const items = entries.map((entry, index) =>
    Row({
      alignItems: 'center',
      gap: LEGEND_GAP,
      margin: upright ? { bottom: LEGEND_GAP } : { right: LEGEND_PADDING },
      name: `legend item ${index}`,
      children: [
        Box({ width: LEGEND_SWATCH, height: LEGEND_SWATCH, backgroundColor: entry.color, name: 'swatch' }),
        Text(entry.label, {
          ...(fontFamily === undefined ? {} : { fontFamily }),
          fontSize: options.labelFontSize ?? 12,
          color: options.labelColor ?? '#000000',
        }),
      ],
    }),
  )

  return upright ? Column({ name: 'legend', children: items }) : Row({ flexWrap: 'wrap', name: 'legend', children: items })
}

/**
 * The chart's own frame: the legend on whichever side, and everything else.
 *
 * The legend is a sibling rather than an overlay, so the plot's `flexGrow`
 * takes what the legend leaves — which is v1's `chartHeight - legendHeight`
 * arrived at by layout instead of by subtraction.
 */
function framed(
  options: BaseChartOptions | undefined,
  bars: SceneNode,
  legendNode: SceneNode | undefined,
  props: { width?: number | string; height?: number | string },
  name: string,
): SceneNode {
  const size = {
    width: (props.width ?? '100%') as `${number}%` | number,
    height: (props.height ?? '100%') as `${number}%` | number,
  }
  if (legendNode === undefined) {
    return Column({ ...size, name, children: bars })
  }

  const where = options?.legendPosition ?? 'bottom'
  if (where === 'left' || where === 'right') {
    return Row({
      ...size,
      name,
      children: where === 'left' ? [legendNode, bars] : [bars, legendNode],
    })
  }
  return Column({
    ...size,
    name,
    children: where === 'top' ? [legendNode, bars] : [bars, legendNode],
  })
}

/**
 * The plot, with a y-axis gutter beside it when one is asked for.
 *
 * # How the gutter measures without measuring
 *
 * v1 sizes it with `measureText(ctx, maxLabel).width + 10` and centres each
 * label on its gridline with `textBaseline: 'middle'`. A builder can do
 * neither directly, and **three properties are wanted at once**: the gutter
 * sizes to its widest label, the labels centre on the gridlines, and the plot
 * is a sibling so a bar never covers the gutter.
 *
 * Absolutely positioned children give the second and third and **do not size
 * their parent** — measured: a gutter holding only absolute labels came out 9
 * pixels wide where the same labels in flow made it 30. In-flow children give
 * the first and drift on the second, because `space-between` spreads label
 * *boxes* evenly rather than their centres — measured at 8.5, 5.5, 1.5, −1.5
 * and −5.5 pixels across five rows of a 120-pixel plot.
 *
 * So the gutter holds **both**: one zero-height in-flow copy of the widest
 * label, which sets the width and draws nothing, and the visible labels
 * absolutely positioned at their gridline fractions and pulled up by half
 * their own height. Measured: 6 pixels wide for single digits against 30 for
 * `10000`, gridlines full height, label centres on their lines.
 *
 * The extreme labels overflow the plot by half their height, which is v1's
 * behaviour too — its top label is centred on the plot's own top edge.
 */
function plotArea(options: BaseChartOptions | undefined, maxValue: number, fontFamily: string | undefined, drawn: readonly SceneNode[]): SceneNode {
  const plot = Box({
    flexGrow: 1,
    positionType: 'relative',
    name: 'plot',
    children: [...grid(options), ...drawn],
  })

  if (!options?.showYAxis) return plot

  const format = options.yAxisLabelFormatter ?? ((value: number) => String(Math.round(value * 100) / 100))
  // v1: `const value = maxValue - (maxValue / 5) * i`, so the first row is the
  // maximum and the last is zero.
  const labels = gridLines().map(fraction => format(maxValue - maxValue * fraction))
  const style = {
    ...(fontFamily === undefined ? {} : { fontFamily }),
    fontSize: options.yAxisFontSize ?? 12,
    color: options.yAxisColor ?? options.axisColor ?? '#000000',
  }
  // The widest by character count rather than by measurement, which is the one
  // thing a builder cannot do. A proportional face can make a shorter string
  // wider — `111` against `00` — so this is a heuristic, and the sizer is why
  // it only has to be close: an underestimate costs a few pixels of gutter,
  // not a wrong layout.
  const widest = labels.reduce((longest, label) => (label.length > longest.length ? label : longest), '')

  return Row({
    flexGrow: 1,
    name: 'plot area',
    children: [
      Column({
        positionType: 'relative',
        name: 'y axis',
        children: [
          Box({ height: 0, overflow: 'hidden', name: 'gutter sizer', children: Text(widest, style) }),
          ...labels.map((label, index) =>
            Box({
              positionType: 'absolute',
              position: { top: percent(gridLines()[index] as number), left: 0 },
              transform: { translateY: '-50%' },
              name: `axis label ${index}`,
              children: Text(label, style),
            }),
          ),
        ],
      }),
      plot,
    ],
  })
}

/**
 * A bar chart.
 *
 * The plot area is `flex: 1` inside a column, so its height is whatever the
 * label strip and legend leave — which is v1's `finalChartHeight` arrived at by
 * subtraction rather than by measurement. Bars are absolutely positioned inside
 * it in percentages, anchored to `bottom: 0`, which is what v1's
 * `barY = chartY + finalChartHeight - barHeight` says.
 */
function barChart(props: ChartProps<'bar'>): SceneNode {
  const { labels, datasets } = props.data as CartesianChartData
  const options = props.options
  const values = datasets.map(dataset => dataset.data)
  const maxValue = Math.max(0, ...values.flat())
  const placed = barLayout(labels.length, datasets.length, values, maxValue)

  const bars = placed.flatMap((group, index) =>
    group.map((bar, datasetIndex) => {
      const value = values[datasetIndex]?.[index] ?? 0
      const drawn = options?.renderValueItem?.({ item: value, index, datasetIndex })
      return Box({
        positionType: 'absolute',
        position: { left: percent(bar.x), bottom: 0 },
        width: percent(bar.width),
        height: percent(bar.height),
        backgroundColor: seriesColor(datasetIndex, datasets[datasetIndex]?.color),
        name: `bar ${index}.${datasetIndex}`,
        children: options?.showValues
          ? Box({
              // Centred on the bar and sitting five pixels above its top, which
              // is v1's `valueX - layout.width / 2` and `valueY = barY - 5`
              // expressed as placement rather than as arithmetic on a measured
              // width.
              positionType: 'absolute',
              position: { bottom: '100%', left: 0, right: 0 },
              margin: { bottom: 5 },
              alignItems: 'center',
              children:
                drawn ??
                Text(String(value), {
                  ...(props.fontFamily === undefined ? {} : { fontFamily: props.fontFamily }),
                  fontSize: options?.valueFontSize ?? 12,
                  color: options?.valueColor ?? '#000000',
                }),
            })
          : undefined,
      })
    }),
  )

  const strip = options?.showLabels
    ? Row({
        name: 'labels',
        children: labels.map((label, index) => {
          const drawn = options.renderLabelItem?.({ item: label, index })
          return Box({
            flexGrow: 1,
            flexBasis: 0,
            alignItems: 'center',
            children:
              drawn ??
              Text(label, {
                ...(props.fontFamily === undefined ? {} : { fontFamily: props.fontFamily }),
                fontSize: options.labelFontSize ?? 12,
                color: options.labelColor ?? '#000000',
              }),
          })
        }),
      })
    : undefined

  const inner = Column({
    flexGrow: 1,
    name: 'body',
    children: [plotArea(options, maxValue, props.fontFamily, bars), ...(strip ? [strip] : [])],
  })
  return framed(
    options,
    inner,
    legend(
      options,
      datasets.map((dataset, index) => ({
        label: dataset.label ?? `Series ${index + 1}`,
        color: seriesColor(index, dataset.color),
      })),
      props.fontFamily,
    ),
    props,
    'bar chart',
  )
}

/**
 * The space a pie or doughnut is drawn in.
 *
 * A square, because the drawing is one: v1's `radius = min(w, h) / 2` keeps a
 * pie circular in any box, and `xMidYMid meet` does the same thing for a
 * viewBox. **The two rules agree, which is why these kinds needed nothing more
 * than the viewBox** — a line chart is the one that does not, since it should
 * fill its box rather than stay square.
 */
const PIE_SPACE = 100

/**
 * How much of the radius v1's ten-pixel inset takes.
 *
 * **A stated divergence.** v1 writes `radius = min(w, h) / 2 - 10`, ten
 * *pixels* regardless of size, which under a viewBox has no meaning — the
 * drawing is authored once and scaled, so a pixel is not a fixed quantity
 * inside it. A proportion is the honest translation and it behaves better at
 * both ends: v1's inset is a fifth of the radius on a hundred-pixel chart and
 * invisible on a thousand-pixel one.
 */
const PIE_INSET = 0.05

/** How far along the radius v1 puts a slice's label: `radius * 0.7`. */
const PIE_LABEL_REACH = 0.7

/** Where one slice begins and ends, in turns clockwise from the top. */
export function sliceAngles(values: readonly number[]): { start: number; end: number }[] {
  const total = values.reduce((sum, value) => sum + value, 0)
  const out: { start: number; end: number }[] = []
  // v1 starts at `-Math.PI / 2` — twelve o'clock — and sweeps clockwise.
  let cursor = -Math.PI / 2
  for (const value of values) {
    // A total of zero has no angles to divide; every slice is empty rather
    // than NaN, for the same reason a zero `maxValue` gives a zero height.
    const sweep = total === 0 ? 0 : (value / total) * Math.PI * 2
    out.push({ start: cursor, end: cursor + sweep })
    cursor += sweep
  }
  return out
}

/** One slice as SVG path data, in {@link PIE_SPACE}'s coordinates. */
export function slicePath(start: number, end: number, outer: number, inner: number): string {
  const centre = PIE_SPACE / 2
  const at = (radius: number, angle: number): string => `${(centre + Math.cos(angle) * radius).toFixed(4)} ${(centre + Math.sin(angle) * radius).toFixed(4)}`
  // A sweep past half a turn needs SVG's large-arc flag, or the renderer draws
  // the short way round and a 300-degree slice comes out as 60.
  const large = end - start > Math.PI ? 1 : 0

  if (inner <= 0) {
    return `M ${centre} ${centre} L ${at(outer, start)} A ${outer} ${outer} 0 ${large} 1 ${at(outer, end)} Z`
  }
  return (
    `M ${at(outer, start)} A ${outer} ${outer} 0 ${large} 1 ${at(outer, end)} ` + `L ${at(inner, end)} A ${inner} ${inner} 0 ${large} 0 ${at(inner, start)} Z`
  )
}

/** A pie or doughnut. */
function pieChart(props: ChartProps<'pie' | 'doughnut'>, innerFraction: number): SceneNode {
  const points = props.data as readonly PieChartDataPoint[]
  assertDrawable([points.map(point => point.value)])

  const outer = (PIE_SPACE / 2) * (1 - PIE_INSET)
  const inner = outer * innerFraction
  const angles = sliceAngles(points.map(point => point.value))

  const slices = Box({
    flexGrow: 1,
    positionType: 'relative',
    name: 'plot',
    children: [
      ...angles.map((angle, index) =>
        Path({
          positionType: 'absolute',
          position: { top: 0, right: 0, bottom: 0, left: 0 },
          // Every slice is drawn in the same square space and stacked, so each
          // one's viewBox is the whole drawing rather than its own bounds —
          // which is what keeps them concentric.
          viewBox: [0, 0, PIE_SPACE, PIE_SPACE],
          d: slicePath(angle.start, angle.end, outer, inner),
          fill: seriesColor(index, points[index]?.color),
          // v1 strokes every slice in white, which is what separates two
          // slices of similar colour.
          stroke: '#ffffff',
          lineWidth: 2,
          name: `slice ${index}`,
        }),
      ),
      // v1 puts a slice's label at seven tenths of the radius along the
      // slice's own middle angle. In percentages of the plot rather than
      // pixels, since the drawing is square and centred and the label rides
      // with it.
      ...(props.options?.showLabels
        ? angles.map((angle, index) => {
            const middle = angle.start + (angle.end - angle.start) / 2
            const reach = ((outer * PIE_LABEL_REACH) / PIE_SPACE) * 100
            const drawn = props.options?.renderLabelItem?.({
              item: points[index]?.label ?? '',
              index,
            })
            return Box({
              positionType: 'absolute',
              position: {
                left: percent(0.5 + (Math.cos(middle) * reach) / 100),
                top: percent(0.5 + (Math.sin(middle) * reach) / 100),
              },
              transform: { translateX: '-50%', translateY: '-50%' },
              name: `slice label ${index}`,
              children:
                drawn ??
                Text(points[index]?.label ?? '', {
                  ...(props.fontFamily === undefined ? {} : { fontFamily: props.fontFamily }),
                  fontSize: props.options?.labelFontSize ?? 12,
                  color: props.options?.labelColor ?? '#000000',
                }),
            })
          })
        : []),
    ],
  })

  return framed(
    props.options,
    slices,
    legend(
      props.options,
      points.map((point, index) => ({
        // v1's pie legend reads `label (value)`, where a cartesian one is just
        // the series name.
        label: `${point.label} (${point.value})`,
        color: seriesColor(index, point.color),
      })),
      props.fontFamily,
    ),
    props,
    props.type === 'doughnut' ? 'doughnut chart' : 'pie chart',
  )
}

/**
 * The space a line plot is drawn in.
 *
 * A hundred by a hundred **stretched** rather than fitted: a line plot must
 * fill its box, and `meet` would letterbox it. The numbers themselves are
 * arbitrary — only the ratios in the path matter, since each axis is scaled
 * independently onto the node.
 */
const LINE_SPACE = 100

/** v1's point marker: `ctx.arc(pointX, pointY, 4, 0, Math.PI * 2)`. */
const LINE_POINT_RADIUS = 4

/**
 * Where each point of a line series sits, as fractions of the plot.
 *
 * ```text
 * x = index / (labels - 1)     v1: chartX + index * (chartWidth / (labels - 1))
 * y = 1 - value / maxValue     v1: chartY + h - (value / maxValue) * h
 * ```
 *
 * **Points span edge to edge, where bars are centred in slots** — v1 divides by
 * `labels - 1` here and by `labels` there, so the first and last points sit on
 * the plot's edges rather than inset. A single label has no span to divide, so
 * v1 divides by one and the point sits at the left edge.
 */
export function linePoints(labels: number, values: readonly number[], maxValue: number): { x: number; y: number }[] {
  const spacing = labels > 1 ? 1 / (labels - 1) : 1
  return values.map((value, index) => ({
    x: index * spacing,
    y: maxValue === 0 ? 1 : 1 - value / maxValue,
  }))
}

/** One series as SVG path data, in {@link LINE_SPACE}'s coordinates. */
export function linePath(points: readonly { x: number; y: number }[]): string {
  return points
    .map((point, index) => {
      const x = (point.x * LINE_SPACE).toFixed(4)
      const y = (point.y * LINE_SPACE).toFixed(4)
      return `${index === 0 ? 'M' : 'L'} ${x} ${y}`
    })
    .join(' ')
}

/** A line chart. */
function lineChart(props: ChartProps<'line'>): SceneNode {
  const { labels, datasets } = props.data as CartesianChartData
  const options = props.options
  const values = datasets.map(dataset => dataset.data)
  assertDrawable(values)
  const maxValue = Math.max(0, ...values.flat())

  const strip = options?.showLabels
    ? Row({
        name: 'labels',
        children: labels.map((label, index) => {
          const drawn = options.renderLabelItem?.({ item: label, index })
          return Box({
            flexGrow: 1,
            flexBasis: 0,
            alignItems: 'center',
            children:
              drawn ??
              Text(label, {
                ...(props.fontFamily === undefined ? {} : { fontFamily: props.fontFamily }),
                fontSize: options.labelFontSize ?? 12,
                color: options.labelColor ?? '#000000',
              }),
          })
        }),
      })
    : undefined

  const inner = Column({
    flexGrow: 1,
    name: 'body',
    children: [
      plotArea(options, maxValue, props.fontFamily, [
        ...datasets.map((dataset, index) =>
          Path({
            positionType: 'absolute',
            position: { top: 0, right: 0, bottom: 0, left: 0 },
            viewBox: [0, 0, LINE_SPACE, LINE_SPACE],
            // The one place a chart needs `none`: a plot fills its box, and
            // `meet` would letterbox it. The pen is not distorted by it —
            // see `PathProps.viewBox`.
            preserveAspectRatio: 'none',
            d: linePath(linePoints(labels.length, dataset.data, maxValue)),
            fill: 'none',
            stroke: seriesColor(index, dataset.color),
            // v1's `ctx.lineWidth = 2` for a line series.
            lineWidth: 2,
            name: `series ${index}`,
          }),
        ),
        // **Markers cannot go in the path.** The series is drawn under
        // `preserveAspectRatio: 'none'`, so a circle authored in the viewBox
        // comes out an ellipse — stretched by the ratio of the two scales,
        // which on a wide plot is not subtle. They are round boxes placed in
        // percentages instead, which the stretch never touches.
        ...datasets.flatMap((dataset, series) =>
          linePoints(labels.length, dataset.data, maxValue).map((point, index) =>
            Box({
              positionType: 'absolute',
              position: { left: percent(point.x), top: percent(point.y) },
              transform: { translateX: '-50%', translateY: '-50%' },
              width: LINE_POINT_RADIUS * 2,
              height: LINE_POINT_RADIUS * 2,
              borderRadius: LINE_POINT_RADIUS,
              backgroundColor: seriesColor(series, dataset.color),
              name: `point ${series}.${index}`,
            }),
          ),
        ),
      ]),
      ...(strip ? [strip] : []),
    ],
  })

  return framed(
    options,
    inner,
    legend(
      options,
      datasets.map((dataset, index) => ({
        label: dataset.label ?? `Series ${index + 1}`,
        color: seriesColor(index, dataset.color),
      })),
      props.fontFamily,
    ),
    props,
    'line chart',
  )
}

/**
 * Builds a chart's node tree.
 *
 * A **builder**, not a node kind: it expands to boxes, paths and text before a
 * scene is encoded, so nothing here crosses the arena and the renderer never
 * learns what a chart is.
 */
export function Chart<T extends ChartType>(props: ChartProps<T>): SceneNode {
  if (props.type === 'bar') return barChart(props as ChartProps<'bar'>)
  if (props.type === 'line') return lineChart(props as ChartProps<'line'>)
  if (props.type === 'pie') return pieChart(props as ChartProps<'pie'>, 0)
  if (props.type === 'doughnut') return pieChart(props as ChartProps<'doughnut'>, 0.6)
  throw new Error(`[canvas] chart type ${JSON.stringify(props.type)} is not built yet`)
}
