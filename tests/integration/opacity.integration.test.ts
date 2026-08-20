import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import { integrationRootBase } from './helpers/integration-font.js'

const W = 220
const H = 120

/**
 * What CSS produces for the same markup.
 *
 * `opacity` composites the subtree once and fades the result, so two overlapping opaque children
 * inside a half-transparent parent are exactly as dark as one of them. Fading each draw on its own
 * instead compounds them, and Chrome shows one flat colour where that would show a darker band.
 */
const HALF_RED_ON_WHITE = 127
const QUARTER_RED_ON_WHITE = 191
const TOLERANCE = 2

async function sample(children: unknown[], opacity: number, width = W, height = H) {
  const canvas = await Root({
    ...integrationRootBase,
    width,
    height,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Box({ width, height, opacity, children: children as never })],
  })
  const { data } = canvas.getContext('2d').getImageData(0, 0, width, height)
  return (x: number, y: number) => {
    const i = (y * width + x) * 4
    return { r: data[i], g: data[i + 1], b: data[i + 2] }
  }
}

const overlappingPair = [
  Box({ width: 100, height: 60, backgroundColor: '#ff0000', positionType: Style.PositionType.Absolute, position: { Left: 20, Top: 20 } }),
  Box({ width: 100, height: 60, backgroundColor: '#ff0000', positionType: Style.PositionType.Absolute, position: { Left: 80, Top: 20 } }),
]

describe('opacity', () => {
  it('fades the subtree as one, so overlapping children do not darken each other', async () => {
    const at = await sample(overlappingPair, 0.5)

    const single = at(40, 50)
    const overlap = at(110, 50)

    expect(Math.abs(single.g - HALF_RED_ON_WHITE)).toBeLessThanOrEqual(TOLERANCE)
    // The whole point: this used to read 63, twice as opaque as the rest of the shape.
    expect(Math.abs(overlap.g - HALF_RED_ON_WHITE)).toBeLessThanOrEqual(TOLERANCE)
    expect(Math.abs(overlap.g - single.g)).toBeLessThanOrEqual(TOLERANCE)
  })

  it('multiplies through nesting, as nested faded groups do in CSS', async () => {
    const canvas = await Root({
      ...integrationRootBase,
      width: 120,
      height: 80,
      workerMode: false,
      gpu: false,
      backgroundColor: '#ffffff',
      children: [Box({ width: 120, height: 80, opacity: 0.5, children: [Box({ width: 120, height: 80, opacity: 0.5, backgroundColor: '#ff0000' })] })],
    })

    const { data } = canvas.getContext('2d').getImageData(0, 0, 120, 80)
    expect(Math.abs(data[1] - QUARTER_RED_ON_WHITE)).toBeLessThanOrEqual(TOLERANCE)
  })

  it('leaves a fully opaque node exactly as it was', async () => {
    const at = await sample(overlappingPair, 1)
    expect(at(110, 50)).toEqual({ r: 255, g: 0, b: 0 })
  })
})
