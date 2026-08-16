import { Canvas, Path2D, type CanvasRenderingContext2D } from 'meo-skia-canvas'
import { createGradient, type GradientBox } from '@/canvas/gradient.canvas.js'
import type { Gradient, Mask } from '@/canvas/canvas.type.js'

/** Centre of a box, and radius of a shape inscribed in one. */
const HALF = 0.5

/** Sweep of a closed arc. */
const FULL_TURN = 2 * Math.PI

/** Scale used when a context reports none; an offscreen canvas cannot be zero pixels wide. */
const UNSCALED = 1

/** True for the gradient form, which composites rather than clips. */
export function isGradientMask(mask: Mask): mask is { gradient: Gradient } {
  return typeof mask === 'object' && 'gradient' in mask
}

/**
 * The mask as a path in canvas coordinates, or `null` for a gradient.
 *
 * Shapes are inscribed in the node's box: a circle takes the shorter side, an ellipse both. Path
 * data is in the node's own coordinates, where `0,0` is its top-left corner.
 */
export function maskPath(mask: Mask, box: GradientBox): Path2D | null {
  const { x, y, width, height } = box

  if (isGradientMask(mask)) return null

  if (typeof mask === 'string' || 'path' in mask) {
    const data = typeof mask === 'string' ? mask : mask.path
    const path = new Path2D()
    // Translation carried by the path rather than the context, which would also move whatever the
    // path is later intersected with.
    path.addPath(new Path2D(data), { a: 1, b: 0, c: 0, d: 1, e: x, f: y })
    return path
  }

  const path = new Path2D()
  const centerX = x + width * HALF
  const centerY = y + height * HALF

  if (mask.shape === 'circle') {
    path.arc(centerX, centerY, Math.min(width, height) * HALF, 0, FULL_TURN)
  } else {
    path.ellipse(centerX, centerY, width * HALF, height * HALF, 0, 0, FULL_TURN)
  }

  return path
}

/** Fill rule in the form `clip` accepts. */
type ClipFillRule = Parameters<CanvasRenderingContext2D['clip']>[1]

/** The mask's fill rule, `nonzero` unless it names one. */
export function maskFillRule(mask: Mask): ClipFillRule {
  return typeof mask === 'object' && 'fillRule' in mask && mask.fillRule ? mask.fillRule : 'nonzero'
}

/**
 * Horizontal and vertical scale a context draws at, from `Root`'s `scale` and any enclosing
 * `transform`. An offscreen canvas is sized in device pixels, so it needs these to match.
 */
export function contextScale(ctx: CanvasRenderingContext2D): { x: number; y: number } {
  const { a, b, c, d } = ctx.getTransform()
  // Column magnitude, so a rotated context reports its scale rather than the cosine of its angle.
  return { x: Math.hypot(a, b) || UNSCALED, y: Math.hypot(c, d) || UNSCALED }
}

/**
 * Draws content through a gradient's alpha: the node into an offscreen canvas of its box,
 * multiplied by the gradient with `destination-in`, composited back.
 *
 * Bounded by the node's box, so content a `transform` pushes outside it is cut off.
 * @param draw Renders the node into whichever context it is handed.
 * @returns Whether the mask was applied; `false` means draw normally instead.
 */
export async function drawWithGradientMask(
  ctx: CanvasRenderingContext2D,
  gradient: Gradient,
  box: GradientBox,
  draw: (target: CanvasRenderingContext2D) => Promise<void>,
  owner: string,
): Promise<boolean> {
  const { x, y, width, height } = box
  const scale = contextScale(ctx)

  const pixelWidth = Math.ceil(width * scale.x)
  const pixelHeight = Math.ceil(height * scale.y)
  if (pixelWidth <= 0 || pixelHeight <= 0) return false

  const offscreen = new Canvas(pixelWidth, pixelHeight)
  const offCtx = offscreen.getContext('2d')
  offCtx.imageSmoothingEnabled = true
  offCtx.imageSmoothingQuality = 'high'

  // The node draws at its page position; translating by its box maps that onto the offscreen.
  offCtx.scale(scale.x, scale.y)
  offCtx.translate(-x, -y)

  await draw(offCtx)

  const { gradient: alpha, reason } = createGradient(offCtx, gradient, box)
  if (!alpha) {
    console.warn(`${owner} ${reason} Mask ignored.`)
    return false
  }

  // `destination-in` keeps existing pixels in proportion to arriving alpha; colour is discarded.
  offCtx.globalCompositeOperation = 'destination-in'
  offCtx.fillStyle = alpha
  offCtx.fillRect(x, y, width, height)

  // Drawn at the node's size: the bitmap is in device pixels and this context is scaled to match,
  // so nothing is resampled.
  ctx.drawImage(offscreen, x, y, width, height)
  return true
}
