import { jest } from '@jest/globals'
import { __mocks__ as skiaCanvasMocks } from '@/__mocks__/skia-canvas.js'
import { __mocks__ as fsMocks } from '@/__mocks__/node-fs.js'
import { __mocks__ as pathMocks } from '@/__mocks__/node-path.js'
import { ImageNode } from '@/__mocks__/image.canvas.util.js'
import { ColumnNode } from '@/__mocks__/layout.canvas.util.js'
import { Style } from '@/constant/common.const.js'
import { RootNode as BaseRootNode } from '@/canvas/root.canvas.util.js'

const registeredFonts = new Map<string, Set<string>>()

export const _clearRegisteredFonts = jest.fn(() => {
  registeredFonts.clear()
})

export const RootNode = jest.fn(function (this: any, props: any) {
  // Validate FIRST - fail fast
  if (!props.width) {
    throw new Error('Width and height are required for Root')
  }

  // Call parent constructor
  ColumnNode.call(this, { name: 'Root', ...props })

  this.props = props

  // Initialize instance properties
  this.scale = props.scale || 1
  ;(this as any).targetWidth = props.width
  ;(this as any).targetHeight = props.height
  this.canvas = undefined
  this.ctx = null

  // Register fonts with caching
  if (props.fonts?.length) {
    for (const font of props.fonts) {
      const family = font.family
      const paths = font.paths.map((p: unknown) => pathMocks.resolve(p))

      if (!registeredFonts.has(family)) {
        registeredFonts.set(family, new Set())
      }

      const cachedPaths = registeredFonts.get(family)!
      const newPaths = paths.filter((p: string) => !cachedPaths.has(p) && fsMocks.existsSync(p))

      if (newPaths.length > 0) {
        skiaCanvasMocks.FontLibrary.use({ [family]: newPaths })
        newPaths.forEach((p: string) => cachedPaths.add(p))
      }
    }
  }

  this.node.setWidth(this.targetWidth)
  this.processInitialChildren()

  // BFS traversal for image nodes
  this.findAllImageNodes = jest.fn(function (this: any) {
    const imageNodes: any[] = []
    const queue: any[] = [this]
    while (queue.length > 0) {
      const node = queue.shift()!
      if (node instanceof ImageNode) {
        imageNodes.push(node)
      }
      if (node.children) {
        queue.push(...node.children)
      }
    }
    return imageNodes
  })

  this.finalizeLayout = jest.fn(() => false)

  this.render = async function (this: any) {
    const imageNodes = this.findAllImageNodes()
    const loadingPromises = imageNodes.map((node: any) => node.getLoadingPromise())

    if (loadingPromises.length > 0) {
      await Promise.allSettled(loadingPromises)
    }

    this.node.calculateLayout(this.targetWidth, undefined, Style.Direction.LTR)

    const needRecalculate = this.finalizeLayout()
    if (needRecalculate) {
      this.node.calculateLayout(this.targetWidth, undefined, Style.Direction.LTR)
    }

    const calculatedContentHeight = this.node.getComputedHeight()
    const finalCanvasWidth = Math.ceil(this.targetWidth * this.scale)
    const finalCanvasHeight = this.targetHeight ? Math.ceil(this.targetHeight * this.scale) : Math.max(1, Math.ceil(calculatedContentHeight * this.scale))
    this.canvas = new skiaCanvasMocks.Canvas(finalCanvasWidth, finalCanvasHeight)
    this.ctx = this.canvas.getContext('2d')
    this.ctx.scale(this.scale, this.scale)

    ColumnNode.prototype.render.call(this, this.ctx, 0, 0)

    if (!this.canvas) {
      throw new Error('Canvas not initialized')
    }

    return this.canvas
  }
})

// Set up prototype chain
RootNode.prototype = Object.create(ColumnNode.prototype)
RootNode.prototype.constructor = RootNode

export const Root = jest.fn(async (props: any) => {
  const instance = new RootNode(props)
  return (instance as unknown as ReturnType<() => BaseRootNode>).render()
})

export const __mocks__ = {
  RootNode,
  Root,
  _clearRegisteredFonts,
  reset: () => {
    _clearRegisteredFonts.mockClear()
    registeredFonts.clear()
  },
}
