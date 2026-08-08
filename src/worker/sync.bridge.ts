import { receiveMessageOnPort, type MessagePort } from 'node:worker_threads'
import type { SyncRequest, SyncResponse } from '@/worker/worker.types.js'

/** How long a synchronous call waits on its worker before giving up. */
export const SYNC_CALL_TIMEOUT_MS = 30_000

/**
 * Synchronous call channel to a render worker.
 *
 * Comlink cannot serve the `*Sync` half of the Canvas API — it is promise-only. That is why the
 * worker used to pre-encode a PNG at render time and hand every sync method the same buffer no
 * matter which format was asked for: `toBufferSync('svg')` returned PNG bytes, and a caller who
 * wrote them to a `.svg` file got a corrupt file with no error anywhere.
 *
 * This channel parks the calling thread in `Atomics.wait` while the worker runs the real method on
 * the real Canvas, so every format and every option behaves as documented.
 *
 * Blocking is not a regression — it is the contract. `toBufferSync` on a plain Canvas blocks for
 * the whole encode too (measured at 60ms for a 1560x1170 WebP). The alternative, shipping raw
 * pixels to the caller's thread and encoding there, relocates the identical stall and adds a
 * multi-megabyte copy per call.
 */
export class SyncChannel {
  /**
   * A single control word, reused for every call. Safe because the caller is parked inside
   * `Atomics.wait` for the whole exchange: no second request can begin on this channel until the
   * current one has been read off the port.
   */
  private readonly signal = new Int32Array(new SharedArrayBuffer(4))

  constructor(
    private readonly port: MessagePort,
    private readonly timeoutMs: number = SYNC_CALL_TIMEOUT_MS,
  ) {}

  call(canvasId: number, method: string, args: unknown[]): unknown {
    Atomics.store(this.signal, 0, 0)
    const request: SyncRequest = { signal: this.signal, canvasId, method, args }
    this.port.postMessage(request)

    // 'not-equal' means the worker finished and raised the flag before this thread got as far as
    // parking. The reply is already sitting on the port, so only a timeout counts as a failure.
    if (Atomics.wait(this.signal, 0, 0, this.timeoutMs) === 'timed-out') {
      throw new Error(`[canvas] ${method}() timed out after ${this.timeoutMs}ms — the render worker did not respond`)
    }

    const envelope = receiveMessageOnPort(this.port)
    if (!envelope) {
      throw new Error(`[canvas] ${method}() was signalled complete but the worker sent no reply`)
    }

    const { result, error } = envelope.message as SyncResponse
    if (error) throw new Error(error)
    return result
  }

  close(): void {
    this.port.close()
  }
}
