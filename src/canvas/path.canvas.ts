import { Path2D, type CanvasRenderingContext2D } from 'meo-skia-canvas'
import { BoxNode } from '@/canvas/layout.canvas.js'
import { createGradient } from '@/canvas/gradient.canvas.js'
import type { BaseProps, CanvasElement, PathPaint, PathProps } from '@/canvas/canvas.type.js'

/** Stroke width when a stroke is asked for without one, matching the Canvas default. */
const DEFAULT_LINE_WIDTH = 1

/**
 * A node that draws SVG path data.
 *
 * Everything a `Box` does — layout, background, border, mask, opacity, transform — still applies;
 * the path is drawn as the node's content, inside its box.
 */
export class PathNode extends BoxNode {
  declare props: PathProps & BaseProps

  constructor(props: PathProps & BaseProps) {
    super({ ...props, name: props.name || 'Path' })
  }

  /**
   * Resolves a paint to something `fillStyle` accepts, measured against the node's box.
   *
   * A gradient failing to build leaves the shape unpainted rather than painted wrongly, and says
   * which of `fill` and `stroke` was dropped.
   */
  private paint(ctx: CanvasRenderingContext2D, paint: PathPaint, box: { x: number; y: number; width: number; height: number }, role: string) {
    if (typeof paint === 'string') return paint

    const { gradient, reason } = createGradient(ctx, paint, box)
    if (!gradient) console.warn(`[PathNode ${this.key}] ${reason} ${role} ignored.`)
    return gradient ?? undefined
  }

  protected override async _renderContent(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    const { d, fill, stroke, lineWidth, fillRule, lineCap, lineJoin, lineDash, lineDashOffset } = this.props
    if (!d) return

    const box = { x, y, width, height }
    const path = new Path2D()
    // Translation carried by the path, so the node's coordinates are its own wherever layout puts it.
    path.addPath(new Path2D(d), { a: 1, b: 0, c: 0, d: 1, e: x, f: y })

    ctx.save()
    try {
      if (fill) {
        const style = this.paint(ctx, fill, box, 'fill')
        if (style) {
          ctx.fillStyle = style
          ctx.fill(path, fillRule ?? 'nonzero')
        }
      }

      if (stroke) {
        const style = this.paint(ctx, stroke, box, 'stroke')
        if (style) {
          ctx.strokeStyle = style
          ctx.lineWidth = lineWidth ?? DEFAULT_LINE_WIDTH
          if (lineCap) ctx.lineCap = lineCap
          if (lineJoin) ctx.lineJoin = lineJoin
          if (lineDash) ctx.setLineDash(lineDash)
          if (lineDashOffset !== undefined) ctx.lineDashOffset = lineDashOffset
          ctx.stroke(path)
        }
      }
    } finally {
      ctx.restore()
    }
  }
}

/**
 * Draws an arbitrary shape from SVG path data — see {@link PathProps}.
 * @example
 * ```ts
 * Path({ d: 'M 0 0 L 100 0 L 50 80 Z', fill: '#38bdf8', width: 100, height: 80 })
 * ```
 */
export const Path = (props: PathProps): CanvasElement => ({
  __type: 'Path',
  props,
})
