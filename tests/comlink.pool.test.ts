import { vi } from 'vitest'

const mockEngine = { renderer: 'CPU', api: 'Vulkan', device: 'mock', threads: 1 } as const
const mockRenderResult = { canvasId: 1, width: 100, height: 100, gpu: false, engine: mockEngine }
const mockEndpoint = {
  render: vi.fn<() => Promise<typeof mockRenderResult>>().mockResolvedValue(mockRenderResult),
  callOnCanvas: vi.fn<() => Promise<Buffer>>().mockResolvedValue(Buffer.from('result')),
  releaseCanvas: vi.fn(),
}

/** Spied so tests can assert *when* threads are created, not just that rendering works. */
const WorkerMock = vi.fn(function (this: Record<string, unknown>) {
  this.on = () => this
  this.terminate = () => {}
})

/** Ports handed to `SyncChannel`; the pool creates one channel per worker it spawns. */
const portMock = () => ({ postMessage: vi.fn(), close: vi.fn(), unref: vi.fn(), on: vi.fn() })
const MessageChannelMock = vi.fn(function (this: Record<string, unknown>) {
  this.port1 = portMock()
  this.port2 = portMock()
})

vi.mock('node:worker_threads', () => ({ Worker: WorkerMock, MessageChannel: MessageChannelMock }))

vi.mock('node:url', () => ({
  fileURLToPath: () => '/mock/worker/comlink.pool.ts',
}))

vi.mock('@/worker/comlink.setup.js', () => ({
  Comlink: {
    wrap: () => mockEndpoint,
    proxy: (fn: unknown) => fn,
    releaseProxy: Symbol('releaseProxy'),
  },
  nodeEndpoint: () => ({}),
}))

let ComlinkPool: typeof import('@/worker/comlink.pool.js').ComlinkPool

beforeEach(async () => {
  vi.clearAllMocks()
  mockEndpoint.render.mockResolvedValue(mockRenderResult)

  const mod = await import('@/worker/comlink.pool.js')
  ComlinkPool = mod.ComlinkPool
})

describe('ComlinkPool', () => {
  it('should not spawn any worker until the first render', async () => {
    const pool = new ComlinkPool(3)
    // Nothing has been rendered, so no thread should exist yet — the pool grows to fit demand
    // rather than to fit the core count.
    expect(WorkerMock).not.toHaveBeenCalled()

    await pool.render({ width: 200 } as any)
    expect(WorkerMock).toHaveBeenCalledTimes(1)

    // A second sequential render reuses the warm worker instead of spawning a sibling.
    await pool.render({ width: 200 } as any)
    expect(WorkerMock).toHaveBeenCalledTimes(1)
    pool.terminate()
  })

  it('should render via an idle worker and return PoolRenderResult', async () => {
    const pool = new ComlinkPool(1)
    const result = await pool.render({ width: 200 } as any)

    expect(result).toEqual({ ...mockRenderResult, workerIdx: 0 })
    expect(mockEndpoint.render).toHaveBeenCalledTimes(1)
    pool.terminate()
  })

  it('should delegate callOnCanvas to the correct worker endpoint', async () => {
    const pool = new ComlinkPool(2)
    // Workers spawn on demand, so one has to render before its index exists. That mirrors real
    // usage: a workerIdx is only ever obtained from the result of a previous render().
    await pool.render({ width: 200 } as any)
    await pool.callOnCanvas(0, 1, 'toBuffer', ['png'])

    expect(mockEndpoint.callOnCanvas).toHaveBeenCalledWith(1, 'toBuffer', ['png'])
    pool.terminate()
  })

  it('should reject callOnCanvas for a worker that does not exist', async () => {
    // Reachable in practice: a canvas collected by the FinalizationRegistry after the pool has
    // terminated still carries its old worker index.
    const pool = new ComlinkPool(2)
    await expect(pool.callOnCanvas(5, 1, 'toBuffer', ['png'])).rejects.toThrow('Worker 5 is not available')
    pool.terminate()
  })

  it('should ignore releaseCanvas for a worker that does not exist', () => {
    const pool = new ComlinkPool(2)
    expect(() => pool.releaseCanvas(5, 1)).not.toThrow()
    expect(mockEndpoint.releaseCanvas).not.toHaveBeenCalled()
    pool.terminate()
  })

  it('should refuse to render once terminated', async () => {
    const pool = new ComlinkPool(2)
    pool.terminate()
    await expect(pool.render({ width: 200 } as any)).rejects.toThrow('Pool has been terminated')
  })

  it('should delegate releaseCanvas to the correct worker endpoint', async () => {
    const pool = new ComlinkPool(2)
    await pool.render({ width: 200 } as any)
    pool.releaseCanvas(0, 5)

    expect(mockEndpoint.releaseCanvas).toHaveBeenCalledWith(5)
    pool.terminate()
  })

  it('should queue renders when all workers are busy', async () => {
    // Single worker pool — second render must queue
    let resolveFirst!: (v: typeof mockRenderResult) => void
    mockEndpoint.render
      .mockImplementationOnce(
        () =>
          new Promise(r => {
            resolveFirst = r
          }),
      )
      .mockResolvedValueOnce(mockRenderResult)

    const pool = new ComlinkPool(1)
    const first = pool.render({ width: 100 } as any)
    const second = pool.render({ width: 200 } as any)

    // First is in-flight, second is queued
    expect(mockEndpoint.render).toHaveBeenCalledTimes(1)

    // Complete the first — should drain queue and start second
    resolveFirst(mockRenderResult)
    const [r1, r2] = await Promise.all([first, second])

    expect(r1.workerIdx).toBe(0)
    expect(r2.workerIdx).toBe(0)
    expect(mockEndpoint.render).toHaveBeenCalledTimes(2)
    pool.terminate()
  })

  it('should reject queued task when worker render fails', async () => {
    let rejectFirst!: (err: Error) => void
    mockEndpoint.render
      .mockImplementationOnce(
        () =>
          new Promise((_, rej) => {
            rejectFirst = rej
          }),
      )
      .mockResolvedValueOnce(mockRenderResult)

    const pool = new ComlinkPool(1)
    const first = pool.render({ width: 100 } as any)
    const second = pool.render({ width: 200 } as any)

    // Fail the first — should still drain queue and process second
    rejectFirst(new Error('worker crashed'))
    await expect(first).rejects.toThrow('worker crashed')

    const r2 = await second
    expect(r2).toEqual({ ...mockRenderResult, workerIdx: 0 })
    pool.terminate()
  })

  it('should clear all state on terminate', async () => {
    const pool = new ComlinkPool(2)
    pool.terminate()

    // After terminate, render should fail because no endpoints exist
    await expect(pool.render({ width: 100 } as any)).rejects.toBeDefined()
  })

  it('should extract function props and pass callFn proxy to worker', async () => {
    const pool = new ComlinkPool(1)
    const fn = () => 'hello'
    await pool.render({ width: 100, options: { formatter: fn } } as any)

    expect(mockEndpoint.render).toHaveBeenCalledTimes(1)
    // First arg should have the function replaced with a sentinel
    const [props, callFn] = mockEndpoint.render.mock.calls[0] as unknown as [any, any]
    expect(props.options.formatter).toHaveProperty('__comlinkFnId', 0)
    expect(typeof callFn).toBe('function')
    pool.terminate()
  })

  it('should not pass callFn when props have no functions', async () => {
    const pool = new ComlinkPool(1)
    await pool.render({ width: 100 } as any)

    expect(mockEndpoint.render).toHaveBeenCalledTimes(1)
    const [, callFn] = mockEndpoint.render.mock.calls[0] as unknown as [any, any]
    expect(callFn).toBeUndefined()
    pool.terminate()
  })

  it('should give each spawned worker its own sync channel', async () => {
    const pool = new ComlinkPool(2)
    await pool.render({ width: 100 } as any)

    // One channel per worker — a shared one would let a second sync call clobber the control word
    // a first caller is still parked on.
    expect(MessageChannelMock).toHaveBeenCalledTimes(1)
    pool.terminate()
  })

  it('should route syncCall to the channel of the owning worker', async () => {
    const pool = new ComlinkPool(1)
    await pool.render({ width: 100 } as any)

    const channel = (pool as any).syncChannels[0]
    const spy = vi.spyOn(channel, 'call').mockReturnValue(Buffer.from('svg'))

    expect(pool.syncCall(0, 42, 'toBufferSync', ['svg'])).toEqual(Buffer.from('svg'))
    expect(spy).toHaveBeenCalledWith(42, 'toBufferSync', ['svg'])
    pool.terminate()
  })

  it('should reject syncCall for a worker that does not exist', () => {
    const pool = new ComlinkPool(1)
    expect(() => pool.syncCall(3, 1, 'toBufferSync', ['png'])).toThrow('Worker 3 is not available')
    pool.terminate()
  })

  it('should close sync channels on terminate', async () => {
    const pool = new ComlinkPool(1)
    await pool.render({ width: 100 } as any)

    const port = (pool as any).syncChannels[0].port
    pool.terminate()
    expect(port.close).toHaveBeenCalled()
  })
})
