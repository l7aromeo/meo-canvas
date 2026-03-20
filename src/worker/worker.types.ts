import type { ExportFormat, ExportOptions, SaveOptions, RenderOptions } from 'skia-canvas'
import type { RootProps } from '@/canvas/canvas.type.js'

// ---------------------------------------------------------------------------
// Canvas method call payload — each entry maps method → (args, result)
// ---------------------------------------------------------------------------

export interface CanvasCallMap {
  toBuffer: { args: [ExportFormat, ExportOptions?]; result: Buffer }
  toURL: { args: [ExportFormat, ExportOptions?]; result: string }
  toFile: { args: [string, SaveOptions?]; result: void }
  toSharp: { args: [RenderOptions?]; result: Buffer }
}

export type CanvasCallMethod = keyof CanvasCallMap
export type CallArgs<M extends CanvasCallMethod> = CanvasCallMap[M]['args']
export type CallResult<M extends CanvasCallMethod> = CanvasCallMap[M]['result']

// ---------------------------------------------------------------------------
// Worker request messages (main → worker)
// ---------------------------------------------------------------------------

export interface WorkerRenderRequest {
  type: 'render'
  taskId: number
  props: RootProps
}

/** Discriminated union — narrows args alongside method in switch statements */
export type WorkerCallRequest =
  | { type: 'call'; taskId: number; canvasId: number; method: 'toBuffer'; args: CallArgs<'toBuffer'> }
  | { type: 'call'; taskId: number; canvasId: number; method: 'toURL'; args: CallArgs<'toURL'> }
  | { type: 'call'; taskId: number; canvasId: number; method: 'toFile'; args: CallArgs<'toFile'> }
  | { type: 'call'; taskId: number; canvasId: number; method: 'toSharp'; args: CallArgs<'toSharp'> }

export interface WorkerReleaseRequest {
  type: 'release'
  canvasId: number
}

export type WorkerRequest = WorkerRenderRequest | WorkerCallRequest | WorkerReleaseRequest

// ---------------------------------------------------------------------------
// Worker response messages (worker → main)
// ---------------------------------------------------------------------------

export interface WorkerRenderResponse {
  taskId: number
  canvasId: number
  buffer: Buffer
  width: number
  height: number
}

export interface WorkerCallResponse {
  taskId: number
  result: Buffer | string | void
}

export interface WorkerErrorResponse {
  taskId: number
  error: string
}

export type WorkerResponse = WorkerRenderResponse | WorkerCallResponse | WorkerErrorResponse
