import { Root } from '@/canvas/root.canvas.js'
import { Box, Column } from '@/canvas/layout.canvas.js'
import type { BoxProps, CanvasElement } from '@/canvas/canvas.type.js'
import { Style } from '@/constant/common.const.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * How a tie between two nodes in the same layer is broken.
 *
 * CSS breaks it by tree order: of two positioned boxes whose `z-index` is `auto` or `0`, the one
 * written later paints over the one written earlier, wherever in the subtree each of them sits.
 *
 * A node lifted out of an ancestor that forms no stacking context used to sort by how deep it had
 * been found, which put it after every shallower sibling however early it came in the document. A
 * card whose first child held a positioned box one level down lost the badge declared after it: the
 * badge painted first and the box painted over it. Making that box a direct child of the card, or
 * taking its `zIndex` away, brought the badge back — the nesting was the whole of it.
 */
const PAGE = 120
const CARD = 78
const WELL = { width: 78, height: 90 }
const BADGE = { width: 20, height: 12 }

const isRed = (data: Uint8ClampedArray, i: number) => data[i] > 200 && data[i + 1] < 60 && data[i + 2] < 60

/** How many pixels of the badge survive, and the band they cover. */
async function badgePixels(first: CanvasElement, badge: CanvasElement) {
  const canvas = await Root({
    ...integrationRootBase,
    width: PAGE,
    height: PAGE + 20,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [
      Column({
        children: Box({
          width: CARD,
          positionType: Style.PositionType.Relative,
          zIndex: 0,
          overflow: Style.Overflow.Hidden,
          children: [first, badge],
        }),
      }),
    ],
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, PAGE, PAGE + 20)
  let count = 0
  for (let i = 0; i < data.length; i += 4) if (isRed(data, i)) count++
  return count
}

const badge = (extra: BoxProps = {}) =>
  Box({ positionType: Style.PositionType.Absolute, position: { Top: 0, Right: 0 }, ...BADGE, backgroundColor: '#ff0000', ...extra })

/** A box that takes part in its parent's stacking order and covers the badge's corner. */
const positionedWell = (children?: CanvasElement | CanvasElement[]) =>
  Box({
    ...WELL,
    overflow: Style.Overflow.Hidden,
    positionType: Style.PositionType.Relative,
    zIndex: 0,
    children: children ?? Box({ width: WELL.width, height: 78, backgroundColor: '#88aa88' }),
  })

/** The same box, holding a negative-index child of its own. */
const wellWithNegativeZ = () =>
  positionedWell([
    Box({ positionType: Style.PositionType.Absolute, zIndex: -1, ...WELL, backgroundColor: '#3355bb' }),
    Box({ width: WELL.width, height: 78, backgroundColor: '#88aa88' }),
  ])

const ALL_OF_IT = BADGE.width * BADGE.height

describe('two nodes in one layer, tied on zIndex', () => {
  it('paints a later badge over a positioned box nested inside an earlier sibling', async () => {
    // The reported failure: nothing of the badge survived.
    expect(await badgePixels(Box({ children: positionedWell() }), badge())).toBe(ALL_OF_IT)
  })

  it('paints it over one holding a negative-index child too', async () => {
    expect(await badgePixels(Box({ children: wellWithNegativeZ() }), badge())).toBe(ALL_OF_IT)
  })

  it('paints it over a positioned box that is a direct sibling', async () => {
    expect(await badgePixels(positionedWell(), badge())).toBe(ALL_OF_IT)
  })

  it('paints it over a plain box, which never took part in the order', async () => {
    const plain = Box({ ...WELL, overflow: Style.Overflow.Hidden, children: Box({ width: WELL.width, height: 78, backgroundColor: '#88aa88' }) })
    expect(await badgePixels(Box({ children: plain }), badge())).toBe(ALL_OF_IT)
  })

  it('lets an earlier box win when it is declared last', async () => {
    // Tree order, not nesting: the same two nodes the other way round, and the well covers it.
    const canvas = await Root({
      ...integrationRootBase,
      width: PAGE,
      height: PAGE + 20,
      workerMode: false,
      gpu: false,
      backgroundColor: '#ffffff',
      children: [
        Column({
          children: Box({
            width: CARD,
            positionType: Style.PositionType.Relative,
            zIndex: 0,
            overflow: Style.Overflow.Hidden,
            children: [badge(), Box({ children: positionedWell() })],
          }),
        }),
      ],
    })

    const { data } = canvas.getContext('2d').getImageData(0, 0, PAGE, PAGE + 20)
    let count = 0
    for (let i = 0; i < data.length; i += 4) if (isRed(data, i)) count++
    expect(count).toBe(0)
  })

  it('still lets an explicit zIndex beat tree order', async () => {
    // A nested box lifted above the badge by its own index, which is what `zIndex` is for.
    const lifted = Box({ children: positionedWell([Box({ width: WELL.width, height: 78, backgroundColor: '#88aa88' })]) })
    expect(await badgePixels(lifted, badge({ zIndex: -1 }))).toBe(0)
  })
})
