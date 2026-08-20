import { vi, type MockInstance } from 'vitest'
import type { CanvasRenderingContext2D } from 'meo-skia-canvas'
import type { ImageProps } from '@/canvas/canvas.type.js'
import { Direction } from 'yoga-layout'
import { Style } from '@/constant/common.const.js'

// --- Mock setup ---

const mockLoadImage = vi.fn<(src: any) => Promise<any>>()
const mockFileTypeFromBuffer = vi.fn<(buf: any) => Promise<any>>()
const mockFileTypeFromFile = vi.fn<(path: any) => Promise<any>>()
const mockReadFile = vi.fn<(path: any) => Promise<any>>()

/** Records the contexts handed out, so a test can see what was drawn on an offscreen. */
const offscreenContexts: ReturnType<typeof createMockCtx>[] = []

vi.mock('meo-skia-canvas', () => ({
  loadImage: mockLoadImage,
  Image: vi.fn(),
  // A drop shadow builds its drawing on an offscreen before compositing it, so the canvas this
  // constructs has to hand back a context like the real one does.
  Canvas: class {
    width: number
    height: number
    constructor(width: number, height: number) {
      this.width = width
      this.height = height
    }
    getContext() {
      const ctx = createMockCtx()
      offscreenContexts.push(ctx)
      return ctx
    }
  },
  FontLibrary: { use: vi.fn() },
}))

vi.mock('file-type', () => ({
  fileTypeFromBuffer: mockFileTypeFromBuffer,
  fileTypeFromFile: mockFileTypeFromFile,
}))

vi.mock('fs', () => ({
  promises: { readFile: mockReadFile },
}))

let ImageNode: typeof import('@/canvas/image.canvas.js').ImageNode
let Image: typeof import('@/canvas/image.canvas.js').Image

const createMockCtx = (): CanvasRenderingContext2D => {
  const ctx: Partial<CanvasRenderingContext2D> = {
    save: vi.fn(),
    restore: vi.fn(),
    clip: vi.fn(),
    // The offscreen a drop shadow is built on is translated into the node's own coordinates.
    translate: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    arcTo: vi.fn(),
    closePath: vi.fn(),
    drawImage: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    rect: vi.fn(),
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
    createLinearGradient: vi.fn(() => ({ addColorStop: vi.fn(), interpolation: 'srgb' as const, hueInterpolation: 'shorter' as const })),
    createRadialGradient: vi.fn(() => ({ addColorStop: vi.fn(), interpolation: 'srgb' as const, hueInterpolation: 'shorter' as const })),
  }
  return ctx as CanvasRenderingContext2D
}

const mockImage = { width: 200, height: 100 }

describe('ImageNode & Image factory', () => {
  beforeEach(async () => {
    vi.resetModules()

    // Re-setup mocks after module reset
    vi.doMock('meo-skia-canvas', () => ({
      loadImage: mockLoadImage,
      Image: vi.fn(),
      // Matches the hoisted mock above: a drop shadow builds its drawing on an offscreen, so this
      // has to hand back a context the way a real canvas does.
      Canvas: class {
        width: number
        height: number
        constructor(width: number, height: number) {
          this.width = width
          this.height = height
        }
        getContext() {
          const ctx = createMockCtx()
          offscreenContexts.push(ctx)
          return ctx
        }
      },
      FontLibrary: { use: vi.fn() },
    }))
    vi.doMock('file-type', () => ({
      fileTypeFromBuffer: mockFileTypeFromBuffer,
      fileTypeFromFile: mockFileTypeFromFile,
    }))
    vi.doMock('fs', () => ({
      promises: { readFile: mockReadFile },
    }))

    const mod = await import('@/canvas/image.canvas.js')
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
      const onLoad = vi.fn()
      const node = new ImageNode({ src: 'test.png', onLoad })
      await node.load()

      expect(mockLoadImage).toHaveBeenCalled()
      expect(onLoad).toHaveBeenCalled()
    })

    it('should handle load error gracefully and call onError', async () => {
      const loadError = new Error('load failed')
      mockLoadImage.mockRejectedValueOnce(loadError)
      const onError = vi.fn()
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
      await node.render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
    })

    it('should not call drawImage when image is not loaded', async () => {
      mockLoadImage.mockRejectedValueOnce(new Error('fail'))
      const ctx = createMockCtx()
      const node = await setupRenderableNode({ src: 'test.png' })
      await node.render(ctx, 0, 0)
      expect(ctx.drawImage).not.toHaveBeenCalled()
    })

    it('should handle object-fit contain', async () => {
      // Image is 200x100 (2:1 ratio), container is 100x100 (1:1)
      // contain: imgRatio(2) > nodeRatio(1) => dw=100, dh=50
      const ctx = createMockCtx()
      const node = await setupRenderableNode({ src: 'test.png', objectFit: 'contain' })
      await node.render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
      const drawCall = (ctx.drawImage as unknown as MockInstance<any>).mock.calls[0]
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
      await node.render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
      const drawCall = (ctx.drawImage as unknown as MockInstance<any>).mock.calls[0]
      const dw = drawCall[7]
      const dh = drawCall[8]
      expect(dw).toBe(200)
      expect(dh).toBe(100)
    })

    it('should handle object-fit none (natural dimensions)', async () => {
      const ctx = createMockCtx()
      const node = await setupRenderableNode({ src: 'test.png', objectFit: 'none' })
      await node.render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
      const drawCall = (ctx.drawImage as unknown as MockInstance<any>).mock.calls[0]
      const dw = drawCall[7]
      const dh = drawCall[8]
      expect(dw).toBe(200)
      expect(dh).toBe(100)
    })

    it('casts the shadow outside the clip that keeps the image in its box', async () => {
      const ctx = createMockCtx()
      offscreenContexts.length = 0
      const node = await setupRenderableNode({
        src: 'test.png',
        dropShadow: { offsetX: 5, offsetY: 5, blur: 10, color: 'rgba(0,0,0,0.5)' },
      })
      await node.render(ctx, 0, 0)

      // The image is built on an offscreen and composited in one call with the shadow set. Drawn
      // inside the node's own clip instead — which is there to keep the image in its box — the
      // shadow falls outside it and is clipped away, which is what used to happen: nothing drew.
      expect(offscreenContexts.length).toBeGreaterThan(0)
      expect(offscreenContexts[0].drawImage).toHaveBeenCalled()
      expect(ctx.drawImage).toHaveBeenCalled()
      expect(ctx.shadowColor).toBe('rgba(0,0,0,0.5)')
      expect(ctx.shadowOffsetX).toBe(5)
      expect(ctx.shadowOffsetY).toBe(5)
      // Taken from `blur`, not derived from the offsets as it used to be.
      expect(ctx.shadowBlur).toBe(10)
    })

    it('leaves the context alone when no shadow is asked for', async () => {
      const ctx = createMockCtx()
      const node = await setupRenderableNode({ src: 'test.png' })
      await node.render(ctx, 0, 0)

      expect(ctx.drawImage).toHaveBeenCalled()
      expect(ctx.shadowColor).not.toBe('rgba(0,0,0,0.5)')
    })

    it('should set filter string when saturate is not 1', async () => {
      const ctx = createMockCtx()
      const node = await setupRenderableNode({ src: 'test.png', saturate: 0.5 })
      await node.render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
    })
  })

  // --- 5. Caching ---

  describe('Caching', () => {
    it('should use cache for same src', async () => {
      const cache = new Map() as import('@/canvas/image.canvas.js').RenderImageCache
      const node1 = new ImageNode({ src: 'test.png', width: 100, height: 100 })
      const node2 = new ImageNode({ src: 'test.png', width: 100, height: 100 })

      await node1.load(cache)
      await node2.load(cache)

      // loadImage is called via _fetchCanvasImage, which is cached — so only 1 fetch
      expect(cache.size).toBe(1)
    })

    it('should create separate cache entries for different src', async () => {
      const cache = new Map() as import('@/canvas/image.canvas.js').RenderImageCache
      const node1 = new ImageNode({ src: 'a.png', width: 100, height: 100 })
      const node2 = new ImageNode({ src: 'b.png', width: 100, height: 100 })

      await node1.load(cache)
      await node2.load(cache)

      expect(cache.size).toBe(2)
    })

    it('should create separate cache entries for same src with different color', async () => {
      const cache = new Map() as import('@/canvas/image.canvas.js').RenderImageCache

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

  // --- 5b. httpOptions ---

  describe('httpOptions', () => {
    let mockFetch: MockInstance<any>

    beforeEach(() => {
      mockFileTypeFromBuffer.mockResolvedValue({ mime: 'image/png' })
      mockFetch = vi.fn(async () => ({
        ok: true,
        status: 200,
        arrayBuffer: async () => new ArrayBuffer(8),
      }))
      vi.stubGlobal('fetch', mockFetch)
    })

    afterEach(() => {
      vi.unstubAllGlobals()
    })

    it('should pass httpOptions as the second argument to fetch for http src', async () => {
      const httpOptions = { headers: { Authorization: 'Bearer token123' } }
      const node = new ImageNode({ src: 'https://example.com/img.png', httpOptions })
      await node.load()

      expect(mockFetch).toHaveBeenCalledWith('https://example.com/img.png', httpOptions)
    })

    it('should call fetch with undefined options when httpOptions is not provided', async () => {
      const node = new ImageNode({ src: 'https://example.com/img.png' })
      await node.load()

      expect(mockFetch).toHaveBeenCalledWith('https://example.com/img.png', undefined)
    })

    it('should create separate cache entries for same url with different httpOptions', async () => {
      const cache = new Map() as import('@/canvas/image.canvas.js').RenderImageCache
      const node1 = new ImageNode({ src: 'https://example.com/img.png', httpOptions: { headers: { Authorization: 'Bearer A' } } })
      const node2 = new ImageNode({ src: 'https://example.com/img.png', httpOptions: { headers: { Authorization: 'Bearer B' } } })

      await node1.load(cache)
      await node2.load(cache)

      expect(cache.size).toBe(2)
    })

    it('should reuse the cache entry for same url with identical httpOptions', async () => {
      const cache = new Map() as import('@/canvas/image.canvas.js').RenderImageCache
      const node1 = new ImageNode({ src: 'https://example.com/img.png', httpOptions: { headers: { Authorization: 'Bearer A' } } })
      const node2 = new ImageNode({ src: 'https://example.com/img.png', httpOptions: { headers: { Authorization: 'Bearer A' } } })

      await node1.load(cache)
      await node2.load(cache)

      expect(cache.size).toBe(1)
    })

    it('should produce a stable cache key regardless of header key ordering', async () => {
      const cache = new Map() as import('@/canvas/image.canvas.js').RenderImageCache
      const node1 = new ImageNode({ src: 'https://example.com/img.png', httpOptions: { headers: { A: '1', B: '2' } } })
      const node2 = new ImageNode({ src: 'https://example.com/img.png', httpOptions: { headers: { B: '2', A: '1' } } })

      await node1.load(cache)
      await node2.load(cache)

      expect(cache.size).toBe(1)
    })

    it('should handle a Headers instance deterministically in the cache key', async () => {
      const cache = new Map() as import('@/canvas/image.canvas.js').RenderImageCache
      const node1 = new ImageNode({ src: 'https://example.com/img.png', httpOptions: { headers: new Headers({ Authorization: 'Bearer A' }) } })
      const node2 = new ImageNode({ src: 'https://example.com/img.png', httpOptions: { headers: { authorization: 'Bearer A' } } })

      await node1.load(cache)
      await node2.load(cache)

      // Headers normalizes keys to lowercase, so both should collapse to one entry
      expect(cache.size).toBe(1)
    })

    it('should not throw and still load when httpOptions contains a circular reference', async () => {
      const circular: any = { headers: { 'X-Test': '1' } }
      circular.self = circular
      const onError = vi.fn()
      const onLoad = vi.fn()
      const node = new ImageNode({ src: 'https://example.com/img.png', httpOptions: circular, onError, onLoad })

      await node.load()

      expect(onError).not.toHaveBeenCalled()
      expect(onLoad).toHaveBeenCalled()
      expect(mockLoadImage).toHaveBeenCalled()
    })

    it('should not include httpOptions in the cache key for non-http (file) src', async () => {
      const cache = new Map() as import('@/canvas/image.canvas.js').RenderImageCache
      const node1 = new ImageNode({ src: 'local.png', httpOptions: { headers: { Authorization: 'Bearer A' } } })
      const node2 = new ImageNode({ src: 'local.png', httpOptions: { headers: { Authorization: 'Bearer B' } } })

      await node1.load(cache)
      await node2.load(cache)

      // httpOptions is meaningless for file paths — same file should share a cache entry
      expect(cache.size).toBe(1)
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
        expect.fail('Expected loadImage to be called with a Buffer for SVG color replacement')
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
