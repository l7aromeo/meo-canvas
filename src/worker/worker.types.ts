import type { RootProps } from '@/canvas/canvas.type.js'

export interface RenderResult {
  canvasId: number
  buffer: Buffer
  width: number
  height: number
}

export interface WorkerAPI {
  render(props: RootProps): Promise<RenderResult>
  callOnCanvas(canvasId: number, method: string, args: unknown[]): Promise<Buffer | string | void>
  releaseCanvas(canvasId: number): void
}
