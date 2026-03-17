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

// Mock worker_threads so worker mode tests don't spin up real OS threads.
// Each MockWorker captures its 'message' handler and responds via postMessage.
let _mockWorkerInstances: MockWorker[] = []
class MockWorker {
  private handlers: Map<string, ((data: any) => void)[]> = new Map()
  constructor(_file: string) {
    _mockWorkerInstances.push(this)
  }
  on(event: string, handler: (data: any) => void) {
    if (!this.handlers.has(event)) this.handlers.set(event, [])
    this.handlers.get(event)!.push(handler)
    return this
  }
  postMessage(data: { id: number; props: any }) {
    setImmediate(() => {
      this.handlers.get('message')?.forEach(h => h({ id: data.id, buffer: Buffer.from('mock-render') }))
    })
  }
  terminate() {}
}
jest.unstable_mockModule('node:worker_threads', () => ({ Worker: MockWorker }))

let Canvas: typeof import('skia-canvas').Canvas
let FontLibrary: typeof import('skia-canvas').FontLibrary
let Root: typeof import('@/canvas/root.canvas.util.js').Root
let RootNode: typeof import('@/canvas/root.canvas.util.js').RootNode
let configure: typeof import('@/canvas/root.canvas.util.js').configure
let ColumnNode: typeof import('@/canvas/layout.canvas.util.js').ColumnNode

describe('RootNode', () => {
  beforeEach(async () => {
    jest.resetModules()

    // Re-import modules after resetting
    const skiaCanvasModule = await import('skia-canvas')
    Canvas = skiaCanvasModule.Canvas
    FontLibrary = skiaCanvasModule.FontLibrary
    const rootModule = await import('@/canvas/root.canvas.util.js')
    Root = rootModule.Root
    RootNode = rootModule.RootNode
    configure = rootModule.configure
    configure({ workerMode: false })
    const layoutModule = await import('@/canvas/layout.canvas.util.js')
    ColumnNode = layoutModule.ColumnNode

    fsMocks.reset()
    pathMocks.reset()
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
    await expect(Root({} as any)).rejects.toThrow('Width and height are required for Root')
  })

  it('should create canvas with scaled dimensions', async () => {
    await Root({ width: 400, scale: 2 })

    expect(Canvas).toHaveBeenCalledWith(800, 1)
  })

  it('should create canvas with correct dimension', async () => {
    await Root({ width: 400, height: 600 })

    expect(Canvas).toHaveBeenCalledWith(400, 600)
  })

  it('should call findAllImageNodes and await loading promises', async () => {
    const mockImageNodeInstance = new imageMocks.ImageNode({})
    mockImageNodeInstance.load = jest.fn(() => Promise.resolve())

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

    await Root({ width: 100, scale: 2 })

    expect(skiaCanvasMocks.mockCanvasContext.scale).toHaveBeenCalledWith(2, 2)
  })

  it('should call super.render (ColumnNode.render) during rendering', async () => {
    const columnNodeRenderSpy = jest.spyOn(ColumnNode.prototype, 'render')
    columnNodeRenderSpy.mockImplementation(() => {
      // Swallow actual rendering during this test
    })

    layoutMocks.yogaNode.getComputedHeight.mockReturnValue(100)
    layoutMocks.yogaNode.getComputedLayout.mockReturnValue({
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
    await Root({ width: 100 })

    expect(Canvas).toHaveBeenCalledWith(100, 1) // Expect height to be 1
  })

  it('should recalculate layout if finalizeLayout returns true', async () => {
    const rootNodeInstance = new RootNode({ width: 100 })

    // Mock the internal finalizeLayout method to return true
    const finalizeLayoutSpy = jest.spyOn(rootNodeInstance, 'finalizeLayout')
    finalizeLayoutSpy.mockReturnValue(true)

    // Mock the Yoga node's getComputedHeight to return a non-zero value for canvas creation
    layoutMocks.yogaNode.getComputedHeight.mockReturnValue(100)
    layoutMocks.yogaNode.getComputedLayout.mockReturnValue({
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

describe('Root (worker mode)', () => {
  let Root: typeof import('@/canvas/root.canvas.util.js').Root
  let configure: typeof import('@/canvas/root.canvas.util.js').configure

  beforeEach(async () => {
    jest.resetModules()
    _mockWorkerInstances = []

    skiaCanvasMocks.reset()
    fsMocks.reset()
    pathMocks.reset()
    layoutMocks.reset()
    imageMocks.reset()

    layoutMocks.yogaNode.getComputedHeight.mockReturnValue(100)
    layoutMocks.yogaNode.getComputedLayout.mockReturnValue({ left: 0, top: 0, width: 100, height: 100 })
    layoutMocks.yogaNode.isDirty.mockReturnValue(false)

    const rootModule = await import('@/canvas/root.canvas.util.js')
    Root = rootModule.Root
    configure = rootModule.configure
    // worker mode is on by default — no configure() call needed
  })

  it('should create worker pool and dispatch render to worker', async () => {
    const result = await Root({ width: 100 })
    expect(_mockWorkerInstances.length).toBeGreaterThan(0)
    expect(result).toBeDefined()
  })

  it('should return an object with toBufferSync that returns the worker buffer', async () => {
    const result = await Root({ width: 100 })
    const buf = result.toBufferSync('png')
    expect(Buffer.isBuffer(buf)).toBe(true)
    expect(buf).toEqual(Buffer.from('mock-render'))
  })

  it('should pass props to the worker via postMessage', async () => {
    const postMessageSpy = jest.spyOn(MockWorker.prototype, 'postMessage')
    await Root({ width: 200, height: 300 })
    expect(postMessageSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        props: expect.objectContaining({ width: 200, height: 300 }),
      }),
    )
    postMessageSpy.mockRestore()
  })

  it('should reject if worker responds with an error', async () => {
    jest.spyOn(MockWorker.prototype, 'postMessage').mockImplementationOnce(function (this: MockWorker, data) {
      setImmediate(() => {
        ;(this as any).handlers?.get('message')?.forEach((h: any) => h({ id: data.id, error: 'render failed' }))
      })
    })
    await expect(Root({ width: 100 })).rejects.toThrow('render failed')
  })

  it('should allow disabling worker mode via configure()', async () => {
    configure({ workerMode: false })
    _mockWorkerInstances = []
    await Root({ width: 100 })
    expect(_mockWorkerInstances.length).toBe(0)
  })
})
