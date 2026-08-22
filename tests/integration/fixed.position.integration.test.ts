import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * `positionType: Fixed`, which Yoga has no notion of.
 *
 * CSS resolves a fixed node against the viewport rather than the nearest positioned ancestor. There
 * is no scrolling viewport here, so what it buys over `Absolute` is reaching past every positioned
 * ancestor in one step — and being captured by a transform or a filter, which CSS makes the
 * containing block for fixed descendants. Both halves are what Chrome does, read as pixels.
 *
 * Yoga lays the node out as `Absolute`, against whatever ancestor it would use; the paint pass then
 * shifts it by the distance between that box and its real containing block, which places it without
 * re-resolving its insets by hand.
 */
const W = 300
const H = 120
const MID = { left: 40, top: 20 }

const isRed = (data: Uint8ClampedArray) => data[0] > 150 && data[1] < 100 && data[2] < 100

async function fixedCorner(middle: Partial<BoxProps>) {
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
            positionType: Style.PositionType.Relative,
            position: { Left: MID.left, Top: MID.top },
            width: 200,
            height: 60,
            backgroundColor: '#bbbbbb',
            ...middle,
            children: [
              Box({
                positionType: Style.PositionType.Fixed,
                position: { Top: 0, Left: 0 },
                width: 30,
                height: 14,
                backgroundColor: '#dd1111',
              }),
            ],
          }),
        ],
      }),
    ],
  })

  const context = canvas.getContext('2d')
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      if (isRed(context.getImageData(x, y, 1, 1).data)) return [x, y]
    }
  }
  return null
}

describe('position fixed', () => {
  it('reaches past a positioned ancestor to the page', async () => {
    expect(await fixedCorner({})).toEqual([0, 0])
  })

  it('is captured by an ancestor carrying a transform', async () => {
    expect(await fixedCorner({ transform: { translateX: 0 } })).toEqual([MID.left, MID.top])
  })

  it('is captured by an ancestor carrying a filter', async () => {
    expect(await fixedCorner({ filter: 'saturate(1.01)' })).toEqual([MID.left, MID.top])
  })

  it('is captured by an ancestor carrying a backdrop filter', async () => {
    expect(await fixedCorner({ backdropFilter: 'blur(1px)' })).toEqual([MID.left, MID.top])
  })

  it('paints above in-flow content, as any positioned node does', async () => {
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
              positionType: Style.PositionType.Fixed,
              position: { Top: 0, Left: 0 },
              width: W,
              height: H,
              backgroundColor: '#dd1111',
            }),
            Box({ width: W, height: H, backgroundColor: '#0066cc' }),
          ],
        }),
      ],
    })
    expect(isRed(canvas.getContext('2d').getImageData(W / 2, H / 2, 1, 1).data)).toBe(true)
  })
})

describe('position fixed against overflow and zIndex', () => {
  /** Two fixed nodes captured by a transformed row, so both land in the same place to be compared. */
  async function firstWins(za: number | undefined, zb: number | undefined) {
    const box = (zIndex: number | undefined, colour: string) =>
      Box({
        positionType: Style.PositionType.Fixed,
        position: { Top: 0, Left: 0 },
        width: W,
        height: 40,
        backgroundColor: colour,
        ...(zIndex === undefined ? {} : { zIndex }),
      })
    const canvas = await Root({
      ...integrationRootBase,
      width: W,
      height: 40,
      workerMode: false,
      gpu: false,
      backgroundColor: '#ffffff',
      children: [Box({ width: W, height: 40, transform: { translateX: 0 }, children: [box(za, '#dd1111'), box(zb, '#0066cc')] })],
    })
    return isRed(canvas.getContext('2d').getImageData(W / 2, 20, 1, 1).data)
  }

  it('orders two fixed nodes by zIndex, whichever came first', async () => {
    expect(await firstWins(2, 1)).toBe(true)
    expect(await firstWins(1, 2)).toBe(false)
  })

  it('gives a tie to the one declared later', async () => {
    expect(await firstWins(undefined, undefined)).toBe(false)
  })

  it('puts a negative zIndex below an auto one', async () => {
    expect(await firstWins(-1, undefined)).toBe(false)
  })

  /** Whether a fixed node survives a clipping ancestor of the given kind. */
  async function escapesClip(clipper: Partial<BoxProps>) {
    const canvas = await Root({
      ...integrationRootBase,
      width: W,
      height: 40,
      workerMode: false,
      gpu: false,
      backgroundColor: '#ffffff',
      children: [
        Box({
          width: W,
          height: 40,
          positionType: Style.PositionType.Relative,
          children: [
            Box({
              width: 100,
              height: 40,
              overflow: Style.Overflow.Hidden,
              backgroundColor: '#bbbbbb',
              ...clipper,
              children: [
                Box({
                  positionType: Style.PositionType.Fixed,
                  position: { Top: 0, Left: 120 },
                  width: 60,
                  height: 40,
                  backgroundColor: '#dd1111',
                }),
              ],
            }),
          ],
        }),
      ],
    })
    return isRed(canvas.getContext('2d').getImageData(150, 20, 1, 1).data)
  }

  it('is not cut by a clipper that does not contain it', async () => {
    // Neither a static nor a relative box is a fixed node's containing block, so neither cuts it —
    // where either would cut an absolute one.
    expect(await escapesClip({})).toBe(true)
    expect(await escapesClip({ positionType: Style.PositionType.Relative })).toBe(true)
  })

  it('is cut by the transformed ancestor that captures it', async () => {
    expect(await escapesClip({ transform: { translateX: 0 } })).toBe(false)
  })
})
