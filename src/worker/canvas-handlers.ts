import type { RootProps } from '@/canvas/canvas.type.js'
import type { Canvas } from 'phyron-skia-canvas'
import type { RenderResult } from '@/worker/worker.types.js'

export interface CanvasHandlerDeps {
  canvases: Map<number, Canvas>
  getNextCanvasId: () => number
  renderRoot: (props: RootProps) => Promise<Canvas>
}

export function createCanvasHandlers(deps: CanvasHandlerDeps) {
  return {
    async render(props: RootProps): Promise<RenderResult> {
      const canvas = await deps.renderRoot(props)
      const canvasId = deps.getNextCanvasId()
      deps.canvases.set(canvasId, canvas)
      return {
        canvasId,
        buffer: canvas.toBufferSync('png'),
        width: canvas.width,
        height: canvas.height,
      }
    },

    async callOnCanvas(canvasId: number, method: string, args: unknown[]): Promise<Buffer | string | void> {
      const canvas = deps.canvases.get(canvasId)
      if (!canvas) {
        throw new Error(`[render.worker] Canvas ${canvasId} not found`)
      }
      switch (method) {
        case 'toBuffer':
          return canvas.toBuffer(...(args as [any, any?]))
        case 'toURL':
          return canvas.toURL(...(args as [any, any?]))
        case 'toFile':
          await canvas.toFile(...(args as [string, any?]))
          return
        case 'toSharp':
          return await canvas.toSharp(...(args as [any?])).toBuffer()
        default:
          throw new Error(`[render.worker] Unknown method: ${method}`)
      }
    },

    releaseCanvas(canvasId: number): void {
      deps.canvases.delete(canvasId)
    },
  }
}
