import { Root } from '@/canvas/root.canvas.js'
import { Box, Row } from '@/canvas/layout.canvas.js'
import { Grid, GridItem } from '@/canvas/grid.canvas.js'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { Style } from '@/constant/common.const.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * `direction: RTL`, which did nothing at all before.
 *
 * Two things were in the way. Every node defaulted its own direction to `LTR` rather than
 * inheriting, so a page set to `RTL` never reached its children — Yoga was told explicitly that
 * each one was left to right. And the root's layout was calculated with `LTR` hard-coded as the
 * owner direction, which the root has none of.
 *
 * Yoga has always handled this: a 200-wide row of two 60-wide boxes puts the first at 140 under RTL
 * and at 0 under LTR, which is what these assert.
 */
const W = 200
const H = 40

const isRed = (data: Uint8ClampedArray) => data[0] > 150 && data[1] < 100 && data[2] < 100

/** Where the first box in a row starts. */
async function firstBoxX(direction: (typeof Style.Direction)[keyof typeof Style.Direction]) {
  const canvas = await Root({
    ...integrationRootBase,
    width: W,
    height: H,
    direction,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [
      Row({
        width: W,
        height: H,
        children: [Box({ width: 60, height: H, backgroundColor: '#dd1111' }), Box({ width: 60, height: H, backgroundColor: '#0066cc' })],
      }),
    ],
  })
  const context = canvas.getContext('2d')
  for (let x = 0; x < W; x++) {
    if (isRed(context.getImageData(x, H / 2, 1, 1).data)) return x
  }
  return null
}

/** Where an absolute box inset from `Start` lands. */
async function startInsetX(direction: (typeof Style.Direction)[keyof typeof Style.Direction]) {
  const canvas = await Root({
    ...integrationRootBase,
    width: W,
    height: H,
    direction,
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
            positionType: Style.PositionType.Absolute,
            position: { Start: 0, Top: 0 },
            width: 20,
            height: 10,
            backgroundColor: '#dd1111',
          }),
        ],
      }),
    ],
  })
  const context = canvas.getContext('2d')
  for (let x = 0; x < W; x++) {
    if (isRed(context.getImageData(x, 5, 1, 1).data)) return x
  }
  return null
}

describe('layout direction', () => {
  it('lays a row out left to right by default', async () => {
    expect(await firstBoxX(Style.Direction.LTR)).toBe(0)
  })

  it('reverses a row under RTL', async () => {
    expect(await firstBoxX(Style.Direction.RTL)).toBe(W - 60)
  })

  it('puts a Start inset on the left under LTR', async () => {
    expect(await startInsetX(Style.Direction.LTR)).toBe(0)
  })

  it('puts a Start inset on the right under RTL', async () => {
    expect(await startInsetX(Style.Direction.RTL)).toBe(W - 20)
  })
})

describe('the position rules under RTL', () => {
  const RED = { width: 20, height: 10, backgroundColor: '#dd1111' } as const

  /** The red box's corner, in a page laid out in `direction`. */
  async function corner(direction: (typeof Style.Direction)[keyof typeof Style.Direction], children: BoxProps['children']) {
    const canvas = await Root({
      ...integrationRootBase,
      width: 200,
      height: 60,
      direction,
      workerMode: false,
      gpu: false,
      backgroundColor: '#ffffff',
      children: [Box({ width: 200, height: 60, positionType: Style.PositionType.Relative, children })],
    })
    const context = canvas.getContext('2d')
    for (let y = 0; y < 60; y++) {
      for (let x = 0; x < 200; x++) {
        if (isRed(context.getImageData(x, y, 1, 1).data)) return [x, y]
      }
    }
    return null
  }

  const { LTR, RTL } = Style.Direction

  it('mirrors a Start inset on an absolute node', async () => {
    const kids = [Box({ positionType: Style.PositionType.Absolute, position: { Start: 10, Top: 0 }, ...RED })]
    expect(await corner(LTR, kids)).toEqual([10, 0])
    // 200 wide, 20 of box, 10 of inset.
    expect(await corner(RTL, kids)).toEqual([170, 0])
  })

  it('mirrors it against a relative containing block too', async () => {
    const kids = [
      Box({
        positionType: Style.PositionType.Relative,
        margin: { Left: 40, Top: 10 },
        width: 120,
        height: 30,
        children: [Box({ positionType: Style.PositionType.Absolute, position: { Start: 10, Top: 0 }, ...RED })],
      }),
    ]
    expect(await corner(LTR, kids)).toEqual([50, 10])
    expect(await corner(RTL, kids)).toEqual([170, 10])
  })

  it('mirrors a Start inset on a fixed node, against the page', async () => {
    const kids = [Box({ positionType: Style.PositionType.Fixed, position: { Start: 0, Top: 0 }, ...RED })]
    expect(await corner(LTR, kids)).toEqual([0, 0])
    expect(await corner(RTL, kids)).toEqual([180, 0])
  })

  it('runs grid tracks from the inline start', async () => {
    // Two equal tracks across 200. The second is on the right under LTR and on the left under RTL,
    // which Yoga cannot do for us: a grid item is placed absolutely, and an absolute offset is a
    // physical left whatever the direction says.
    const kids = [
      Grid({
        columns: 2,
        width: 200,
        height: 10,
        children: [
          GridItem({ children: [Box({ width: 100, height: 10 })] }),
          GridItem({ children: [Box({ width: 100, height: 10, backgroundColor: '#dd1111' })] }),
        ],
      }),
    ]
    expect((await corner(LTR, kids))?.[0]).toBe(100)
    expect((await corner(RTL, kids))?.[0]).toBe(0)
  })
})
