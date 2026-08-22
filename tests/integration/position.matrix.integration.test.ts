import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * Where a child lands, for every combination of its own `positionType` and its parent's.
 *
 * Two rules meet here and neither is obvious on its own. A static box ignores its own insets, so a
 * parent that names `Left`/`Top` without a `positionType` does not move — and its children start
 * from where it would have been anyway. An absolutely positioned box resolves against the nearest
 * positioned* ancestor, so a static parent is skipped and a relative one is not.
 *
 * Every expectation is what Chrome put there, read with `getBoundingClientRect` on the equivalent
 * markup. Geometry rather than paint order, which is why a layout query was trustworthy here where
 * a hit test would not have been.
 */
const W = 300
const H = 120
const MID = { left: 40, top: 20, width: 200, height: 60 }
const KID = { left: 10, top: 10, width: 30, height: 14 }

const POSITION = {
  static: Style.PositionType.Static,
  relative: Style.PositionType.Relative,
  absolute: Style.PositionType.Absolute,
} as const

const isRed = (data: Uint8ClampedArray) => data[0] > 150 && data[1] < 100 && data[2] < 100

/** The child's top-left corner, found by scanning for the only red box on the page. */
async function childCorner(parent: keyof typeof POSITION, child: keyof typeof POSITION) {
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
            positionType: POSITION[parent],
            position: { Left: MID.left, Top: MID.top },
            width: MID.width,
            height: MID.height,
            backgroundColor: '#bbbbbb',
            children: [
              Box({
                positionType: POSITION[child],
                position: { Left: KID.left, Top: KID.top },
                width: KID.width,
                height: KID.height,
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

describe('where a child lands, by its position and its parent’s', () => {
  it.each([
    // A static parent ignores its own insets, so it stays at the origin.
    ['static', 'static', [0, 0]],
    ['static', 'relative', [10, 10]],
    // Skips the static parent and resolves against the positioned box above it.
    ['static', 'absolute', [10, 10]],

    ['relative', 'static', [40, 20]],
    ['relative', 'relative', [50, 30]],
    ['relative', 'absolute', [50, 30]],

    ['absolute', 'static', [40, 20]],
    ['absolute', 'relative', [50, 30]],
    ['absolute', 'absolute', [50, 30]],
  ] as const)('parent %s, child %s', async (parent, child, expected) => {
    expect(await childCorner(parent, child)).toEqual([...expected])
  })
})
