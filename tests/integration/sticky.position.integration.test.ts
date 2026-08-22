import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * `positionType: Sticky`, which Yoga has no notion of.
 *
 * A sticky node stays in the flow, and its insets are a constraint rather than an offset: it moves
 * only where the flow would put it nearer an edge of the scrollport than the inset allows. Nothing
 * scrolls here, so that clamp is the whole of what sticky does — and it is what Chrome does with no
 * scrolling ancestor either, which is what these expectations were read from.
 *
 * The difference from `Relative` is the point: relative adds both offsets unconditionally, sticky
 * adds neither unless the flow position violates them.
 */
const W = 300
const H = 200
const KID = { width: 30, height: 14 }

const isRed = (data: Uint8ClampedArray) => data[0] > 150 && data[1] < 100 && data[2] < 100

/** Where the sticky box ends up, given the space above it and the margin before it. */
async function stickyCorner(spacer: number, marginLeft: number, extra: Partial<BoxProps>) {
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
          Box({ width: W, height: spacer }),
          Box({
            positionType: Style.PositionType.Sticky,
            margin: { Left: marginLeft },
            width: KID.width,
            height: KID.height,
            backgroundColor: '#dd1111',
            ...extra,
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

describe('position sticky', () => {
  it('moves to the inset when the flow would put it nearer the edge', async () => {
    expect(await stickyCorner(5, 0, { position: { Left: 10, Top: 10 } })).toEqual([10, 10])
  })

  it('stays where the flow put it when the inset is already satisfied', async () => {
    expect(await stickyCorner(80, 50, { position: { Left: 10, Top: 10 } })).toEqual([50, 80])
  })

  it('does not move at all without insets, unlike relative', async () => {
    expect(await stickyCorner(5, 0, {})).toEqual([0, 5])
  })

  it('leaves the near edges alone when only the far ones are named', async () => {
    // `Right` is a ceiling, not a pull: it stops the box going further right than 10 from the
    // page's edge, and a box at x=0 is nowhere near that. Chrome does not move it either.
    expect(await stickyCorner(5, 0, { position: { Right: 10, Bottom: 10 } })).toEqual([0, 5])
  })

  it('holds a box back from the far edge when the flow pushes it past', async () => {
    // Flowed to x=280, which puts its right edge at 310 — past the 290 that `Right: 10` allows.
    expect(await stickyCorner(5, 280, { position: { Right: 10 } })).toEqual([W - 10 - KID.width, 5])
  })

  it('reads a percentage inset against the scrollport', async () => {
    expect(await stickyCorner(5, 0, { position: { Left: '10%', Top: '10%' } })).toEqual([30, 20])
  })

  it('reads one inset given for every edge', async () => {
    // 20 on all four sides: the left and top push it in, the right and bottom cannot pull it back
    // past them, so it lands at the near corner.
    expect(await stickyCorner(5, 0, { position: 20 })).toEqual([20, 20])
  })

  it('paints above in-flow content, as any positioned node does', async () => {
    const canvas = await Root({
      ...integrationRootBase,
      width: W,
      height: H * 2,
      workerMode: false,
      gpu: false,
      backgroundColor: '#ffffff',
      children: [
        Box({
          width: W,
          height: H * 2,
          positionType: Style.PositionType.Relative,
          children: [
            Box({ positionType: Style.PositionType.Sticky, width: W, height: H, flexShrink: 0, backgroundColor: '#dd1111' }),
            Box({ width: W, height: H, flexShrink: 0, backgroundColor: '#0066cc', transform: { translateY: -H } }),
          ],
        }),
      ],
    })
    expect(isRed(canvas.getContext('2d').getImageData(W / 2, H / 2, 1, 1).data)).toBe(true)
  })
})
