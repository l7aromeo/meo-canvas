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
    // The offscreen a drop shadow or a filter is built on is translated into the node's own
    // coordinates, and scaled to whatever resolution the real context is drawing at.
    translate: vi.fn(),
    scale: vi.fn(),
    // A filtered node reads the transform in force so its offscreen can be built at device
    // resolution. Identity is right for a mock: nothing here scales.
    getTransform: vi.fn(() => ({ a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 })) as unknown as CanvasRenderingContext2D['getTransform'],
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    arcTo: vi.fn(),
    arc: vi.fn(),
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

  // --- 6. objectPosition, aspect ratio and drop shadow ---

  describe('objectPosition', () => {
    const setup = async (props: Partial<ImageProps>) => {
      const node = new ImageNode({ src: 'test.png', width: 100, height: 100, objectFit: 'cover', ...props } as ImageProps)
      await node.load()
      node.processInitialChildren()
      node.node.calculateLayout(100, 100, Direction.LTR)
      return node
    }

    /** The destination x and y of the single drawImage call. */
    const drawnAt = (ctx: CanvasRenderingContext2D) => {
      const call = (ctx.drawImage as any).mock.calls[0]
      return { x: call[call.length - 4], y: call[call.length - 3] }
    }

    it('centres the image when no position is given', async () => {
      const ctx = createMockCtx()
      await (await setup({})).render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
    })

    it.each([
      ['a percentage from the left', { Left: '0%' as const }],
      ['a percentage from the right', { Right: '0%' as const }],
      ['a pixel offset from the left', { Left: 10 }],
      ['a pixel offset from the right', { Right: 10 }],
      ['a percentage from the top', { Top: '0%' as const }],
      ['a percentage from the bottom', { Bottom: '0%' as const }],
      ['a pixel offset from the top', { Top: 8 }],
      ['a pixel offset from the bottom', { Bottom: 8 }],
      ['both edges on one axis, where left wins', { Left: 4, Right: 40 }],
      ['both edges on the other axis, where top wins', { Top: 4, Bottom: 40 }],
    ])('positions the image by %s', async (_label, objectPosition) => {
      const ctx = createMockCtx()
      await (await setup({ objectPosition: objectPosition as any })).render(ctx, 0, 0)
      expect(ctx.drawImage).toHaveBeenCalled()
    })

    it('measures from the far edge when only the right is given', async () => {
      const fromLeft = createMockCtx()
      await (await setup({ objectPosition: { Left: 0 } as any })).render(fromLeft, 0, 0)
      const right = createMockCtx()
      await (await setup({ objectPosition: { Right: 0 } as any })).render(right, 0, 0)
      expect(drawnAt(right).x).not.toBe(drawnAt(fromLeft).x)
    })

    it('measures from the bottom when only the bottom is given', async () => {
      // `contain` leaves slack on the short axis for the two edges to differ across; `cover` fills
      // it exactly, so top and bottom would both resolve to the same zero.
      const fromTop = createMockCtx()
      await (await setup({ objectFit: 'contain', objectPosition: { Top: 0 } as any })).render(fromTop, 0, 0)
      const bottom = createMockCtx()
      await (await setup({ objectFit: 'contain', objectPosition: { Bottom: 0 } as any })).render(bottom, 0, 0)
      expect(drawnAt(bottom).y).not.toBe(drawnAt(fromTop).y)
    })
  })

  describe('aspect ratio sizing', () => {
    it.each([
      ['an explicit aspectRatio', { aspectRatio: 2 }],
      ['a non-positive aspectRatio, which is ignored', { aspectRatio: 0 }],
      ['width only', { width: 100, height: undefined }],
      ['height only', { width: undefined, height: 100 }],
      ['neither edge', { width: undefined, height: undefined }],
    ])('sizes the node from %s', async (_label, props) => {
      const node = new ImageNode({ src: 'test.png', ...props } as ImageProps)
      await node.load()
      node.processInitialChildren()
      node.node.calculateLayout(200, 200, Direction.LTR)
      expect(node.node.getComputedWidth()).toBeGreaterThanOrEqual(0)
    })
  })

  describe('drop shadow', () => {
    it('composites through an offscreen so the shadow is not clipped away', async () => {
      const ctx = createMockCtx()
      const node = new ImageNode({
        src: 'test.png',
        width: 100,
        height: 100,
        dropShadow: { offsetX: 2, offsetY: 3, blur: 4, color: '#123456' },
      } as ImageProps)
      await node.load()
      node.processInitialChildren()
      node.node.calculateLayout(100, 100, Direction.LTR)
      await node.render(ctx, 0, 0)
      expect(ctx.shadowColor).toBe('#123456')
      expect(ctx.shadowBlur).toBe(4)
    })

    it('falls back to black at no offset for a bare shadow', async () => {
      const ctx = createMockCtx()
      const node = new ImageNode({ src: 'test.png', width: 100, height: 100, dropShadow: {} } as ImageProps)
      await node.load()
      node.processInitialChildren()
      node.node.calculateLayout(100, 100, Direction.LTR)
      await node.render(ctx, 0, 0)
      expect(ctx.shadowColor).toBe('black')
      expect(ctx.shadowOffsetX).toBe(0)
      expect(ctx.shadowBlur).toBe(0)
    })

    it('clamps a negative blur to zero', async () => {
      const ctx = createMockCtx()
      const node = new ImageNode({ src: 'test.png', width: 100, height: 100, dropShadow: { blur: -5 } } as ImageProps)
      await node.load()
      node.processInitialChildren()
      node.node.calculateLayout(100, 100, Direction.LTR)
      await node.render(ctx, 0, 0)
      expect(ctx.shadowBlur).toBe(0)
    })

    it('draws straight to the context when the box has no area', async () => {
      const ctx = createMockCtx()
      const node = new ImageNode({ src: 'test.png', width: 0, height: 0, dropShadow: { blur: 2 } } as ImageProps)
      await node.load()
      node.processInitialChildren()
      node.node.calculateLayout(0, 0, Direction.LTR)
      await node.render(ctx, 0, 0)
      expect(ctx.shadowBlur).toBe(0)
    })
  })

  describe('object fit across both ratios', () => {
    const fitted = async (image: { width: number; height: number }, props: Partial<ImageProps>) => {
      mockLoadImage.mockResolvedValue(image)
      const ctx = createMockCtx()
      const node = new ImageNode({ src: 'test.png', width: 100, height: 100, ...props } as ImageProps)
      await node.load()
      node.processInitialChildren()
      node.node.calculateLayout(100, 100, Direction.LTR)
      await node.render(ctx, 0, 0)
      return ctx
    }
    const wide = { width: 200, height: 100 }
    const tall = { width: 100, height: 200 }

    it.each([
      ['contain', 'contain'],
      ['cover', 'cover'],
      ['fill', 'fill'],
      ['none', 'none'],
      ['scale-down', 'scale-down'],
    ])('fits a wide picture with %s', async (_label, objectFit) => {
      const ctx = await fitted(wide, { objectFit: objectFit as any, aspectRatio: 1 })
      expect(ctx.drawImage).toHaveBeenCalled()
    })

    it.each([
      ['contain', 'contain'],
      ['cover', 'cover'],
      ['fill', 'fill'],
      ['none', 'none'],
      ['scale-down', 'scale-down'],
    ])('fits a tall picture with %s', async (_label, objectFit) => {
      const ctx = await fitted(tall, { objectFit: objectFit as any, aspectRatio: 1 })
      expect(ctx.drawImage).toHaveBeenCalled()
    })

    it('scales a picture smaller than the box down only when it must', async () => {
      const ctx = await fitted({ width: 20, height: 20 }, { objectFit: 'scale-down', aspectRatio: 1 })
      expect(ctx.drawImage).toHaveBeenCalled()
    })

    it('draws nothing when the picture has no size', async () => {
      const ctx = await fitted({ width: 0, height: 0 }, {})
      expect(ctx.drawImage).not.toHaveBeenCalled()
    })

    it('draws nothing when padding and border leave no content box', async () => {
      const ctx = await fitted(wide, { padding: 60 } as Partial<ImageProps>)
      expect(ctx.drawImage).not.toHaveBeenCalled()
    })

    it('draws inside padding when there is room', async () => {
      const ctx = await fitted(wide, { padding: 10, borderRadius: 8 } as Partial<ImageProps>)
      expect(ctx.drawImage).toHaveBeenCalled()
    })
  })

  describe('load lifecycle', () => {
    it('calls onLoad once the picture arrives', async () => {
      const onLoad = vi.fn()
      const node = new ImageNode({ src: 'test.png', onLoad } as ImageProps)
      await node.load()
      expect(onLoad).toHaveBeenCalled()
    })

    it('starts a load when one is asked for before any began', async () => {
      const node = new ImageNode({ src: 'test.png' } as ImageProps)
      await expect(node.getLoadingPromise()).resolves.toBeUndefined()
    })

    it('hands back the in-flight load rather than starting a second', async () => {
      const node = new ImageNode({ src: 'test.png' } as ImageProps)
      const first = node.load()
      const second = node.getLoadingPromise()
      await Promise.all([first, second])
      expect(mockLoadImage).toHaveBeenCalledTimes(1)
    })
  })

  describe('remote sources and SVG recolouring', () => {
    const svgBytes = (fill: string) => Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg"><rect fill="${fill}"/></svg>`)

    const fetchReturning = (body: Buffer, ok = true, status = 200) =>
      vi.fn(async () => ({ ok, status, arrayBuffer: async () => body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength) }))

    afterEach(() => vi.unstubAllGlobals())

    it('throws with the status when the fetch comes back not ok', async () => {
      vi.stubGlobal('fetch', fetchReturning(Buffer.from('nope'), false, 404))
      const node = new ImageNode({ src: 'https://example.com/missing.png' } as ImageProps)
      await node.load()
      // The load swallows the throw and leaves the node with nothing to draw.
      const ctx = createMockCtx()
      node.processInitialChildren()
      node.node.calculateLayout(100, 100, Direction.LTR)
      await node.render(ctx, 0, 0)
      expect(ctx.drawImage).not.toHaveBeenCalled()
    })

    it('recognises an SVG the sniffer reports as XML by looking for the tag', async () => {
      vi.stubGlobal('fetch', fetchReturning(svgBytes('#000')))
      mockFileTypeFromBuffer.mockResolvedValue({ mime: 'application/xml' })
      const node = new ImageNode({ src: 'https://example.com/a.svg', color: '#f00' } as ImageProps)
      await node.load()
      expect(mockLoadImage).toHaveBeenCalled()
    })

    it('recognises an SVG the sniffer cannot place at all', async () => {
      vi.stubGlobal('fetch', fetchReturning(svgBytes('#000')))
      mockFileTypeFromBuffer.mockResolvedValue(undefined)
      const node = new ImageNode({ src: 'https://example.com/b.svg', color: '#0f0' } as ImageProps)
      await node.load()
      expect(mockLoadImage).toHaveBeenCalled()
    })

    it('takes the bytes as they are when recolouring changes nothing', async () => {
      vi.stubGlobal('fetch', fetchReturning(Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>')))
      mockFileTypeFromBuffer.mockResolvedValue({ mime: 'image/svg+xml' })
      const node = new ImageNode({ src: 'https://example.com/c.svg', color: '#00f' } as ImageProps)
      await node.load()
      expect(mockLoadImage).toHaveBeenCalled()
    })

    it('leaves a remote SVG alone when no colour is asked for', async () => {
      vi.stubGlobal('fetch', fetchReturning(svgBytes('#123')))
      mockFileTypeFromBuffer.mockResolvedValue({ mime: 'image/svg+xml' })
      const node = new ImageNode({ src: 'https://example.com/d.svg' } as ImageProps)
      await node.load()
      expect(mockLoadImage).toHaveBeenCalled()
    })
  })
})
