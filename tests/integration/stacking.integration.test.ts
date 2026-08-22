import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps, CanvasElement } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * Paint order among siblings, against Chrome.
 *
 * Three bands, which is CSS 2.1 Appendix E: negative `z-index` below, then in-flow content, then
 * positioned descendants carrying `z-index: auto` or `0` above it. An absolutely positioned child
 * covers a later in-flow sibling whichever order they were declared in.
 *
 * A child joins those bands when it is positioned *or* when it names a `zIndex`: `z-index` applies
 * to a flex item even where `position` is static (CSS Flexbox 5.4), and every child here is a flex
 * item. An in-flow child naming one is lifted out of the flow band and ordered against the
 * positioned ones.
 *
 * Every expectation below was read off Chrome **as rendered pixels**, not with `elementFromPoint`.
 * Hit-testing does not follow background paint order here — Appendix E puts an in-flow background
 * at layer 3 and its inline content at layer 5 — and a hit-test reading claims the opposite of what
 * the screen shows for the absolute-versus-later-sibling case. It was believed once already, and
 * it produced a wrong fix.
 */
const W = 200
const H = 160
/** Inside the band every layer covers, whichever way it got there. */
const PROBE = [100, 80] as const

const RED = 'rgb(221,17,17)'
const BLUE = 'rgb(0,102,204)'

/**
 * An in-flow layer covering the probe band.
 *
 * The parent is twice as tall as a layer, so two in-flow children lay out one above the other and
 * the second is painted back over the first with a transform. A transform rather than a negative
 * margin because it moves paint without touching layout: a margin has to fight `flexShrink` and
 * the column's own sizing, and got the overlap wrong twice before this.
 */
const flow = (colour: string, props: Partial<BoxProps> = {}, first = false) =>
  Box({
    width: W,
    height: H,
    flexShrink: 0,
    backgroundColor: colour,
    ...(first ? {} : { transform: { translateY: -H } }),
    ...props,
  })

const absolute = (colour: string, props: Partial<BoxProps> = {}) =>
  Box({
    positionType: Style.PositionType.Absolute,
    position: { Top: 0, Left: 0 },
    width: W,
    height: H,
    backgroundColor: colour,
    ...props,
  })

async function colourAt(children: CanvasElement[]) {
  const canvas = await Root({
    ...integrationRootBase,
    width: W,
    height: H * 2,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    // Relative, so it is the containing block for the absolute children below. An unpositioned
    // box is static now, and an absolute child would resolve against the root instead.
    children: [Box({ width: W, height: H * 2, positionType: Style.PositionType.Relative, children })],
  })
  const { data } = canvas.getContext('2d').getImageData(PROBE[0], PROBE[1], 1, 1)
  return `rgb(${data[0]},${data[1]},${data[2]})`
}

describe('stacking order — in-flow children', () => {
  it('lifts an in-flow child naming a zIndex above a later sibling', async () => {
    expect(await colourAt([flow('#dd1111', { zIndex: 5 }, true), flow('#0066cc')])).toBe(RED)
  })

  it('drops an in-flow child naming a negative zIndex below a sibling declared before it', async () => {
    expect(await colourAt([flow('#dd1111', {}, true), flow('#0066cc', { zIndex: -1 })])).toBe(RED)
  })

  it('lifts an in-flow child naming an explicit zIndex of 0', async () => {
    // Not the same as `auto`. For a flex item an explicit `0` creates a stacking context where
    // `auto` does not, so this one rises above the later sibling and the next test's does not.
    expect(await colourAt([flow('#dd1111', { zIndex: 0 }, true), flow('#0066cc')])).toBe(RED)
  })

  it('leaves two unindexed in-flow children to document order', async () => {
    expect(await colourAt([flow('#dd1111', {}, true), flow('#0066cc')])).toBe(BLUE)
  })
})

describe('stacking order — positioned children', () => {
  it('paints a higher zIndex above a lower one whatever the order', async () => {
    expect(await colourAt([absolute('#0066cc', { zIndex: 999 }), absolute('#dd1111', { zIndex: 1 })])).toBe(BLUE)
    expect(await colourAt([absolute('#dd1111', { zIndex: 1 }), absolute('#0066cc', { zIndex: 999 })])).toBe(BLUE)
  })

  it('orders an unset zIndex and an explicit 0 by declaration', async () => {
    expect(await colourAt([absolute('#dd1111'), absolute('#0066cc', { zIndex: 0 })])).toBe(BLUE)
    expect(await colourAt([absolute('#0066cc', { zIndex: 0 }), absolute('#dd1111')])).toBe(RED)
  })
})

describe('stacking order — an explicit positionType', () => {
  // CSS `relative` is positioned, so it joins the band above the flow exactly as `absolute` does.
  // An ordinary child names no positionType at all and stays in the flow; Yoga's default is
  // `Relative`, but a child that never asked for one is CSS `static`.
  const relative = (colour: string, props: Partial<BoxProps> = {}, first = false) =>
    flow(colour, { positionType: Style.PositionType.Relative, ...props }, first)

  it('leaves an explicit Static child in the flow, exactly as naming nothing does', async () => {
    const staticFirst = flow('#dd1111', { positionType: Style.PositionType.Static }, true)
    expect(await colourAt([staticFirst, flow('#0066cc')])).toBe(BLUE)
  })

  it('lifts a relative child above a later in-flow sibling', async () => {
    expect(await colourAt([relative('#dd1111', {}, true), flow('#0066cc')])).toBe(RED)
  })

  it('lifts a relative child above an earlier in-flow sibling too', async () => {
    expect(await colourAt([flow('#0066cc', {}, true), relative('#dd1111')])).toBe(RED)
  })

  it('drops a relative child naming a negative zIndex below the flow', async () => {
    expect(await colourAt([relative('#dd1111', { zIndex: -1 }, true), flow('#0066cc')])).toBe(BLUE)
  })

  it('orders a relative and an absolute child by declaration when neither names a zIndex', async () => {
    expect(await colourAt([relative('#dd1111', {}, true), absolute('#0066cc')])).toBe(BLUE)
    expect(await colourAt([absolute('#0066cc'), relative('#dd1111', {}, true)])).toBe(RED)
  })
})

describe('stacking order — positioned against in-flow', () => {
  it('paints an absolute child above a later in-flow sibling, with no zIndex named', async () => {
    expect(await colourAt([absolute('#dd1111'), flow('#0066cc', {}, true)])).toBe(RED)
  })

  it('does the same when the absolute child names zIndex 0', async () => {
    expect(await colourAt([absolute('#dd1111', { zIndex: 0 }), flow('#0066cc', {}, true)])).toBe(RED)
  })

  it('paints an absolute child above an earlier in-flow sibling too', async () => {
    expect(await colourAt([flow('#0066cc', {}, true), absolute('#dd1111')])).toBe(RED)
  })

  it('keeps a negative absolute child below in-flow content', async () => {
    expect(await colourAt([absolute('#dd1111', { zIndex: -1 }), flow('#0066cc', {}, true)])).toBe(BLUE)
  })

  it('lets an in-flow child outrank a later absolute one by naming a higher zIndex', async () => {
    expect(await colourAt([flow('#dd1111', { zIndex: 1 }, true), absolute('#0066cc')])).toBe(RED)
  })
})

/**
 * The whole matrix, one assertion per cell.
 *
 * Each row is a child under test declared *first*, against a plain in-flow sibling declared after
 * it. `RED` means the child under test paints on top, `BLUE` means the later plain sibling does.
 * Every expectation is what Chrome rendered for the equivalent markup, read as pixels.
 *
 * Chrome was measured for `display: block`, `flex` and `grid`. Flex and grid agree on all twelve
 * cells and are what this table encodes; block differs in one place -- it ignores `z-index` on a
 * static child -- and is unreachable here, since Yoga has no block layout and `Grid` is flex
 * underneath.
 */
describe('stacking order — every position against every zIndex', () => {
  const cell = (position: 'static' | 'relative' | 'absolute', zIndex: number | undefined) => {
    const props: Partial<BoxProps> = zIndex === undefined ? {} : { zIndex }
    if (position === 'relative') props.positionType = Style.PositionType.Relative
    if (position === 'static') props.positionType = Style.PositionType.Static
    return position === 'absolute' ? absolute('#dd1111', props) : flow('#dd1111', props, true)
  }

  it.each([
    ['static', undefined, BLUE],
    ['static', -1, BLUE],
    ['static', 0, RED],
    ['static', 2, RED],
    ['relative', undefined, RED],
    ['relative', -1, BLUE],
    ['relative', 0, RED],
    ['relative', 2, RED],
    ['absolute', undefined, RED],
    ['absolute', -1, BLUE],
    ['absolute', 0, RED],
    ['absolute', 2, RED],
  ] as const)('%s with zIndex %s against a later plain sibling', async (position, zIndex, expected) => {
    const under = cell(position, zIndex)
    const sibling = position === 'absolute' ? flow('#0066cc', {}, true) : flow('#0066cc')
    expect(await colourAt([under, sibling])).toBe(expected)
  })
})

/**
 * The same cells with the plain sibling declared *first*, so declaration order cannot be what
 * decides. A cell that reads RED above and RED here is winning on its own properties.
 */
describe('stacking order — every position against every zIndex, declared second', () => {
  it.each([
    // The one cell order changes: two plain in-flow boxes tie, and the later one wins.
    ['static', undefined, RED],
    ['static', -1, BLUE],
    ['static', 0, RED],
    ['static', 2, RED],
    ['relative', undefined, RED],
    ['relative', -1, BLUE],
    ['relative', 0, RED],
    ['relative', 2, RED],
    ['absolute', undefined, RED],
    ['absolute', -1, BLUE],
    ['absolute', 0, RED],
    ['absolute', 2, RED],
  ] as const)('%s with zIndex %s declared after a plain sibling', async (position, zIndex, expected) => {
    const props: Partial<BoxProps> = zIndex === undefined ? {} : { zIndex }
    if (position === 'relative') props.positionType = Style.PositionType.Relative
    if (position === 'static') props.positionType = Style.PositionType.Static
    const under = position === 'absolute' ? absolute('#dd1111', props) : flow('#dd1111', props)
    expect(await colourAt([flow('#0066cc', {}, true), under])).toBe(expected)
  })
})
