import { vi } from 'vitest'

const mockEngine = { renderer: 'CPU', api: 'Vulkan', device: 'mock', threads: 1 } as const
const mockRenderResult = { canvasId: 1, width: 100, height: 100, gpu: false, engine: mockEngine }
const mockEndpoint = {
  render: vi.fn<() => Promise<typeof mockRenderResult>>().mockResolvedValue(mockRenderResult),
  callOnCanvas: vi.fn<() => Promise<Buffer>>().mockResolvedValue(Buffer.from('result')),
  releaseCanvas: vi.fn(),
}

/**
 * Every worker the pool has spawned, in index order, with its listeners reachable.
 *
 * A thread dying is an event, so a test that cannot fire one cannot tell a pool that reports the
 * failure apart from a pool that hangs forever — both simply never resolve.
 */
interface MockWorker {
  listeners: Map<string, ((arg: never) => void)[]>
  emit: (event: string, arg?: unknown) => void
  terminate: () => void
}
const spawned: MockWorker[] = []

/** Spied so tests can assert *when* threads are created, not just that rendering works. */
const WorkerMock = vi.fn(function (this: Record<string, unknown>) {
  const listeners = new Map<string, ((arg: never) => void)[]>()
  this.listeners = listeners
  this.on = (event: string, listener: (arg: never) => void) => {
    listeners.set(event, [...(listeners.get(event) ?? []), listener])
    return this
  }
  this.emit = (event: string, arg?: unknown) => listeners.get(event)?.forEach(listener => listener(arg as never))
  this.terminate = () => {}
  spawned.push(this as unknown as MockWorker)
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
  spawned.length = 0
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

/**
 * A worker that goes away owes an answer it can no longer send. Comlink settles a call by receiving
 * a message, so without this the promise stays pending: the caller waits on a render that will
 * never arrive, with no error and no timeout to end it. Every case here would hang before failing,
 * which is why each one is written against a promise that must reject rather than a value.
 */
describe('ComlinkPool when a worker dies', () => {
  /** A render the worker will never answer — the shape of a thread that died mid-call. */
  const neverAnswers = () => mockEndpoint.render.mockReturnValue(new Promise(() => {}))

  it('rejects the in-flight render when the thread errors', async () => {
    neverAnswers()
    const pool = new ComlinkPool(1)

    const render = pool.render({ width: 100 } as any)
    spawned[0].emit('error', new Error("Cannot find package 'comlink'"))

    await expect(render).rejects.toThrow("Worker 0 died before its render finished: Cannot find package 'comlink'")
    pool.terminate()
  })

  it('keeps the original failure as the cause', async () => {
    neverAnswers()
    const pool = new ComlinkPool(1)
    const boom = new Error('native module failed to load')

    const render = pool.render({ width: 100 } as any)
    spawned[0].emit('error', boom)

    await expect(render).rejects.toThrow(expect.objectContaining({ cause: boom }))
    pool.terminate()
  })

  it('rejects the in-flight render when the thread exits non-zero', async () => {
    neverAnswers()
    const pool = new ComlinkPool(1)

    const render = pool.render({ width: 100 } as any)
    // How an out-of-memory kill arrives: no error object, just a thread that is gone.
    spawned[0].emit('exit', 1)

    await expect(render).rejects.toThrow('worker exited with code 1')
    pool.terminate()
  })

  it('ignores a clean exit, which is a worker finishing rather than failing', async () => {
    const pool = new ComlinkPool(1)
    await pool.render({ width: 100 } as any)

    spawned[0].emit('exit', 0)

    // Still usable: a zero exit says nothing failed, so nothing should be retired over it.
    await expect(pool.render({ width: 100 } as any)).resolves.toMatchObject({ workerIdx: 0 })
    pool.terminate()
  })

  it('does not hand a dead worker out again', async () => {
    neverAnswers()
    const pool = new ComlinkPool(2)

    const first = pool.render({ width: 100 } as any)
    spawned[0].emit('error', new Error('gone'))
    await expect(first).rejects.toThrow()

    // The next render must reach a different thread; reusing the dead index would hang again.
    mockEndpoint.render.mockResolvedValue(mockRenderResult)
    await expect(pool.render({ width: 100 } as any)).resolves.toMatchObject({ workerIdx: 1 })
    pool.terminate()
  })

  it('replaces a dead worker rather than counting it against the ceiling', async () => {
    neverAnswers()
    const pool = new ComlinkPool(1)

    const first = pool.render({ width: 100 } as any)
    spawned[0].emit('error', new Error('gone'))
    await expect(first).rejects.toThrow()

    mockEndpoint.render.mockResolvedValue(mockRenderResult)
    await expect(pool.render({ width: 100 } as any)).resolves.toMatchObject({ workerIdx: 1 })
    // A pool of one that lost its only worker has to be able to spawn another, or it is finished.
    expect(WorkerMock).toHaveBeenCalledTimes(2)
    pool.terminate()
  })

  it('fails a queued render when every worker dies under it', async () => {
    neverAnswers()
    const pool = new ComlinkPool(1)

    const inFlight = pool.render({ width: 100 } as any)
    const queued = pool.render({ width: 200 } as any)

    spawned[0].emit('error', new Error('gone'))
    await expect(inFlight).rejects.toThrow('Worker 0 died')

    // The queued task was never bound to worker 0, so it moves to a replacement — and fails there
    // too, once, rather than retrying against a failure that repeats.
    spawned[1].emit('error', new Error('gone again'))
    await expect(queued).rejects.toThrow('Worker 1 died')
    pool.terminate()
  })

  it('rejects queued renders when the pool is terminated under them', async () => {
    neverAnswers()
    const pool = new ComlinkPool(1)

    const inFlight = pool.render({ width: 100 } as any)
    const queued = pool.render({ width: 200 } as any)

    pool.terminate()

    await expect(queued).rejects.toThrow('Pool was terminated before this render started')
    // The in-flight one is the caller's to abandon; the queued one had never started, and dropping
    // it silently left its promise pending forever.
    void inFlight.catch(() => {})
  })

  it('stays quiet when terminate makes its own workers exit', async () => {
    const pool = new ComlinkPool(1)
    await pool.render({ width: 100 } as any)

    pool.terminate()
    // `terminate()` exits every thread non-zero on purpose. Treating that as a death would retire
    // workers of an already-dead pool and reject through a `deaths` array that no longer exists.
    expect(() => spawned[0].emit('exit', 1)).not.toThrow()
  })
})
