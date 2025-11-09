import { jest } from '@jest/globals'
import { __mocks__ as skiaCanvasMocks } from '@/__mocks__/skia-canvas.js'
import { __mocks__ as fsMocks } from '@/__mocks__/node-fs.js'
import { __mocks__ as pathMocks } from '@/__mocks__/node-path.js'
import { __mocks__ as layoutMocks } from '@/__mocks__/layout.canvas.util.js'
import { __mocks__ as imageMocks } from '@/__mocks__/image.canvas.util.js'

jest.unstable_mockModule('skia-canvas', () => skiaCanvasMocks)

jest.unstable_mockModule('node:fs', () => fsMocks)

jest.unstable_mockModule('node:path', () => pathMocks)

jest.unstable_mockModule('@/canvas/layout.canvas.util.js', () => layoutMocks)

jest.unstable_mockModule('@/canvas/image.canvas.util.js', () => imageMocks)

let Canvas: typeof import('skia-canvas').Canvas
let FontLibrary: typeof import('skia-canvas').FontLibrary
let Root: typeof import('@/canvas/root.canvas.util.js').Root
let RootNode: typeof import('@/canvas/root.canvas.util.js').RootNode
let ColumnNode: typeof import('@/canvas/layout.canvas.util.js').ColumnNode

describe('RootNode', () => {
  beforeEach(async () => {
    // Make beforeEach async
    jest.resetModules() // Reset module registry to ensure fresh mocks

    // Re-import modules after resetting
    const skiaCanvasModule = await import('skia-canvas')
    Canvas = skiaCanvasModule.Canvas
    FontLibrary = skiaCanvasModule.FontLibrary
    const rootModule = await import('@/canvas/root.canvas.util.js')
    Root = rootModule.Root
    RootNode = rootModule.RootNode
    const layoutModule = await import('@/canvas/layout.canvas.util.js')
    ColumnNode = layoutModule.ColumnNode

    fsMocks.reset()
    pathMocks.reset()
    skiaCanvasMocks.reset()

    layoutMocks.mockYogaNode.getComputedHeight.mockReturnValue(0)
    layoutMocks.mockYogaNode.calculateLayout.mockClear()
    layoutMocks.mockYogaNode.getComputedLayout.mockReturnValue({
      left: 0,
      top: 0,
      width: 0,
      height: 0,
    })
    layoutMocks.mockYogaNode.insertChild.mockClear()
    layoutMocks.mockYogaNode.setAspectRatio.mockClear()
    layoutMocks.mockYogaNode.setWidth.mockClear()
    layoutMocks.mockYogaNode.isDirty.mockReturnValue(false)
    layoutMocks.mockYogaNode.markDirty.mockClear()
    layoutMocks.mockYogaNode.getComputedBorder.mockClear()
    layoutMocks.mockYogaNode.getComputedPadding.mockClear()

    imageMocks.reset()
    layoutMocks.reset()
  })

  it('should throw an error if width is not provided', async () => {
    await expect(Root({} as any)).rejects.toThrow('Width and height are required for Root')
  })

  it('should create canvas with scaled dimensions', async () => {
    await Root({ width: 400, scale: 2 })

    expect(Canvas).toHaveBeenCalledWith(800, 1)
  })

  it('should create canvas with correct dimension', async () => {
    layoutMocks.mockYogaNode.getComputedHeight.mockReturnValue(600) // Mock computed height to match expected

    await Root({ width: 400, height: 600 })

    expect(Canvas).toHaveBeenCalledWith(400, 600)
  })

  it('should call findAllImageNodes and await loading promises', async () => {
    const mockImageNodeInstance = new imageMocks.ImageNode({})
    mockImageNodeInstance.getLoadingPromise = jest.fn(() => Promise.resolve())

    // Create RootNode instance and add the mock ImageNode as a child
    const rootNodeInstance = new RootNode({ width: 100 })
    rootNodeInstance.children.push(mockImageNodeInstance)

    const columnNodeRenderSpy = jest.spyOn(ColumnNode.prototype, 'render')
    columnNodeRenderSpy.mockImplementation(() => {
      // Swallow actual rendering during this test
    })

    // Mock the internal finalizeLayout method
    const finalizeLayoutSpy = jest.spyOn(rootNodeInstance, 'finalizeLayout')
    finalizeLayoutSpy.mockReturnValue(false) // No re-layout for this test

    // Mock the Yoga node's getComputedHeight to return a non-zero value for canvas creation
    layoutMocks.mockYogaNode.getComputedHeight.mockReturnValue(100)
    layoutMocks.mockYogaNode.getComputedLayout.mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    })

    await rootNodeInstance.render()

    expect(mockImageNodeInstance.getLoadingPromise).toHaveBeenCalled()
    columnNodeRenderSpy.mockRestore()
    finalizeLayoutSpy.mockRestore()
  })

  it('should call ctx.scale with the correct scale factor', async () => {
    const mockCtx = {
      scale: jest.fn(),
      getContext: jest.fn(() => mockCtx),
    }
    skiaCanvasMocks.Canvas.mockImplementation(() => ({
      getContext: mockCtx.getContext,
    }))

    layoutMocks.mockYogaNode.getComputedHeight.mockReturnValue(100)
    layoutMocks.mockYogaNode.getComputedLayout.mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    })

    await Root({ width: 100, scale: 2 })

    expect(mockCtx.scale).toHaveBeenCalledWith(2, 2)
  })

  it('should call super.render (ColumnNode.render) during rendering', async () => {
    const columnNodeRenderSpy = jest.spyOn(ColumnNode.prototype, 'render')
    columnNodeRenderSpy.mockImplementation(() => {
      // Swallow actual rendering during this test
    }) // Prevent actual rendering during this test

    layoutMocks.mockYogaNode.getComputedHeight.mockReturnValue(100)
    layoutMocks.mockYogaNode.getComputedLayout.mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    })

    await Root({ width: 100 })

    expect(columnNodeRenderSpy).toHaveBeenCalled()
    columnNodeRenderSpy.mockRestore()
  })

  it('should ensure canvas height is at least 1 even if content height is 0', async () => {
    layoutMocks.mockYogaNode.getComputedHeight.mockReturnValue(0) // Simulate 0 content height

    await Root({ width: 100 })

    expect(Canvas).toHaveBeenCalledWith(100, 1) // Expect height to be 1
  })

  it('should recalculate layout if finalizeLayout returns true', async () => {
    const rootNodeInstance = new RootNode({ width: 100 })

    // Mock the internal finalizeLayout method to return true
    const finalizeLayoutSpy = jest.spyOn(rootNodeInstance, 'finalizeLayout')
    finalizeLayoutSpy.mockReturnValue(true)

    // Mock the Yoga node's getComputedHeight to return a non-zero value for canvas creation
    layoutMocks.mockYogaNode.getComputedHeight.mockReturnValue(100)
    layoutMocks.mockYogaNode.getComputedLayout.mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    })

    const calculateLayoutSpy = jest.spyOn(rootNodeInstance.node, 'calculateLayout')

    await rootNodeInstance.render()

    // Expect calculateLayout to be called twice: once in constructor, once after finalizeLayout
    expect(calculateLayoutSpy).toHaveBeenCalledTimes(2)
    finalizeLayoutSpy.mockRestore()
  })

  it('should not re-register already registered fonts', async () => {
    fsMocks.existsSync.mockReturnValue(true)
    pathMocks.resolve.mockImplementation(p => `/mock/path/to/${p}`)

    await Root({
      width: 100,
      fonts: [{ family: 'TestFont', paths: ['font.ttf'] }],
    })
    await Root({
      width: 100,
      fonts: [{ family: 'TestFont', paths: ['font.ttf'] }],
    })

    expect(FontLibrary.use).toHaveBeenCalledTimes(1)
    expect(FontLibrary.use).toHaveBeenCalledWith({
      TestFont: ['/mock/path/to/font.ttf'],
    })
  })

  it('should add new font paths to an existing family', async () => {
    fsMocks.existsSync.mockReturnValue(true)
    pathMocks.resolve.mockImplementation(p => `/mock/path/to/${p}`)

    // Register first font
    await Root({
      width: 100,
      fonts: [{ family: 'ExistingFont', paths: ['font1.ttf'] }],
    })

    // Register second font for the same family
    await Root({
      width: 100,
      fonts: [{ family: 'ExistingFont', paths: ['font2.ttf'] }],
    })

    expect(FontLibrary.use).toHaveBeenCalledTimes(2)
    expect(FontLibrary.use).toHaveBeenCalledWith({
      ExistingFont: ['/mock/path/to/font1.ttf'],
    })
    expect(FontLibrary.use).toHaveBeenCalledWith({
      ExistingFont: ['/mock/path/to/font2.ttf'],
    })
  })
})
