import type { CanvasGradient, CanvasRenderingContext2D } from 'meo-skia-canvas'
import type { Gradient, GradientDirection } from '@/canvas/canvas.type.js'

/** The node's box in canvas coordinates, which is what a gradient's endpoints are measured against. */
export interface GradientBox {
  x: number
  y: number
  width: number
  height: number
}

/** Where a single colour sits when there is nothing to spread it between. */
const LONE_COLOR_STOP = 0.5

/** Half the diagonal: the centre-to-corner distance, so a radial gradient covers the whole box. */
const DIAGONAL_HALF = 0.5

/** Each named direction as the endpoints it means for a box of the given size. */
const DIRECTIONS: Record<string, (width: number, height: number) => [number, number, number, number]> = {
  'to-right': (w, _h) => [0, 0, w, 0],
  'to-left': (w, _h) => [w, 0, 0, 0],
  'to-bottom': (_w, h) => [0, 0, 0, h],
  'to-top': (_w, h) => [0, h, 0, 0],
  'to-top-right': (w, h) => [0, h, w, 0],
  'to-top-left': (w, h) => [w, h, 0, 0],
  'to-bottom-right': (w, h) => [0, 0, w, h],
  'to-bottom-left': (w, h) => [w, 0, 0, h],
}

/** Resolves a direction to endpoints in the node's own coordinates, or `null` if it names nothing. */
function endpointsFor(direction: GradientDirection, width: number, height: number): [number, number, number, number] | null {
  if (Array.isArray(direction) && direction.length === 4) return direction
  if (typeof direction !== 'string') return null

  const resolve = DIRECTIONS[direction.toLowerCase()]
  return resolve ? resolve(width, height) : null
}

/** A quarter turn. Canvas measures a conic sweep from three o'clock; CSS measures it from twelve. */
const QUARTER_TURN = Math.PI / 2

/** Turns a CSS `from` angle into the one a canvas conic gradient wants. */
function degreesToCanvasAngle(degrees: number): number {
  return (degrees * Math.PI) / 180 - QUARTER_TURN
}

/**
 * A position along one edge of the box, in the node's own coordinates.
 *
 * A percentage is a share of that edge, a plain number below one is the same share written as a
 * fraction, and anything larger is taken as pixels. Missing means the middle.
 */
function alongEdge(value: number | `${number}%` | undefined, extent: number): number {
  if (value === undefined) return extent / 2
  if (typeof value === 'string') return (parseFloat(value) / 100) * extent
  return value <= 1 ? value * extent : value
}

/**
 * A gradient, or the reason there isn't one.
 *
 * The reason is returned rather than warned about: what happens next differs by caller — a
 * background falls back to its colour, a mask is dropped — so the caller pairs the two into one
 * message.
 */
export type GradientResult = { gradient: CanvasGradient; reason?: undefined } | { gradient: null; reason: string }

/**
 * Builds a gradient for a node's box.
 *
 * Shared by the background fill and {@link Mask}: a mask reads the alpha where a background reads
 * the colour, and nothing else differs.
 */
export function createGradient(ctx: CanvasRenderingContext2D, gradient: Gradient, box: GradientBox): GradientResult {
  const { type = 'linear', colors } = gradient
  const direction = 'direction' in gradient ? (gradient.direction ?? 'to-bottom') : 'to-bottom'
  const { x, y, width, height } = box

  if (!colors?.length) return { gradient: null, reason: 'Gradient specified but no colors provided.' }
  if (width <= 0 || height <= 0) return { gradient: null, reason: 'Cannot draw gradient with zero width/height.' }

  let built: CanvasGradient | null = null

  if (type === 'linear') {
    const endpoints = endpointsFor(direction, width, height)
    if (!endpoints) return { gradient: null, reason: `Invalid linear gradient direction: ${JSON.stringify(direction)}.` }

    const [x0, y0, x1, y1] = endpoints
    built = ctx.createLinearGradient(x + x0, y + y0, x + x1, y + y1)
  } else if (type === 'radial') {
    const centerX = x + width / 2
    const centerY = y + height / 2
    const radius = DIAGONAL_HALF * Math.sqrt(width * width + height * height)
    if (radius > 0) built = ctx.createRadialGradient(centerX, centerY, 0, centerX, centerY, radius)
  } else if (type === 'conic') {
    const at = 'at' in gradient ? gradient.at : undefined
    const from = ('from' in gradient ? gradient.from : undefined) ?? 0
    const centerX = x + alongEdge(at?.x, width)
    const centerY = y + alongEdge(at?.y, height)
    built = ctx.createConicGradient(degreesToCanvasAngle(from), centerX, centerY)
  }

  if (!built) return { gradient: null, reason: `Could not create ${type} gradient.` }

  colors.forEach((color, index) => {
    const stop = colors.length > 1 ? Math.max(0, Math.min(1, index / (colors.length - 1))) : LONE_COLOR_STOP
    built.addColorStop(stop, color)
  })

  return { gradient: built }
}
