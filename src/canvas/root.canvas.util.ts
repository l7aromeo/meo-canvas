import { Canvas, FontLibrary, type CanvasRenderingContext2D } from 'skia-canvas'
import { ColumnNode, BoxNode, RowNode } from '@/canvas/layout.canvas.util.js'
import type { BaseProps, RootProps, NodeDescriptor } from '@/canvas/canvas.type.js'
import { ImageNode } from '@/canvas/image.canvas.util.js'
import { TextNode } from '@/canvas/text.canvas.util.js'
import { ChartNode } from '@/canvas/chart.canvas.util.js'
import { GridNode, GridItemNode } from '@/canvas/grid.canvas.util.js'
import { Style } from '@/constant/common.const.js'
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

/** Engine configuration */
let _workerMode = true
let _workerPoolSize = Math.max(1, cpus().length - 1)
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
 */
export function configure(options: CanvasEngineConfig) {
  if (options.workerMode !== undefined) _workerMode = options.workerMode
  if (options.workers !== undefined) _workerPoolSize = options.workers
  if (_workerMode) {
    _workerPool = new WorkerPool(_workerPoolSize)
  }
}

/**
 * Minimal Canvas-compatible wrapper returned when rendering in worker mode.
 * Exposes toBuffer / toBufferSync so callers can use the result identically.
 */
class WorkerCanvas {
  constructor(private readonly _buffer: Buffer) {}
  toBufferSync(_format?: string) {
    return this._buffer
  }
  toBuffer(_format?: string): Promise<Buffer> {
    return Promise.resolve(this._buffer)
  }
}

/** Lazy-instantiated worker pool singleton */
class WorkerPool {
  private workers: Worker[] = []
  private idle: Worker[] = []
  private queue: Array<{ id: number; props: RootProps }> = []
  private pending = new Map<number, { resolve: (b: Buffer) => void; reject: (e: Error) => void }>()
  private nextId = 0

  constructor(size: number) {
    this.init(size)
  }

  private async init(size: number) {
    const workerFile = path.join(path.dirname(fileURLToPath(import.meta.url)), '../render.worker.js')

    for (let i = 0; i < size; i++) {
      const worker = new Worker(workerFile)
      worker.on('message', ({ id, buffer, error }: { id: number; buffer?: Buffer; error?: string }) => {
        const task = this.pending.get(id)
        if (!task) return
        this.pending.delete(id)
        this.idle.push(worker)
        this.drain()
        if (error) {
          task.reject(new Error(error))
        } else {
          task.resolve(buffer!)
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
      worker.postMessage({ id: task.id, props: task.props })
    }
  }

  render(props: RootProps): Promise<Buffer> {
    return new Promise((resolve, reject) => {
      const id = this.nextId++
      this.pending.set(id, { resolve, reject })
      if (this.idle.length > 0) {
        const worker = this.idle.pop()!
        worker.postMessage({ id, props })
      } else {
        this.queue.push({ id, props })
      }
    })
  }

  terminate() {
    this.workers.forEach(w => w.terminate())
  }
}

/**
 * Converts a NodeDescriptor tree into actual BoxNode instances.
 * Used both for non-worker rendering (inline tree building) and inside
 * the render worker (reconstructing the tree from serialized descriptors).
 */
export function buildTree(descriptor: NodeDescriptor): BoxNode {
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
  /** The canvas instance used for rendering */
  private canvas: Canvas | undefined
  /** The 2D rendering context for the canvas */
  private ctx: CanvasRenderingContext2D | null = null
  /** Target width for the canvas in pixels */
  private readonly targetWidth: number
  /** Target height for the canvas in pixels */
  private readonly targetHeight: number
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

    // Convert any NodeDescriptor children to actual BoxNode instances
    if (this.props.children) {
      const childArray = Array.isArray(this.props.children) ? this.props.children : [this.props.children]
      this.props.children = childArray.map(child => {
        if (child && typeof child === 'object' && '__type' in child) {
          return buildTree(child as NodeDescriptor)
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
    // Step 1: Load all images with a concurrency limit to avoid overwhelming remote sources
    const imageNodes = this.findAllImageNodes()
    if (imageNodes.length > 0) {
      const CONCURRENCY = 5
      const queue = [...imageNodes]
      const workers = Array.from({ length: Math.min(CONCURRENCY, queue.length) }, async () => {
        while (queue.length > 0) {
          const node = queue.shift()!
          await node.load()
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
  }
}

/**
 * Creates and renders a new root node with the given properties.
 * When worker mode is enabled via configure(), rendering runs in a worker thread
 * and the returned object implements the same toBuffer/toBufferSync interface.
 * @param props Configuration properties for the root node
 * @returns Promise resolving to the rendered Canvas (or WorkerCanvas in worker mode)
 */
export const Root = async (props: RootProps): Promise<Canvas> => {
  if (_workerMode) {
    if (!_workerPool) {
      _workerPool = new WorkerPool(_workerPoolSize)
    }
    const buffer = await _workerPool.render(props)
    return new WorkerCanvas(buffer) as unknown as Canvas
  }
  return new RootNode(props).render()
}
