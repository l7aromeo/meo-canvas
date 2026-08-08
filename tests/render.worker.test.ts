import { vi } from 'vitest'
import type { Canvas } from 'phyron-skia-canvas'
import { createCanvasHandlers } from '@/worker/canvas-handlers.js'

function asCanvas(mock: object): Canvas {
  return mock as unknown as Canvas
}

describe('createCanvasHandlers', () => {
  const engine = { renderer: 'CPU', api: 'Vulkan', device: 'mock', threads: 1 } as const

  /**
   * Rendering must not encode anything. It used to eagerly produce a PNG that most callers threw
   * away, and that buffer was then returned from every sync method regardless of the format asked
   * for — the reason `toBufferSync('svg')` handed back PNG bytes.
   */
  it('stores the canvas on render and encodes nothing', async () => {
    const canvases = new Map<number, Canvas>()
    let nextId = 0
    const mockCanvas = {
      toBufferSync: vi.fn(() => Buffer.from('png-bytes')),
      width: 400,
      height: 300,
      gpu: true,
      engine,
    }

    const handlers = createCanvasHandlers({
      canvases,
      getNextCanvasId: () => nextId++,
      renderRoot: vi.fn(async () => mockCanvas as unknown as Canvas),
    })

    const result = await handlers.render({ width: 400, height: 300 } as any)

    expect(result).toEqual({ canvasId: 0, width: 400, height: 300, gpu: true, engine })
    expect(canvases.get(0)).toBe(mockCanvas)
    expect(mockCanvas.toBufferSync).not.toHaveBeenCalled()
  })

  it('callSync runs the real method with the format it was given', () => {
    const mockCanvas = {
      toBufferSync: vi.fn((format: string) => Buffer.from(`${format}-bytes`)),
      width: 100,
      height: 100,
    }
    const handlers = createCanvasHandlers({
      canvases: new Map<number, Canvas>([[0, asCanvas(mockCanvas)]]),
      getNextCanvasId: () => 1,
      renderRoot: vi.fn(),
    })

    expect(handlers.callSync(0, 'toBufferSync', ['svg'])).toEqual(Buffer.from('svg-bytes'))
    expect(mockCanvas.toBufferSync).toHaveBeenCalledWith('svg')
  })

  /** `method` arrives as a string over a port, so the dispatch must not be an open lookup. */
  it('callSync refuses methods outside the allowlist', () => {
    const mockCanvas = { constructor: vi.fn(), toBufferSync: vi.fn(), width: 1, height: 1 }
    const handlers = createCanvasHandlers({
      canvases: new Map<number, Canvas>([[0, asCanvas(mockCanvas)]]),
      getNextCanvasId: () => 1,
      renderRoot: vi.fn(),
    })

    expect(() => handlers.callSync(0, 'constructor', [])).toThrow('not callable synchronously')
    expect(() => handlers.callSync(0, 'toBuffer', ['png'])).toThrow('not callable synchronously')
  })

  it('callSync throws for a missing canvas', () => {
    const handlers = createCanvasHandlers({
      canvases: new Map(),
      getNextCanvasId: () => 0,
      renderRoot: vi.fn(),
    })

    expect(() => handlers.callSync(99, 'toBufferSync', ['png'])).toThrow('Canvas 99 not found')
  })

  it('throws when callOnCanvas targets missing canvas', async () => {
    const handlers = createCanvasHandlers({
      canvases: new Map(),
      getNextCanvasId: () => 0,
      renderRoot: vi.fn(),
    })

    await expect(handlers.callOnCanvas(99, 'toBuffer', ['png'])).rejects.toThrow('Canvas 99 not found')
  })

  it('releaseCanvas removes canvas from map', () => {
    const canvases = new Map<number, Canvas>([[1, asCanvas({})]])
    const handlers = createCanvasHandlers({
      canvases,
      getNextCanvasId: () => 0,
      renderRoot: vi.fn(),
    })

    handlers.releaseCanvas(1)
    expect(canvases.has(1)).toBe(false)
  })

  it('delegates toBuffer to canvas', async () => {
    const mockCanvas = {
      toBuffer: vi.fn(async () => Buffer.from('jpg')),
      toBufferSync: vi.fn(),
      width: 100,
      height: 100,
    }
    const canvases = new Map<number, Canvas>([[0, asCanvas(mockCanvas)]])
    const handlers = createCanvasHandlers({
      canvases,
      getNextCanvasId: () => 1,
      renderRoot: vi.fn(),
    })

    const buf = await handlers.callOnCanvas(0, 'toBuffer', ['jpg', { quality: 0.9 }])
    expect(buf).toEqual(Buffer.from('jpg'))
    expect(mockCanvas.toBuffer).toHaveBeenCalledWith('jpg', { quality: 0.9 })
  })

  it('delegates toURL to canvas', async () => {
    const mockCanvas = {
      toURL: vi.fn(async () => 'data:image/png;base64,abc'),
      toBufferSync: vi.fn(),
      width: 100,
      height: 100,
    }
    const canvases = new Map<number, Canvas>([[0, asCanvas(mockCanvas)]])
    const handlers = createCanvasHandlers({
      canvases,
      getNextCanvasId: () => 1,
      renderRoot: vi.fn(),
    })

    const url = await handlers.callOnCanvas(0, 'toURL', ['png'])
    expect(url).toBe('data:image/png;base64,abc')
    expect(mockCanvas.toURL).toHaveBeenCalledWith('png')
  })

  it('delegates toFile to canvas', async () => {
    const mockCanvas = {
      toFile: vi.fn(async () => undefined),
      toBufferSync: vi.fn(),
      width: 100,
      height: 100,
    }
    const canvases = new Map<number, Canvas>([[0, asCanvas(mockCanvas)]])
    const handlers = createCanvasHandlers({
      canvases,
      getNextCanvasId: () => 1,
      renderRoot: vi.fn(),
    })

    await handlers.callOnCanvas(0, 'toFile', ['output.png', { quality: 1 }])
    expect(mockCanvas.toFile).toHaveBeenCalledWith('output.png', { quality: 1 })
  })

  it('delegates saveAs to canvas', async () => {
    const mockCanvas = {
      saveAs: vi.fn(async () => undefined),
      toBufferSync: vi.fn(),
      width: 100,
      height: 100,
    }
    const canvases = new Map<number, Canvas>([[0, asCanvas(mockCanvas)]])
    const handlers = createCanvasHandlers({
      canvases,
      getNextCanvasId: () => 1,
      renderRoot: vi.fn(),
    })

    await handlers.callOnCanvas(0, 'saveAs', ['out.png', { quality: 1 }])
    expect(mockCanvas.saveAs).toHaveBeenCalledWith('out.png', { quality: 1 })
  })

  /**
   * `toSharp` returns a Sharp instance, which cannot cross a thread boundary. It is built on the
   * calling thread from raw pixels instead, so the worker must not offer it.
   */
  it('refuses toSharp over the async channel', async () => {
    const mockCanvas = { toSharp: vi.fn(), toBufferSync: vi.fn(), width: 1, height: 1 }
    const canvases = new Map<number, Canvas>([[0, asCanvas(mockCanvas)]])
    const handlers = createCanvasHandlers({
      canvases,
      getNextCanvasId: () => 1,
      renderRoot: vi.fn(),
    })

    await expect(handlers.callOnCanvas(0, 'toSharp', [{}])).rejects.toThrow('Unknown method: toSharp')
    expect(mockCanvas.toSharp).not.toHaveBeenCalled()
  })

  it('throws for unknown callOnCanvas method', async () => {
    const mockCanvas = { toBufferSync: vi.fn(), width: 1, height: 1 }
    const canvases = new Map<number, Canvas>([[0, asCanvas(mockCanvas)]])
    const handlers = createCanvasHandlers({
      canvases,
      getNextCanvasId: () => 1,
      renderRoot: vi.fn(),
    })

    await expect(handlers.callOnCanvas(0, 'unknown' as any, [])).rejects.toThrow('Unknown method: unknown')
  })
})
