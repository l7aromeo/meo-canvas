import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { loadImage } from 'meo-skia-canvas'
import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { integrationFontFamily, integrationRootBase } from './helpers/integration-font.js'

const WIDTH = 120
const HEIGHT = 60
const PAGE_COUNT = 3
const FPS = 10
/** `progress` midpoint — the swatch flips colour here, so no two halves encode alike. */
const HALFWAY = 0.5
/** Per-page delays in milliseconds, deliberately unequal and multiples of 10ms (GIF's resolution). */
const DELAYS = [200, 100, 300]

/** Byte length of a PNG chunk's length field, which sits immediately before the chunk type. */
const PNG_CHUNK_TYPE_LENGTH = 4

/**
 * Frame count an APNG declares in its animation-control chunk.
 *
 * `acTL` carries `num_frames` as a big-endian `u32` directly after the four-byte chunk type, and
 * is the file's own statement of how many frames it holds.
 */
const apngFrameCount = (buffer: Buffer): number => {
  const chunk = buffer.indexOf(Buffer.from('acTL'))
  if (chunk < 0) throw new Error('no acTL chunk — this APNG declares no animation')
  return buffer.readUInt32BE(chunk + PNG_CHUNK_TYPE_LENGTH)
}

/**
 * A page whose content depends on the page index, so encoders cannot collapse identical frames
 * and a decoded frame count is therefore meaningful.
 */
const swatch = (progress: number, label: string) =>
  Box({
    width: WIDTH,
    height: HEIGHT,
    backgroundColor: progress < HALFWAY ? '#1d4ed8' : '#b91c1c',
    children: [Text(label, { color: '#ffffff', fontSize: 18, fontFamily: integrationFontFamily })],
  })

const pagedRoot = (overrides: Record<string, unknown>) =>
  Root({
    ...integrationRootBase,
    width: WIDTH,
    height: HEIGHT,
    workerMode: false,
    children: ({ index, progress }: { index: number; progress: number }) => swatch(progress, String(index)),
    ...overrides,
  } as never)

describe('paged rendering', () => {
  it('produces one page per requested page', async () => {
    const canvas = await pagedRoot({ pages: PAGE_COUNT })
    expect(canvas.pages).toHaveLength(PAGE_COUNT)
  })

  it('still renders exactly one page when no pages are requested', async () => {
    const canvas = await Root({
      ...integrationRootBase,
      width: WIDTH,
      height: HEIGHT,
      workerMode: false,
      children: [swatch(0, 'x')],
    })
    expect(canvas.pages).toHaveLength(1)
  })

  it('round-trips a GIF whose decoded frame count matches the pages rendered', async () => {
    const canvas = await pagedRoot({ pages: PAGE_COUNT })
    const gif = await canvas.toBuffer('gif', { fps: FPS })

    expect(gif.subarray(0, 3).toString('latin1')).toBe('GIF')
    const decoded = await loadImage(gif)
    expect(decoded.frames).toBe(PAGE_COUNT)
  })

  it('writes an APNG declaring one frame per page rendered', async () => {
    const canvas = await pagedRoot({ pages: PAGE_COUNT })
    const apng = await canvas.toBuffer('apng', { fps: FPS })

    // Read the count out of the file rather than via `loadImage`, which reports frame counts for
    // GIF but not for APNG — it answers 1 for any APNG, however many frames the file declares.
    expect(apngFrameCount(apng)).toBe(PAGE_COUNT)
  })

  it('honours per-page delays through frameDelays', async () => {
    const canvas = await pagedRoot({ pages: DELAYS.length })
    const gif = await canvas.toBuffer('gif', { frameDelays: DELAYS })

    const decoded = await loadImage(gif)
    expect(decoded.frames).toBe(DELAYS.length)
    // GIF stores hundredths of a second; every delay here is a whole number of them.
    expect(Array.from(decoded.delays).slice(0, DELAYS.length)).toEqual(DELAYS)
  })

  it('derives the page count from duration and fps', async () => {
    const duration = 0.5
    const fps = 12
    const canvas = await pagedRoot({ duration, fps })

    expect(canvas.pages).toHaveLength(Math.ceil(duration * fps))
  })

  it('gathers every page into a multi-page PDF', async () => {
    const one = await pagedRoot({ pages: 1 })
    const many = await pagedRoot({ pages: PAGE_COUNT })

    const onePdf = await one.toBuffer('pdf')
    const manyPdf = await many.toBuffer('pdf')

    expect(onePdf.subarray(0, 4).toString('latin1')).toBe('%PDF')
    expect(manyPdf.subarray(0, 4).toString('latin1')).toBe('%PDF')
    // Distinct content on every page cannot fit in the space a single page takes.
    expect(manyPdf.length).toBeGreaterThan(onePdf.length)
  })

  it('awaits an async page builder', async () => {
    const canvas = await pagedRoot({
      pages: PAGE_COUNT,
      children: async ({ index, progress }: { index: number; progress: number }) => {
        await new Promise(resolve => setTimeout(resolve, 1))
        return swatch(progress, String(index))
      },
    })

    expect(canvas.pages).toHaveLength(PAGE_COUNT)
  })
})

/**
 * Loaded from `dist` for the reason `worker-sync.integration.test.ts` documents: the pool starts a
 * worker by path — `render.worker.js` — which exists only once the package has been built.
 * Importing the sources would spawn a worker that never loads, and the render would hang.
 */
const DIST = join(dirname(fileURLToPath(import.meta.url)), '../../dist/esm/index.js')
const built = existsSync(DIST)

describe.skipIf(!built)('paged rendering in worker mode', () => {
  let builtRoot: typeof import('@/canvas/root.canvas.js').Root
  let builtTerminate: typeof import('@/canvas/root.canvas.js').terminate
  let builtBox: typeof import('@/canvas/layout.canvas.js').Box
  let builtText: typeof import('@/canvas/text.canvas.js').Text

  beforeAll(async () => {
    ;({ Root: builtRoot, terminate: builtTerminate, Box: builtBox, Text: builtText } = await import(DIST))
  })

  afterAll(async () => {
    await builtTerminate()
  })

  it('renders every page inside the worker', async () => {
    const canvas = await builtRoot({
      ...integrationRootBase,
      width: WIDTH,
      height: HEIGHT,
      workers: 1,
      pages: PAGE_COUNT,
      children: ({ index, progress }: { index: number; progress: number }) =>
        builtBox({
          width: WIDTH,
          height: HEIGHT,
          backgroundColor: progress < HALFWAY ? '#1d4ed8' : '#b91c1c',
          children: [builtText(String(index), { color: '#ffffff', fontSize: 18, fontFamily: integrationFontFamily })],
        }),
    } as never)

    try {
      const gif = await canvas.toBuffer('gif', { fps: FPS })
      const decoded = await loadImage(gif)
      expect(decoded.frames).toBe(PAGE_COUNT)
    } finally {
      canvas.release()
    }
  })
})
