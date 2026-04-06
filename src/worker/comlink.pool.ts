import { Worker } from 'node:worker_threads'
import { fileURLToPath } from 'node:url'
import * as path from 'node:path'
import { Comlink, nodeEndpoint } from '@/worker/comlink.setup.js'
import type { Remote } from 'comlink'
import type { WorkerAPI, RenderResult, CallFn } from '@/worker/worker.types.js'
import type { RootProps } from '@/canvas/canvas.type.js'

export interface PoolRenderResult extends RenderResult {
  workerIdx: number
}

interface QueuedTask {
  props: RootProps
  callFn: CallFn | undefined
  resolve: (result: PoolRenderResult) => void
  reject: (err: Error) => void
}

/** Sentinel embedded in serialized props to mark where a function was extracted. */
export const FN_MARKER = '__comlinkFnId'

/**
 * Deeply walks an object tree, replaces function values with `{ [FN_MARKER]: id }` sentinels,
 * and collects the original functions in a Map keyed by their assigned id.
 * Returns the cleaned (function-free) tree that is safe for structured clone.
 */
export function extractFunctions<T>(obj: T, fnMap: Map<number, (...args: unknown[]) => unknown>, nextId: { value: number }): T {
  if (obj === null || obj === undefined) return obj
  if (typeof obj === 'function') {
    const id = nextId.value++
    fnMap.set(id, obj as (...args: unknown[]) => unknown)
    return { [FN_MARKER]: id } as unknown as T
  }
  if (typeof obj !== 'object') return obj

  // Preserve binary data types — don't walk into them
  if (Buffer.isBuffer(obj)) return obj
  if (obj instanceof ArrayBuffer) return obj
  if (ArrayBuffer.isView(obj)) return obj

  if (Array.isArray(obj)) {
    return obj.map(item => extractFunctions(item, fnMap, nextId)) as unknown as T
  }

  const result: Record<string, unknown> = {}
  for (const key of Object.keys(obj as Record<string, unknown>)) {
    result[key] = extractFunctions((obj as Record<string, unknown>)[key], fnMap, nextId)
  }
  return result as T
}

/**
 * Deeply walks an object tree received on the worker side, replaces
 * `{ [FN_MARKER]: id }` sentinels with async functions that delegate
 * to the main-thread callback proxy.
 */
export function restoreFunctions<T>(obj: T, callFn: (id: number, ...args: unknown[]) => Promise<unknown>): T {
  if (obj === null || obj === undefined) return obj
  if (typeof obj !== 'object') return obj

  if (Buffer.isBuffer(obj)) return obj
  if (obj instanceof ArrayBuffer) return obj
  if (ArrayBuffer.isView(obj)) return obj

  // Check for sentinel
  if (FN_MARKER in (obj as Record<string, unknown>)) {
    const id = (obj as Record<string, unknown>)[FN_MARKER] as number
    return ((...args: unknown[]) => callFn(id, ...args)) as unknown as T
  }

  if (Array.isArray(obj)) {
    return obj.map(item => restoreFunctions(item, callFn)) as unknown as T
  }

  const result: Record<string, unknown> = {}
  for (const key of Object.keys(obj as Record<string, unknown>)) {
    result[key] = restoreFunctions((obj as Record<string, unknown>)[key], callFn)
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
      void this.executeRender(idx, task.props, task.callFn, task.resolve, task.reject)
    }
  }

  private async executeRender(
    idx: number,
    props: RootProps,
    callFn: CallFn | undefined,
    resolve: (result: PoolRenderResult) => void,
    reject: (err: Error) => void,
  ) {
    try {
      const result = await this.endpoints[idx].render(props, callFn)
      resolve({ ...result, workerIdx: idx })
    } catch (err) {
      reject(err instanceof Error ? err : new Error(String(err)))
    } finally {
      this.release(idx)
    }
  }

  async render(props: RootProps): Promise<PoolRenderResult> {
    // Extract functions from props, replacing them with serializable sentinels.
    // A single Comlink.proxy() callback is created at the top level so Comlink
    // can correctly transfer it via its proxy transfer handler.
    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    const cleaned = extractFunctions(props, fnMap, { value: 0 })

    let callFnProxy: CallFn | undefined
    if (fnMap.size > 0) {
      callFnProxy = Comlink.proxy(async (id: number, ...args: unknown[]) => {
        const fn = fnMap.get(id)
        if (!fn) throw new Error(`[ComlinkPool] Function #${id} not found`)
        return fn(...args)
      })
    }

    const cleanup = () => {
      if (callFnProxy) {
        try {
          ;(callFnProxy as any)[Comlink.releaseProxy]?.()
        } catch {
          // Proxy may already be released
        }
      }
    }

    // Direct path — idle worker available
    const idx = this.acquire()
    if (idx !== null) {
      try {
        const result = await this.endpoints[idx].render(cleaned, callFnProxy)
        return { ...result, workerIdx: idx }
      } finally {
        this.release(idx)
        cleanup()
      }
    }

    // Queued path — cleanup AFTER the queued task completes, not before
    return new Promise<PoolRenderResult>((resolve, reject) => {
      this.queue.push({
        props: cleaned,
        callFn: callFnProxy,
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
