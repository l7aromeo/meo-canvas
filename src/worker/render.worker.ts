import { parentPort } from 'node:worker_threads'
import { Comlink, nodeEndpoint } from '@/worker/comlink.setup.js'
import { restoreFunctions } from '@/worker/comlink.pool.js'
import { createCanvasHandlers } from '@/worker/canvas-handlers.js'
import { RootNode } from '@/canvas/root.canvas.js'
import type { Canvas } from 'skia-canvas'
import type { WorkerAPI, CallFn } from '@/worker/worker.types.js'

if (!parentPort) {
  throw new Error('[render.worker] Must be run as a worker thread')
}

const canvases = new Map<number, Canvas>()
let nextCanvasId = 0

const handlers = createCanvasHandlers({
  canvases,
  getNextCanvasId: () => nextCanvasId++,
  renderRoot: async props => new RootNode(props).render(),
})

const api: WorkerAPI = {
  async render(props, callFn?: CallFn) {
    const resolved = callFn ? restoreFunctions(props, callFn) : props
    return handlers.render(resolved)
  },

  callOnCanvas: handlers.callOnCanvas.bind(handlers),
  releaseCanvas: handlers.releaseCanvas.bind(handlers),
}

Comlink.expose(api, nodeEndpoint(parentPort))
