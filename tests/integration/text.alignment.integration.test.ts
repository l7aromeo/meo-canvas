import { Canvas, FontLibrary } from 'meo-skia-canvas'
import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import type { TextProps } from '@/canvas/canvas.type.js'
import { INTEGRATION_FONT_FAMILY, integrationRootBase } from './helpers/integration-font.js'

const WIDTH = 260
const HEIGHT = 100
const FONT_SIZE = 32
/** Renders oversampled so a baseline can be read to an eighth of a pixel rather than to one. */
const SCALE = 8

/**
 * Where Chrome puts the baseline for each case, in CSS pixels from the top of a 260x100 box.
 *
 * Measured against this same Roboto file (md5 8f793587dcf03f31c551c5b60d175fc2) served over HTTP,
 * `font: 32px/38.4px`, the box a flex container and the text its item. Each figure is the top of a
 * zero-size inline-block appended to the line, which is the one element CSS guarantees sits on the
 * baseline. Chrome rounds the face's metrics to whole pixels where Skia does not, which is the
 * whole of the tolerance below.
 */
const CHROME = {
  'middle, explicit line-height': 62.3,
  'middle, default line-height': 62.5,
  top: 31.5,
  bottom: 93.1,
  'first of two lines': 43.1,
  'second of two lines': 81.5,
} as const

/** Chrome's rounding against Skia's exact metrics comes to an eighth of a pixel; this is room to spare. */
const TOLERANCE = 0.5

/** `toBeCloseTo` takes a digit count rather than a distance, which is not what these figures need. */
function expectWithin(actual: number, expected: number, tolerance = TOLERANCE) {
  expect(Math.abs(actual - expected), `${actual.toFixed(3)} is more than ${tolerance} from ${expected}`).toBeLessThanOrEqual(tolerance)
}

let inkAscent: Record<string, number>

beforeAll(() => {
  FontLibrary.use({ [INTEGRATION_FONT_FAMILY]: [integrationRootBase.fonts![0].paths![0]] })
  // What this rasterizer puts above a baseline it was handed, at the scale the cases render at, so
  // the comparison subtracts its own quantization instead of assuming Chrome's.
  inkAscent = Object.fromEntries(['Ping', 'gasp'].map(text => [text, pixelInkAscent(text)]))
})

/** First row carrying ink, in unscaled pixels. `NaN` when the canvas is blank. */
function inkTop(canvas: Canvas, width: number, height: number, from = 0): number {
  const { data } = canvas.getContext('2d').getImageData(0, 0, width, height)
  for (let y = from; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (data[(y * width + x) * 4] < 250) return y / SCALE
    }
  }
  return NaN
}

function pixelInkAscent(text: string): number {
  const canvas = new Canvas(WIDTH * SCALE, HEIGHT * SCALE, { gpu: false })
  const ctx = canvas.getContext('2d')
  ctx.scale(SCALE, SCALE)
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(0, 0, WIDTH, HEIGHT)
  ctx.font = `${FONT_SIZE}px ${INTEGRATION_FONT_FAMILY}`
  ctx.textBaseline = 'alphabetic'
  ctx.fillStyle = '#000000'
  ctx.fillText(text, 20, 50)
  return 50 - inkTop(canvas, WIDTH * SCALE, HEIGHT * SCALE)
}

async function render(text: string, props: Partial<TextProps>) {
  return Root({
    ...integrationRootBase,
    width: WIDTH,
    scale: SCALE,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [
      Box({
        width: WIDTH,
        height: HEIGHT,
        backgroundColor: '#ffffff',
        children: [
          Text(text, {
            width: WIDTH,
            height: HEIGHT,
            fontSize: FONT_SIZE,
            fontFamily: INTEGRATION_FONT_FAMILY,
            color: '#000000',
            textAlign: 'center',
            ...props,
          }),
        ],
      }),
    ],
  })
}

/** The baseline a rendered case landed on, recovered from where its ink starts. */
async function baselineOf(text: string, props: Partial<TextProps>, from = 0) {
  const canvas = await render(text, props)
  return inkTop(canvas, WIDTH * SCALE, HEIGHT * SCALE, from) + inkAscent[text.split('\n')[0]]
}

describe('text baseline against a browser', () => {
  it('centres a line where CSS centres it', async () => {
    expectWithin(await baselineOf('Ping', { verticalAlign: 'middle', lineHeight: 38.4 }), CHROME['middle, explicit line-height'])
  })

  it('takes the default line box from the face, as `line-height: normal` does', async () => {
    // The old default was `fontSize * 1.2`, which for this face is 9% tighter than the face itself
    // and put the baseline somewhere CSS never would.
    expectWithin(await baselineOf('Ping', { verticalAlign: 'middle' }), CHROME['middle, default line-height'])
  })

  it('aligns to the top of the box the way CSS does', async () => {
    expectWithin(await baselineOf('Ping', { verticalAlign: 'top', lineHeight: 38.4 }), CHROME.top)
  })

  it('aligns to the bottom of the box the way CSS does', async () => {
    expectWithin(await baselineOf('Ping', { verticalAlign: 'bottom', lineHeight: 38.4 }), CHROME.bottom)
  })

  it('spaces two lines by exactly the line-height', async () => {
    const canvas = await render('Ping\ngasp', { verticalAlign: 'middle', lineHeight: 38.4 })
    const first = inkTop(canvas, WIDTH * SCALE, HEIGHT * SCALE) + inkAscent.Ping

    // Past the first line's ink to find the second, whose own ink ascent is shorter — `gasp` has no
    // capital and no ascender.
    const belowFirst = Math.ceil((first + 10) * SCALE)
    const second = inkTop(canvas, WIDTH * SCALE, HEIGHT * SCALE, belowFirst) + inkAscent.gasp

    expectWithin(first, CHROME['first of two lines'])
    expectWithin(second, CHROME['second of two lines'])
    expectWithin(second - first, 38.4)
  })
})

describe('text baseline against itself', () => {
  /**
   * The property the browser gets from a strut and this gets from the face's metrics: what the line
   * is made of cannot move it. A descender must not push the line up, nor an ascender down.
   */
  it('puts every string on the same baseline, tail or no tail', async () => {
    const strings = ['acorn', 'hill', 'papaya', 'Ping', 'HELLO', '12345']
    const ascents = Object.fromEntries(strings.map(s => [s, pixelInkAscent(s)]))

    const baselines = []
    for (const text of strings) {
      const canvas = await render(text, { verticalAlign: 'middle', lineHeight: 38.4 })
      baselines.push(inkTop(canvas, WIDTH * SCALE, HEIGHT * SCALE) + ascents[text])
    }

    for (const baseline of baselines) {
      expectWithin(baseline, baselines[0])
    }
  })
})
