import { Canvas, FontLibrary, type CanvasRenderingContext2D } from 'skia-canvas'
import { ColumnNode, BoxNode } from '@/canvas/layout.canvas.util.js'
import type { BaseProps, RootProps } from '@/canvas/canvas.type.js'
import { ImageNode } from '@/canvas/image.canvas.util.js'
import { Style } from '@/constant/common.const.js'
import * as path from 'node:path'
import * as fs from 'node:fs'

/** Registry to track fonts that have already been loaded */
const registeredFonts = new Map<string, Set<string>>()

// Exported for testing purposes only
export const _clearRegisteredFonts = () => {
  registeredFonts.clear()
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
  /** Scale factor for rendering (e.g. 2 for 2x resolution) */
  private readonly scale: number

  /**
   * Creates a new root node for canvas rendering
   * @param props - Configuration properties for the root node
   * @throws Error if width property is not provided
   */
  constructor(props: RootProps & BaseProps) {
    // Call the parent constructor with root name and props
    super({ name: 'Root', ...props })

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
    this.node.setWidth(this.targetWidth)

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
    // Step 1: Load all images
    const imageNodes = this.findAllImageNodes()
    const loadingPromises = imageNodes.map(node => node.getLoadingPromise())

    if (loadingPromises.length > 0) {
      await Promise.allSettled(loadingPromises)
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
    const finalCanvasHeight = Math.max(1, Math.ceil(calculatedContentHeight * this.scale))

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
 * Creates and renders a new root node with the given properties
 * @param props - Configuration properties for the root node
 * @returns Promise resolving to the rendered Canvas instance
 */
export const Root = async (props: RootProps) => await new RootNode(props).render()
