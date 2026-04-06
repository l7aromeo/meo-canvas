import { parentPort } from 'node:worker_threads'
import { Comlink, nodeEndpoint } from '@/worker/comlink.setup.js'
import { RootNode } from '@/canvas/root.canvas.js'
import type { Canvas } from 'skia-canvas'
import type { WorkerAPI, RenderResult } from '@/worker/worker.types.js'

if (!parentPort) {
  throw new Error('[render.worker] Must be run as a worker thread')
}

const canvases = new Map<number, Canvas>()
let nextCanvasId = 0

const api: WorkerAPI = {
  async render(props) {
    const canvas = await new RootNode(props).render()
    const canvasId = nextCanvasId++
    canvases.set(canvasId, canvas)
    const result: RenderResult = {
      canvasId,
      buffer: canvas.toBufferSync('png'),
      width: canvas.width,
      height: canvas.height,
    }
    return result
  },

  async callOnCanvas(canvasId, method, args) {
    const canvas = canvases.get(canvasId)
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

  releaseCanvas(canvasId) {
    canvases.delete(canvasId)
  },
}

Comlink.expose(api, nodeEndpoint(parentPort))
