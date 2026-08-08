import type { RootProps } from '@/canvas/canvas.type.js'
import type { EngineDetails } from 'skia-canvas'

export type CallFn = (id: number, ...args: unknown[]) => Promise<unknown>

/**
 * What the worker reports back once a render finishes.
 *
 * Deliberately carries no image buffer. The worker used to encode a PNG here on every render and
 * ship it across, whether or not the caller ever asked for one — roughly 32ms and 81KB per card
 * that most callers threw away, because they wanted WebP and re-encoded downstream. Buffers are
 * now produced on demand, in the format actually requested.
 *
 * `gpu` and `engine` are snapshotted instead of proxied: they are plain values that cannot change
 * once the canvas has been rendered, so a round trip to read them would buy nothing.
 */
export interface RenderResult {
  canvasId: number
  width: number
  height: number
  gpu: boolean
  engine: EngineDetails
}

/** A synchronous method call travelling to the worker over the raw (non-Comlink) port. */
export interface SyncRequest {
  signal: Int32Array
  canvasId: number
  method: string
  args: unknown[]
}

/** The worker's reply. Exactly one of `result` / `error` is meaningful. */
export interface SyncResponse {
  result?: unknown
  error?: string
}

export interface WorkerAPI {
  render(props: RootProps, callFn?: CallFn): Promise<RenderResult>
  callOnCanvas(canvasId: number, method: string, args: unknown[]): Promise<Buffer | string | void>
  releaseCanvas(canvasId: number): void
}
