import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * Which ancestor an absolutely positioned node is placed against.
 *
 * CSS resolves it against the nearest *positioned* ancestor, skipping every static box between.
 * Yoga's own default position type is `Relative`, which makes every ancestor a containing block —
 * so an absolute node used to land against its immediate parent wherever CSS would have gone
 * further up. Yoga offers `Static` for exactly this, and an unpositioned node is given it.
 */
const W = 200
const H = 100
const INSET = 40

/** The top-left corner of the only red box on the page. */
async function redCornerOf(middleProps: Partial<BoxProps>) {
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
        children: [
          Box({
            width: 120,
            height: 60,
            margin: { Left: INSET, Top: INSET },
            ...middleProps,
            children: [
              Box({
                positionType: Style.PositionType.Absolute,
                position: { Top: 0, Left: 0 },
                width: 30,
                height: 20,
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
      const [r, , b] = context.getImageData(x, y, 1, 1).data
      if (r > 150 && b < 100) return [x, y]
    }
  }
  return null
}

describe('absolute containing block', () => {
  it('skips a middle box that named no positionType', async () => {
    // Static, so the absolute node resolves against the root and lands in its corner.
    expect(await redCornerOf({})).toEqual([0, 0])
  })

  it('skips a middle box that named Static explicitly', async () => {
    expect(await redCornerOf({ positionType: Style.PositionType.Static })).toEqual([0, 0])
  })

  it('stops at a relative middle box', async () => {
    expect(await redCornerOf({ positionType: Style.PositionType.Relative })).toEqual([INSET, INSET])
  })

  it('stops at an absolute middle box', async () => {
    // Inset rather than margined, so the box still lands at INSET and the child's corner is
    // comparable with the relative case above.
    expect(
      await redCornerOf({
        positionType: Style.PositionType.Absolute,
        position: { Top: INSET, Left: INSET },
        margin: { Left: 0, Top: 0 },
      }),
    ).toEqual([INSET, INSET])
  })
})
