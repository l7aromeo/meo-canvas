/**
 * Worker thread entry point for off-main-thread canvas rendering.
 *
 * Message protocol (main → worker):
 *   { type: 'render', taskId, props }                      — render and keep Canvas alive
 *   { type: 'call',   taskId, canvasId, method, args }     — call a method on a live Canvas
 *   { type: 'release', canvasId }                          — free Canvas from memory
 *
 * Responses (worker → main):
 *   WorkerRenderResponse  — render complete (includes pre-encoded PNG buffer)
 *   WorkerCallResponse    — method call result
 *   WorkerErrorResponse   — any failure
 */
import { parentPort } from 'worker_threads'
import { RootNode } from '@/canvas/root.canvas.util.js'
import type { Canvas } from 'skia-canvas'
import type { WorkerRequest, WorkerRenderResponse, WorkerCallResponse, WorkerErrorResponse } from '@/canvas/worker.types.js'

if (!parentPort) {
  throw new Error('[render.worker] Must be run as a worker thread')
}

const canvases = new Map<number, Canvas>()
let nextCanvasId = 0

function reply(msg: WorkerRenderResponse | WorkerCallResponse | WorkerErrorResponse) {
  parentPort!.postMessage(msg)
}

parentPort.on('message', async (msg: WorkerRequest) => {
  if (msg.type === 'render') {
    try {
      const canvas = await new RootNode(msg.props).render()
      const canvasId = nextCanvasId++
      canvases.set(canvasId, canvas)
      reply({ taskId: msg.taskId, canvasId, buffer: canvas.toBufferSync('png'), width: canvas.width, height: canvas.height })
    } catch (err) {
      reply({ taskId: msg.taskId, error: String(err) })
    }
  } else if (msg.type === 'call') {
    const canvas = canvases.get(msg.canvasId)
    if (!canvas) {
      reply({ taskId: msg.taskId, error: `[render.worker] Canvas ${msg.canvasId} not found` })
      return
    }
    try {
      let result: Buffer | string | void
      switch (msg.method) {
        case 'toBuffer':
          result = await canvas.toBuffer(...msg.args)
          break
        case 'toURL':
          result = await canvas.toURL(...msg.args)
          break
        case 'toFile':
          result = await canvas.toFile(...msg.args)
          break
        case 'toSharp':
          // Sharp instances can't be transferred across threads — serialize to buffer
          result = await canvas.toSharp(...msg.args).toBuffer()
          break
      }
      reply({ taskId: msg.taskId, result })
    } catch (err) {
      reply({ taskId: msg.taskId, error: String(err) })
    }
  } else {
    // type === 'release'
    canvases.delete(msg.canvasId)
  }
})
