import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { Gradient } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

const SIZE = 100
const COLORS = ['#ff0000', '#00ff00', '#0000ff', '#ff0000']

/**
 * What Blink makes of the same sweep. CSS puts the first stop at twelve o'clock and runs clockwise,
 * which is a quarter turn from where a canvas conic gradient starts — these are the readings from
 * Chrome's canvas given that mapping.
 */
const CHROME = {
  centre: { top: [253, 1, 0], right: [60, 194, 0], bottom: [0, 125, 130], left: [66, 0, 189] },
  from90: { top: [69, 0, 186], right: [253, 1, 0] },
  offset: { top: [214, 41, 0], right: [100, 155, 0] },
}

/** Interpolation across a steep sweep lands a few values apart at a sampled point. */
const TOLERANCE = 6

async function samples(gradient: Gradient) {
  const canvas = await Root({
    ...integrationRootBase,
    width: SIZE,
    height: SIZE,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Box({ width: SIZE, height: SIZE, gradient })],
  })
  const ctx = canvas.getContext('2d')
  const at = (x: number, y: number) => {
    const { data } = ctx.getImageData(x, y, 1, 1)
    return [data[0], data[1], data[2]] as [number, number, number]
  }
  return { top: at(50, 10), right: at(90, 50), bottom: at(50, 90), left: at(10, 50) }
}

function expectNear(actual: [number, number, number], expected: number[]) {
  for (let channel = 0; channel < 3; channel++) {
    expect(Math.abs(actual[channel] - expected[channel])).toBeLessThanOrEqual(TOLERANCE)
  }
}

describe('conic gradient', () => {
  it('starts at twelve o’clock and sweeps clockwise, as CSS does', async () => {
    // The half of this worth testing is the quarter turn: a canvas conic gradient starts at three
    // o'clock, so an unmapped sweep puts the first colour on the right edge and every stop lands a
    // quarter turn early — which still looks like a gradient.
    const { top, right, bottom, left } = await samples({ type: 'conic', colors: COLORS })

    expectNear(top, CHROME.centre.top)
    expectNear(right, CHROME.centre.right)
    expectNear(bottom, CHROME.centre.bottom)
    expectNear(left, CHROME.centre.left)
  })

  it('rotates the sweep by from', async () => {
    const { top, right } = await samples({ type: 'conic', colors: COLORS, from: 90 })

    expectNear(top, CHROME.from90.top)
    expectNear(right, CHROME.from90.right)
  })

  it('turns about the point at names', async () => {
    const { top, right } = await samples({ type: 'conic', colors: COLORS, at: { x: '30%', y: '70%' } })

    expectNear(top, CHROME.offset.top)
    expectNear(right, CHROME.offset.right)
  })

  it('reads a fraction and a percentage as the same position', async () => {
    const asFraction = await samples({ type: 'conic', colors: COLORS, at: { x: 0.3, y: 0.7 } })
    const asPercentage = await samples({ type: 'conic', colors: COLORS, at: { x: '30%', y: '70%' } })

    expect(asFraction).toEqual(asPercentage)
    // And both actually moved the centre, rather than both quietly ignoring it.
    expectNear(asFraction.top, CHROME.offset.top)
  })

  it('takes the enum and the plain string alike', async () => {
    const viaEnum = await samples({ type: Style.GradientType.Conic, colors: COLORS })
    const viaString = await samples({ type: 'conic', colors: COLORS })

    expect(viaEnum).toEqual(viaString)
    expectNear(viaEnum.top, CHROME.centre.top)
  })

  it('leaves linear and radial gradients alone', async () => {
    const linear = await samples({ type: 'linear', colors: ['#ff0000', '#0000ff'], direction: 'to-bottom' })
    const radial = await samples({ type: 'radial', colors: ['#ff0000', '#0000ff'] })

    // Top red, bottom blue for the linear run; a radial one is red in the middle and blue at the
    // edges, so opposite edges read alike — to within the dither, which is on by default and moves
    // a channel by a few values.
    expect(linear.top[0]).toBeGreaterThan(200)
    expect(linear.bottom[2]).toBeGreaterThan(200)
    expectNear(radial.top, radial.bottom)
    expectNear(radial.left, radial.right)
  })
})
