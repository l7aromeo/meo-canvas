import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { CanvasElement } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

const BACKDROP = '#ffcc00'
const SOURCE = '#00aaff'

/**
 * What Blink makes of the same pair, read out of its own canvas with `globalCompositeOperation`
 * set and `getImageData` — Chrome's compositor rather than the blend formulas rewritten by hand.
 */
const CHROME: Record<string, [number, number, number]> = {
  normal: [0, 170, 255],
  multiply: [0, 136, 0],
  screen: [255, 238, 255],
  overlay: [255, 221, 0],
  darken: [0, 170, 0],
  lighten: [255, 204, 255],
  'color-dodge': [255, 255, 0],
  'color-burn': [255, 179, 0],
  'hard-light': [0, 221, 255],
  'soft-light': [255, 212, 0],
  difference: [255, 34, 255],
  exclusion: [255, 102, 255],
  hue: [138, 216, 255],
  saturation: [255, 204, 0],
  color: [138, 216, 255],
  luminosity: [166, 133, 0],
}

const TOLERANCE = 1

async function colourAt(children: CanvasElement[], point: [number, number] = [20, 20]) {
  const canvas = await Root({
    ...integrationRootBase,
    width: 40,
    height: 40,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Box({ width: 40, height: 40, backgroundColor: BACKDROP, children })],
  })
  const { data } = canvas.getContext('2d').getImageData(point[0], point[1], 1, 1)
  return [data[0], data[1], data[2]] as [number, number, number]
}

describe('mixBlendMode', () => {
  it.each(Object.entries(CHROME))('matches Chrome for %s', async (mode, expected) => {
    const [r, g, b] = await colourAt([Box({ width: 40, height: 40, backgroundColor: SOURCE, mixBlendMode: mode })])

    expect(Math.abs(r - expected[0])).toBeLessThanOrEqual(TOLERANCE)
    expect(Math.abs(g - expected[1])).toBeLessThanOrEqual(TOLERANCE)
    expect(Math.abs(b - expected[2])).toBeLessThanOrEqual(TOLERANCE)
  })

  it('takes the enum and the plain string alike', async () => {
    const viaEnum = await colourAt([Box({ width: 40, height: 40, backgroundColor: SOURCE, mixBlendMode: Style.BlendMode.Multiply })])
    const viaString = await colourAt([Box({ width: 40, height: 40, backgroundColor: SOURCE, mixBlendMode: 'multiply' })])

    expect(viaEnum).toEqual(viaString)
    expect(viaEnum).toEqual(CHROME.multiply)
  })

  it('blends the subtree as one picture, not each child in turn', async () => {
    // The distinction CSS draws. Two overlapping opaque children under one blend mode blend with
    // the backdrop together: where they overlap only the topmost colour reaches the backdrop.
    // Blending each child on its own would put the first child's result into the second's backdrop
    // and land somewhere else entirely.
    const overlapping = (mixBlendMode?: string) =>
      Box({
        width: 40,
        height: 40,
        mixBlendMode,
        children: [
          Box({
            width: 30,
            height: 30,
            backgroundColor: '#00aaff',
            positionType: Style.PositionType.Absolute,
            position: { Top: 0, Left: 0 },
          }),
          Box({
            width: 30,
            height: 30,
            backgroundColor: '#ff0066',
            positionType: Style.PositionType.Absolute,
            position: { Top: 10, Left: 10 },
          }),
        ],
      })

    // In the overlap the group sees only #ff0066, so the answer is that colour multiplied with the
    // backdrop — the same as a lone square of it.
    const asGroup = await colourAt([overlapping(Style.BlendMode.Multiply)])
    const lone = await colourAt([Box({ width: 40, height: 40, backgroundColor: '#ff0066', mixBlendMode: Style.BlendMode.Multiply })])

    // Chrome multiplies #ff0066 over #ffcc00 to [255,0,0]. Asserting the value as well as the
    // agreement matters: with no blending at all both readings are #ff0066 and agree anyway.
    const CHROME_MULTIPLY = [255, 0, 0]
    expect(asGroup).toEqual(lone)
    expect(asGroup).toEqual(CHROME_MULTIPLY)
  })

  it('leaves normal as an ordinary paint', async () => {
    const blended = await colourAt([Box({ width: 40, height: 40, backgroundColor: SOURCE, mixBlendMode: Style.BlendMode.Normal })])
    const plain = await colourAt([Box({ width: 40, height: 40, backgroundColor: SOURCE })])

    expect(blended).toEqual(plain)
  })
})
