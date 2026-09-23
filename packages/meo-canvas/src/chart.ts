/**
 * Charts, as a builder that expands to ordinary nodes before encoding. Geometry
 * is written as fractions of the plot and resolved by layout, so nothing is
 * measured here; no browser draws a chart, so this arithmetic is the reference.
 */

import { Box, Column, Path, Row, Text, type SceneNode } from './node.js'

/** The four kinds of chart. */
export type ChartType = 'pie' | 'doughnut' | 'bar' | 'line'

/** One series of a cartesian chart. */
export interface ChartDataset {
  /** The series name, shown in the legend. Absent leaves it out of the legend. */
  readonly label?: string
  /**
   * One value per category, in the same order as
   * {@link CartesianChartData.labels}.
   *
   * A series shorter than the labels leaves the trailing categories empty
   * rather than shifting; the two are matched by position, not zipped.
   */
  readonly data: readonly number[]
  /** The series colour. Absent takes the next colour from the built-in cycle. */
  readonly color?: string
}

/** Data for a bar or line chart. */
export interface CartesianChartData {
  /** The category names along the x axis. Their count sets the number of groups. */
  readonly labels: readonly string[]
  /** The series drawn against those categories, each contributing one bar or line. */
  readonly datasets: readonly ChartDataset[]
}

/** One slice of a pie or doughnut. */
export interface PieChartDataPoint {
  /** The slice's name, shown in the legend. */
  readonly label: string
  /**
   * The slice's size, as a share of the total of every value.
   *
   * Not a percentage: the values are summed and each slice takes its
   * proportion, so they need not add to anything in particular.
   */
  readonly value: number
  /** The slice's colour. Absent takes the next colour from the built-in cycle. */
  readonly color?: string
}

/** Bands on a cartesian plot: five, so six gridlines counting both edges. */
export const GRID_DIVISIONS = 5

/** The share of a group's width left empty, half at each end, so its bars fill the middle 80%. */
export const BAR_GROUP_SPACING = 0.2

/**
 * Refuses a negative value. The geometry has no zero baseline, so a negative bar
 * would draw upward from below the plot or overflow it; drawing one properly
 * needs bars on both sides of an axis, which this chart does not have.
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

/**
 * Where every bar of a cartesian chart sits, as fractions of the plot area.
 *
 * **This is the reference.** In fractions the arithmetic holds whatever size the
 * plot turns out to be, which is what lets layout resolve it:
 *
 * ```text
 * groupWidth = 1 / labels
 * spacing    = groupWidth * 0.2
 * barWidth   = (groupWidth - spacing) / series
 * x          = index * groupWidth + spacing/2 + series * barWidth
 * height     = value / maxValue
 * ```
 *
 * `y` is not returned: a bar is anchored to the bottom of the plot, and a
 * bottom-aligned box says that without arithmetic.
 */
export function barLayout(
  labels: number,
  series: number,
  values: readonly (readonly number[])[],
  maxValue: number,
): {
  /** Left edge, as a fraction of the plot's width. */
  x: number
  /** Bar width, as a fraction of the plot's width. */
  width: number
  /**
   * Bar height, as a fraction of the plot's height.
   *
   * Zero when every value is zero, rather than the `NaN` the division would give.
   */
  height: number
}[][] {
  assertDrawable(values)

  const groupWidth = 1 / labels
  const spacing = groupWidth * BAR_GROUP_SPACING
  const width = (groupWidth - spacing) / series

  return Array.from({ length: labels }, (_, index) =>
    Array.from({ length: series }, (_, s) => ({
      x: index * groupWidth + spacing / 2 + s * width,
      width,
      // An all-zero chart would divide zero by zero, and a NaN height reaches
      // layout as absent and draws nothing at all, which reads as a broken renderer.
      height: maxValue === 0 ? 0 : (values[s]?.[index] ?? 0) / maxValue,
    })),
  )
}

/** Where each gridline falls, as a fraction from the top of the plot. */
export function gridLines(divisions: number = GRID_DIVISIONS): number[] {
  return Array.from({ length: divisions + 1 }, (_, i) => i / divisions)
}

/** The default series colours, in order. */
const PALETTE = ['#4e79a7', '#f28e2c', '#e15759', '#76b7b2', '#59a14f', '#edc949', '#af7aa1', '#ff9da7']

/** The colour a series takes when it names none. */
export function seriesColor(index: number, given?: string): string {
  return given ?? (PALETTE[index % PALETTE.length] as string)
}

/**
 * A fraction as the percentage string the style vocabulary takes. Typed as the
 * template literal because `Length` refuses a bare `string`.
 */
const percent = (fraction: number): `${number}%` => `${fraction * 100}%`

/** Options every chart understands. */
export interface BaseChartOptions {
  /** Draw the category names along the x axis. */
  readonly showLabels?: boolean
  /** Draw each datum's own number beside it. */
  readonly showValues?: boolean
  /** Draw the y axis and its scale. */
  readonly showYAxis?: boolean
  /** Draw the legend. A chart whose series have no `label` has nothing to put in it. */
  readonly showLegend?: boolean
  /** Which side the legend sits on. */
  readonly legendPosition?: LegendPosition
  /** Type size for the category labels, in pixels. */
  readonly labelFontSize?: number
  /** Type size for the per-datum values. */
  readonly valueFontSize?: number
  /** Type size for the y axis scale. */
  readonly yAxisFontSize?: number
  /** Colour of the category labels. */
  readonly labelColor?: string
  /** Colour of the per-datum values. */
  readonly valueColor?: string
  /** Colour of the y axis scale text. */
  readonly yAxisColor?: string
  /** Colour of the axis rules themselves, as distinct from their text. */
  readonly axisColor?: string
  /** The horizontal gridlines behind the plot. Absent draws none. */
  readonly grid?: {
    /** Draw them. Absent draws none, so the object alone is not enough. */
    readonly show?: boolean
    /** Their colour. */
    readonly color?: string
  }
  /**
   * Formats a y axis number before it is drawn — for a unit, a currency or a
   * thousands separator.
   *
   * Receives the raw value, so the formatter decides the rounding as well as
   * the text.
   */
  readonly yAxisLabelFormatter?: (value: number) => string
  /** Formats a category label before it is drawn, given its index as well. */
  readonly xAxisLabelFormatter?: (label: string, index: number) => string
  /**
   * A doughnut's hole, as a fraction of its outer radius. Defaults to `0.6`, and
   * the other three kinds ignore it.
   */
  readonly innerRadius?: number
  /**
   * Draw the label beside each value yourself.
   *
   * The returned node is *placed*: an absolutely positioned box centred on the
   * point by ordinary layout, so it takes part in the scene like any other node.
   */
  readonly renderLabelItem?: (props: { item: string; index: number }) => SceneNode | null | undefined
  /** As {@link renderLabelItem}, for the value drawn against each bar. */
  readonly renderValueItem?: (props: { item: number; index: number; datasetIndex: number }) => SceneNode | null | undefined
  /** As {@link renderLabelItem}, for one legend entry. */
  readonly renderLegendItem?: (props: { item: ChartDataset | PieChartDataPoint; index: number; color: string }) => SceneNode | null | undefined
}

/** What a chart is asked for. */
export interface ChartProps<T extends ChartType> {
  /**
   * Which chart to draw.
   *
   * It also decides what `data` must be: this is the discriminant, so a bar
   * chart handed pie data is a type error rather than an empty plot.
   */
  readonly type: T
  /** The values, in the shape `type` selects. */
  readonly data: T extends 'bar' | 'line' ? CartesianChartData : readonly PieChartDataPoint[]
  /** What to draw besides the data itself, and in what colours. */
  readonly options?: BaseChartOptions
  /** The chart's width. Absent lets layout decide, as for any other node. */
  readonly width?: number | string
  /** The chart's height, on the same terms. */
  readonly height?: number | string
  /** The face every piece of text in the chart is set in. */
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

/** The side of a legend swatch, in pixels. */
const LEGEND_SWATCH = 15
/** The gap between a swatch and its label. */
const LEGEND_GAP = 5
/** The space between one legend item and the next. */
const LEGEND_PADDING = 20

/** Which side of the chart the legend sits on. */
export type LegendPosition = 'top' | 'bottom' | 'left' | 'right'

/**
 * The legend: a wrapping row of swatch-and-label pairs, or a column beside the
 * plot. `flexWrap` does the wrapping and the plot takes the rest through
 * `flexGrow`, so no label is measured.
 */
function legend(
  options: BaseChartOptions | undefined,
  entries: readonly { label: string; color: string; source: ChartDataset | PieChartDataPoint }[],
  fontFamily: string | undefined,
): SceneNode | undefined {
  if (!options?.showLegend || entries.length === 0) return undefined

  const upright = options.legendPosition === 'left' || options.legendPosition === 'right'
  const items = entries.map((entry, index) => {
    const drawn = options.renderLegendItem?.({ item: entry.source, index, color: entry.color })
    if (drawn) return drawn
    return Row({
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
    })
  })

  return upright ? Column({ name: 'legend', children: items }) : Row({ flexWrap: 'wrap', name: 'legend', children: items })
}

/**
 * The chart's own frame: the legend on whichever side, and everything else. The
 * legend is a sibling rather than an overlay, so the plot takes what it leaves.
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
 * The plot, with a y-axis gutter beside it when asked for. A zero-height in-flow
 * copy of the widest label sets the gutter's width, and the visible labels sit
 * absolutely on their gridlines: absolute children do not size their parent (9
 * pixels against 30), and in-flow ones drift off their lines under `space-between`.
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
  // The first label is the maximum and the last is zero.
  const labels = gridLines().map(fraction => format(maxValue - maxValue * fraction))
  const style = {
    ...(fontFamily === undefined ? {} : { fontFamily }),
    fontSize: options.yAxisFontSize ?? 12,
    color: options.yAxisColor ?? options.axisColor ?? '#000000',
  }
  // Widest by character count, which a proportional face can get wrong; an
  // underestimate costs a few pixels of gutter rather than a wrong layout.
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
 * A bar chart. Bars sit absolutely in percentages of the plot, anchored to
 * `bottom: 0`, and the plot's `flex: 1` takes what the labels and legend leave.
 */
function barChart(props: ChartProps<'bar'>): SceneNode {
  const { labels, datasets } = props.data
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
              // Centred on the bar, five pixels above its top.
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
          const shown = options.xAxisLabelFormatter?.(label, index) ?? label
          return Box({
            flexGrow: 1,
            flexBasis: 0,
            // `justifyContent`, not `alignItems`: a slot is a row, so `alignItems`
            // centres only vertically. A pixel sees this and a byte comparison
            // cannot; `the label strip centres each label in its slot` pins it.
            justifyContent: 'center',
            alignItems: 'center',
            children:
              drawn ??
              Text(shown, {
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
        // The row carries what it stands for, so a legend hatch can be handed
        // the series rather than the two strings drawn from it.
        source: dataset,
      })),
      props.fontFamily,
    ),
    props,
    'bar chart',
  )
}

/** The square a pie is drawn in; `xMidYMid meet` keeps it circular in any box. */
const PIE_SPACE = 100

/**
 * The inset between a pie and its box, as a share of the radius. A proportion,
 * because the drawing is authored once in a viewBox and scaled.
 */
const PIE_INSET = 0.05

/** How far along the radius a slice's label sits. */
const PIE_LABEL_REACH = 0.7

/** Where each slice begins and ends, sweeping clockwise from twelve o'clock. */
export function sliceAngles(values: readonly number[]): {
  /** Where the slice begins, in radians. Zero is three o'clock, as `Math` has it. */
  start: number
  /** Where it ends. Sweeps clockwise from `start`. */
  end: number
}[] {
  const total = values.reduce((sum, value) => sum + value, 0)
  const out: { start: number; end: number }[] = []
  // Twelve o'clock.
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

/** One slice as SVG path data, in `PIE_SPACE`'s coordinates. */
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
  const points = props.data
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
          // White separates two slices of similar colour.
          stroke: '#ffffff',
          lineWidth: 2,
          name: `slice ${index}`,
        }),
      ),
      // Along each slice's middle angle, in percentages of the plot, since the
      // drawing is square and centred and the label rides with it.
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
        // A pie's legend reads `label (value)`; a cartesian one is the series name.
        label: `${point.label} (${point.value})`,
        color: seriesColor(index, point.color),
        source: point,
      })),
      props.fontFamily,
    ),
    props,
    props.type === 'doughnut' ? 'doughnut chart' : 'pie chart',
  )
}

/**
 * The space a line plot is drawn in, **stretched** rather than fitted so it fills
 * its box. The size is arbitrary: each axis is scaled onto the node independently.
 */
const LINE_SPACE = 100

/** The radius of a line point's marker, in pixels. */
const LINE_POINT_RADIUS = 4

/**
 * Where each point of a line series sits, as fractions of the plot.
 *
 * ```text
 * x = index / (labels - 1)
 * y = 1 - value / maxValue
 * ```
 *
 * **Points span edge to edge, where bars are centred in slots**, so the first and
 * last points sit on the plot's edges. A single label has no span to divide, and
 * its point sits at the left edge.
 */
export function linePoints(
  labels: number,
  values: readonly number[],
  maxValue: number,
): {
  /** Distance across the plot, as a fraction of its width. */
  x: number
  /**
   * Distance **down** the plot, as a fraction of its height.
   *
   * Inverted from the value: `0` is the top of the plot and `1` the bottom, so
   * a point can be placed without the caller flipping it. A maximum of zero
   * puts every point on the floor rather than dividing by it.
   */
  y: number
}[] {
  const spacing = labels > 1 ? 1 / (labels - 1) : 1
  return values.map((value, index) => ({
    x: index * spacing,
    y: maxValue === 0 ? 1 : 1 - value / maxValue,
  }))
}

/** One series as SVG path data, in `LINE_SPACE`'s coordinates. */
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
  const { labels, datasets } = props.data
  const options = props.options
  const values = datasets.map(dataset => dataset.data)
  assertDrawable(values)
  const maxValue = Math.max(0, ...values.flat())

  const strip = options?.showLabels
    ? Row({
        name: 'labels',
        children: labels.map((label, index) => {
          const drawn = options.renderLabelItem?.({ item: label, index })
          const shown = options.xAxisLabelFormatter?.(label, index) ?? label
          return Box({
            flexGrow: 1,
            flexBasis: 0,
            // Centred as `barChart`'s strip is, and for the same reason.
            justifyContent: 'center',
            alignItems: 'center',
            children:
              drawn ??
              Text(shown, {
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
            lineWidth: 2,
            name: `series ${index}`,
          }),
        ),
        // **Markers cannot go in the path**: under `preserveAspectRatio: 'none'`
        // a circle comes out an ellipse. Round boxes placed in percentages are
        // untouched by the stretch.
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
        // The row carries what it stands for, so a legend hatch can be handed
        // the series rather than the two strings drawn from it.
        source: dataset,
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
  if (props.type === 'doughnut') {
    return pieChart(props as ChartProps<'doughnut'>, props.options?.innerRadius ?? 0.6)
  }
  throw new Error(`[canvas] chart type ${JSON.stringify(props.type)} is not built yet`)
}
