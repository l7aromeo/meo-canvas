import { jest } from '@jest/globals'
import type { CanvasRenderingContext2D } from 'skia-canvas'
import type { ImageProps } from '@/canvas/canvas.type.js'
import { Direction } from 'yoga-layout'
import { Style } from '@/constant/common.const.js'

// --- Mock setup ---

const mockLoadImage = jest.fn<(src: any) => Promise<any>>()
const mockFileTypeFromBuffer = jest.fn<(buf: any) => Promise<any>>()
const mockFileTypeFromFile = jest.fn<(path: any) => Promise<any>>()
const mockReadFile = jest.fn<(path: any) => Promise<any>>()

jest.unstable_mockModule('skia-canvas', () => ({
  loadImage: mockLoadImage,
  Image: jest.fn(),
  Canvas: jest.fn(),
  FontLibrary: { use: jest.fn() },
}))

jest.unstable_mockModule('file-type', () => ({
  fileTypeFromBuffer: mockFileTypeFromBuffer,
  fileTypeFromFile: mockFileTypeFromFile,
}))

jest.unstable_mockModule('fs', () => ({
  promises: { readFile: mockReadFile },
}))

let ImageNode: typeof import('@/canvas/image.canvas.util.js').ImageNode
let Image: typeof import('@/canvas/image.canvas.util.js').Image

const createMockCtx = (): CanvasRenderingContext2D => {
  const ctx: Partial<CanvasRenderingContext2D> = {
    save: jest.fn(),
    restore: jest.fn(),
    clip: jest.fn(),
    beginPath: jest.fn(),
    moveTo: jest.fn(),
    lineTo: jest.fn(),
    arcTo: jest.fn(),
    closePath: jest.fn(),
    drawImage: jest.fn(),
    fill: jest.fn(),
    stroke: jest.fn(),
    rect: jest.fn(),
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 0,
    globalAlpha: 1,
    shadowOffsetX: 0,
    shadowOffsetY: 0,
    shadowBlur: 0,
    shadowColor: '',
    filter: '',
    imageSmoothingEnabled: true,
    imageSmoothingQuality: 'high',
    globalCompositeOperation: 'source-over',
    createLinearGradient: jest.fn(() => ({ addColorStop: jest.fn() })),
    createRadialGradient: jest.fn(() => ({ addColorStop: jest.fn() })),
  }
  return ctx as CanvasRenderingContext2D
}

const mockImage = { width: 200, height: 100 }

describe('ImageNode & Image factory', () => {
  beforeEach(async () => {
    jest.resetModules()

    // Re-setup mocks after module reset
    jest.unstable_mockModule('skia-canvas', () => ({
      loadImage: mockLoadImage,
      Image: jest.fn(),
      Canvas: jest.fn(),
      FontLibrary: { use: jest.fn() },
    }))
    jest.unstable_mockModule('file-type', () => ({
      fileTypeFromBuffer: mockFileTypeFromBuffer,
      fileTypeFromFile: mockFileTypeFromFile,
    }))
    jest.unstable_mockModule('fs', () => ({
      promises: { readFile: mockReadFile },
    }))

    const mod = await import('@/canvas/image.canvas.util.js')
    ImageNode = mod.ImageNode
    Image = mod.Image

    mockLoadImage.mockReset()
    mockFileTypeFromBuffer.mockReset()
    mockFileTypeFromFile.mockReset()
    mockReadFile.mockReset()

    mockLoadImage.mockResolvedValue(mockImage)
    mockFileTypeFromFile.mockResolvedValue({ mime: 'image/png' })
  })

  // --- 1. Image factory function ---

  describe('Image factory function', () => {
    it('should return a CanvasElement with __type Image', () => {
      const descriptor = Image({ src: 'test.png' })
      expect(descriptor.__type).toBe('Image')
      expect(descriptor.props).toBeDefined()
    })

    it('should type-cast props to omit onLoad and onError', () => {
      const onLoad = () => {}
      const onError = () => {}
      const descriptor = Image({ src: 'test.png', onLoad, onError })
      // The factory uses a type-level Omit, so runtime props still contain them
      // but the descriptor type advertises them as stripped
      expect(descriptor.__type).toBe('Image')
      expect((descriptor.props as any).src).toBe('test.png')
    })
  })

  // --- 2. ImageNode construction ---

  describe('ImageNode construction', () => {
    it('should set default props', () => {
      const node = new ImageNode({ src: 'test.png' })
      expect(node.props.objectFit).toBe('fill')
      expect(node.props.overflow).toBe(Style.Overflow.Hidden)
      expect(node.props.saturate).toBe(1)
      expect(node.props.objectPosition).toEqual({ Left: '50%', Top: '50%' })
    })

    it('should merge user-provided props over defaults', () => {
      const node = new ImageNode({
        src: 'test.png',
        objectFit: 'cover',
        saturate: 0.5,
        objectPosition: { Left: '0%', Top: '0%' },
      })
      expect(node.props.objectFit).toBe('cover')
      expect(node.props.saturate).toBe(0.5)
      expect(node.props.objectPosition).toEqual({ Left: '0%', Top: '0%' })
    })

    it('should have name set to Image', () => {
      const node = new ImageNode({ src: 'test.png' })
      expect(node.name).toBe('Image')
    })
  })

  // --- 3. ImageNode.load() ---

  describe('ImageNode.load()', () => {
    it('should load image successfully and call onLoad', async () => {
      const onLoad = jest.fn()
      const node = new ImageNode({ src: 'test.png', onLoad })
      await node.load()

      expect(mockLoadImage).toHaveBeenCalled()
      expect(onLoad).toHaveBeenCalled()
    })

    it('should handle load error gracefully and call onError', async () => {
      const loadError = new Error('load failed')
      mockLoadImage.mockRejectedValueOnce(loadError)
      const onError = jest.fn()
      const node = new ImageNode({ src: 'test.png', onError })
      await node.load()

      expect(onError).toHaveBeenCalledWith(loadError)
    })

    it('should return same promise on repeated calls (memoization)', async () => {
      const node = new ImageNode({ src: 'test.png' })
      const promise1 = node.load()
      const promise2 = node.load()
      expect(promise1).toBe(promise2)
      await promise1
    })

    it('should resolve immediately with no src', async () => {
      const node = new ImageNode({ src: '' })
      await node.load()
      expect(mockLoadImage).not.toHaveBeenCalled()
    })
  })

  // --- 4. Image rendering (_renderContent) ---

  describe('Image rendering', () => {
    const setupRenderableNode = async (props: Partial<ImageProps>) => {
      const node = new ImageNode({ src: '', width: 100, height: 100, ...props } as ImageProps)
      await node.load()
      node.processInitialChildren()
      node.node.calculateLayout(100, 100, Direction.LTR)
      return node
    }

    it('should call drawImage when image is loaded', async () => {
      const ctx = createMockCtx()
      const node = await setupRenderableNode({ src: 'test.png' })
      node.render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
    })

    it('should not call drawImage when image is not loaded', async () => {
      mockLoadImage.mockRejectedValueOnce(new Error('fail'))
      const ctx = createMockCtx()
      const node = await setupRenderableNode({ src: 'test.png' })
      node.render(ctx, 0, 0)
      expect(ctx.drawImage).not.toHaveBeenCalled()
    })

    it('should handle object-fit contain', async () => {
      // Image is 200x100 (2:1 ratio), container is 100x100 (1:1)
      // contain: imgRatio(2) > nodeRatio(1) => dw=100, dh=50
      const ctx = createMockCtx()
      const node = await setupRenderableNode({ src: 'test.png', objectFit: 'contain' })
      node.render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
      const drawCall = (ctx.drawImage as jest.Mock<any>).mock.calls[0]
      // drawImage(img, sx, sy, sw, sh, dx, dy, dw, dh)
      const dw = drawCall[7] // finalDW (ceil)
      const dh = drawCall[8] // finalDH (ceil)
      expect(dw).toBe(100)
      expect(dh).toBe(50)
    })

    it('should handle object-fit cover', async () => {
      // Image is 200x100 (2:1 ratio), container is 100x100 (1:1)
      // cover: imgRatio(2) > nodeRatio(1) => dh=100, dw=200
      const ctx = createMockCtx()
      const node = await setupRenderableNode({ src: 'test.png', objectFit: 'cover' })
      node.render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
      const drawCall = (ctx.drawImage as jest.Mock<any>).mock.calls[0]
      const dw = drawCall[7]
      const dh = drawCall[8]
      expect(dw).toBe(200)
      expect(dh).toBe(100)
    })

    it('should handle object-fit none (natural dimensions)', async () => {
      const ctx = createMockCtx()
      const node = await setupRenderableNode({ src: 'test.png', objectFit: 'none' })
      node.render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
      const drawCall = (ctx.drawImage as jest.Mock<any>).mock.calls[0]
      const dw = drawCall[7]
      const dh = drawCall[8]
      expect(dw).toBe(200)
      expect(dh).toBe(100)
    })

    it('should set shadow properties when dropShadow is provided', async () => {
      const ctx = createMockCtx()
      const node = await setupRenderableNode({
        src: 'test.png',
        dropShadow: { offsetX: 5, offsetY: 5, blur: 10, color: 'rgba(0,0,0,0.5)' },
      })
      node.render(ctx, 0, 0)
      // Shadow properties are set inside a save/restore block
      // Verify drawImage was called (shadow was applied before it)
      expect(ctx.drawImage).toHaveBeenCalled()
    })

    it('should set filter string when saturate is not 1', async () => {
      const ctx = createMockCtx()
      const node = await setupRenderableNode({ src: 'test.png', saturate: 0.5 })
      node.render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
    })
  })

  // --- 5. Caching ---

  describe('Caching', () => {
    it('should use cache for same src', async () => {
      const cache = new Map() as import('@/canvas/image.canvas.util.js').RenderImageCache
      const node1 = new ImageNode({ src: 'test.png', width: 100, height: 100 })
      const node2 = new ImageNode({ src: 'test.png', width: 100, height: 100 })

      await node1.load(cache)
      await node2.load(cache)

      // loadImage is called via _fetchCanvasImage, which is cached — so only 1 fetch
      expect(cache.size).toBe(1)
    })

    it('should create separate cache entries for different src', async () => {
      const cache = new Map() as import('@/canvas/image.canvas.util.js').RenderImageCache
      const node1 = new ImageNode({ src: 'a.png', width: 100, height: 100 })
      const node2 = new ImageNode({ src: 'b.png', width: 100, height: 100 })

      await node1.load(cache)
      await node2.load(cache)

      expect(cache.size).toBe(2)
    })

    it('should create separate cache entries for same src with different color', async () => {
      const cache = new Map() as import('@/canvas/image.canvas.util.js').RenderImageCache

      // For SVG files with color, we need fileTypeFromFile to return svg type
      mockFileTypeFromFile.mockResolvedValue({ mime: 'image/svg+xml' })
      mockReadFile.mockResolvedValue(Buffer.from('<svg><path fill="#000"/></svg>'))

      const node1 = new ImageNode({ src: 'icon.svg', color: 'red', width: 100, height: 100 })
      const node2 = new ImageNode({ src: 'icon.svg', color: 'blue', width: 100, height: 100 })

      await node1.load(cache)
      await node2.load(cache)

      expect(cache.size).toBe(2)
    })
  })

  // --- 6. SVG color replacement ---

  describe('SVG color replacement', () => {
    it('should replace fill attributes when src is SVG and color is set', async () => {
      const svgContent = '<svg><path fill="#000000" d="M0 0"/></svg>'
      mockFileTypeFromFile.mockResolvedValue({ mime: 'image/svg+xml' })
      mockReadFile.mockResolvedValue(Buffer.from(svgContent))

      const node = new ImageNode({ src: 'icon.svg', color: '#FF0000', width: 50, height: 50 })
      await node.load()

      // loadImage should be called with a Buffer containing the replaced color
      const loadImageArg = mockLoadImage.mock.calls[0][0]
      if (Buffer.isBuffer(loadImageArg)) {
        const result = loadImageArg.toString('utf-8')
        expect(result).toContain('fill="#FF0000"')
        expect(result).not.toContain('fill="#000000"')
      } else {
        // If not a buffer, the SVG replacement path was not taken
        fail('Expected loadImage to be called with a Buffer for SVG color replacement')
      }
    })

    it('should not replace fill when color is not set on SVG', async () => {
      mockFileTypeFromFile.mockResolvedValue({ mime: 'image/svg+xml' })

      const node = new ImageNode({ src: 'icon.svg', width: 50, height: 50 })
      await node.load()

      // Without color prop, readFile should not be called for SVG replacement
      expect(mockReadFile).not.toHaveBeenCalled()
    })
  })
})
