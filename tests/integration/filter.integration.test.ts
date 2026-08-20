import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { CanvasElement } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

const SOURCE = '#3366cc'

/**
 * What Chrome makes of the same filter over the same colour, read out of its own canvas with
 * `ctx.filter` set and `getImageData` — Blink's filter implementation rather than a hand
 * calculation.
 */
const CHROME: Record<string, [number, number, number]> = {
  'grayscale(1)': [99, 99, 99],
  'brightness(1.4)': [71, 142, 255],
  'hue-rotate(90deg)': [204, 62, 146],
  'invert(1)': [204, 153, 50],
  'saturate(2)': [4, 106, 255],
  'contrast(0.5)': [89, 114, 165],
  'sepia(1)': [137, 122, 95],
  'opacity(0.5)': [153, 179, 230],
}

/** Skia and Blink round the last step differently, so a channel may sit one value apart. */
const ROUNDING_TOLERANCE = 1

async function pixels(children: CanvasElement[], width = 40, height = 40, at: [number, number] = [20, 20]) {
  const canvas = await Root({
    ...integrationRootBase,
    width,
    height,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children,
  })
  const { data } = canvas.getContext('2d').getImageData(at[0], at[1], 1, 1)
  return [data[0], data[1], data[2]] as [number, number, number]
}

describe('filter', () => {
  it.each(Object.entries(CHROME))('matches Chrome for %s', async (filter, expected) => {
    const [r, g, b] = await pixels([Box({ width: 40, height: 40, backgroundColor: SOURCE, filter })])

    expect(Math.abs(r - expected[0])).toBeLessThanOrEqual(ROUNDING_TOLERANCE)
    expect(Math.abs(g - expected[1])).toBeLessThanOrEqual(ROUNDING_TOLERANCE)
    expect(Math.abs(b - expected[2])).toBeLessThanOrEqual(ROUNDING_TOLERANCE)
  })

  it('applies to descendants, not just the node itself', async () => {
    const [r, g, b] = await pixels([
      Box({ width: 40, height: 40, filter: 'grayscale(1)', children: [Box({ width: 40, height: 40, backgroundColor: SOURCE })] }),
    ])

    expect(r).toBe(g)
    expect(g).toBe(b)
  })

  it('filters the subtree once, not each child on its own', async () => {
    // The distinction CSS draws, and the one `opacity` used to get wrong. It only shows under a
    // filter that clamps: brightness(2) on the composited overlap of a half-red and a half-green
    // square is not the same as brightening each square and then compositing.
    //
    //   composite first:  (67,133,0) at .75 alpha -> x2 -> (133,255,0) -> over white -> (164,255,64)
    //   brighten first:   (255,0,0) and (0,255,0), each at .5, composited -> (128,191,64)
    //
    // Skia's rounding puts the real figure a couple of values off the arithmetic, which is still an
    // order of magnitude inside the gap between the two answers.
    const canvas = await Root({
      ...integrationRootBase,
      width: 40,
      height: 40,
      workerMode: false,
      gpu: false,
      backgroundColor: '#ffffff',
      children: [
        Box({
          width: 40,
          height: 40,
          filter: 'brightness(2)',
          children: [
            Box({
              width: 30,
              height: 30,
              backgroundColor: '#c80000',
              opacity: 0.5,
              positionType: Style.PositionType.Absolute,
              position: { Top: 0, Left: 0 },
            }),
            Box({
              width: 30,
              height: 30,
              backgroundColor: '#00c800',
              opacity: 0.5,
              positionType: Style.PositionType.Absolute,
              position: { Top: 10, Left: 10 },
            }),
          ],
        }),
      ],
    })

    const { data } = canvas.getContext('2d').getImageData(20, 20, 1, 1)
    const COMPOSITE_FIRST_RED = 164
    const FILTER_EACH_RED = 128

    expect(Math.abs(data[0] - COMPOSITE_FIRST_RED)).toBeLessThanOrEqual(3)
    expect(Math.abs(data[0] - FILTER_EACH_RED)).toBeGreaterThan(20)
  })

  it('lets a blur reach outside the node, as CSS does', async () => {
    // A filter is not clipped to the box. Drawn into an offscreen the size of the node, the blur
    // would be cut off square at the edge and nothing would fall outside it.
    const outside = await pixels(
      [Box({ width: 60, height: 60, padding: 20, children: [Box({ width: 20, height: 20, backgroundColor: '#000000', filter: 'blur(4px)' })] })],
      60,
      60,
      [22, 30],
    )

    expect(outside[0]).toBeLessThan(250)
  })

  it('runs the saturate shorthand before the chain', async () => {
    const both = await pixels([Box({ width: 40, height: 40, backgroundColor: SOURCE, saturate: 2, filter: 'grayscale(1)' } as never)])
    const chainOnly = await pixels([Box({ width: 40, height: 40, backgroundColor: SOURCE, filter: 'saturate(2) grayscale(1)' })])

    for (let channel = 0; channel < 3; channel++) {
      expect(Math.abs(both[channel] - chainOnly[channel])).toBeLessThanOrEqual(ROUNDING_TOLERANCE)
    }
  })
})
