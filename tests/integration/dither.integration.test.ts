import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { PNG } from 'pngjs'
import { Root } from '@/canvas/root.canvas.js'
import { Box, Row } from '@/canvas/layout.canvas.js'
import { integrationRootBase } from './helpers/integration-font.js'

/** Worker mode starts a worker by path from the build, so these cases need one to exist. */
const DIST = join(dirname(fileURLToPath(import.meta.url)), '../../dist/esm/index.js')
const built = existsSync(DIST)

const WIDTH = 400
const HEIGHT = 48

/** A long, subtle ramp: the case an eight-bit surface has the fewest values to spend on. */
const RAMP = { type: 'linear', direction: 'to-right', colors: ['#0b1220', '#1e2b4a'] } as const

/**
 * How many tones the ramp really carries across a span, averaged down each column.
 *
 * A dither adds no values to a single row — it spreads two neighbouring ones over neighbouring
 * pixels — so counting one row scores a dithered ramp no better than the banded one it fixes.
 * Averaging the column is what vision does, and what makes the difference measurable.
 */
function columnTones(pixels: Uint8ClampedArray, width: number, height: number, from: number, to: number) {
  const tones = new Set<string>()
  for (let x = from; x < to; x++) {
    let r = 0
    let g = 0
    let b = 0
    for (let y = 0; y < height; y++) {
      const i = (y * width + x) * 4
      r += pixels[i]
      g += pixels[i + 1]
      b += pixels[i + 2]
    }
    tones.add(`${(r / height).toFixed(3)},${(g / height).toFixed(3)},${(b / height).toFixed(3)}`)
  }
  return tones.size
}

/**
 * The most tones an undithered eight-bit ramp between two colours can hold.
 *
 * Every step along it changes at least one channel by one level, so the count cannot exceed the
 * channel spans taken together. Derived from the ramp rather than written down, so it stays the
 * real ceiling if the colours here ever change — and it is what makes "dithered" measurable as
 * "more tones than eight bits has", rather than as a number someone once observed.
 */
const QUANTIZED_CEILING = (() => {
  const channels = (hex: string) => [1, 3, 5].map(at => parseInt(hex.slice(at, at + 2), 16))
  const [from, to] = RAMP.colors.map(channels)
  return from.reduce((total, level, channel) => total + Math.abs(to[channel] - level), 1)
})()

/**
 * `gpu: false` for a figure that does not move between machines. Both backends dither, and to
 * within a few percent of each other, but only the CPU one is present on every runner.
 */
const base = { ...integrationRootBase, width: WIDTH, height: HEIGHT, workerMode: false, gpu: false } as const

describe('dither', () => {
  it('carries more tones across a ramp than the same ramp undithered', async () => {
    const tonesFor = async (dither: boolean) => {
      const canvas = await Root({ ...base, dither, children: [Box({ width: WIDTH, height: HEIGHT, gradient: RAMP })] })
      const { data } = canvas.getContext('2d').getImageData(0, 0, WIDTH, HEIGHT)
      return columnTones(data, WIDTH, HEIGHT, 0, WIDTH)
    }

    const banded = await tonesFor(false)
    const dithered = await tonesFor(true)

    // Undithered, the ramp cannot beat what eight bits can express. Dithered, it does — which is
    // the whole claim, stated without pinning a figure a Skia release could move.
    expect(banded).toBeLessThanOrEqual(QUANTIZED_CEILING)
    expect(dithered).toBeGreaterThan(QUANTIZED_CEILING)
  })

  it('applies to every page, not only the first', async () => {
    // Each page is drawn on a context of its own, which starts at the renderer's default: a
    // setting applied once at canvas creation would reach page one and no further.
    const canvas = await Root({
      ...base,
      dither: true,
      pages: 3,
      fps: 1,
      children: () => Box({ width: WIDTH, height: HEIGHT, gradient: RAMP }),
    })

    expect(canvas.pages).toHaveLength(3)
    for (const page of canvas.pages) {
      const { data } = page.getImageData(0, 0, WIDTH, HEIGHT)
      expect(columnTones(data, WIDTH, HEIGHT, 0, WIDTH)).toBeGreaterThan(QUANTIZED_CEILING)
    }
  })

  it('leaves a sibling alone when one node turns it off', async () => {
    const half = WIDTH / 2
    const canvas = await Root({
      ...base,
      dither: true,
      children: [
        Row({
          width: WIDTH,
          height: HEIGHT,
          children: [Box({ width: half, height: HEIGHT, gradient: RAMP }), Box({ width: half, height: HEIGHT, gradient: RAMP, dither: false })],
        }),
      ],
    })

    const { data } = canvas.getContext('2d').getImageData(0, 0, WIDTH, HEIGHT)
    const inherited = columnTones(data, WIDTH, HEIGHT, 0, half)
    const turnedOff = columnTones(data, WIDTH, HEIGHT, half, WIDTH)

    // Same gradient over the same span, drawn side by side: the only difference is the prop on the
    // second one, and the first one takes the root's answer rather than its neighbour's.
    expect(turnedOff).toBeLessThanOrEqual(QUANTIZED_CEILING)
    expect(inherited).toBeGreaterThan(QUANTIZED_CEILING)
  })
})

/**
 * Worker mode, which reaches the canvas by a different route.
 *
 * `Root` hands the whole props object across by structured clone, but the engine settings beside
 * this one are rebuilt field by field in `canvasOptions()` — an allowlist a later change could
 * route this through and drop it from, with nothing above noticing.
 */
describe.skipIf(!built)('dither in worker mode', () => {
  let WorkerRoot: typeof Root
  let WorkerBox: typeof Box
  let terminate: typeof import('@/canvas/root.canvas.js').terminate

  beforeAll(async () => {
    ;({ Root: WorkerRoot, Box: WorkerBox, terminate } = await import(DIST))
  })

  afterAll(async () => {
    await terminate()
  })

  it('reaches the canvas across the worker boundary', async () => {
    const tonesFor = async (dither: boolean) => {
      const canvas = await WorkerRoot({
        ...base,
        workerMode: true,
        dither,
        children: [WorkerBox({ width: WIDTH, height: HEIGHT, gradient: RAMP })],
      })
      const png = PNG.sync.read(await canvas.toBuffer('png'))
      return columnTones(png.data as unknown as Uint8ClampedArray, WIDTH, HEIGHT, 0, WIDTH)
    }

    expect(await tonesFor(false)).toBeLessThanOrEqual(QUANTIZED_CEILING)
    expect(await tonesFor(true)).toBeGreaterThan(QUANTIZED_CEILING)
  })
})
