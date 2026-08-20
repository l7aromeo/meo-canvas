import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

const W = 160
const H = 120

/** Midpoint of each edge of a 100x60 box at 30,30 with an 8px border. */
const MID = {
  top: [80, 33],
  right: [126, 60],
  bottom: [80, 86],
  left: [33, 60],
} as const

async function sampler(borderColor: BoxProps['borderColor'], extra: Partial<BoxProps> = {}) {
  const canvas = await Root({
    ...integrationRootBase,
    width: W,
    height: H,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [
      Box({
        width: W,
        height: H,
        alignItems: Style.Align.FlexStart,
        children: [Box({ width: 100, height: 60, margin: { Left: 30, Top: 30 }, backgroundColor: '#f8fafc', border: 8, borderColor, ...extra })],
      }),
    ],
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, W, H)
  return (x: number, y: number) => {
    const i = (y * W + x) * 4
    return `rgb(${data[i]},${data[i + 1]},${data[i + 2]})`
  }
}

describe('borderColor', () => {
  it('paints every edge when given one colour', async () => {
    const at = await sampler('#2563eb')
    for (const [x, y] of Object.values(MID)) {
      expect(at(x, y)).toBe('rgb(37,99,235)')
    }
  })

  it('paints each edge its own colour when given four', async () => {
    // `border` widths already took an edge object while every edge shared one colour, so a rule
    // down one side could not be a different colour from the rest.
    const at = await sampler({ Top: '#ff0000', Right: '#00ff00', Bottom: '#0000ff', Left: '#000000' })

    expect(at(...MID.top)).toBe('rgb(255,0,0)')
    expect(at(...MID.right)).toBe('rgb(0,255,0)')
    expect(at(...MID.bottom)).toBe('rgb(0,0,255)')
    expect(at(...MID.left)).toBe('rgb(0,0,0)')
  })

  it('falls back to black for an edge left out', async () => {
    const at = await sampler({ Left: '#ff0000' })

    expect(at(...MID.left)).toBe('rgb(255,0,0)')
    expect(at(...MID.top)).toBe('rgb(0,0,0)')
    expect(at(...MID.right)).toBe('rgb(0,0,0)')
  })

  it('splits a rounded corner between the two edges that meet there', async () => {
    // The colour has to change on the bend rather than running round it, which is the join CSS
    // makes. Drawn as one arc in one colour, half the corner would be wrong.
    const at = await sampler({ Left: '#ff0000', Top: '#00ff00', Right: '#00ff00', Bottom: '#ff0000' }, { borderRadius: 20 })

    expect(at(34, 44)).toBe('rgb(255,0,0)')
    expect(at(44, 34)).toBe('rgb(0,255,0)')
  })
})
