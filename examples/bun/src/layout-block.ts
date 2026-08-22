/**
 * Block layout: stacking, margins and box sizing.
 *
 * Block is the display CSS starts from and the one a flex container is not, so
 * it earns an example rather than a row in the flex one: children stack down
 * whatever their width, and a margin between two of them collapses to the
 * larger rather than summing.
 *
 * The three panels drew empty when this was written — a block container that
 * was not the page root laid out none of its children — and they draw now. The
 * cause was not layout at all: the child was painted before its own parent and
 * the parent's background covered it.
 */

import { Box, Root, type Children, type SceneNode } from 'meo-canvas'

import { FORMATS, draw } from './write.js'

const RED = '#dc2828'
const BLUE = '#2850dc'
const GREEN = '#288c3c'

/** A block of a fixed height and a stated width. */
const bar = (colour: string, width: number, rest: Record<string, unknown> = {}): SceneNode => Box({ width, height: 24, backgroundColor: colour, ...rest })

/** A block panel with a background, so an empty one is still visible. */
const panel = (children: Children): SceneNode => Box({ display: 'block', width: 180, height: 90, padding: 4, backgroundColor: '#eeeef2', children })

const canvas = await Root({
  width: 400,
  height: 110,
  backgroundColor: '#ffffff',
  padding: 8,
  flexDirection: 'row',
  gap: 6,
  children: [
    // Stacking: three blocks of different widths, each on its own line.
    panel([bar(RED, 60), bar(BLUE, 120), bar(GREEN, 90)]),
    // Margins: the middle bar is pushed down and right, and the gap between it
    // and its neighbours is its own rather than the sum of two.
    panel([bar(RED, 60), bar(BLUE, 60, { margin: { top: 8, bottom: 8, left: 30 } }), bar(GREEN, 60)]),
    // Box sizing: the same declared width, one counting its border and one not.
    panel([
      bar(RED, 100, { boxSizing: 'border-box', border: 6, borderColor: '#14141e' }),
      bar(BLUE, 100, { boxSizing: 'content-box', border: 6, borderColor: '#14141e' }),
    ]),
  ],
})

await draw('layout-block', canvas, FORMATS)
