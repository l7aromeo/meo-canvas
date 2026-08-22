import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { Image } from '@/canvas/image.canvas.js'
import { Grid, GridItem } from '@/canvas/grid.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { integrationRootBase, integrationFontFamily } from './helpers/integration-font.js'

/**
 * The position and stacking rules on node kinds other than `Box`.
 *
 * Everything else was checked on `Box` alone. `TextNode` and `ImageNode` extend it and override
 * only what they draw, so they should stack the same way — worth proving rather than assuming.
 *
 * `Grid` is the one with a reason to differ: it places its items by making them absolute in Yoga,
 * which makes each item a box Yoga will resolve an absolute descendant against though nothing asked
 * it to be one. CSS keeps a grid item static, and Chrome resolves past it, so the paint pass shifts
 * such a descendant back out.
 */
const IMAGE = join(dirname(fileURLToPath(import.meta.url)), '../fixtures/images/objectfit-40x20.png')
const W = 200
const H = 60

const isRed = (data: Uint8ClampedArray) => data[0] > 150 && data[1] < 100 && data[2] < 100

const anyRed = (canvas: Awaited<ReturnType<typeof Root>>, width: number, height: number) => {
  const context = canvas.getContext('2d')
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (isRed(context.getImageData(x, y, 1, 1).data)) return true
    }
  }
  return false
}

/** A box of fixed size holding `content`, against an opaque cover of the same size declared after. */
async function liftsAboveCover(content: BoxProps['children'], lift: Partial<BoxProps>) {
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
          Box({ width: W, height: H, flexShrink: 0, overflow: Style.Overflow.Hidden, ...lift, children: content }),
          Box({ width: W, height: H, flexShrink: 0, backgroundColor: '#0066cc', transform: { translateY: -H } }),
        ],
      }),
    ],
  })
  return anyRed(canvas, W, H)
}

describe('Text and Image stack like Box', () => {
  const text = [Text('XXXX', { fontFamily: integrationFontFamily, fontSize: 30, color: '#dd1111' })]
  const image = [Image({ src: IMAGE, width: 40, height: 20, color: '#dd1111' })]

  it('leaves a text box under a later sibling when it names no zIndex', async () => {
    expect(await liftsAboveCover(text, {})).toBe(false)
  })

  it('lifts a text box by its zIndex', async () => {
    expect(await liftsAboveCover(text, { zIndex: 5 })).toBe(true)
  })

  it('leaves an image box under a later sibling when it names no zIndex', async () => {
    expect(await liftsAboveCover(image, {})).toBe(false)
  })

  it('lifts an image box by its zIndex', async () => {
    expect(await liftsAboveCover(image, { zIndex: 5 })).toBe(true)
  })
})

describe('Grid', () => {
  /** Two overlapping grid items, each holding an absolute box; which colour survives. */
  async function firstItemWins(za: number | undefined, zb: number | undefined) {
    const item = (zIndex: number | undefined, colour: string) =>
      GridItem({
        children: [
          Box({
            positionType: Style.PositionType.Absolute,
            position: { Top: 0, Left: 0 },
            width: W,
            height: H,
            backgroundColor: colour,
            ...(zIndex === undefined ? {} : { zIndex }),
          }),
        ],
      })
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
          children: [Grid({ columns: 1, width: W, height: H, children: [item(za, '#dd1111'), item(zb, '#0066cc')] })],
        }),
      ],
    })
    return isRed(canvas.getContext('2d').getImageData(W / 2, H / 2, 1, 1).data)
  }

  it('orders items by zIndex, whichever came first', async () => {
    expect(await firstItemWins(2, 1)).toBe(true)
    expect(await firstItemWins(1, 2)).toBe(false)
  })

  it('gives a tie to the item declared later', async () => {
    expect(await firstItemWins(undefined, undefined)).toBe(false)
  })

  /** Where an absolute grandchild of a grid item lands. */
  async function grandchildCorner(item: Partial<BoxProps>) {
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
            Grid({
              columns: 1,
              width: W,
              height: H,
              children: [
                GridItem({
                  margin: { Top: 20 },
                  ...item,
                  children: [
                    Box({
                      positionType: Style.PositionType.Absolute,
                      position: { Top: 0, Left: 0 },
                      width: 20,
                      height: 10,
                      backgroundColor: '#dd1111',
                    }),
                  ],
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

  it('does not make a static item a containing block, though it places it absolutely', async () => {
    // The grid's own placement must not be mistaken for the caller asking for one. Chrome resolves
    // past a static grid item to the positioned box above it.
    expect(await grandchildCorner({})).toEqual([0, 0])
  })

  it('does make an item that names a positionType one', async () => {
    expect(await grandchildCorner({ positionType: Style.PositionType.Relative })).toEqual([0, 20])
  })
})
