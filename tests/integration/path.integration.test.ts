import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { Root } from '@/canvas/root.canvas.js'
import { Path } from '@/canvas/path.canvas.js'
import { integrationRootBase } from './helpers/integration-font.js'

const SIZE = 40
/** Path data covering the top half of the box, so inside and outside are both easy to sample. */
const TOP_HALF = `M 0 0 H ${SIZE} V ${SIZE / 2} H 0 Z`

const pixel = (raw: Buffer, x: number, y: number) => {
  const offset = (y * SIZE + x) * 4
  return { r: raw[offset], g: raw[offset + 1], b: raw[offset + 2], a: raw[offset + 3] }
}

const render = async (props: Record<string, unknown>) => {
  const canvas = await Root({
    ...integrationRootBase,
    width: SIZE,
    height: SIZE,
    workerMode: false,
    children: [Path({ d: TOP_HALF, width: SIZE, height: SIZE, ...props } as never)],
  } as never)
  return canvas.toBufferSync('raw')
}

/**
 * `Path` exists so a shape the components cannot describe does not force a drawing context, which
 * worker mode cannot provide. These assert what reached the pixels, since that is the whole claim.
 */
describe('Path', () => {
  const centre = SIZE / 2

  it('fills the shape and nothing outside it', async () => {
    const raw = await render({ fill: '#ff0000' })

    expect(pixel(raw, centre, 5)).toEqual({ r: 255, g: 0, b: 0, a: 255 })
    expect(pixel(raw, centre, SIZE - 5).a).toBe(0)
  })

  it('draws nothing without a fill or a stroke', async () => {
    // A shape with no paint is not an error; it is a shape nobody asked to see.
    const raw = await render({})
    expect(pixel(raw, centre, 5).a).toBe(0)
  })

  it('strokes the outline without filling the interior', async () => {
    const raw = await render({ stroke: '#00ff00', lineWidth: 4 })

    expect(pixel(raw, centre, centre - 1)).toEqual({ r: 0, g: 255, b: 0, a: 255 })
    expect(pixel(raw, centre, 10).a).toBe(0)
  })

  it('takes a gradient as paint, measured against the node box', async () => {
    const raw = await render({ fill: { type: 'linear', direction: 'to-right', colors: ['#ff0000', '#0000ff'] } })

    const left = pixel(raw, 2, 5)
    const right = pixel(raw, SIZE - 3, 5)
    expect(left.r).toBeGreaterThan(left.b)
    expect(right.b).toBeGreaterThan(right.r)
  })

  it('cuts a hole with the evenodd rule', async () => {
    const nested = `M 0 0 H ${SIZE} V ${SIZE} H 0 Z M 10 10 H 30 V 30 H 10 Z`

    const holed = await render({ d: nested, fill: '#ff0000', fillRule: 'evenodd' })
    const solid = await render({ d: nested, fill: '#ff0000', fillRule: 'nonzero' })

    expect(pixel(holed, centre, centre).a).toBe(0)
    expect(pixel(solid, centre, centre).a).toBe(255)
  })

  it('breaks a stroke into dashes', async () => {
    const line = `M 0 ${centre} H ${SIZE}`
    const dashed = await render({ d: line, stroke: '#00ff00', lineWidth: 6, lineDash: [4, 8] })

    // Somewhere along a dashed line there is a gap; a solid one has none.
    const alphas = Array.from({ length: SIZE }, (_, x) => pixel(dashed, x, centre).a)
    expect(alphas.some(a => a === 0)).toBe(true)
    expect(alphas.some(a => a > 0)).toBe(true)
  })

  it('is laid out like any other node', async () => {
    // Placed by flexbox rather than by its own coordinates: with padding above it, the shape moves
    // down with the node instead of staying at the canvas origin.
    const canvas = await Root({
      ...integrationRootBase,
      width: SIZE,
      height: SIZE,
      workerMode: false,
      padding: { Top: 20 },
      children: [Path({ d: `M 0 0 H ${SIZE} V 10 H 0 Z`, fill: '#ff0000', width: SIZE, height: 20 } as never)],
    } as never)

    const raw = canvas.toBufferSync('raw')
    expect(pixel(raw, centre, 5).a).toBe(0)
    expect(pixel(raw, centre, 25)).toEqual({ r: 255, g: 0, b: 0, a: 255 })
  })

  it('keeps the shape when a gradient paint cannot be built', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const raw = await render({ fill: { type: 'linear', direction: 'sideways', colors: ['#ff0000'] } })

    // Nothing is painted, because there is no paint — but the warning says which prop was dropped.
    expect(pixel(raw, centre, 5).a).toBe(0)
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('fill ignored.'))
    warn.mockRestore()
  })
})

/** Loaded from `dist` for the reason the other worker suites document: the pool starts by path. */
const DIST = join(dirname(fileURLToPath(import.meta.url)), '../../dist/esm/index.js')

describe.skipIf(!existsSync(DIST))('Path in worker mode', () => {
  const centre = SIZE / 2

  it('crosses the worker boundary as a descriptor', async () => {
    // The whole point of describing a shape rather than drawing it: `Path` is plain data, so it
    // survives structured clone where a context could not.
    const { Root: builtRoot, Path: builtPath, terminate } = await import(DIST)

    const canvas = await builtRoot({
      ...integrationRootBase,
      width: SIZE,
      height: SIZE,
      workers: 1,
      children: [builtPath({ d: TOP_HALF, fill: '#ff0000', width: SIZE, height: SIZE })],
    } as never)

    try {
      const raw = canvas.toBufferSync('raw')
      expect(pixel(raw, centre, 5)).toEqual({ r: 255, g: 0, b: 0, a: 255 })
      expect(pixel(raw, centre, SIZE - 5).a).toBe(0)
    } finally {
      canvas.release()
      await terminate()
    }
  })
})
