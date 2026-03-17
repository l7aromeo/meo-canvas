/**
 * Worker thread entry point for off-main-thread canvas rendering.
 * Receives serialized RootProps (with NodeDescriptor children), builds the
 * node tree, renders to canvas, encodes to PNG, and posts the buffer back.
 */
import { parentPort } from 'worker_threads'
import { RootNode } from '@/canvas/root.canvas.util.js'
import type { RootProps } from '@/canvas/canvas.type.js'

if (!parentPort) {
  throw new Error('[render.worker] Must be run as a worker thread')
}

parentPort.on('message', async ({ id, props }: { id: number; props: RootProps }) => {
  try {
    const canvas = await new RootNode(props).render()
    const buffer = canvas.toBufferSync('png')
    parentPort!.postMessage({ id, buffer })
  } catch (err: any) {
    parentPort!.postMessage({ id, error: String(err) })
  }
})
