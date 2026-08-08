import { vi } from 'vitest'
import type { MessagePort } from 'node:worker_threads'

/**
 * `receiveMessageOnPort` is mocked because a faithful test would deadlock.
 *
 * The channel blocks its own thread in `Atomics.wait`, so a real worker replying over a real port
 * can never be serviced from that same thread — the reply lands only after the wait ends, and the
 * wait ends only once the reply arrives. A real worker is exercised in the integration suite; what
 * is checked here is the protocol around it: flag handling, error propagation and the timeout.
 */
const receiveMessageOnPort = vi.fn<(port: MessagePort) => { message: unknown } | undefined>()

vi.mock('node:worker_threads', () => ({
  get receiveMessageOnPort() {
    return receiveMessageOnPort
  },
}))

let SyncChannel: typeof import('@/worker/sync.bridge.js').SyncChannel

beforeEach(async () => {
  vi.clearAllMocks()
  ;({ SyncChannel } = await import('@/worker/sync.bridge.js'))
})

/**
 * Stands in for the worker: raises the caller's flag the moment the request is posted, which is
 * what makes `Atomics.wait` return 'not-equal' and fall straight through.
 */
function respondingPort() {
  const postMessage = vi.fn((request: { signal: Int32Array }) => {
    Atomics.store(request.signal, 0, 1)
  })
  return { postMessage, close: vi.fn() } as unknown as MessagePort & { postMessage: typeof postMessage }
}

describe('SyncChannel', () => {
  it('sends the method and args, then returns the worker result', () => {
    const port = respondingPort()
    receiveMessageOnPort.mockReturnValue({ message: { result: Buffer.from('webp') } })

    const channel = new SyncChannel(port)
    const result = channel.call(7, 'toBufferSync', ['webp', { quality: 0.8 }])

    expect(result).toEqual(Buffer.from('webp'))
    expect(port.postMessage).toHaveBeenCalledTimes(1)
    const request = port.postMessage.mock.calls[0][0] as Record<string, unknown>
    expect(request.canvasId).toBe(7)
    expect(request.method).toBe('toBufferSync')
    expect(request.args).toEqual(['webp', { quality: 0.8 }])
  })

  it('rethrows a worker-side failure on the calling thread', () => {
    const port = respondingPort()
    receiveMessageOnPort.mockReturnValue({ message: { error: 'Canvas 3 not found' } })

    expect(() => new SyncChannel(port).call(3, 'toBufferSync', ['png'])).toThrow('Canvas 3 not found')
  })

  /** A raised flag with nothing on the port would otherwise return undefined as if it were data. */
  it('throws when the flag is raised but no reply arrives', () => {
    const port = respondingPort()
    receiveMessageOnPort.mockReturnValue(undefined)

    expect(() => new SyncChannel(port).call(1, 'toBufferSync', ['png'])).toThrow('no reply')
  })

  it('gives up rather than blocking forever when the worker never answers', () => {
    // Posts nothing back and leaves the flag at 0, so the wait runs to its deadline.
    const port = { postMessage: vi.fn(), close: vi.fn() } as unknown as MessagePort

    expect(() => new SyncChannel(port, 20).call(1, 'toBufferSync', ['png'])).toThrow('timed out after 20ms')
    expect(receiveMessageOnPort).not.toHaveBeenCalled()
  })

  it('resets the flag between calls so a second call still waits', () => {
    const port = respondingPort()
    receiveMessageOnPort.mockReturnValue({ message: { result: 'ok' } })

    const channel = new SyncChannel(port)
    expect(channel.call(1, 'toURLSync', ['png'])).toBe('ok')
    expect(channel.call(1, 'toURLSync', ['webp'])).toBe('ok')
    expect(port.postMessage).toHaveBeenCalledTimes(2)
  })

  it('closes its port', () => {
    const port = respondingPort()
    new SyncChannel(port).close()
    expect(port.close).toHaveBeenCalled()
  })
})
