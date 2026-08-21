import { Canvas, FontLibrary, type CanvasRenderingContext2D } from 'meo-skia-canvas'
import type { ExportFormat, ExportOptions, SaveOptions, RenderOptions, EngineDetails, ColorType, ColorSpace } from 'meo-skia-canvas'
import { createCanvas, type CanvasEngineOptions } from '@/canvas/canvas.engine.js'
import { createRequire } from 'node:module'
import { ColumnNode, BoxNode, RowNode } from '@/canvas/layout.canvas.js'
import type {
  BaseProps,
  RootProps,
  CanvasElement,
  RootPropsWithWorker,
  RootPropsWithoutWorker,
  RootContent,
  RootNodeProps,
  RenderedCanvas,
  AnimatedFormat,
  StillFormat,
  AnimationExportOptions,
  StillExportOptions,
  Children,
  ImageProps,
  ChartItem,
  ResolvedChartItem,
  ResolvedChartProps,
  ChartProps,
  ChartType,
} from '@/canvas/canvas.type.js'
import type { ComlinkPool as ComlinkPoolType, PoolRenderResult } from '@/worker/comlink.pool.js'
import { ImageNode, type RenderImageCache } from '@/canvas/image.canvas.js'
import { deleteDiskCache } from '@/util/disk.cache.js'
import { TextNode } from '@/canvas/text.canvas.js'
import { invalidateTextMeasurements } from '@/canvas/text.metrics.js'
import { ChartNode } from '@/canvas/chart.canvas.js'
import { GridNode, GridItemNode } from '@/canvas/grid.canvas.js'
import { PathNode } from '@/canvas/path.canvas.js'
import { asNodeProps, planPages, resolveFps } from '@/canvas/page.plan.js'
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
 * Terminates every worker thread and frees the pool.
 *
 * The pool starts lazily on the first worker-mode render and lives for the life of the process, so
 * a script that renders and exits will hang without this. A long-running server does not need it
 * until shutdown.
 * @example
 * ```ts
 * const canvas = await Root({ width: 400, children: [...] })
 * await canvas.toFile('out.png')
 * canvas.release()
 * await terminate()
 * ```
 */
export function terminate() {
  if (_workerPool) {
    _workerPool.terminate()
    _workerPool = null
  }
}

/**
 * Restores the `Buffer` identity that a thread hop strips.
 *
 * Structured clone preserves the bytes but not the subclass, so anything arriving from a worker is
 * a plain `Uint8Array` no matter what the signature says. Callers notice the moment they reach for
 * a Buffer-only method: `.toString('utf8')` silently falls through to `Array.prototype.toString`
 * and yields `"60,63,120,109"` instead of `"<?xm"`, with no error to point at.
 *
 * The view is wrapped, not copied — this stays O(1) even for a multi-megabyte raw export.
 */
function asBuffer(value: unknown): Buffer {
  const bytes = value as Uint8Array
  return Buffer.isBuffer(bytes) ? bytes : Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength)
}

/** Sharp is an optional peer of the renderer; only `toSharp()` needs it. */
let _sharp: ((...args: never[]) => unknown) | null = null
function loadSharp(): (...args: never[]) => unknown {
  if (!_sharp) {
    try {
      _sharp = createRequire(import.meta.url)('sharp')
    } catch {
      throw new Error('[canvas] toSharp() requires the `sharp` package to be installed')
    }
  }
  return _sharp!
}

/**
 * A Canvas that lives in a worker thread.
 *
 * Every method behaves as its counterpart on a real Canvas does. Sync methods block the calling
 * thread while the worker runs the real call (over the pool's synchronous channel); async methods go through
 * Comlink. The two members that cannot be honoured — `getContext()` and `newPage()` — say so
 * instead of returning something that only resembles the real thing.
 */
export class WorkerCanvas {
  /** Width of the rendered canvas in device pixels — the root's width times its `scale`. */
  readonly width: number
  /** Height of the rendered canvas in device pixels. */
  readonly height: number
  /** Snapshots, not proxies: none of these can change once the canvas has been rendered. */
  readonly gpu: boolean
  /** Which backend took the render, and what it reports about itself. */
  readonly engine: EngineDetails
  /** What the engine settled on, which is not always what `Root` asked for. */
  readonly colorType: ColorType
  /** The space the canvas composited in. Exports convert out of it when asked. */
  readonly colorSpace: ColorSpace

  private readonly _pool: ComlinkPoolType
  private readonly _workerIdx: number
  private readonly _canvasId: number

  /**
   * Encoded results, keyed by format and options.
   *
   * The canvas is immutable once rendered — nothing here can draw to it — so a repeated
   * `toBufferSync('webp')` is guaranteed to produce identical bytes and is served from here rather
   * than paying a second round trip and a second encode.
   */
  private readonly _cache = new Map<string, Buffer | string>()

  constructor(opts: PoolRenderResult & { pool: ComlinkPoolType }) {
    this.width = opts.width
    this.height = opts.height
    this.gpu = opts.gpu
    this.engine = opts.engine
    this.colorType = opts.colorType
    this.colorSpace = opts.colorSpace
    this._pool = opts.pool
    this._workerIdx = opts.workerIdx
    this._canvasId = opts.canvasId
    canvasRegistry.register(this, { workerIdx: opts.workerIdx, canvasId: opts.canvasId }, this)
  }

  private _sync(method: string, args: unknown[]): unknown {
    return this._pool.syncCall(this._workerIdx, this._canvasId, method, args)
  }

  /**
   * Sync call whose result is worth keeping. Only for methods that return data, never files.
   *
   * `transform` runs once, on the way into the cache, so a repeat call costs a Map lookup and
   * returns the identical instance rather than re-wrapping the bytes.
   */
  private _syncCached<T extends Buffer | string>(method: string, args: unknown[], transform: (value: unknown) => T): T {
    const key = `${method}:${JSON.stringify(args)}`
    const hit = this._cache.get(key)
    if (hit !== undefined) return hit as T
    const value = transform(this._sync(method, args))
    this._cache.set(key, value)
    return value
  }

  // --- Sync methods: block on the worker, honour the format actually asked for ---

  /**
   * Encodes the canvas and hands back the bytes, blocking until the worker has done it.
   *
   * An animated format takes every page as a frame; a still one takes the first. The result is
   * cached, so asking twice for the same format and options costs a map lookup rather than a second
   * encode.
   */
  toBufferSync(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): Buffer
  toBufferSync(format?: StillFormat, options?: StillExportOptions): Buffer
  toBufferSync(format: ExportFormat = 'png', options?: ExportOptions): Buffer {
    return this._syncCached<Buffer>('toBufferSync', [format, options], asBuffer)
  }

  /** {@link toBufferSync}, encoded as a `data:` URL instead of raw bytes. */
  toURLSync(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): string
  toURLSync(format?: StillFormat, options?: StillExportOptions): string
  toURLSync(format: ExportFormat = 'png', options?: ExportOptions): string {
    return this._syncCached<string>('toURLSync', [format, options], String)
  }

  /**
   * Encodes to a data URL, blocking on the worker.
   * @deprecated `toDataURL()` is synchronous; use it instead.
   */
  toDataURLSync(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): string
  toDataURLSync(format?: StillFormat, options?: StillExportOptions): string
  toDataURLSync(format: ExportFormat = 'png', options?: ExportOptions): string {
    return this._syncCached<string>('toDataURLSync', [format, options], String)
  }

  /**
   * The `HTMLCanvasElement` spelling of {@link toURLSync}, taking a quality rather than an options
   * object. Synchronous despite the name, as it is on a real canvas.
   */
  toDataURL(format: ExportFormat = 'png', quality?: number): string {
    return this._syncCached<string>('toDataURL', [format, quality], String)
  }

  /**
   * Writes the canvas to disk, blocking on the worker.
   *
   * Not split by format the way `toBuffer` is: the format comes from the filename extension here,
   * so there is no format argument for the animation options to be checked against.
   */
  toFileSync(filename: string, options?: SaveOptions): void {
    this._sync('toFileSync', [filename, options])
  }

  /**
   * Writes the canvas to disk from the worker thread, blocking until it lands.
   * @deprecated Use {@link WorkerCanvas.toFileSync} instead.
   */
  saveAsSync(filename: string, options?: SaveOptions): void {
    this._sync('saveAsSync', [filename, options])
  }

  // --- Async methods: delegate to worker via Comlink ---

  /**
   * Encodes the canvas on the worker and resolves with the bytes.
   *
   * The asynchronous form to prefer: the encode runs on the worker's thread, so several canvases
   * can be encoded at once without blocking the caller.
   */
  toBuffer(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): Promise<Buffer>
  toBuffer(format: StillFormat, options?: StillExportOptions): Promise<Buffer>
  toBuffer(format: ExportFormat, options?: ExportOptions): Promise<Buffer> {
    return this._pool.callOnCanvas(this._workerIdx, this._canvasId, 'toBuffer', [format, options]).then(asBuffer)
  }

  /** {@link toBuffer}, resolved as a `data:` URL instead of raw bytes. */
  toURL(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): Promise<string>
  toURL(format: StillFormat, options?: StillExportOptions): Promise<string>
  toURL(format: ExportFormat, options?: ExportOptions): Promise<string> {
    return this._pool.callOnCanvas(this._workerIdx, this._canvasId, 'toURL', [format, options]) as Promise<string>
  }

  /** Writes the canvas to disk, taking the format from the file's extension. */
  toFile(filename: string, options?: SaveOptions): Promise<void> {
    return this._pool.callOnCanvas(this._workerIdx, this._canvasId, 'toFile', [filename, options]) as Promise<void>
  }

  /**
   * Writes the canvas to disk from the worker thread.
   * @deprecated Use {@link WorkerCanvas.toFile} instead.
   */
  saveAs(filename: string, options?: SaveOptions): Promise<void> {
    return this._pool.callOnCanvas(this._workerIdx, this._canvasId, 'saveAs', [filename, options]) as Promise<void>
  }

  /**
   * Returns a real Sharp, as skia-canvas does — not a Buffer, which is what this used to hand back.
   *
   * A Sharp instance cannot cross a thread boundary, so the pixels are fetched and the wrapper is
   * built here. That mirrors what skia-canvas itself does internally with `toBuffer('raw')`.
   */
  toSharp(options?: RenderOptions): ReturnType<Canvas['toSharp']> {
    const density = options?.density ?? 1
    const raw = this.toBufferSync('raw', { ...options, density })
    const sharp = loadSharp() as (input: Buffer, opts: unknown) => { withMetadata(m: unknown): unknown }
    return sharp(raw, {
      raw: { width: this.width * density, height: this.height * density, channels: 4 },
    }).withMetadata({ density: density * 72 }) as ReturnType<Canvas['toSharp']>
  }

  /** Hands the pixels to sharp for further processing, blocking on the worker. */
  toSharpSync(options?: RenderOptions): ReturnType<Canvas['toSharp']> {
    return this.toSharp(options)
  }

  // --- Async convenience getters ---

  /** Shorthand for `toBuffer('png')`. Lossless, and the format to reach for unless there is a reason not to. */
  get png(): Promise<Buffer> {
    return this.toBuffer('png')
  }
  /** Shorthand for `toBuffer('webp')`. Smaller than PNG at the same quality, and takes every page as an animation. */
  get webp(): Promise<Buffer> {
    return this.toBuffer('webp')
  }
  /** Shorthand for `toBuffer('jpg')`. Lossy and opaque — no alpha channel. */
  get jpg(): Promise<Buffer> {
    return this.toBuffer('jpg')
  }
  /** Shorthand for `toBuffer('svg')`. Vector output: the drawing as paths rather than pixels. */
  get svg(): Promise<Buffer> {
    return this.toBuffer('svg')
  }
  /** Shorthand for `toBuffer('pdf')`. A document, one page per rendered page. */
  get pdf(): Promise<Buffer> {
    return this.toBuffer('pdf')
  }
  /** Shorthand for `toBuffer('raw')`. Pixel data in the canvas's own `colorType`, with no container around it. */
  get raw(): Promise<Buffer> {
    return this.toBuffer('raw')
  }
  /** Encodes at the renderer's default frame rate. Pass `fps` to {@link WorkerCanvas.toBuffer} to choose one. */
  get gif(): Promise<Buffer> {
    return this.toBuffer('gif')
  }
  /** Encodes at the renderer's default frame rate. Pass `fps` to {@link WorkerCanvas.toBuffer} to choose one. */
  get apng(): Promise<Buffer> {
    return this.toBuffer('apng')
  }
  /** Shorthand for `toBuffer('avif')`. Smaller again than WebP, at a much higher encode cost. */
  get avif(): Promise<Buffer> {
    return this.toBuffer('avif')
  }
  /** Shorthand for `toBuffer('tiff')`. A sheet per page, for print pipelines. */
  get tiff(): Promise<Buffer> {
    return this.toBuffer('tiff')
  }
  /** Shorthand for `toBuffer('ico')`. An icon, one size per page. */
  get ico(): Promise<Buffer> {
    return this.toBuffer('ico')
  }
  /** Shorthand for `toBuffer('bmp')`. Uncompressed pixels in a container almost nothing needs. */
  get bmp(): Promise<Buffer> {
    return this.toBuffer('bmp')
  }

  // --- Members a worker-held canvas genuinely cannot provide ---

  /**
   * `getContext`, `newPage` and `pages` all hand back a live `CanvasRenderingContext2D` bound to
   * native memory in the worker. Proxying one would mean a thread round trip per `fillRect`, so
   * these throw rather than return something that only looks like a context. Drawing is expressed
   * as a component tree here, which is what the worker replays on the other side.
   */
  getContext(): never {
    throw new Error('[canvas] getContext() is not available in worker mode — describe drawing with a component tree, or use Root({ workerMode: false })')
  }

  /** Not available in worker mode: a page is added while drawing, and drawing happens in the worker. */
  newPage(): never {
    throw new Error('[canvas] newPage() is not available in worker mode — use Root({ workerMode: false })')
  }

  /** Not available in worker mode — see {@link newPage}. */
  get pages(): never {
    throw new Error('[canvas] pages is not available in worker mode — use Root({ workerMode: false })')
  }

  /** Release the Canvas from worker memory. Call when done with this object. */
  release(): void {
    this._pool.releaseCanvas(this._workerIdx, this._canvasId)
    canvasRegistry.unregister(this)
    this._cache.clear()
  }
}

/** The chart options whose value is a callback returning something to draw. */
const CHART_ITEM_OPTIONS = ['renderLegendItem', 'renderLabelItem', 'renderValueItem'] as const

/**
 * Builds a descriptor into a node, and hands a node straight back.
 *
 * The narrowing is the `in` check rather than an assertion: a descriptor carries `__type` and a
 * `BoxNode` does not, so the compiler resolves the union itself and this returns a
 * {@link ResolvedChartItem} because it provably cannot return anything else.
 */
function resolveChartItem(item: ChartItem): ResolvedChartItem {
  if (!item) return item
  return '__type' in item ? buildTree(item) : item
}

function withBuiltChartItems(props: ChartProps<ChartType>): ResolvedChartProps<ChartType> {
  const options = props.options as Record<string, unknown> | undefined
  if (!options) return props as ResolvedChartProps<ChartType>

  const wrapped: Record<string, unknown> = {}
  for (const name of CHART_ITEM_OPTIONS) {
    const render = options[name]
    if (typeof render !== 'function') continue
    const callback = render as (args: never) => ChartItem
    wrapped[name] = (args: never): ResolvedChartItem => resolveChartItem(callback(args))
  }

  // The keys are read by name from a `readonly string[]`, which no signature can follow, so the
  // reassembly is asserted. What it asserts is exactly what the loop above just did, and the one
  // claim that used to matter -- that no descriptor survives -- is now `resolveChartItem`'s return
  // type rather than anybody's word for it.
  if (!Object.keys(wrapped).length) return props as ResolvedChartProps<ChartType>
  return { ...props, options: { ...options, ...wrapped } } as ResolvedChartProps<ChartType>
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
    case 'Path':
      return new PathNode(descriptor.props)
    case 'Text':
      return new TextNode(descriptor.text, descriptor.props)
    case 'Chart':
      return new ChartNode(withBuiltChartItems(descriptor.props as ChartProps<ChartType>))
  }
}

/**
 * Root node that manages the canvas rendering context and coordinates overall layout and drawing.
 * Inherits from ColumnNode to provide vertical layout capabilities.
 */
export class RootNode extends ColumnNode {
  declare props: RootNodeProps & BaseProps
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
  constructor(props: RootNodeProps & BaseProps) {
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
  /** Every node in the tree carrying a `backgroundImage`, whatever kind of node it is. */
  private findAllBackgroundImageNodes(): BoxNode[] {
    const nodes: BoxNode[] = []
    const queue: BoxNode[] = [this]
    let head = 0
    while (head < queue.length) {
      const node = queue[head++]
      if (node.props.backgroundImage?.src) nodes.push(node)
      queue.push(...node.children)
    }
    return nodes
  }

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

            // A face that has just arrived changes what an identical font string measures — the
            // same `12px Roboto` resolved to a fallback a moment ago. Measurements taken under the
            // old set have to stop being reachable, or the fallback's geometry outlives it.
            invalidateTextMeasurements()
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
      const contentHeight = await this.prepare(new Map(), diskCacheKeys)

      this.canvas = createCanvas(this.canvasWidth(), this.canvasHeight(contentHeight), this.canvasOptions())
      this.ctx = this.canvas.getContext('2d')
      this.ctx.scale(this.scale, this.scale)

      await this.drawInto(this.ctx)

      if (!this.canvas) {
        throw new Error('Canvas not initialized')
      }

      return this.canvas
    } finally {
      // Freed before the disk cache is cleaned up, not after. Both run in the same `finally`, but
      // deleting cache files is asynchronous, and awaiting it first would hold the whole layout
      // tree in WASM memory across that I/O for no reason. This is the earliest point the tree is
      // provably dead: layout is read during `drawInto`, and nothing consults it afterwards.
      this.freeLayoutTree()

      if (diskCacheKeys?.size) {
        await Promise.allSettled([...diskCacheKeys].map(key => deleteDiskCache(key)))
      }
    }
  }

  /**
   * Loads images and settles the layout, returning the height the content came out at.
   *
   * Split from {@link RootNode.render} so a paged render can drive it directly: pages share one
   * canvas but not one tree, and the canvas cannot be sized until the first tree has been laid out.
   *
   * `imageCache` is supplied by the caller rather than created here precisely so a paged render can
   * pass the same map to every page. An image referenced by sixty frames then loads once instead of
   * sixty times.
   */
  async prepare(imageCache: RenderImageCache, diskCacheKeys?: Set<string>): Promise<number> {
    // Load all images with a concurrency limit to avoid overwhelming remote sources.
    // Both kinds of source go through one queue: an `Image`'s own picture and any node's
    // `backgroundImage`, which share a cache and should share the concurrency limit too.
    const queue: Array<() => Promise<void>> = [
      ...this.findAllImageNodes().map(node => () => node.load(imageCache, diskCacheKeys)),
      ...this.findAllBackgroundImageNodes().map(node => () => node.loadBackgroundImage(imageCache, diskCacheKeys)),
    ]
    if (queue.length > 0) {
      let qIdx = 0
      const workers = Array.from({ length: Math.min(this.imageConcurrency, queue.length) }, async () => {
        while (qIdx < queue.length) {
          const load = queue[qIdx++]
          await load()
        }
      })
      await Promise.allSettled(workers).then(results => {
        results.forEach(r => {
          if (r.status === 'rejected') console.warn('[RootNode] Image load worker failed:', r.reason)
        })
      })
    }

    this.node.calculateLayout(this.targetWidth, undefined, Style.Direction.LTR)

    // Allow nodes to finalize their layout, recalculating if any of them changed size.
    const needRecalculate = this.finalizeLayout()
    if (needRecalculate) {
      this.node.calculateLayout(this.targetWidth, undefined, Style.Direction.LTR)
    }

    return this.node.getComputedHeight()
  }

  /** Draws the prepared tree into a context. Call {@link RootNode.prepare} first. */
  async drawInto(ctx: CanvasRenderingContext2D): Promise<void> {
    await super.render(ctx, 0, 0)
  }

  /**
   * Engine options for the canvas this root draws into, or `undefined` when none were named.
   *
   * Only the keys the caller gave are included, and nothing is passed at all when there are none:
   * the defaults belong to the renderer rather than to this layer, which has no business restating
   * them.
   */
  canvasOptions(): CanvasEngineOptions | undefined {
    const { gpu, colorType, colorSpace } = this.props
    const options: CanvasEngineOptions = {
      ...(gpu !== undefined && { gpu }),
      ...(colorType !== undefined && { colorType }),
      ...(colorSpace !== undefined && { colorSpace }),
    }
    return Object.keys(options).length > 0 ? options : undefined
  }

  /** Canvas width in device pixels. */
  canvasWidth(): number {
    return Math.ceil(this.targetWidth * this.scale)
  }

  /**
   * Canvas height in device pixels, falling back to the height the content laid out at when no
   * explicit height was given.
   */
  canvasHeight(contentHeight: number): number {
    return this.targetHeight ? Math.ceil(this.targetHeight * this.scale) : Math.max(1, Math.ceil(contentHeight * this.scale))
  }

  /** Scale factor applied to every page's context. */
  get renderScale(): number {
    return this.scale
  }

  /**
   * Tells every image in this tree which moment of the render it is being drawn for.
   *
   * Animated sources play at their own rate, so they need the page's clock rather than its index —
   * a 10fps GIF drawn into a 24fps render advances on some pages and not others. Reuses the walk
   * that collects images for loading, so a page pays for one traversal, not two.
   */
  setPageTime(seconds: number): void {
    for (const image of this.findAllImageNodes()) {
      image.setPageTime(seconds)
    }
  }

  /** Registers this render's fonts. Public so a paged render can do it once for the whole sequence. */
  registerFonts(): Promise<void> {
    return this._registerFonts()
  }

  /** Releases the Yoga tree. Public so a paged render can free each page as soon as it is drawn. */
  releaseLayoutTree(): void {
    this.freeLayoutTree()
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
 * Renders a sequence of pages onto one canvas.
 *
 * One `RootNode` per page, not one reused across pages: the tree is built in the constructor and
 * freed once drawn, and a freed Yoga node cannot be laid out again. Building a fresh tree is cheap
 * — plain object construction — next to the layout and image work each page needs anyway.
 *
 * The costly parts are shared instead. One image cache spans every page, so a source referenced by
 * the whole sequence is fetched once; fonts register once; and the disk cache is swept once at the
 * end rather than per page.
 *
 * Each page's tree is freed as soon as it has been drawn, so memory stays flat across a long
 * sequence instead of holding every page's layout at once.
 */
export async function renderPages(props: RootProps, pages: (Children | Children[])[]): Promise<Canvas> {
  const fps = resolveFps(props.fps)
  const diskCacheKeys = props.useDiskCache ? new Set<string>() : undefined
  const imageCache: RenderImageCache = new Map()

  // Dropped rather than spread: `children` here is the builder that produced `pages`, and
  // `pagedChildren` is the wire form of the same thing. A node draws one page and takes neither.
  const { children: _builder, pagedChildren: _resolved, ...pageProps } = props

  let canvas: Canvas | undefined
  let fontsRegistered = false

  try {
    for (const [index, children] of pages.entries()) {
      const node = new RootNode({ ...pageProps, children } as RootNodeProps & BaseProps)
      try {
        // The same clock the page builder was handed, so an animated source and a track that were
        // described against the same moment stay in step.
        node.setPageTime(index / fps)

        if (!fontsRegistered) {
          await node.registerFonts()
          fontsRegistered = true
        }

        const contentHeight = await node.prepare(imageCache, diskCacheKeys)
        const width = node.canvasWidth()
        const height = node.canvasHeight(contentHeight)

        // The first page owns the canvas; every later one appends to it.
        const ctx = canvas ? canvas.newPage(width, height) : (canvas = createCanvas(width, height, node.canvasOptions())).getContext('2d')
        ctx.scale(node.renderScale, node.renderScale)

        await node.drawInto(ctx)
      } finally {
        node.releaseLayoutTree()
      }
    }

    if (!canvas) {
      throw new Error('[canvas] a paged render produced no pages')
    }
    return canvas
  } finally {
    if (diskCacheKeys?.size) {
      await Promise.allSettled([...diskCacheKeys].map(key => deleteDiskCache(key)))
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
export function Root(props: RootPropsWithWorker & RootContent): Promise<WorkerCanvas>
export function Root(props: RootPropsWithoutWorker & RootContent): Promise<RenderedCanvas>
export async function Root(props: RootProps): Promise<Canvas | WorkerCanvas> {
  // Determine worker mode
  const workerMode = props.workerMode ?? true
  const workerPoolSize = props.workers ?? Math.max(1, cpus().length - 1)

  // Runs on this thread even for a worker render. The builder is a function, and a function cannot
  // be structured-cloned; resolving it here sends the worker plain data in a single transfer
  // instead of a round trip per page. It also keeps nested `onLoad`/`onError` callbacks working —
  // those are extracted from the props on the way out, which a tree returned later through the
  // callback proxy would bypass.
  const pages = await planPages(props)

  if (workerMode) {
    // Lazy initialize worker pool — dynamic import to avoid loading Comlink in non-worker contexts
    if (!_workerPool) {
      const { ComlinkPool } = await import('@/worker/comlink.pool.js')
      _workerPool = new ComlinkPool(workerPoolSize)
    }
    const result = await _workerPool.render(pages ? { ...props, children: undefined, pagedChildren: pages } : props)
    return new WorkerCanvas({ ...result, pool: _workerPool })
  }

  // Non-worker mode — render directly and return Canvas
  return pages ? renderPages(props, pages) : new RootNode(asNodeProps(props)).render()
}
