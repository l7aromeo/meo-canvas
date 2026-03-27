import { Canvas, FontLibrary, type CanvasRenderingContext2D } from 'skia-canvas'
import type { ExportFormat, ExportOptions, SaveOptions, RenderOptions } from 'skia-canvas'
import { ColumnNode, BoxNode, RowNode } from '@/canvas/layout.canvas.js'
import type { BaseProps, RootProps, CanvasElement, RootPropsWithWorker, RootPropsWithoutWorker } from '@/canvas/canvas.type.js'
import type { CanvasCallMethod, CallArgs, CallResult, WorkerCallRequest, WorkerResponse, WorkerRequest } from '@/worker/worker.types.js'
import { ImageNode, type RenderImageCache } from '@/canvas/image.canvas.js'
import { deleteDiskCache } from '@/util/disk.cache.js'
import { TextNode } from '@/canvas/text.canvas.js'
import { ChartNode } from '@/canvas/chart.canvas.js'
import { GridNode, GridItemNode } from '@/canvas/grid.canvas.js'
import { Style } from '@/constant/common.const.js'
import { WorkerPreProcessor } from '@/canvas/canvas.helper.js'
import * as path from 'node:path'
import * as fs from 'node:fs'
import { cpus } from 'node:os'
import { Worker } from 'node:worker_threads'
import { fileURLToPath } from 'node:url'

/** Registry to track fonts that have already been loaded */
const registeredFonts = new Map<string, Set<string>>()

// Exported for testing purposes only
export const _clearRegisteredFonts = () => {
  registeredFonts.clear()
}

/**
 * FinalizationRegistry to clean up WorkerCanvas instances that were not explicitly released.
 * This is a safety net — users should still call .release() explicitly.
 */
const canvasRegistry = new FinalizationRegistry<{ workerIdx: number; canvasId: number }>(heldValue => {
  // Best-effort cleanup — worker may already be terminated
  try {
    // Access workers via a public method or make it accessible
    // For now, just try to send the message and let errors be caught
    if (_workerPool) {
      ;(_workerPool as any).workers?.[heldValue.workerIdx]?.postMessage({ type: 'release', canvasId: heldValue.canvasId })
    }
  } catch {
    // Worker already gone — nothing to clean up
  }
})

/** Engine configuration — legacy support for configure() */
let _defaultWorkerMode = true
let _defaultWorkerPoolSize = Math.max(1, cpus().length - 1)
let _workerPool: WorkerPool | null = null

export interface CanvasEngineConfig {
  /** Run rendering in worker threads to avoid blocking the event loop (default: true) */
  workerMode?: boolean
  /** Number of worker threads in the pool (default: os.cpus().length - 1) */
  workers?: number
}

/**
 * Configure the canvas rendering engine.
 * Call this once at application startup before rendering.
 * @deprecated Pass workerMode and workers directly to Root() props instead.
 */
export function configure(options: CanvasEngineConfig) {
  if (options.workerMode !== undefined) _defaultWorkerMode = options.workerMode
  if (options.workers !== undefined) _defaultWorkerPoolSize = options.workers
}

/**
 * Terminate all worker pools and free worker thread resources.
 * Call this when shutting down a long-running server to clean up immediately.
 * After calling, you must call configure() again before rendering.
 */
export function terminate() {
  if (_workerPool) {
    _workerPool.terminate()
    _workerPool = null
  }
}

interface PendingTask {
  resolve: (value: unknown) => void
  reject: (err: Error) => void
}

interface PoolRenderResult {
  buffer: Buffer
  canvasId: number
  workerIdx: number
  width: number
  height: number
}

/**
 * Proxies all skia-canvas Canvas APIs to a Canvas instance living inside a worker thread.
 * Sync methods (toBufferSync, toURLSync) return from a pre-encoded PNG buffer.
 * Async methods (toBuffer, toURL, toFile, getters) delegate to the worker.
 */
class WorkerCanvas {
  readonly width: number
  readonly height: number
  private readonly _buffer: Buffer // pre-encoded PNG for sync use
  private readonly _pool: WorkerPool
  private readonly _workerIdx: number
  private readonly _canvasId: number

  constructor(opts: PoolRenderResult & { pool: WorkerPool }) {
    this._buffer = opts.buffer
    this.width = opts.width
    this.height = opts.height
    this._pool = opts.pool
    this._workerIdx = opts.workerIdx
    this._canvasId = opts.canvasId
    // Register for finalizer-based cleanup if user forgets to call .release()
    canvasRegistry.register(this, { workerIdx: opts.workerIdx, canvasId: opts.canvasId }, this)
  }

  private _call<M extends CanvasCallMethod>(method: M, ...args: CallArgs<M>): Promise<CallResult<M>> {
    return this._pool.callOnCanvas(this._workerIdx, this._canvasId, method, args)
  }

  // --- Sync methods: return from pre-encoded PNG buffer ---

  toBufferSync(_format?: ExportFormat, _options?: ExportOptions): Buffer {
    return this._buffer
  }

  toURLSync(_format?: ExportFormat, _options?: ExportOptions): string {
    return `data:image/png;base64,${this._buffer.toString('base64')}`
  }

  // --- Async methods: delegate to worker ---

  toBuffer(format: ExportFormat, options?: ExportOptions): Promise<Buffer> {
    return this._call('toBuffer', format, options)
  }

  toURL(format: ExportFormat, options?: ExportOptions): Promise<string> {
    return this._call('toURL', format, options)
  }

  toFile(filename: string, options?: SaveOptions): Promise<void> {
    return this._call('toFile', filename, options)
  }

  /** Returns a Buffer (Sharp instance cannot be transferred across threads) */
  toSharp(options?: RenderOptions): Promise<Buffer> {
    return this._call('toSharp', options)
  }

  toSharpSync(_options?: RenderOptions): never {
    throw new Error('[canvas] toSharpSync() is not available in worker mode — use toSharp() instead')
  }

  // --- Async convenience getters ---

  get png(): Promise<Buffer> {
    return this._call('toBuffer', 'png')
  }
  get webp(): Promise<Buffer> {
    return this._call('toBuffer', 'webp')
  }
  get jpg(): Promise<Buffer> {
    return this._call('toBuffer', 'jpg')
  }
  get svg(): Promise<Buffer> {
    return this._call('toBuffer', 'svg')
  }
  get pdf(): Promise<Buffer> {
    return this._call('toBuffer', 'pdf')
  }
  get raw(): Promise<Buffer> {
    return this._call('toBuffer', 'raw')
  }

  /** Release the Canvas from worker memory. Call when done with this object. */
  release(): void {
    this._pool.releaseCanvas(this._workerIdx, this._canvasId)
    // Unregister from finalizer since we're explicitly cleaning up
    canvasRegistry.unregister(this)
  }
}

/** Worker thread pool — routes render and canvas-call messages */
class WorkerPool {
  private workers: Worker[] = []
  private idle: Worker[] = []
  private queue: Array<{ id: number; props: RootProps }> = []
  private pending = new Map<number, PendingTask>()
  private nextId = 0

  constructor(size: number) {
    this.init(size)
  }

  private init(size: number) {
    const workerFile = path.join(path.dirname(fileURLToPath(import.meta.url)), '../worker/render.worker.js')

    for (let i = 0; i < size; i++) {
      const workerIdx = i
      const worker = new Worker(workerFile)
      worker.on('message', (msg: WorkerResponse) => {
        const task = this.pending.get(msg.taskId)
        if (!task) return
        this.pending.delete(msg.taskId)

        if ('error' in msg) {
          task.reject(new Error(msg.error))
          return
        }

        if ('canvasId' in msg) {
          // Render complete — put worker back to idle
          this.idle.push(worker)
          this.drain()
          const result: PoolRenderResult = { buffer: msg.buffer, canvasId: msg.canvasId, workerIdx, width: msg.width, height: msg.height }
          task.resolve(result)
        } else {
          // Canvas method call complete
          task.resolve(msg.result)
        }
      })
      this.workers.push(worker)
      this.idle.push(worker)
    }
  }

  private drain() {
    while (this.queue.length > 0 && this.idle.length > 0) {
      const task = this.queue.shift()!
      const worker = this.idle.pop()!
      const request: WorkerRequest = { type: 'render', taskId: task.id, props: task.props }
      worker.postMessage(request)
    }
  }

  render(props: RootProps): Promise<PoolRenderResult> {
    const sanitizedProps = WorkerPreProcessor.process(props)
    return new Promise<PoolRenderResult>((resolve, reject) => {
      const id = this.nextId++
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject })
      if (this.idle.length > 0) {
        const worker = this.idle.pop()!
        const request: WorkerRequest = { type: 'render', taskId: id, props: sanitizedProps }
        worker.postMessage(request)
      } else {
        this.queue.push({ id, props: sanitizedProps })
      }
    })
  }

  callOnCanvas<M extends CanvasCallMethod>(workerIdx: number, canvasId: number, method: M, args: CallArgs<M>): Promise<CallResult<M>> {
    return new Promise<CallResult<M>>((resolve, reject) => {
      const id = this.nextId++
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject })
      const request = { type: 'call' as const, taskId: id, canvasId, method, args } as WorkerCallRequest
      this.workers[workerIdx].postMessage(request)
    })
  }

  releaseCanvas(workerIdx: number, canvasId: number): void {
    const request: WorkerRequest = { type: 'release', canvasId }
    this.workers[workerIdx]?.postMessage(request)
  }

  terminate() {
    this.workers.forEach(w => w.terminate())
  }
}

/**
 * Converts a CanvasElement tree into actual BoxNode instances.
 * Used both for non-worker rendering (inline tree building) and inside
 * the render worker (reconstructing the tree from serialized descriptors).
 */
export function buildTree(descriptor: CanvasElement): BoxNode {
  switch (descriptor.__type) {
    case 'Box':
      return new BoxNode({ ...descriptor.props, children: descriptor.children?.map(buildTree) })
    case 'Column':
      return new ColumnNode({ ...descriptor.props, children: descriptor.children?.map(buildTree) })
    case 'Row':
      return new RowNode({ ...descriptor.props, children: descriptor.children?.map(buildTree) })
    case 'Grid':
      return new GridNode({ ...descriptor.props, children: descriptor.children?.map(buildTree) as any })
    case 'GridItem':
      return new GridItemNode({ ...descriptor.props, children: descriptor.children?.map(buildTree) as any })
    case 'Image':
      return new ImageNode(descriptor.props as any)
    case 'Text':
      return new TextNode(descriptor.text, descriptor.props)
    case 'Chart':
      return new ChartNode(descriptor.props as any)
  }
}

/**
 * Root node that manages the canvas rendering context and coordinates overall layout and drawing.
 * Inherits from ColumnNode to provide vertical layout capabilities.
 */
export class RootNode extends ColumnNode {
  declare props: RootProps & BaseProps
  /** The canvas instance used for rendering */
  private canvas: Canvas | undefined
  /** The 2D rendering context for the canvas */
  private ctx: CanvasRenderingContext2D | null = null
  /** Target width for the canvas in pixels */
  private readonly targetWidth: number
  /** Target height for the canvas in pixels */
  private readonly targetHeight: number | undefined
  /** Scale factor for rendering (e.g. 2 for 2x resolution) */
  private readonly scale: number

  /**
   * Creates a new root node for canvas rendering
   * @param props Configuration properties for the root node
   * @throws Error if width property is not provided
   */
  constructor(props: RootProps & BaseProps) {
    // Call the parent constructor with root name and props
    super({ name: 'Root', ...props })

    this.props = props

    // Validate the required width property
    if (!props.width) {
      throw new Error('Width and height are required for Root')
    }

    // Register provided fonts with caching
    if (props.fonts?.length) {
      for (const font of props.fonts) {
        const family = font.family
        const paths = font.paths.map(p => path.resolve(p))

        if (!registeredFonts.has(family)) {
          registeredFonts.set(family, new Set())
        }

        const cachedPaths = registeredFonts.get(family)!
        const newPaths = paths.filter(p => !cachedPaths.has(p) && fs.existsSync(p))

        if (newPaths.length > 0) {
          FontLibrary.use({ [family]: newPaths })
          newPaths.forEach(p => cachedPaths.add(p))
        }
      }
    }

    // Set up scale and width
    this.scale = props.scale || 1
    this.targetWidth = props.width
    this.targetHeight = props.height
    this.node.setWidth(this.targetWidth)

    // Convert any CanvasElement children to actual BoxNode instances
    if (this.props.children) {
      const childArray = Array.isArray(this.props.children) ? this.props.children : [this.props.children]
      this.props.children = childArray.map(child => {
        if (child && typeof child === 'object' && '__type' in child) {
          return buildTree(child as CanvasElement)
        }
        return child
      }) as any
    }

    // Initialize children nodes
    this.processInitialChildren()
  }

  /**
   * Traverses the node tree to find all ImageNode instances using breadth-first search
   * @returns Array of all ImageNode instances found in the tree
   */
  private findAllImageNodes(): ImageNode[] {
    const imageNodes: ImageNode[] = []
    const queue: BoxNode[] = [this]
    while (queue.length > 0) {
      const node = queue.shift()!
      if (node instanceof ImageNode) {
        imageNodes.push(node)
      }
      queue.push(...node.children)
    }
    return imageNodes
  }

  /**
   * Renders the entire node tree to a canvas, handling image loading, layout calculation,
   * and final drawing
   * @returns Promise resolving to the rendered Canvas instance
   */
  async render(): Promise<Canvas> {
    const diskCacheKeys = this.props.useDiskCache ? new Set<string>() : undefined

    try {
      // Step 1: Load all images with a concurrency limit to avoid overwhelming remote sources.
      // A per-render cache deduplicates identical src+color combinations within this render pass.
      const imageNodes = this.findAllImageNodes()
      if (imageNodes.length > 0) {
        const imageCache: RenderImageCache = new Map()
        const CONCURRENCY = 5
        const queue = [...imageNodes]
        const workers = Array.from({ length: Math.min(CONCURRENCY, queue.length) }, async () => {
          while (queue.length > 0) {
            const node = queue.shift()!
            await node.load(imageCache, diskCacheKeys)
          }
        })
        await Promise.allSettled(workers)
      }

      // Step 2: Calculate initial layout
      this.node.calculateLayout(this.targetWidth, undefined, Style.Direction.LTR)

      // Step 3: Allow nodes to finalize their layout
      const needRecalculate = this.finalizeLayout()
      if (needRecalculate) {
        this.node.calculateLayout(this.targetWidth, undefined, Style.Direction.LTR)
      }

      // Step 4: Create a canvas with calculated dimensions
      const calculatedContentHeight = this.node.getComputedHeight()
      const finalCanvasWidth = Math.ceil(this.targetWidth * this.scale)
      const finalCanvasHeight = this.targetHeight ? Math.ceil(this.targetHeight * this.scale) : Math.max(1, Math.ceil(calculatedContentHeight * this.scale))

      // Step 5: Set up canvas context
      this.canvas = new Canvas(finalCanvasWidth, finalCanvasHeight)
      this.ctx = this.canvas.getContext('2d')
      this.ctx.scale(this.scale, this.scale)

      // Step 6: Render content
      super.render(this.ctx, 0, 0)

      if (!this.canvas) {
        throw new Error('Canvas not initialized')
      }

      return this.canvas
    } finally {
      if (diskCacheKeys?.size) {
        await Promise.allSettled([...diskCacheKeys].map(key => deleteDiskCache(key)))
      }
    }
  }
}

/**
 * Creates and renders a new root node with the given properties.
 * Rendering runs in worker threads by default for non-blocking operation.
 * @example
 * // Worker mode (default) - .release() available
 * const canvas = await Root({ width: 400, children: [...] })
 * canvas.release() // ✓ OK
 * @example
 * // Worker mode explicit - .release() available
 * const canvas = await Root({ width: 400, workerMode: true, workers: 2 })
 * canvas.release() // ✓ OK
 * @example
 * // Non-worker mode - .release() NOT available, workers not allowed
 * const canvas = await Root({ width: 400, workerMode: false })
 * canvas.release() // ✗ TypeScript error
 * @param props Configuration properties for the root node
 * @returns Canvas with .release() in worker mode, plain Canvas otherwise
 */
export function Root(props: RootPropsWithWorker): Promise<Canvas & { release(): void }>
export function Root(props: RootPropsWithoutWorker): Promise<Canvas>
export async function Root(props: RootProps): Promise<Canvas | (Canvas & { release(): void })> {
  // Determine worker mode: props override legacy configure()
  const workerMode = props.workerMode ?? _defaultWorkerMode
  const workerPoolSize = props.workers ?? _defaultWorkerPoolSize

  if (workerMode) {
    // Lazy initialize worker pool
    if (!_workerPool) {
      _workerPool = new WorkerPool(workerPoolSize)
    }
    const result = await _workerPool.render(props)
    return new WorkerCanvas({ ...result, pool: _workerPool }) as unknown as Canvas & { release(): void }
  }

  // Non-worker mode — render directly and return Canvas
  return new RootNode(props).render()
}
