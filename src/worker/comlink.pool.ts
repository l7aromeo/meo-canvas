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

  private readonly size: number
  private readonly workerFile: string

  /**
   * Tracked explicitly rather than inferred from `endpoints.length`. An empty pool used to mean
   * "terminated", which was safe only while every worker was created in the constructor. With
   * lazy spawning an empty pool is also the normal state before the first render, so the two
   * cases have to be told apart.
   */
  private terminated = false

  /**
   * Workers are spawned on demand rather than up front.
   *
   * The pool defaults to one worker per core, and every worker is a full thread with its own
   * runtime and its own copy of any registered font faces — roughly twelve megabytes each while
   * completely idle, and eighteen Poppins faces add eight megabytes more to any worker that
   * actually renders. Allocating that for every core punished machines for having them: a
   * fourteen-core workstation paid a hundred and eighty-seven megabytes to render one card at a
   * time, of which twelve threads never rendered anything.
   *
   * Spawning lazily means the pool grows to fit real concurrency instead of the core count. A
   * caller that renders sequentially ends up with exactly one worker; a caller that renders eight
   * cards at once still gets eight. `size` remains the ceiling, so nothing that relied on the
   * concurrency limit changes.
   */
  constructor(size: number) {
    this.size = size
    this.workerFile = path.join(path.dirname(fileURLToPath(import.meta.url)), '../worker/render.worker.js')
  }

  /** Adds one worker to the pool and returns its index. */
  private spawn(): number {
    const idx = this.workers.length
    const worker = new Worker(this.workerFile)
    this.workers.push(worker)
    this.endpoints.push(Comlink.wrap<WorkerAPI>(nodeEndpoint(worker)))
    return idx
  }

  /**
   * Prefers an already-warm worker over a new one. A worker that has rendered before has its
   * fonts loaded and its runtime warm, so reusing it is both faster and cheaper than spawning a
   * sibling that would have to repeat that work.
   */
  private acquire(): number | null {
    const reused = this.idle.pop()
    if (reused !== undefined) return reused
    if (this.terminated) return null
    return this.workers.length < this.size ? this.spawn() : null
  }

  private release(idx: number) {
    this.idle.push(idx)
    this.drain()
  }

  private drain() {
    while (this.queue.length > 0) {
      const idx = this.acquire()
      if (idx === null) return
      const task = this.queue.shift()!
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
    if (this.terminated) {
      throw new Error('[ComlinkPool] Pool has been terminated')
    }

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
    // The endpoint can legitimately be gone: a canvas released by the FinalizationRegistry may be
    // collected after the pool has terminated. Rejecting is clearer than dereferencing undefined,
    // which surfaced as an unrelated TypeError.
    const endpoint = this.endpoints[workerIdx]
    if (!endpoint) return Promise.reject(new Error(`[ComlinkPool] Worker ${workerIdx} is not available`))
    return endpoint.callOnCanvas(canvasId, method, args) as Promise<Buffer | string | void>
  }

  releaseCanvas(workerIdx: number, canvasId: number): void {
    // Best-effort: this is also reached from the FinalizationRegistry, which can run after the
    // pool is gone. Nothing to release in that case.
    this.endpoints[workerIdx]?.releaseCanvas(canvasId)
  }

  terminate() {
    this.terminated = true
    this.workers.forEach(w => w.terminate())
    this.workers = []
    this.endpoints = []
    this.idle = []
    this.queue = []
  }
}
