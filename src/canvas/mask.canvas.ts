import { Canvas, Path2D, type CanvasRenderingContext2D } from 'meo-skia-canvas'
import { createGradient, type GradientBox } from '@/canvas/gradient.canvas.js'
import type { Gradient, Mask } from '@/canvas/canvas.type.js'

/** Half of something, for the centre of a box or the radius of a shape inscribed in it. */
const HALF = 0.5

/** A full turn, for the arc that closes a circle. */
const FULL_TURN = 2 * Math.PI

/**
 * The fallback when a context reports no scale at all.
 *
 * A degenerate transform would size an offscreen canvas at zero pixels, which cannot be allocated —
 * so the mask would fail where an unmasked node would merely have drawn nothing.
 */
const UNSCALED = 1

/** Narrows a mask to the gradient form, which is the one that has to composite rather than clip. */
export function isGradientMask(mask: Mask): mask is { gradient: Gradient } {
  return typeof mask === 'object' && 'gradient' in mask
}

/**
 * The mask as a path in canvas coordinates, or `null` when it is a gradient.
 *
 * Shapes are inscribed in the node's box rather than given their own geometry: a circle in a square
 * is a circle, and in an oblong it is the largest circle that fits. That keeps a mask something you
 * put on a node you have already sized, instead of a second set of dimensions to keep in step with
 * the first.
 *
 * Path data is read in the node's own coordinates — `0,0` is its top-left corner, not the canvas's
 * — and translated here. Anything else would make a path depend on where its node happened to land.
 */
export function maskPath(mask: Mask, box: GradientBox): Path2D | null {
  const { x, y, width, height } = box

  if (isGradientMask(mask)) return null

  if (typeof mask === 'string' || 'path' in mask) {
    const data = typeof mask === 'string' ? mask : mask.path
    const path = new Path2D()
    // Translated by adding the node's origin to the path's own transform, which keeps the caller's
    // coordinates untouched — the alternative, translating the context, would also move whatever
    // the path is later intersected with.
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

/** The fill rule as `clip` takes it, which is where the two spellings have to agree. */
type ClipFillRule = Parameters<CanvasRenderingContext2D['clip']>[1]

/** How the fill rule reaches `clip`, defaulting the way the Canvas API does. */
export function maskFillRule(mask: Mask): ClipFillRule {
  return typeof mask === 'object' && 'fillRule' in mask && mask.fillRule ? mask.fillRule : 'nonzero'
}

/**
 * The horizontal and vertical scale a context is currently drawing at.
 *
 * An offscreen canvas is allocated in device pixels, and the context it will be drawn back onto is
 * already scaled — by `Root`'s `scale` prop, and by any `transform` above this node. Sizing the
 * offscreen from the layout box alone would render a masked node at one pixel per point and then
 * stretch it, which on a 2x card is visibly softer than everything around it.
 */
export function contextScale(ctx: CanvasRenderingContext2D): { x: number; y: number } {
  const { a, b, c, d } = ctx.getTransform()
  // Column lengths rather than `a` and `d`, so a rotated context reports the scale it draws at
  // instead of the cosine of its angle.
  return { x: Math.hypot(a, b) || UNSCALED, y: Math.hypot(c, d) || UNSCALED }
}

/**
 * Draws content through a gradient's alpha.
 *
 * A gradient cannot clip — clipping is a yes-or-no test per pixel, and the whole point of a
 * gradient mask is the answers in between. So the node is drawn into an offscreen canvas of its
 * own, multiplied by the gradient with `destination-in`, and the result composited back.
 *
 * The offscreen is exactly the node's box: content that a `transform` pushes outside that box is
 * cut off, which is the one place this differs from clipping and is why it is documented rather
 * than discovered.
 * @param draw Renders the node into whichever context it is handed.
 * @returns Whether the mask was applied; `false` means the caller should draw normally instead.
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

  // The node draws at its layout position, which is somewhere on the page rather than at the
  // offscreen's origin. Translating by its box lets the same drawing code run unchanged.
  offCtx.scale(scale.x, scale.y)
  offCtx.translate(-x, -y)

  await draw(offCtx)

  const { gradient: alpha, reason } = createGradient(offCtx, gradient, box)
  if (!alpha) {
    console.warn(`${owner} ${reason} Mask ignored.`)
    return false
  }

  // `destination-in` keeps what is already there in proportion to what arrives, so filling the box
  // with the gradient multiplies the node by its alpha. Colour is irrelevant; only alpha survives.
  offCtx.globalCompositeOperation = 'destination-in'
  offCtx.fillStyle = alpha
  offCtx.fillRect(x, y, width, height)

  // Back at the node's own size: the bitmap is in device pixels and the destination context is
  // scaled, so the two agree and nothing is resampled.
  ctx.drawImage(offscreen, x, y, width, height)
  return true
}
