import type { CanvasRenderingContext2D } from 'meo-skia-canvas'
import * as YogaTypes from 'yoga-layout'
import { Style } from '@/constant/common.const.js'
import type { BoxProps } from '@/canvas/canvas.type.js'

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
