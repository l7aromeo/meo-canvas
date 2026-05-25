import { vi } from 'vitest'
import type { RootPropsWithoutWorker } from '@/canvas/canvas.type.js'

const pathMocks = vi.hoisted(() => ({
  resolve: vi.fn((p: string) => p),
  join: vi.fn((...args: string[]) => args.join('/')),
  dirname: vi.fn((p: string) => {
    const parts = p.split('/')
    parts.pop()
    return parts.join('/') || '.'
  }),
}))

vi.mock('node:path', () => pathMocks)
vi.mock('skia-canvas', async () => (await import('@/__mocks__/skia-canvas.js')).__mocks__)
vi.mock('node:fs', async () => (await import('@/__mocks__/node-fs.js')).__mocks__)
vi.mock('@/canvas/layout.canvas.js', async () => (await import('@/__mocks__/layout.canvas.js')).__mocks__)
vi.mock('@/canvas/image.canvas.js', async () => (await import('@/__mocks__/image.canvas.js')).__mocks__)

// Mock worker_threads to prevent real OS threads during unit tests.
vi.mock('node:worker_threads', () => ({
  Worker: class {
    on() {
      return this
    }
    postMessage() {}
    terminate() {}
  },
}))

let Canvas: typeof import('skia-canvas').Canvas
let FontLibrary: typeof import('skia-canvas').FontLibrary
let Root: typeof import('@/canvas/root.canvas.js').Root
let RootNode: typeof import('@/canvas/root.canvas.js').RootNode
let ColumnNode: typeof import('@/canvas/layout.canvas.js').ColumnNode
let skiaCanvasMocks: typeof import('@/__mocks__/skia-canvas.js').__mocks__
let fsMocks: typeof import('@/__mocks__/node-fs.js').__mocks__
let layoutMocks: typeof import('@/__mocks__/layout.canvas.js').__mocks__
let imageMocks: typeof import('@/__mocks__/image.canvas.js').__mocks__

describe('RootNode', () => {
  beforeEach(async () => {
    vi.resetModules()

    skiaCanvasMocks = (await import('@/__mocks__/skia-canvas.js')).__mocks__
    fsMocks = (await import('@/__mocks__/node-fs.js')).__mocks__
    layoutMocks = (await import('@/__mocks__/layout.canvas.js')).__mocks__
    imageMocks = (await import('@/__mocks__/image.canvas.js')).__mocks__

    pathMocks.resolve.mockReset()
    pathMocks.resolve.mockImplementation((p: string) => p)
    pathMocks.join.mockReset()
    pathMocks.join.mockImplementation((...args: string[]) => args.join('/'))
    pathMocks.dirname.mockReset()
    pathMocks.dirname.mockImplementation((p: string) => {
      const parts = p.split('/')
      parts.pop()
      return parts.join('/') || '.'
    })

    // Re-import modules after resetting
    const skiaCanvasModule = await import('skia-canvas')
    Canvas = skiaCanvasModule.Canvas
    FontLibrary = skiaCanvasModule.FontLibrary
    const rootModule = await import('@/canvas/root.canvas.js')
    Root = rootModule.Root
    RootNode = rootModule.RootNode
    const layoutModule = await import('@/canvas/layout.canvas.js')
    ColumnNode = layoutModule.ColumnNode

    fsMocks.reset()
    skiaCanvasMocks.reset()
    imageMocks.reset()
    layoutMocks.reset()

    layoutMocks.yogaNode.getComputedHeight.mockReturnValue(0)
    layoutMocks.yogaNode.calculateLayout.mockClear()
    layoutMocks.yogaNode.getComputedLayout.mockReturnValue({
      left: 0,
      top: 0,
      width: 0,
      height: 0,
    })
    layoutMocks.yogaNode.insertChild.mockClear()
    layoutMocks.yogaNode.setAspectRatio.mockClear()
    layoutMocks.yogaNode.setWidth.mockClear()
    layoutMocks.yogaNode.isDirty.mockReturnValue(false)
    layoutMocks.yogaNode.markDirty.mockClear()
    layoutMocks.yogaNode.getComputedBorder.mockClear()
    layoutMocks.yogaNode.getComputedPadding.mockClear()
  })

  it('should throw an error if width is not provided', async () => {
    await expect(Root({ workerMode: false } as RootPropsWithoutWorker)).rejects.toThrow('Width and height are required for Root')
  })

  it('should create canvas with scaled dimensions', async () => {
    await Root({ workerMode: false, width: 400, scale: 2 })

    expect(Canvas).toHaveBeenCalledWith(800, 1)
  })

  it('should create canvas with correct dimension', async () => {
    await Root({ workerMode: false, width: 400, height: 600 })

    expect(Canvas).toHaveBeenCalledWith(400, 600)
  })

  it('should call findAllImageNodes and await loading promises', async () => {
    const { ImageNode } = await import('@/canvas/image.canvas.js')
    const mockImageNodeInstance = new ImageNode({ src: 'test.png' })
    mockImageNodeInstance.load = vi.fn(() => Promise.resolve())

    // Create RootNode instance and add the mock ImageNode as a child
    const rootNodeInstance = new RootNode({ width: 100 })
    rootNodeInstance.children.push(mockImageNodeInstance)

    const columnNodeRenderSpy = vi.spyOn(ColumnNode.prototype, 'render')
    columnNodeRenderSpy.mockImplementation(async () => {
      // Swallow actual rendering during this test
    })

    // Mock the internal finalizeLayout method
    const finalizeLayoutSpy = vi.spyOn(rootNodeInstance, 'finalizeLayout')
    finalizeLayoutSpy.mockReturnValue(false) // No re-layout for this test

    // Mock the Yoga node's getComputedHeight to return a non-zero value for canvas creation
    layoutMocks.yogaNode.getComputedHeight.mockReturnValue(100)
    layoutMocks.yogaNode.getComputedLayout.mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    })

    await rootNodeInstance.render()

    expect(mockImageNodeInstance.load).toHaveBeenCalled()
    columnNodeRenderSpy.mockRestore()
    finalizeLayoutSpy.mockRestore()
  })

  it('should call ctx.scale with the correct scale factor', async () => {
    layoutMocks.yogaNode.getComputedHeight.mockReturnValue(100)
    layoutMocks.yogaNode.getComputedLayout.mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    })

    await Root({ workerMode: false, width: 100, scale: 2 })

    const canvasInstance = vi.mocked(Canvas).mock.results.at(-1)?.value
    const ctx = canvasInstance?.getContext.mock.results.at(-1)?.value
    expect(ctx?.scale).toHaveBeenCalledWith(2, 2)
  })

  it('should call super.render (ColumnNode.render) during rendering', async () => {
    const columnNodeRenderSpy = vi.spyOn(ColumnNode.prototype, 'render')
    columnNodeRenderSpy.mockImplementation(async () => {
      // Swallow actual rendering during this test
    })

    layoutMocks.yogaNode.getComputedHeight.mockReturnValue(100)
    layoutMocks.yogaNode.getComputedLayout.mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    })

    await Root({ workerMode: false, width: 100 })

    expect(columnNodeRenderSpy).toHaveBeenCalled()
    columnNodeRenderSpy.mockRestore()
  })

  it('should ensure canvas height is at least 1 even if content height is 0', async () => {
    await Root({ workerMode: false, width: 100 })

    expect(Canvas).toHaveBeenCalledWith(100, 1) // Expect height to be 1
  })

  it('should recalculate layout if finalizeLayout returns true', async () => {
    const rootNodeInstance = new RootNode({ width: 100 })

    // Mock the internal finalizeLayout method to return true
    const finalizeLayoutSpy = vi.spyOn(rootNodeInstance, 'finalizeLayout')
    finalizeLayoutSpy.mockReturnValue(true)

    // Mock the Yoga node's getComputedHeight to return a non-zero value for canvas creation
    layoutMocks.yogaNode.getComputedHeight.mockReturnValue(100)
    layoutMocks.yogaNode.getComputedLayout.mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    })

    const calculateLayoutSpy = vi.spyOn(rootNodeInstance.node, 'calculateLayout')

    await rootNodeInstance.render()

    // Expect calculateLayout to be called twice: once in constructor, once after finalizeLayout
    expect(calculateLayoutSpy).toHaveBeenCalledTimes(2)
    finalizeLayoutSpy.mockRestore()
  })

  it('should not re-register already registered fonts', async () => {
    fsMocks.existsSync.mockReturnValue(true)
    pathMocks.resolve.mockImplementation(p => `/mock/path/to/${p}`)

    await Root({ workerMode: false, width: 100, fonts: [{ family: 'TestFont', paths: ['font.ttf'] }] })
    await Root({ workerMode: false, width: 100, fonts: [{ family: 'TestFont', paths: ['font.ttf'] }] })

    expect(FontLibrary.use).toHaveBeenCalledTimes(1)
    expect(FontLibrary.use).toHaveBeenCalledWith({
      TestFont: ['/mock/path/to/font.ttf'],
    })
  })

  it('should add new font paths to an existing family', async () => {
    fsMocks.existsSync.mockReturnValue(true)
    pathMocks.resolve.mockImplementation(p => `/mock/path/to/${p}`)
    vi.mocked(FontLibrary.use).mockClear()

    // Register first font
    await Root({ workerMode: false, width: 100, fonts: [{ family: 'ExistingFont', paths: ['font1.ttf'] }] })

    // Register second font for the same family
    await Root({ workerMode: false, width: 100, fonts: [{ family: 'ExistingFont', paths: ['font2.ttf'] }] })

    expect(FontLibrary.use).toHaveBeenCalledTimes(2)
    expect(FontLibrary.use).toHaveBeenCalledWith({
      ExistingFont: ['/mock/path/to/font1.ttf'],
    })
    expect(FontLibrary.use).toHaveBeenCalledWith({
      ExistingFont: ['/mock/path/to/font2.ttf'],
    })
  })
})
