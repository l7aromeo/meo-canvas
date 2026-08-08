import type { RootProps } from '@/canvas/canvas.type.js'
import type { Canvas } from 'phyron-skia-canvas'
import type { RenderResult } from '@/worker/worker.types.js'

export interface CanvasHandlerDeps {
  canvases: Map<number, Canvas>
  getNextCanvasId: () => number
  renderRoot: (props: RootProps) => Promise<Canvas>
}

/**
 * Canvas methods reachable over the synchronous channel.
 *
 * An allowlist rather than a bare `canvas[method]` dispatch: `method` arrives as a string over a
 * message port, and an open lookup would reach anything on the object or its prototype chain.
 */
const SYNC_METHODS: ReadonlySet<string> = new Set(['toBufferSync', 'toURLSync', 'toDataURLSync', 'toDataURL', 'toFileSync', 'saveAsSync'])

/** Async counterparts, reached through Comlink. */
const ASYNC_METHODS: ReadonlySet<string> = new Set(['toBuffer', 'toURL', 'toFile', 'saveAs'])

export function createCanvasHandlers(deps: CanvasHandlerDeps) {
  const mustGet = (canvasId: number): Canvas => {
    const canvas = deps.canvases.get(canvasId)
    if (!canvas) {
      throw new Error(`[render.worker] Canvas ${canvasId} not found`)
    }
    return canvas
  }

  return {
    async render(props: RootProps): Promise<RenderResult> {
      const canvas = await deps.renderRoot(props)
      const canvasId = deps.getNextCanvasId()
      deps.canvases.set(canvasId, canvas)
      return {
        canvasId,
        width: canvas.width,
        height: canvas.height,
        gpu: canvas.gpu,
        engine: canvas.engine,
      }
    },

    /** Runs a `*Sync` method on the real Canvas. Called from the raw port, off Comlink. */
    callSync(canvasId: number, method: string, args: unknown[]): unknown {
      if (!SYNC_METHODS.has(method)) {
        throw new Error(`[render.worker] ${method}() is not callable synchronously`)
      }
      const canvas = mustGet(canvasId)
      return (canvas[method as keyof Canvas] as (...a: unknown[]) => unknown).apply(canvas, args)
    },

    async callOnCanvas(canvasId: number, method: string, args: unknown[]): Promise<Buffer | string | void> {
      if (!ASYNC_METHODS.has(method)) {
        throw new Error(`[render.worker] Unknown method: ${method}`)
      }
      const canvas = mustGet(canvasId)
      return (canvas[method as keyof Canvas] as (...a: unknown[]) => Promise<Buffer | string | void>).apply(canvas, args)
    },

    releaseCanvas(canvasId: number): void {
      deps.canvases.delete(canvasId)
    },
  }
}
