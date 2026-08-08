import { Canvas, FontLibrary, type CanvasRenderingContext2D } from 'phyron-skia-canvas'
import type { ExportFormat, ExportOptions, SaveOptions, RenderOptions } from 'phyron-skia-canvas'
import { ColumnNode, BoxNode, RowNode } from '@/canvas/layout.canvas.js'
import type {
  BaseProps,
  RootProps,
  CanvasElement,
  RootPropsWithWorker,
  RootPropsWithoutWorker,
  Children,
  ImageProps,
  ChartProps,
  ChartType,
} from '@/canvas/canvas.type.js'
import type { ComlinkPool as ComlinkPoolType, PoolRenderResult } from '@/worker/comlink.pool.js'
import { ImageNode, type RenderImageCache } from '@/canvas/image.canvas.js'
import { deleteDiskCache } from '@/util/disk.cache.js'
import { TextNode } from '@/canvas/text.canvas.js'
import { ChartNode } from '@/canvas/chart.canvas.js'
import { GridNode, GridItemNode } from '@/canvas/grid.canvas.js'
import { Style } from '@/constant/common.const.js'
import * as path from 'node:path'
import * as fs from 'node:fs'
import { cpus } from 'node:os'

/** Registry to track fonts that have already been loaded */
const registeredFonts = new Map<string, Set<string>>()
let _fontRegistrationLock: Promise<void> | null = null

// Clears the font registry between test runs (internal, not exported from index)
const _clearRegisteredFonts = () => {
  registeredFonts.clear()
  _fontRegistrationLock = null
}

/**
 * FinalizationRegistry to clean up WorkerCanvas instances that were not explicitly released.
 * This is a safety net — users should still call .release() explicitly.
 */
const canvasRegistry = new FinalizationRegistry<{ workerIdx: number; canvasId: number }>(heldValue => {
  try {
    _workerPool?.releaseCanvas(heldValue.workerIdx, heldValue.canvasId)
  } catch {
    // Worker already gone — nothing to clean up
  }
})

let _workerPool: ComlinkPoolType | null = null

/**
 * Terminate all worker pools and free worker thread resources.
 * Call this when shutting down a long-running server to clean up immediately.
 */
export function terminate() {
  if (_workerPool) {
    _workerPool.terminate()
    _workerPool = null
  }
}

/**
 * Proxies all skia-canvas Canvas APIs to a Canvas instance living inside a worker thread.
 * Sync methods (toBufferSync, toURLSync) return from a pre-encoded PNG buffer.
 * Async methods (toBuffer, toURL, toFile, getters) delegate to the worker.
 */
class WorkerCanvas {
  readonly width: number
  readonly height: number
  private readonly _buffer: Buffer
  private readonly _pool: ComlinkPoolType
  private readonly _workerIdx: number
  private readonly _canvasId: number

  constructor(opts: PoolRenderResult & { pool: ComlinkPoolType }) {
    this._buffer = opts.buffer
    this.width = opts.width
    this.height = opts.height
    this._pool = opts.pool
    this._workerIdx = opts.workerIdx
    this._canvasId = opts.canvasId
    canvasRegistry.register(this, { workerIdx: opts.workerIdx, canvasId: opts.canvasId }, this)
  }

  // --- Sync methods: return from pre-encoded PNG buffer ---

  toBufferSync(_format?: ExportFormat, _options?: ExportOptions): Buffer {
    return this._buffer
  }

  toURLSync(_format?: ExportFormat, _options?: ExportOptions): string {
    return `data:image/png;base64,${this._buffer.toString('base64')}`
  }

  // --- Async methods: delegate to worker via Comlink ---

  toBuffer(format: ExportFormat, options?: ExportOptions): Promise<Buffer> {
    return this._pool.callOnCanvas(this._workerIdx, this._canvasId, 'toBuffer', [format, options]) as Promise<Buffer>
  }

  toURL(format: ExportFormat, options?: ExportOptions): Promise<string> {
    return this._pool.callOnCanvas(this._workerIdx, this._canvasId, 'toURL', [format, options]) as Promise<string>
  }

  toFile(filename: string, options?: SaveOptions): Promise<void> {
    return this._pool.callOnCanvas(this._workerIdx, this._canvasId, 'toFile', [filename, options]) as Promise<void>
  }

  toSharp(options?: RenderOptions): Promise<Buffer> {
    return this._pool.callOnCanvas(this._workerIdx, this._canvasId, 'toSharp', [options]) as Promise<Buffer>
  }

  toSharpSync(_options?: RenderOptions): never {
    throw new Error('[canvas] toSharpSync() is not available in worker mode — use toSharp() instead')
  }

  // --- Async convenience getters ---

  get png(): Promise<Buffer> {
    return this.toBuffer('png')
  }
  get webp(): Promise<Buffer> {
    return this.toBuffer('webp')
  }
  get jpg(): Promise<Buffer> {
    return this.toBuffer('jpg')
  }
  get svg(): Promise<Buffer> {
    return this.toBuffer('svg')
  }
  get pdf(): Promise<Buffer> {
    return this.toBuffer('pdf')
  }
  get raw(): Promise<Buffer> {
    return this.toBuffer('raw')
  }

  /** Release the Canvas from worker memory. Call when done with this object. */
  release(): void {
    this._pool.releaseCanvas(this._workerIdx, this._canvasId)
    canvasRegistry.unregister(this)
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
      return new GridNode({ ...descriptor.props, children: descriptor.children?.map(buildTree) as Children[] })
    case 'GridItem':
      return new GridItemNode({ ...descriptor.props, children: descriptor.children?.map(buildTree) as Children[] })
    case 'Image':
      return new ImageNode(descriptor.props as ImageProps)
    case 'Text':
      return new TextNode(descriptor.text, descriptor.props)
    case 'Chart':
      return new ChartNode(descriptor.props as ChartProps<ChartType>)
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
  /** Max concurrent image fetches during render (default: 5) */
  private readonly imageConcurrency: number

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

    // Set up scale and width
    this.scale = props.scale || 1
    this.targetWidth = props.width
    this.targetHeight = props.height
    this.imageConcurrency = props.imageConcurrency ?? 5
    this.node.setWidth(this.targetWidth)

    // Convert any CanvasElement children to actual BoxNode instances
    if (this.props.children) {
      const childArray = Array.isArray(this.props.children) ? this.props.children : [this.props.children]
      const converted: Children[] = childArray.map(child => {
        if (child && typeof child === 'object' && '__type' in child) {
          return buildTree(child as CanvasElement)
        }
        return child as Children
      })
      this.props.children = converted
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
    let head = 0
    while (head < queue.length) {
      const node = queue[head++]
      if (node instanceof ImageNode) {
        imageNodes.push(node)
      }
      queue.push(...node.children)
    }
    return imageNodes
  }

  /**
   * Registers fonts with serialization to prevent duplicate FontLibrary.use() calls
   * when multiple Root() instances are created concurrently.
   */
  private async _registerFonts(): Promise<void> {
    if (!this.props.fonts?.length) return

    // Wait for any in-flight registration to complete
    if (_fontRegistrationLock) await _fontRegistrationLock

    _fontRegistrationLock = (async () => {
      try {
        for (const font of this.props.fonts!) {
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
      } finally {
        _fontRegistrationLock = null
      }
    })()

    await _fontRegistrationLock
  }

  /**
   * Renders the entire node tree to a canvas, handling image loading, layout calculation,
   * and final drawing
   * @returns Promise resolving to the rendered Canvas instance
   */
  override async render(ctx: CanvasRenderingContext2D, offsetX?: number, offsetY?: number): Promise<void>
  async render(ctx?: CanvasRenderingContext2D, offsetX?: number, offsetY?: number): Promise<Canvas>
  async render(ctx?: CanvasRenderingContext2D, offsetX = 0, offsetY = 0): Promise<Canvas | void> {
    // If ctx is provided, delegate to parent render (used when called as a child node)
    if (ctx) {
      await super.render(ctx, offsetX, offsetY)
      return
    }

    // Register fonts with serialization to prevent duplicate FontLibrary.use() across concurrent Root() calls
    await this._registerFonts()

    const diskCacheKeys = this.props.useDiskCache ? new Set<string>() : undefined

    try {
      // Step 1: Load all images with a concurrency limit to avoid overwhelming remote sources.
      // A per-render cache deduplicates identical src+color combinations within this render pass.
      const imageNodes = this.findAllImageNodes()
      if (imageNodes.length > 0) {
        const imageCache: RenderImageCache = new Map()
        const queue = [...imageNodes]
        let qIdx = 0
        const workers = Array.from({ length: Math.min(this.imageConcurrency, queue.length) }, async () => {
          while (qIdx < queue.length) {
            const node = queue[qIdx++]
            await node.load(imageCache, diskCacheKeys)
          }
        })
        await Promise.allSettled(workers).then(results => {
          results.forEach(r => {
            if (r.status === 'rejected') console.warn('[RootNode] Image load worker failed:', r.reason)
          })
        })
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
      await super.render(this.ctx, 0, 0)

      if (!this.canvas) {
        throw new Error('Canvas not initialized')
      }

      return this.canvas
    } finally {
      // Freed before the disk cache is cleaned up, not after. Both run in the same `finally`, but
      // deleting cache files is asynchronous, and awaiting it first would hold the whole layout
      // tree in WASM memory across that I/O for no reason. This is the earliest point the tree is
      // provably dead: layout is read during `super.render`, and nothing consults it afterwards.
      this.freeLayoutTree()

      if (diskCacheKeys?.size) {
        await Promise.allSettled([...diskCacheKeys].map(key => deleteDiskCache(key)))
      }
    }
  }

  /**
   * Releases the Yoga layout tree back to the WASM heap.
   *
   * Yoga nodes are allocated inside WebAssembly memory, which the JavaScript collector cannot
   * see or reclaim: a node stays allocated until something calls `free()` on it, no matter how
   * unreachable the JavaScript object holding it becomes. Nothing did, so every rendered card
   * left its entire node tree behind — roughly ten to fifteen megabytes per render for a
   * moderately sized layout, growing without limit for the life of the process.
   *
   * `freeRecursive` releases this node and every descendant, so one call at the root covers the
   * whole tree. It runs only after the layout has been read and the content drawn; at this point
   * the tree is dead and nothing reads from it again.
   *
   * Guarded against running twice. Freeing an already-freed Yoga node dereferences a stale WASM
   * pointer, which aborts the process rather than raising an exception a caller could handle.
   */
  private _layoutTreeFreed = false

  private freeLayoutTree(): void {
    if (this._layoutTreeFreed) return
    this._layoutTreeFreed = true
    try {
      this.node.freeRecursive()
    } catch (e) {
      // A failure here must not mask the render's own result, and leaking is preferable to
      // aborting: report it and carry on.
      console.warn('[RootNode] Failed to free the layout tree:', e)
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
  // Determine worker mode
  const workerMode = props.workerMode ?? true
  const workerPoolSize = props.workers ?? Math.max(1, cpus().length - 1)

  if (workerMode) {
    // Lazy initialize worker pool — dynamic import to avoid loading Comlink in non-worker contexts
    if (!_workerPool) {
      const { ComlinkPool } = await import('@/worker/comlink.pool.js')
      _workerPool = new ComlinkPool(workerPoolSize)
    }
    const result = await _workerPool.render(props)
    return new WorkerCanvas({ ...result, pool: _workerPool }) as unknown as Canvas & { release(): void }
  }

  // Non-worker mode — render directly and return Canvas
  return new RootNode(props).render()
}
