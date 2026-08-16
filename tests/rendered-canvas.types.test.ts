import type { Canvas } from 'meo-skia-canvas'
import type { RenderedCanvas } from '@/canvas/canvas.type.js'

/**
 * Non-worker renders hand back the renderer's own `Canvas`, whose `toBuffer` takes any format with
 * any options. Worker renders go through `WorkerCanvas`, which narrows the animation options to the
 * formats that can animate. The two modes disagreeing means the same mistake is a compile error in
 * one and a runtime `TypeError` in the other, purely because of where the render happened.
 *
 * `RenderedCanvas` is that narrowing applied to the bare canvas. It changes no behaviour — the
 * object returned is the same one — so these assertions are compile-time only and never run.
 */
describe('RenderedCanvas narrows exports the way WorkerCanvas does', () => {
  it('accepts animation timing on the formats that animate', () => {
    const accepted = (canvas: RenderedCanvas) => {
      void canvas.toBuffer('gif', { fps: 30 })
      void canvas.toBuffer('apng', { loop: 0 })
      void canvas.toBuffer('webp', { fps: 24 })
      void canvas.toBuffer('avif', { fps: 24, frameDelays: [100, 200] })
      void canvas.toBufferSync('gif', { fps: 30 })
      void canvas.toURL('apng', { fps: 30 })
      void canvas.toURLSync('gif', { loop: 2 })
    }

    expect(accepted).toBeTypeOf('function')
  })

  it('rejects animation timing on the formats that cannot', () => {
    const rejected = (canvas: RenderedCanvas) => {
      // @ts-expect-error — `png` encodes one page, so `fps` would do nothing
      void canvas.toBuffer('png', { fps: 30 })

      // @ts-expect-error — `pdf` gathers pages as sheets, with no timeline
      void canvas.toBuffer('pdf', { loop: 0 })

      // @ts-expect-error — the sync path is narrowed the same way
      void canvas.toBufferSync('jpg', { fps: 12 })

      // @ts-expect-error — and so is the URL path
      void canvas.toURL('svg', { frameDelays: [100] })
    }

    expect(rejected).toBeTypeOf('function')
  })

  it('keeps the rest of the canvas reachable', () => {
    const rest = (canvas: RenderedCanvas) => {
      // Everything a non-worker render is handed a real canvas *for* has to survive the narrowing.
      void canvas.pages.length
      void canvas.width
      void canvas.height
      void canvas.getContext('2d')
      void canvas.newPage(10, 10)
      void canvas.toFile('out.png')
      void canvas.toBuffer('png', { quality: 0.8, density: 2 })
    }

    expect(rest).toBeTypeOf('function')
  })

  it("is still the renderer's canvas at runtime", () => {
    // A type-only narrowing: assignable from the real thing, because it is the real thing.
    const fromReal = (canvas: Canvas): RenderedCanvas => canvas
    expect(fromReal).toBeTypeOf('function')
  })
})
