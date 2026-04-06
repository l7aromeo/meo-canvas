import { Worker } from 'node:worker_threads'
import { fileURLToPath } from 'node:url'
import * as path from 'node:path'
import { Comlink, nodeEndpoint } from '@/worker/comlink.setup.js'
import type { Remote } from 'comlink'
import type { WorkerAPI, RenderResult } from '@/worker/worker.types.js'
import type { RootProps } from '@/canvas/canvas.type.js'

export interface PoolRenderResult extends RenderResult {
  workerIdx: number
}

interface QueuedTask {
  props: RootProps
  resolve: (result: PoolRenderResult) => void
  reject: (err: Error) => void
}

/**
 * Deeply walks an object tree, wraps any function values with Comlink.proxy(),
 * and tracks wrapped proxies for deterministic cleanup.
 */
export function wrapFunctions<T>(obj: T, proxies: Set<unknown>): T {
  if (obj === null || obj === undefined) return obj
  if (typeof obj === 'function') {
    const wrapped = Comlink.proxy(obj)
    proxies.add(wrapped)
    return wrapped as unknown as T
  }
  if (typeof obj !== 'object') return obj

  // Preserve binary data types — don't walk into them
  if (Buffer.isBuffer(obj)) return obj
  if (obj instanceof ArrayBuffer) return obj
  if (ArrayBuffer.isView(obj)) return obj

  if (Array.isArray(obj)) {
    return obj.map(item => wrapFunctions(item, proxies)) as unknown as T
  }

  const result: Record<string, unknown> = {}
  for (const key of Object.keys(obj as Record<string, unknown>)) {
    result[key] = wrapFunctions((obj as Record<string, unknown>)[key], proxies)
  }
  return result as T
}

/**
 * Pool of Comlink-wrapped worker threads.
 * Manages idle/queue scheduling and proxy lifecycle.
 */
export class ComlinkPool {
  private workers: Worker[] = []
  private endpoints: Remote<WorkerAPI>[] = []
  private idle: number[] = []
  private queue: QueuedTask[] = []

  constructor(size: number) {
    const workerFile = path.join(path.dirname(fileURLToPath(import.meta.url)), '../worker/render.worker.js')

    for (let i = 0; i < size; i++) {
      const worker = new Worker(workerFile)
      const endpoint = Comlink.wrap<WorkerAPI>(nodeEndpoint(worker))
      this.workers.push(worker)
      this.endpoints.push(endpoint)
      this.idle.push(i)
    }
  }

  private acquire(): number | null {
    return this.idle.pop() ?? null
  }

  private release(idx: number) {
    this.idle.push(idx)
    this.drain()
  }

  private drain() {
    while (this.queue.length > 0 && this.idle.length > 0) {
      const task = this.queue.shift()!
      const idx = this.idle.pop()!
      void this.executeRender(idx, task.props, task.resolve, task.reject)
    }
  }

  private async executeRender(idx: number, props: RootProps, resolve: (result: PoolRenderResult) => void, reject: (err: Error) => void) {
    try {
      const result = await this.endpoints[idx].render(props)
      resolve({ ...result, workerIdx: idx })
    } catch (err) {
      reject(err instanceof Error ? err : new Error(String(err)))
    } finally {
      this.release(idx)
    }
  }

  async render(props: RootProps): Promise<PoolRenderResult> {
    const proxies = new Set<unknown>()
    const wrapped = wrapFunctions(props, proxies)

    const cleanup = () => {
      for (const p of proxies) {
        try {
          ;(p as any)[Comlink.releaseProxy]?.()
        } catch {
          // Proxy may already be released
        }
      }
    }

    // Direct path — idle worker available
    const idx = this.acquire()
    if (idx !== null) {
      try {
        const result = await this.endpoints[idx].render(wrapped)
        return { ...result, workerIdx: idx }
      } finally {
        this.release(idx)
        cleanup()
      }
    }

    // Queued path — cleanup AFTER the queued task completes, not before
    return new Promise<PoolRenderResult>((resolve, reject) => {
      this.queue.push({
        props: wrapped,
        resolve: result => {
          cleanup()
          resolve(result)
        },
        reject: err => {
          cleanup()
          reject(err)
        },
      })
    })
  }

  callOnCanvas(workerIdx: number, canvasId: number, method: string, args: unknown[]): Promise<Buffer | string | void> {
    return this.endpoints[workerIdx].callOnCanvas(canvasId, method, args) as Promise<Buffer | string | void>
  }

  releaseCanvas(workerIdx: number, canvasId: number): void {
    this.endpoints[workerIdx].releaseCanvas(canvasId)
  }

  terminate() {
    this.workers.forEach(w => w.terminate())
    this.workers = []
    this.endpoints = []
    this.idle = []
    this.queue = []
  }
}
