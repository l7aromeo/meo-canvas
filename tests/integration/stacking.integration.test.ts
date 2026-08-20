import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { CanvasElement } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

const W = 200
const H = 160

/** The absolute child's box: 120x90 at { Bottom: 0, Right: 10 }, so its centre is (130, 115). */
const INSIDE_ABSOLUTE = [130, 115] as const

const RED = 'rgb(221,17,17)'
const BLUE = 'rgb(0,102,204)'

const absoluteChild = (zIndex?: number) =>
  Box({
    positionType: Style.PositionType.Absolute,
    position: { Bottom: 0, Right: 10 },
    width: 120,
    height: 90,
    backgroundColor: '#dd1111',
    ...(zIndex === undefined ? {} : { zIndex }),
  })

/** An opaque in-flow box that fills the parent, so anything painted under it is hidden. */
const inFlowSibling = () => Box({ width: W, height: H, backgroundColor: '#0066cc' })

async function colourAt(children: CanvasElement[], point: readonly [number, number] = INSIDE_ABSOLUTE) {
  const canvas = await Root({
    ...integrationRootBase,
    width: W,
    height: H,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Box({ width: W, height: H, positionType: Style.PositionType.Relative, children })],
  })

  const { data } = canvas.getContext('2d').getImageData(point[0], point[1], 1, 1)
  return `rgb(${data[0]},${data[1]},${data[2]})`
}

describe('stacking order', () => {
  it('paints an absolute child above a later sibling when it declared no zIndex', async () => {
    // The case Chrome disagreed with. An absolute child without a zIndex used to fall back into the
    // flow, so a sibling declared after it painted on top — a card that declares its background
    // decoration first lost the decoration behind its content.
    expect(await colourAt([absoluteChild(), inFlowSibling()])).toBe(RED)
  })

  it('paints an absolute child above an earlier sibling too', async () => {
    expect(await colourAt([inFlowSibling(), absoluteChild()])).toBe(RED)
  })

  it('paints an absolute child with an explicit zIndex above a later sibling', async () => {
    expect(await colourAt([absoluteChild(1), inFlowSibling()])).toBe(RED)
  })

  it('keeps a negative zIndex below in-flow content', async () => {
    expect(await colourAt([absoluteChild(-1), inFlowSibling()])).toBe(BLUE)
  })

  it('orders an unset zIndex and an explicit 0 by declaration, as CSS does for auto and 0', async () => {
    // `z-index: auto` and `z-index: 0` share one layer and paint in tree order, so whichever is
    // declared second wins — the unset one must not sort as though it had no index at all.
    const green = 'rgb(17,170,17)'
    const zeroChild = Box({
      positionType: Style.PositionType.Absolute,
      position: { Bottom: 0, Right: 10 },
      width: 120,
      height: 90,
      backgroundColor: '#11aa11',
      zIndex: 0,
    })

    expect(await colourAt([absoluteChild(), zeroChild])).toBe(green)
    expect(await colourAt([zeroChild, absoluteChild()])).toBe(RED)
  })

  it('paints a higher zIndex above a lower one regardless of declaration order', async () => {
    const under = Box({
      positionType: Style.PositionType.Absolute,
      position: { Bottom: 0, Right: 10 },
      width: 120,
      height: 90,
      backgroundColor: '#11aa11',
      zIndex: 2,
    })

    expect(await colourAt([under, absoluteChild(1)])).toBe('rgb(17,170,17)')
  })
})
