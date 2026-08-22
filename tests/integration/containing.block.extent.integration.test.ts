import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Grid } from '@/canvas/grid.canvas.js'
import type { BoxProps, CanvasElement } from '@/canvas/canvas.type.js'
import { Style } from '@/constant/common.const.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * The *extent* of a containing block, not only where it starts.
 *
 * Yoga resolves an absolute node against the nearest ancestor it was told is not `Static`, which is
 * the box CSS names nearly always. Two cases part company: a `Fixed` node, whose containing block is
 * the page or whatever ancestor captured it; and anything absolute under a box a layout placed for
 * its own reasons, which CSS keeps static — a `Grid` and its items.
 *
 * Those two used to be corrected by shifting the painted origin, which lands a `Top`/`Left` node
 * correctly and nothing else: `Right` and `Bottom` are measured from the far edge, a percentage is a
 * fraction of the width or the height, and `Left` with `Right` stretches across what lies between.
 * All three came out against the box Yoga had used rather than the one CSS names, so a fixed footer
 * inside a narrow card sat at the card's right edge instead of the page's.
 */
const PAGE_WIDTH = 300
const PAGE_HEIGHT = 200
const CARD = { x: 20, y: 20, width: 100, height: 60 }
const SIZE = 20

/** The rectangle the red node covers. */
async function redBounds(tree: CanvasElement | CanvasElement[]) {
  const canvas = await Root({
    ...integrationRootBase,
    width: PAGE_WIDTH,
    height: PAGE_HEIGHT,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: tree,
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, PAGE_WIDTH, PAGE_HEIGHT)
  let left = Infinity
  let top = Infinity
  let right = -Infinity
  let bottom = -Infinity

  for (let y = 0; y < PAGE_HEIGHT; y++) {
    for (let x = 0; x < PAGE_WIDTH; x++) {
      const i = (y * PAGE_WIDTH + x) * 4
      if (data[i] > 150 && data[i + 1] < 100 && data[i + 2] < 100) {
        if (x < left) left = x
        if (x > right) right = x
        if (y < top) top = y
        if (y > bottom) bottom = y
      }
    }
  }

  return right < 0 ? null : { x: left, y: top, width: right - left + 1, height: bottom - top + 1 }
}

const TOLERANCE = 1

function expectBox(actual: Awaited<ReturnType<typeof redBounds>>, want: Partial<{ x: number; y: number; width: number; height: number }>) {
  expect(actual, 'the node did not render').not.toBeNull()
  for (const [key, value] of Object.entries(want) as [keyof typeof want, number][]) {
    expect(Math.abs(actual![key] - value), `${key} was ${actual![key]}, wanted ${value}`).toBeLessThanOrEqual(TOLERANCE)
  }
}

const red = (props: BoxProps) => Box({ backgroundColor: '#cc2222', ...props })

/** A positioned card narrower and shorter than the page, so the two boxes cannot be confused. */
const card = (child: CanvasElement, extra: BoxProps = {}) =>
  Box({
    width: CARD.width,
    height: CARD.height,
    margin: { Left: CARD.x, Top: CARD.y },
    backgroundColor: '#dddddd',
    positionType: Style.PositionType.Relative,
    ...extra,
    children: child,
  })

/** A two-track grid whose first item holds the node under test. */
const inGridItem = (child: CanvasElement) =>
  Grid({ columns: 2, gap: 10, children: [Box({ height: 40, backgroundColor: '#dddddd', children: child }), Box({ height: 40 })] })

describe('a fixed node measured against the page', () => {
  it('takes Right from the page edge, not from the box it is nested in', async () => {
    const fixed = red({ positionType: Style.PositionType.Fixed, position: { Right: 0 }, width: SIZE, height: SIZE })
    expectBox(await redBounds(card(fixed)), { x: PAGE_WIDTH - SIZE })
  })

  it('takes Bottom from the page edge', async () => {
    const fixed = red({ positionType: Style.PositionType.Fixed, position: { Bottom: 0 }, width: SIZE, height: SIZE })
    expectBox(await redBounds(card(fixed)), { y: PAGE_HEIGHT - SIZE })
  })

  it('reads a percentage inset as a fraction of the page', async () => {
    const fixed = red({ positionType: Style.PositionType.Fixed, position: { Top: '10%', Left: '10%' }, width: SIZE, height: SIZE })
    expectBox(await redBounds(card(fixed)), { x: PAGE_WIDTH * 0.1, y: PAGE_HEIGHT * 0.1 })
  })

  it('reads a percentage width as a fraction of the page', async () => {
    const fixed = red({ positionType: Style.PositionType.Fixed, position: { Top: 0, Left: 0 }, width: '50%', height: SIZE })
    expectBox(await redBounds(card(fixed)), { width: PAGE_WIDTH / 2 })
  })

  it('stretches from one page edge to the other when given both insets', async () => {
    const fixed = red({ positionType: Style.PositionType.Fixed, position: { Left: 0, Right: 0 }, height: SIZE })
    expectBox(await redBounds(card(fixed)), { x: 0, width: PAGE_WIDTH })
  })

  it('keeps its static position on an axis with no inset', async () => {
    // CSS leaves such an axis where the flow would have put it, which is inside the card.
    const fixed = red({ positionType: Style.PositionType.Fixed, position: { Right: 0 }, width: SIZE, height: SIZE })
    expectBox(await redBounds(card(fixed)), { y: CARD.y })
  })

  it('measures against the ancestor that captured it rather than the page', async () => {
    // A transform makes its node the containing block for a fixed descendant, as it does in CSS.
    const fixed = red({ positionType: Style.PositionType.Fixed, position: { Top: 0, Right: 0 }, width: SIZE, height: SIZE })
    expectBox(await redBounds(card(fixed, { transform: { scale: 1 } })), { x: CARD.x + CARD.width - SIZE, y: CARD.y })
  })
})

describe('an absolute node under a grid item', () => {
  it('takes Right from the page edge, because CSS keeps a grid item static', async () => {
    const absolute = red({ positionType: Style.PositionType.Absolute, position: { Right: 0 }, width: SIZE, height: SIZE })
    expectBox(await redBounds(inGridItem(absolute)), { x: PAGE_WIDTH - SIZE })
  })

  it('reads a percentage width as a fraction of the page', async () => {
    const absolute = red({ positionType: Style.PositionType.Absolute, position: { Top: 0, Left: 0 }, width: '50%', height: SIZE })
    expectBox(await redBounds(inGridItem(absolute)), { width: PAGE_WIDTH / 2 })
  })
})

describe('an absolute node whose containing block Yoga already had right', () => {
  it('takes Right from its positioned ancestor', async () => {
    const absolute = red({ positionType: Style.PositionType.Absolute, position: { Right: 0 }, width: SIZE, height: SIZE })
    expectBox(await redBounds(card(absolute)), { x: CARD.x + CARD.width - SIZE })
  })

  it('reads a percentage width as a fraction of that ancestor', async () => {
    const absolute = red({ positionType: Style.PositionType.Absolute, position: { Top: 0, Left: 0 }, width: '50%', height: SIZE })
    expectBox(await redBounds(card(absolute)), { width: CARD.width / 2 })
  })

  it('skips a static ancestor on its way up, as CSS does', async () => {
    const absolute = red({ positionType: Style.PositionType.Absolute, position: { Right: 0 }, width: SIZE, height: SIZE })
    const plain = Box({ width: 40, height: 30, children: absolute })
    expectBox(await redBounds(card(plain)), { x: CARD.x + CARD.width - SIZE })
  })
})
