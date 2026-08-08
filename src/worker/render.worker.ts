import { parentPort, workerData, type MessagePort } from 'node:worker_threads'
import { Comlink, nodeEndpoint } from '@/worker/comlink.setup.js'
import { restoreFunctions } from '@/worker/comlink.pool.js'
import { createCanvasHandlers } from '@/worker/canvas-handlers.js'
import { RootNode } from '@/canvas/root.canvas.js'
import type { Canvas } from 'skia-canvas'
import type { WorkerAPI, CallFn, SyncRequest, SyncResponse } from '@/worker/worker.types.js'

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

/**
 * The synchronous channel runs on its own port because Comlink has claimed `parentPort`, and
 * because Comlink is promise-only and so cannot carry a `*Sync` call at all.
 */
const syncPort = (workerData as { syncPort?: MessagePort } | null)?.syncPort
syncPort?.on('message', (request: SyncRequest) => {
  let response: SyncResponse
  try {
    response = { result: handlers.callSync(request.canvasId, request.method, request.args) }
  } catch (err) {
    response = { error: err instanceof Error ? err.message : String(err) }
  }

  // Post before raising the flag, never after. The caller wakes inside `Atomics.wait` and reads
  // the port on the very next line, so a reply still in flight would read as "no reply" and throw.
  // Every path has to answer, including the failure one, or the caller blocks until its timeout.
  syncPort.postMessage(response)
  Atomics.store(request.signal, 0, 1)
  Atomics.notify(request.signal, 0)
})
