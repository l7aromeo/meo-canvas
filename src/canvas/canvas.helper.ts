import type { CanvasRenderingContext2D } from 'skia-canvas'
import * as YogaTypes from 'yoga-layout'
import { Style } from '@/constant/common.const.js'
import type { BoxProps, RootProps, CanvasElement, CartesianChartData, PieChartDataPoint, ChartType, ChartDataset } from '@/canvas/canvas.type.js'

export const drawBorders = ({
  ctx,
  node,
  x,
  y,
  width,
  height,
  radii,
  borderColor,
  borderStyle,
}: {
  ctx: CanvasRenderingContext2D
  node: YogaTypes.Node
  x: number
  y: number
  width: number
  height: number
  radii: {
    TopLeft: number
    TopRight: number
    BottomLeft: number
    BottomRight: number
  }
  borderColor: BoxProps['borderColor']
  borderStyle: BoxProps['borderStyle']
}) => {
  const borderAll = node.getBorder(YogaTypes.Edge.All) || 0
  const borderTop = Math.max(0, node.getBorder(YogaTypes.Edge.Top) || borderAll)
  const borderRight = Math.max(0, node.getBorder(YogaTypes.Edge.Right) || borderAll)
  const borderBottom = Math.max(0, node.getBorder(YogaTypes.Edge.Bottom) || borderAll)
  const borderLeft = Math.max(0, node.getBorder(YogaTypes.Edge.Left) || borderAll)

  const hasBorder = borderTop > 0 || borderRight > 0 || borderBottom > 0 || borderLeft > 0
  const boxSizing = node.getBoxSizing()

  if (hasBorder && borderColor) {
    ctx.strokeStyle = borderColor
    ctx.lineCap = 'butt'
    ctx.lineJoin = 'miter' // Use miter for sharp corners unless rounded

    const setDash = (width: number) => {
      if (borderStyle === Style.Border.Dotted && width > 0) {
        // Dotted: tight spacing with round caps for circular dots
        ctx.lineCap = 'round'
        ctx.setLineDash([0, width * 2]) // 0-length dash with spacing creates dots with round caps
      } else if (borderStyle === Style.Border.Dashed && width > 0) {
        ctx.lineCap = 'butt'
        const dashLength = Math.max(2, width * 1.5)
        const gapLength = Math.max(1, width)
        ctx.setLineDash([dashLength, gapLength])
      } else {
        ctx.lineCap = 'butt'
        ctx.setLineDash([]) // Solid line
      }
    }

    /**
     * Draws a rounded corner arc for the border.
     * @param cx The x-coordinate of the visual center of the corner curve.
     * @param cy The y-coordinate of the visual center of the corner curve.
     * @param radius The visual radius of the corner curve.
     * @param startAngle The starting angle of the arc in radians.
     * @param endAngle The ending angle of the arc in radians.
     * @param border1 The border width leading into the corner.
     * @param border2 The border width leading out of the corner.
     */
    const drawCornerArc = (cx: number, cy: number, radius: number, startAngle: number, endAngle: number, border1: number, border2: number) => {
      if (radius <= 0) return

      const cornerWidth = Math.max(border1, border2)
      if (cornerWidth <= 0) return

      let centerlineArcRadius: number

      if (boxSizing === Style.BoxSizing.ContentBox) {
        // For content-box, the border is outside the box, so the centerline radius is the visual radius plus half the border width.
        centerlineArcRadius = radius + cornerWidth / 2
      } else {
        // For border-box, the border is inside the box, so the centerline radius is the visual radius minus half the border width.
        // Ensure the centerline radius is not negative.
        centerlineArcRadius = Math.max(0, radius - cornerWidth / 2)

        if (centerlineArcRadius <= 0 && radius > 0) {
          // Draw cap for border-box when border is thicker than radius allows for centerline arc
          ctx.fillStyle = borderColor! // Use border color for fill
          ctx.beginPath()
          // Cap is centered on the visual corner center with the visual radius
          ctx.arc(cx, cy, radius, 0, 2 * Math.PI)
          ctx.fill()
          return // Cap drawn, skip arc stroke
        }
      }
      // Draw the normal arc stroke using the calculated centerline radius
      ctx.beginPath()
      ctx.lineWidth = cornerWidth
      setDash(cornerWidth)
      ctx.arc(cx, cy, centerlineArcRadius, startAngle, endAngle)
      ctx.stroke()
    }

    /**
     * Draws a straight line segment for the border.
     * @param x1 The x-coordinate of the starting point.
     * @param y1 The y-coordinate of the starting point.
     * @param x2 The x-coordinate of the ending point.
     * @param y2 The y-coordinate of the ending point.
     * @param borderWidth The width of the border.
     */
    const drawLine = (x1: number, y1: number, x2: number, y2: number, borderWidth: number) => {
      if (borderWidth <= 0) return
      ctx.beginPath()
      ctx.lineWidth = borderWidth
      setDash(borderWidth)
      ctx.moveTo(x1, y1)
      ctx.lineTo(x2, y2)
      ctx.stroke()
    }

    // Calculate half-border widths
    const halfBt = borderTop / 2
    const halfBr = borderRight / 2
    const halfBb = borderBottom / 2
    const halfBl = borderLeft / 2

    // Calculate effective visual radii, clamped to half dimensions of the *layout box*
    const maxRadiusX = width / 2 // This matches CSS behavior where radius is relative to the box it's applied to.
    const maxRadiusY = height / 2
    const rTL = Math.max(0, Math.min(radii.TopLeft, maxRadiusX, maxRadiusY))
    const rTR = Math.max(0, Math.min(radii.TopRight, maxRadiusX, maxRadiusY))
    const rBR = Math.max(0, Math.min(radii.BottomRight, maxRadiusX, maxRadiusY))
    const rBL = Math.max(0, Math.min(radii.BottomLeft, maxRadiusX, maxRadiusY))

    // --- Draw border segments based on boxSizing ---
    // For content-box, coordinates are offset *outwards* from x, y, width, height
    if (boxSizing === Style.BoxSizing.ContentBox) {
      // Top line segment
      void drawLine(x + rTL, y - halfBt, x + width - rTR, y - halfBt, borderTop)
      // Right line segment
      void drawLine(x + width + halfBr, y + rTR, x + width + halfBr, y + height - rBR, borderRight)
      // Bottom line segment
      void drawLine(x + width - rBR, y + height + halfBb, x + rBL, y + height + halfBb, borderBottom)
      // Left line segment
      void drawLine(x - halfBl, y + height - rBL, x - halfBl, y + rTL, borderLeft)

      void drawCornerArc(x + rTL, y + rTL, rTL, Math.PI, 1.5 * Math.PI, borderLeft, borderTop)
      void drawCornerArc(x + width - rTR, y + rTR, rTR, 1.5 * Math.PI, 2 * Math.PI, borderTop, borderRight)
      void drawCornerArc(x + width - rBR, y + height - rBR, rBR, 0, 0.5 * Math.PI, borderRight, borderBottom)
      void drawCornerArc(x + rBL, y + height - rBL, rBL, 0.5 * Math.PI, Math.PI, borderBottom, borderLeft)
    } else {
      // For border-box, coordinates are offset *inwards* from x, y, width, height
      // Top line segment
      void drawLine(x + rTL, y + halfBt, x + width - rTR, y + halfBt, borderTop)
      // Right line segment
      void drawLine(x + width - halfBr, y + rTR, x + width - halfBr, y + height - rBR, borderRight)
      // Bottom line segment
      void drawLine(x + width - rBR, y + height - halfBb, x + rBL, y + height - halfBb, borderBottom)
      // Left line segment
      void drawLine(x + halfBl, y + height - rBL, x + halfBl, y + rTL, borderLeft)

      // Draw corner arcs (centers relative to layout box corners, adjusted for inward border)
      // Pass visual radius (rTL, rTR etc.) to drawCornerArc
      void drawCornerArc(x + rTL, y + rTL, rTL, Math.PI, 1.5 * Math.PI, borderLeft, borderTop) // Top-Left
      void drawCornerArc(x + width - rTR, y + rTR, rTR, 1.5 * Math.PI, 2 * Math.PI, borderTop, borderRight) // Top-Right
      void drawCornerArc(x + width - rBR, y + height - rBR, rBR, 0, 0.5 * Math.PI, borderRight, borderBottom) // Bottom-Right
      void drawCornerArc(x + rBL, y + height - rBL, rBL, 0.5 * Math.PI, Math.PI, borderBottom, borderLeft) // Bottom-Left
    }
  }
}

/**
 * Draws an optimized rounded rectangle path on the canvas context.
 * Automatically clamps radius values to prevent visual artifacts based on box dimensions.
 * Uses arc-based rendering for crisp corners and consistent border appearance.
 * @param ctx The canvas 2D rendering context to draw on
 * @param x Left position of the rectangle
 * @param y Top position of the rectangle
 * @param width Width of the rectangle
 * @param height Height of the rectangle
 * @param radii Corner radius values for each corner. Values are clamped to box constraints.
 */
export const drawRoundedRectPath = (
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radii: { TopLeft: number; TopRight: number; BottomRight: number; BottomLeft: number },
) => {
  if (width <= 0 || height <= 0) {
    ctx.beginPath()
    ctx.rect(x, y, width, height)
    return
  }

  ctx.beginPath()

  // Clamp radius values to prevent visual artifacts
  const maxRadius = Math.min(width / 2, height / 2)
  const clampedTL = Math.max(0, Math.min(radii.TopLeft, maxRadius))
  const clampedTR = Math.max(0, Math.min(radii.TopRight, maxRadius))
  const clampedBR = Math.max(0, Math.min(radii.BottomRight, maxRadius))
  const clampedBL = Math.max(0, Math.min(radii.BottomLeft, maxRadius))

  ctx.moveTo(x + clampedTL, y)

  // Draw top edge and top-right corner
  ctx.lineTo(x + width - clampedTR, y)
  clampedTR > 0 ? ctx.arc(x + width - clampedTR, y + clampedTR, clampedTR, 1.5 * Math.PI, 0) : ctx.lineTo(x + width, y)

  // Draw right edge and bottom-right corner
  ctx.lineTo(x + width, y + height - clampedBR)
  clampedBR > 0 ? ctx.arc(x + width - clampedBR, y + height - clampedBR, clampedBR, 0, 0.5 * Math.PI) : ctx.lineTo(x + width, y + height)

  // Draw bottom edge and bottom-left corner
  ctx.lineTo(x + clampedBL, y + height)
  clampedBL > 0 ? ctx.arc(x + clampedBL, y + height - clampedBL, clampedBL, 0.5 * Math.PI, Math.PI) : ctx.lineTo(x, y + height)

  // Draw left edge and top-left corner
  ctx.lineTo(x, y + clampedTL)
  clampedTL > 0 ? ctx.arc(x + clampedTL, y + clampedTL, clampedTL, Math.PI, 1.5 * Math.PI) : ctx.lineTo(x, y)

  ctx.closePath()
}

/**
 * Calculates border radius values from props
 * @param radiusProp Border radius property value
 * @returns Calculated border radii for all corners
 */
export const parseBorderRadius = (
  radiusProp: BoxProps['borderRadius'],
): {
  TopLeft: number
  TopRight: number
  BottomRight: number
  BottomLeft: number
} => {
  const radii = { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 }
  if (typeof radiusProp === 'number') {
    radii.TopLeft = radii.TopRight = radii.BottomRight = radii.BottomLeft = Math.max(0, radiusProp)
  } else if (typeof radiusProp === 'object' && radiusProp !== null) {
    radii.TopLeft = Math.max(0, radiusProp.TopLeft ?? 0)
    radii.TopRight = Math.max(0, radiusProp.TopRight ?? 0)
    radii.BottomRight = Math.max(0, radiusProp.BottomRight ?? 0)
    radii.BottomLeft = Math.max(0, radiusProp.BottomLeft ?? 0)
  }
  return radii
}

/**
 * Parses a percentage value or a number, returning the calculated value based on the base.
 * @param value The value to parse, can be a number, a percentage string, or undefined.
 * @param base The base value to calculate the percentage from.
 * @returns The parsed number, or 0 if the value is not a number or a valid percentage.
 */
export function parsePercentage(value: number | string | undefined, base: number): number {
  if (typeof value === 'number') {
    return value
  }
  if (typeof value === 'string' && value.endsWith('%')) {
    return base !== 0 ? (parseFloat(value) / 100) * base : 0
  }
  return 0
}

/**
 * Pre-processes a RootProps descriptor tree on the main thread before postMessage.
 * Executes function props against available data, stores results as serializable fields,
 * then strips remaining functions as a safety net.
 */
export class WorkerPreProcessor {
  private static readonly CHART_COLORS = ['#FF6384', '#36A2EB', '#FFCE56', '#4BC0C0', '#9966FF', '#FF9F40', '#C9CBCF']

  static process(props: RootProps): RootProps {
    const cloned = { ...props }
    if (cloned.children) {
      const childArray = Array.isArray(cloned.children) ? cloned.children : [cloned.children]
      cloned.children = childArray.map(child => {
        if (child && typeof child === 'object' && '__type' in child) {
          return this.processDescriptor(child as CanvasElement)
        }
        return child
      }) as any
    }
    return this.stripNonSerializable(cloned)
  }

  private static processDescriptor(desc: CanvasElement): CanvasElement {
    if (desc.__type === 'Chart') {
      return this.processChartDescriptor(desc)
    }

    if ('children' in desc && desc.children) {
      return { ...desc, children: desc.children.map(c => this.processDescriptor(c)) } as CanvasElement
    }

    return desc
  }

  private static processChartDescriptor(desc: CanvasElement & { __type: 'Chart' }): CanvasElement {
    const props = { ...desc.props }
    const options = { ...(props.options || {}) } as Record<string, any>
    const chartType = props.type as ChartType
    const data = props.data as any

    // xAxisLabelFormatter
    if (typeof options.xAxisLabelFormatter === 'function' && (chartType === 'bar' || chartType === 'line')) {
      const cartData = data as CartesianChartData
      if (cartData.labels) {
        options._preComputedXAxisLabels = cartData.labels.map((label: string, index: number) => {
          try {
            return options.xAxisLabelFormatter(label, index)
          } catch (err) {
            console.warn(`[WorkerPreProcessor] xAxisLabelFormatter threw for label "${label}" at index ${index}:`, err)
            return label
          }
        })
      }
      delete options.xAxisLabelFormatter
    }

    // yAxisLabelFormatter
    if (typeof options.yAxisLabelFormatter === 'function' && (chartType === 'bar' || chartType === 'line')) {
      const cartData = data as CartesianChartData
      const maxValue = Math.max(...cartData.datasets.flatMap((d: ChartDataset) => d.data))
      const labels: string[] = []
      for (let i = 0; i <= 5; i++) {
        const value = maxValue - (maxValue / 5) * i
        try {
          labels.push(options.yAxisLabelFormatter(value))
        } catch (err) {
          console.warn(`[WorkerPreProcessor] yAxisLabelFormatter threw for value ${value}:`, err)
          labels.push(String(value))
        }
      }
      options._preComputedYAxisLabels = labels
      delete options.yAxisLabelFormatter
    }

    // renderLegendItem
    if (typeof options.renderLegendItem === 'function') {
      if (chartType === 'bar' || chartType === 'line') {
        const cartData = data as CartesianChartData
        options._preComputedLegendItems = cartData.datasets.map((item: ChartDataset, index: number) => {
          const color = item.color || this.generateColor(index)
          try {
            const result = options.renderLegendItem({ item, index, color })
            return this.nodeToDescriptor(result)
          } catch (err) {
            console.warn(`[WorkerPreProcessor] renderLegendItem threw at index ${index}:`, err)
            return null
          }
        })
      } else {
        const pieData = data as PieChartDataPoint[]
        options._preComputedLegendItems = pieData.map((item: PieChartDataPoint, index: number) => {
          const color = item.color || this.generateColor(index)
          try {
            const result = options.renderLegendItem({ item, index, color })
            return this.nodeToDescriptor(result)
          } catch (err) {
            console.warn(`[WorkerPreProcessor] renderLegendItem threw at index ${index}:`, err)
            return null
          }
        })
      }
      delete options.renderLegendItem
    }

    // renderLabelItem
    if (typeof options.renderLabelItem === 'function') {
      if (chartType === 'bar' || chartType === 'line') {
        const cartData = data as CartesianChartData
        options._preComputedLabelItems = cartData.labels.map((label: string, index: number) => {
          try {
            const result = options.renderLabelItem({ item: label, index })
            return this.nodeToDescriptor(result)
          } catch (err) {
            console.warn(`[WorkerPreProcessor] renderLabelItem threw at index ${index}:`, err)
            return null
          }
        })
      } else {
        const pieData = data as PieChartDataPoint[]
        options._preComputedLabelItems = pieData.map((item: PieChartDataPoint, index: number) => {
          try {
            const result = options.renderLabelItem({ item, index })
            return this.nodeToDescriptor(result)
          } catch (err) {
            console.warn(`[WorkerPreProcessor] renderLabelItem threw at index ${index}:`, err)
            return null
          }
        })
      }
      delete options.renderLabelItem
    }

    // renderValueItem
    if (typeof options.renderValueItem === 'function' && (chartType === 'bar' || chartType === 'line')) {
      const cartData = data as CartesianChartData
      options._preComputedValueItems = cartData.datasets.map((dataset: ChartDataset, datasetIndex: number) =>
        dataset.data.map((value: number, index: number) => {
          try {
            const result = options.renderValueItem({ item: value, index, datasetIndex })
            return this.nodeToDescriptor(result)
          } catch (err) {
            console.warn(`[WorkerPreProcessor] renderValueItem threw at dataset ${datasetIndex}, index ${index}:`, err)
            return null
          }
        }),
      )
      delete options.renderValueItem
    }

    props.options = options
    return { ...desc, props } as CanvasElement
  }

  /**
   * Converts a BoxNode instance or CanvasElement to a CanvasElement.
   * Public API functions (Box(), Text(), etc.) already return CanvasElements,
   * but if someone returns an actual class instance, we convert it via initialProps.
   */
  private static nodeToDescriptor(node: any): CanvasElement | null | undefined {
    if (!node) return node
    // Already a CanvasElement (has __type)
    if (typeof node === 'object' && '__type' in node) return node as CanvasElement
    // Class instance (e.g. BoxNode, TextNode, ImageNode) — convert via duck-typing
    if (typeof node === 'object' && node.constructor !== Object) {
      if ('initialProps' in node && 'name' in node) {
        const { children, ...rest } = node.initialProps || {}
        const childArray = Array.isArray(children) ? children : children ? [children] : []
        const convertedChildren = childArray
          .filter((c: any) => c)
          .map((c: any) => this.nodeToDescriptor(c))
          .filter((c: any): c is CanvasElement => !!c)

        const name = node.name as string
        const type =
          name === 'TextNode'
            ? 'Text'
            : name === 'Row'
              ? 'Row'
              : name === 'Image'
                ? 'Image'
                : rest.flexDirection === Style.FlexDirection.Column
                  ? 'Column'
                  : 'Box'
        if (type === 'Text') {
          // TextNode stores original text in segments — fall back to joined segment text
          const text = node.segments?.map((s: any) => s.text).join('') ?? ''
          return { __type: 'Text', text, props: rest } as CanvasElement
        }
        if (type === 'Image') {
          return { __type: 'Image', props: rest } as CanvasElement
        }
        return {
          __type: type,
          props: rest,
          ...(convertedChildren.length > 0 ? { children: convertedChildren } : {}),
        } as CanvasElement
      }
      console.warn('[WorkerPreProcessor] Render function returned an unrecognized class instance. Use descriptor functions (Box(), Text(), etc.) instead.')
      return null
    }
    return node
  }

  private static generateColor(index: number): string {
    return this.CHART_COLORS[index % this.CHART_COLORS.length]
  }

  static stripNonSerializable<T>(obj: T): T {
    if (obj === null || obj === undefined) return obj
    if (typeof obj === 'function') return undefined as unknown as T
    if (typeof obj === 'symbol') return undefined as unknown as T
    if (typeof obj !== 'object') return obj

    // Preserve binary data types
    if (Buffer.isBuffer(obj)) return obj
    if (obj instanceof ArrayBuffer) return obj
    if (ArrayBuffer.isView(obj)) return obj

    if (Array.isArray(obj)) {
      return obj.map(item => this.stripNonSerializable(item)) as unknown as T
    }

    const result: Record<string, unknown> = {}
    for (const key of Object.keys(obj as Record<string, unknown>)) {
      const value = (obj as Record<string, unknown>)[key]
      if (typeof value === 'function' || typeof value === 'symbol') continue
      result[key] = this.stripNonSerializable(value)
    }
    return result as T
  }
}
