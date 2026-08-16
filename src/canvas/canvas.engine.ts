import { Canvas, type CanvasRenderingContext2D } from 'meo-skia-canvas'

/** Engine options a canvas is constructed with, named by the renderer's own constructor. */
export type CanvasEngineOptions = NonNullable<ConstructorParameters<typeof Canvas>[2]>

/** Builds a canvas, passing options only when there are some to pass. */
export function createCanvas(width: number, height: number, options?: CanvasEngineOptions): Canvas {
  return options ? new Canvas(width, height, options) : new Canvas(width, height)
}

/**
 * The engine settings of the canvas a context draws into, for an offscreen that will be composited
 * back onto it. `undefined` when the context cannot say, leaving the renderer to its defaults.
 *
 * A float destination drawn through an eight-bit offscreen would clip the colour it was chosen to
 * keep, and an offscreen on a different backend resolves anti-aliased edges slightly differently.
 */
export function mirrorEngine(ctx: CanvasRenderingContext2D): CanvasEngineOptions | undefined {
  const canvas = ctx.canvas as Canvas | undefined
  if (!canvas) return undefined

  const { colorType, colorSpace, gpu } = canvas
  return { colorType, colorSpace, gpu }
}
