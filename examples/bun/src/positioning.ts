/**
 * Every `positionType`, every kind of `zIndex`, and what clipping does to each.
 *
 * Overlap is the whole subject, so every cell here is a stack of boxes that
 * cover one another. A box that fails to move is a colour that fails to appear.
 */

import { Box, Root, type Children, type SceneNode } from 'meo-canvas'

import { FORMATS, draw } from './write.js'

const RED = '#dc2828'
const BLUE = '#2850dc'
const GREEN = '#288c3c'
const GOLD = '#e6aa1e'

/** A card the cells are built from. */
const card = (colour: string, offset: number, rest: Record<string, unknown> = {}): SceneNode =>
  Box({
    positionType: 'absolute',
    position: { top: offset, left: offset },
    width: 44,
    height: 34,
    backgroundColor: colour,
    ...rest,
  })

/** A panel the cards sit in. */
const panel = (children: Children, rest: Record<string, unknown> = {}): SceneNode =>
  Box({ positionType: 'relative', width: 86, height: 74, backgroundColor: '#eeeef2', children, ...rest })

const canvas = await Root({
  width: 400,
  height: 96,
  backgroundColor: '#ffffff',
  padding: 8,
  flexDirection: 'row',
  gap: 6,
  children: [
    // Paint order with no z-index: later siblings cover earlier ones.
    panel([card(RED, 4), card(BLUE, 16), card(GREEN, 28)]),
    // An explicit index overrides tree order, and a negative one sinks behind
    // the parent's background — which is what a stacking context decides.
    panel([card(RED, 4, { zIndex: 2 }), card(BLUE, 16, { zIndex: -1 }), card(GREEN, 28)]),
    // The four ways a node can be positioned. `static` ignores its inset, which
    // is why its card sits at the origin rather than at the offset it names.
    panel([
      card(RED, 10, { positionType: 'static' }),
      card(BLUE, 24, { positionType: 'relative' }),
      card(GREEN, 38, { positionType: 'sticky' }),
      card(GOLD, 4, { width: 20, height: 20 }),
    ]),
    // Clipping: the same overflowing child in a clipped panel and an unclipped
    // one, which is the only way to say the clip happened.
    panel(card(RED, 40, { width: 70, height: 60 }), { overflow: 'hidden' }),
    panel(card(RED, 40, { width: 70, height: 60 })),
    // A single inset edge, so `top` and `left` are distinguishable from a
    // shorthand that sets all four.
    panel([
      Box({ positionType: 'absolute', position: { top: 30 }, width: 44, height: 34, backgroundColor: BLUE }),
      Box({ positionType: 'absolute', position: { left: 30 }, width: 20, height: 20, backgroundColor: GREEN }),
    ]),
  ],
})

await draw('positioning', canvas, FORMATS)
