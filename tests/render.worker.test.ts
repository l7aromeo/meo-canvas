import { vi } from 'vitest'
import type { Canvas } from 'phyron-skia-canvas'
import { createCanvasHandlers } from '@/worker/canvas-handlers.js'

function asCanvas(mock: object): Canvas {
  return mock as unknown as Canvas
}

describe('createCanvasHandlers', () => {
  it('stores canvas on render and returns png buffer metadata', async () => {
    const canvases = new Map<number, Canvas>()
    let nextId = 0
    const mockCanvas = {
      toBufferSync: vi.fn(() => Buffer.from('png-bytes')),
      width: 400,
      height: 300,
    }

    const handlers = createCanvasHandlers({
      canvases,
      getNextCanvasId: () => nextId++,
      renderRoot: vi.fn(async () => mockCanvas as unknown as Canvas),
    })

    const result = await handlers.render({ width: 400, height: 300 } as any)

    expect(result).toEqual({
      canvasId: 0,
      buffer: Buffer.from('png-bytes'),
      width: 400,
      height: 300,
    })
    expect(canvases.get(0)).toBe(mockCanvas)
    expect(mockCanvas.toBufferSync).toHaveBeenCalledWith('png')
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

  it('delegates toSharp and returns buffer', async () => {
    const mockCanvas = {
      toSharp: vi.fn(() => ({ toBuffer: vi.fn(async () => Buffer.from('sharp')) })),
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

    const buf = await handlers.callOnCanvas(0, 'toSharp', [{}])
    expect(buf).toEqual(Buffer.from('sharp'))
    expect(mockCanvas.toSharp).toHaveBeenCalledWith({})
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
