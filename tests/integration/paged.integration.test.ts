import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { loadImage } from 'meo-skia-canvas'
import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { Image } from '@/canvas/image.canvas.js'
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

    // Both halves: what the file declares in `acTL`, and what a decoder actually walks. The
    // renderer only learned to demux APNG in 5.2.0 — before that it read one back as a single
    // still — so the two agreeing is itself the thing worth pinning.
    expect(apngFrameCount(apng)).toBe(PAGE_COUNT)
    expect((await loadImage(apng)).frames).toBe(PAGE_COUNT)
  })

  it.each(['webp', 'avif'] as const)('round-trips an animated %s', async format => {
    // WebP and AVIF animate as of the renderer's 5.2.0; before it they encoded a single page.
    const canvas = await pagedRoot({ pages: PAGE_COUNT })
    const encoded = await canvas.toBuffer(format, { fps: FPS })

    expect((await loadImage(encoded)).frames).toBe(PAGE_COUNT)
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

/**
 * An animated source is the mirror of an animated export: the library can write one, so it should
 * be able to draw one. The expectation people arrive with is `<img src="x.gif">` — it plays — and
 * it has to play at the source's own rate, not at the render's, since the two rarely match.
 */
describe('animated image sources', () => {
  /** A four-frame GIF whose frames are flat, distinct colours, with deliberately uneven timing. */
  const FRAME_COLOURS = ['#ff0000', '#00ff00', '#0000ff', '#ffff00']
  const SOURCE_DELAYS = [500, 500, 500, 500]

  const animatedSource = async () => {
    const frames = await Root({
      ...integrationRootBase,
      width: 20,
      height: 20,
      workerMode: false,
      pages: FRAME_COLOURS.length,
      children: ({ index }: { index: number }) => Box({ width: 20, height: 20, backgroundColor: FRAME_COLOURS[index] }),
    } as never)
    return frames.toBuffer('gif', { frameDelays: SOURCE_DELAYS })
  }

  /**
   * The colour at the centre of one page, as `#rrggbb`.
   *
   * A raster export writes a single page — the current one unless asked otherwise — so the page has
   * to be named. It is numbered from 1, which is the renderer's convention, not from 0.
   */
  const centre = (canvas: { toBufferSync(format: 'raw', options?: { page?: number }): Buffer }, page: number, size: number) => {
    const raw = canvas.toBufferSync('raw', { page: page + 1 })
    const offset = ((size / 2) * size + size / 2) * 4
    return '#' + [raw[offset], raw[offset + 1], raw[offset + 2]].map(c => c.toString(16).padStart(2, '0')).join('')
  }

  it('plays at the source rate, so each half-second lands on the next frame', async () => {
    const src = await animatedSource()
    const SIZE = 20

    // 2 frames a second against a source that changes twice a second: page N shows frame N/2.
    const canvas = await Root({
      ...integrationRootBase,
      width: SIZE,
      height: SIZE,
      workerMode: false,
      pages: 8,
      fps: 2,
      children: () => Image({ src, width: SIZE, height: SIZE }),
    } as never)

    expect(canvas.pages).toHaveLength(8)
    expect(centre(canvas, 0, SIZE)).toBe(FRAME_COLOURS[0])
    expect(centre(canvas, 1, SIZE)).toBe(FRAME_COLOURS[1])
    expect(centre(canvas, 2, SIZE)).toBe(FRAME_COLOURS[2])
    expect(centre(canvas, 3, SIZE)).toBe(FRAME_COLOURS[3])
    // Two seconds is the whole source, so it comes back around.
    expect(centre(canvas, 4, SIZE)).toBe(FRAME_COLOURS[0])
  })

  it('holds the last frame when looping is off', async () => {
    const src = await animatedSource()
    const SIZE = 20

    const canvas = await Root({
      ...integrationRootBase,
      width: SIZE,
      height: SIZE,
      workerMode: false,
      pages: 6,
      fps: 2,
      children: () => Image({ src, width: SIZE, height: SIZE, loop: false }),
    } as never)

    expect(centre(canvas, 5, SIZE)).toBe(FRAME_COLOURS[FRAME_COLOURS.length - 1])
  })

  it('pins the frame it is told to, ignoring the clock', async () => {
    const src = await animatedSource()
    const SIZE = 20

    const canvas = await Root({
      ...integrationRootBase,
      width: SIZE,
      height: SIZE,
      workerMode: false,
      pages: 4,
      fps: 2,
      children: () => Image({ src, width: SIZE, height: SIZE, frame: 2 }),
    } as never)

    for (const page of [0, 1, 2, 3]) {
      expect(centre(canvas, page, SIZE)).toBe(FRAME_COLOURS[2])
    }
  })

  it('counts a negative frame from the end, as the renderer does', async () => {
    const src = await animatedSource()
    const SIZE = 20

    const canvas = await Root({
      ...integrationRootBase,
      width: SIZE,
      height: SIZE,
      workerMode: false,
      children: [Image({ src, width: SIZE, height: SIZE, frame: -1 })],
    } as never)

    expect(centre(canvas, 0, SIZE)).toBe(FRAME_COLOURS[FRAME_COLOURS.length - 1])
  })

  it('draws the first frame in a still render, as it always has', async () => {
    const src = await animatedSource()
    const SIZE = 20

    const canvas = await Root({
      ...integrationRootBase,
      width: SIZE,
      height: SIZE,
      workerMode: false,
      children: [Image({ src, width: SIZE, height: SIZE })],
    } as never)

    expect(centre(canvas, 0, SIZE)).toBe(FRAME_COLOURS[0])
  })
})
