import { vi } from 'vitest'

const mockRenderResult = { canvasId: 1, buffer: Buffer.from('png'), width: 100, height: 100 }
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

vi.mock('node:worker_threads', () => ({ Worker: WorkerMock }))

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
})
