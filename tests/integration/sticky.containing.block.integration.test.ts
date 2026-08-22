import { Root } from '@/canvas/root.canvas.js'
import { Box, Column } from '@/canvas/layout.canvas.js'
import type { BoxProps, CanvasElement } from '@/canvas/canvas.type.js'
import { Style } from '@/constant/common.const.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * The box a sticky node is held inside.
 *
 * CSS gives a sticky box two limits, not one. Its insets hold it inside the scrollport — the page
 * here, since nothing scrolls — and it is then constrained to its containing block, which is its
 * parent's content box. The second limit is what makes a sticky heading stop at the foot of its own
 * section rather than following the page down.
 *
 * Only the first was applied, so a node in content running past the page was dragged up to the
 * page's edge and out of the section it belongs to. Chrome, with no scrolling, leaves it where the
 * flow put it.
 */
const WIDTH = 200
const HEIGHT = 200
const NODE = 20

/** The vertical band the sticky node covers, or `null` where it is off the page. */
async function stickyBand(children: CanvasElement[]) {
  const canvas = await Root({
    ...integrationRootBase,
    width: WIDTH,
    height: HEIGHT,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Column({ children })],
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, WIDTH, HEIGHT)
  let top = Infinity
  let bottom = -Infinity
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const i = (y * WIDTH + x) * 4
      if (data[i] > 150 && data[i + 1] < 100 && data[i + 2] < 100) {
        if (y < top) top = y
        if (y > bottom) bottom = y
      }
    }
  }
  return bottom < 0 ? null : { y: top, height: bottom - top + 1 }
}

const sticky = (position: BoxProps['position']) => Box({ height: NODE, backgroundColor: '#cc2222', positionType: Style.PositionType.Sticky, position })

/** A run of page above the section under test, which does not shrink to make the content fit. */
const spacer = (height: number) => Box({ height, flexShrink: 0, backgroundColor: '#eeeeee' })

/** The section holding the sticky node, which is its containing block. */
const section = (height: number, child: CanvasElement, extra: BoxProps = {}) =>
  Box({ height, flexShrink: 0, backgroundColor: '#dddddd', ...extra, children: child })

describe('a sticky node held inside its containing block', () => {
  it('stays in its own section rather than being pulled to the page edge', async () => {
    // The section starts at 190 on a 200-tall page, so `Bottom: 0` would put the node at 180 if the
    // page were the only limit. Its section begins below that, and CSS does not let it leave.
    expect(await stickyBand([spacer(190), section(60, sticky({ Bottom: 0 }))])).toMatchObject({ y: 190 })
  })

  it('still sticks to an inset the flow put it past', async () => {
    // The other half of sticky, which the containing block must not undo: the flow puts the node at
    // 10, the inset says no nearer the top than 50, and its section is tall enough to allow it.
    expect(await stickyBand([spacer(10), section(150, sticky({ Top: 50 }))])).toMatchObject({ y: 50 })
  })

  it('stops where its section stops when the inset asks for more than the section allows', async () => {
    // The section runs 10..40, so a 20-tall node can reach 20 and no further.
    expect(await stickyBand([spacer(10), section(30, sticky({ Top: 50 }))])).toMatchObject({ y: 20 })
  })

  it('is held by the section content box, inside its padding', async () => {
    const padded = section(100, sticky({ Top: 0 }), { padding: 15 })
    expect(await stickyBand([padded])).toMatchObject({ y: 15 })
  })

  it('leaves a node below the page where the flow put it', async () => {
    // Nothing has scrolled, so a node past the fold has not been reached yet and does not stick.
    expect(await stickyBand([spacer(240), section(60, sticky({ Bottom: 0 }))])).toBeNull()
  })
})
