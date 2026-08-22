import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * Whether a nested node can outrank a shallower one.
 *
 * A box that forms no stacking context does not contain its own positioned descendants: CSS gives
 * those to the nearest ancestor that does form one, so a `z-index` deep in the tree competes with a
 * shallow sibling rather than being trapped under its parent's place in the order.
 *
 * A stacking context is formed by a non-auto `z-index`, and by anything that has to composite the
 * subtree as one picture — opacity below 1, a transform, a filter, a blend mode, a mask. A
 * positioned box whose `z-index` is `auto` forms none, which is the case worth remembering.
 *
 * Each expectation is what Chrome rendered for the equivalent markup, read as pixels.
 */
const W = 300
const H = 26

/** Red is (221,17,17) and blue is (0,102,204); the page is white, so read all three channels. */
const isRed = (data: Uint8ClampedArray) => data[0] > 150 && data[1] < 100 && data[2] < 100

/** A deep child at z=999 against a shallow sibling at z=1. */
async function deepChildWins(wrapper: Partial<BoxProps>) {
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
        positionType: Style.PositionType.Relative,
        children: [
          Box({
            width: W,
            height: H,
            ...wrapper,
            children: [
              Box({
                positionType: Style.PositionType.Absolute,
                position: { Top: 0, Left: 0 },
                width: W,
                height: H,
                backgroundColor: '#dd1111',
                zIndex: 999,
              }),
            ],
          }),
          Box({
            positionType: Style.PositionType.Absolute,
            position: { Top: 0, Left: 0 },
            width: W,
            height: H,
            backgroundColor: '#0066cc',
            zIndex: 1,
          }),
        ],
      }),
    ],
  })
  return isRed(canvas.getContext('2d').getImageData(W / 2, H / 2, 1, 1).data)
}

describe('a wrapper that forms no stacking context', () => {
  it('lets a deep child out when it is static', async () => {
    expect(await deepChildWins({})).toBe(true)
  })

  it('lets a deep child out when it is relative with an auto zIndex', async () => {
    expect(await deepChildWins({ positionType: Style.PositionType.Relative })).toBe(true)
  })

  it('lets a deep child out when it is absolute with an auto zIndex', async () => {
    expect(await deepChildWins({ positionType: Style.PositionType.Absolute, position: { Top: 0, Left: 0 } })).toBe(true)
  })
})

describe('a wrapper that forms a stacking context', () => {
  it.each([
    ['a zIndex of 0', { positionType: Style.PositionType.Relative, zIndex: 0 }],
    ['a zIndex on a static box', { zIndex: 0 }],
    ['opacity below 1', { opacity: 0.99 }],
    ['a transform', { transform: { translateX: 0 } }],
    ['a filter', { filter: 'saturate(1.01)' }],
    ['a blend mode', { mixBlendMode: 'multiply' }],
  ] as const)('traps a deep child under %s', async (_label, wrapper) => {
    expect(await deepChildWins(wrapper as Partial<BoxProps>)).toBe(false)
  })
})
