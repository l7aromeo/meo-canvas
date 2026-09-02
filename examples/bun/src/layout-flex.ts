/**
 * Flexbox: every direction, both wraps, and each way of distributing a line.
 *
 * Rows of coloured blocks rather than a picture, because the question each row
 * answers is *where things sit*. A block that fails to move is visible against
 * its neighbours; a prettier scene would hide it.
 */

import { Box, Root, Row, type Children, type SceneNode } from 'meo-canvas'

import { FORMATS, draw } from './write.js'

const RED = '#dc2828'
const BLUE = '#2850dc'
const GREEN = '#288c3c'

/** One coloured block of a fixed size. */
const block = (colour: string, width: number, height = 18): SceneNode => Box({ width, height, backgroundColor: colour })

/** A strip with a background, so an empty row is still visible. */
const strip = (children: Children, rest: Record<string, unknown> = {}): SceneNode =>
  Row({ width: 180, height: 26, padding: 4, gap: 4, backgroundColor: '#eeeef2', children, ...rest })

const canvas = await Root({
  width: 400,
  height: 372,
  backgroundColor: '#ffffff',
  padding: 8,
  flexDirection: 'column',
  gap: 6,
  children: [
    // Direction: the same three blocks, read two ways.
    strip([block(RED, 30), block(BLUE, 30), block(GREEN, 30)]),
    strip([block(RED, 30), block(BLUE, 30), block(GREEN, 30)], { flexDirection: 'row-reverse' }),
    // Justify: where the free space goes along the main axis.
    strip([block(RED, 24), block(BLUE, 24)], { justifyContent: 'space-between' }),
    strip([block(RED, 24), block(BLUE, 24)], { justifyContent: 'center' }),
    strip([block(RED, 24), block(BLUE, 24)], { justifyContent: 'space-evenly' }),
    // Align: where a shorter item sits across the line.
    strip([block(RED, 24, 8), block(BLUE, 24, 18)], { height: 30, alignItems: 'flex-end' }),
    strip([block(RED, 24, 8), block(BLUE, 24, 18)], { height: 30, alignItems: 'center' }),
    // Grow, shrink and basis: the three ways a length is negotiated.
    strip([
      Box({ width: 20, height: 18, backgroundColor: RED, flexGrow: 1 }),
      block(BLUE, 20),
      Box({ width: 20, height: 18, backgroundColor: GREEN, flexGrow: 2 }),
    ]),
    strip([Box({ width: 200, height: 18, backgroundColor: RED, flexShrink: 1 }), Box({ width: 200, height: 18, backgroundColor: BLUE, flexShrink: 3 })]),
    // Wrap: a line that cannot hold its children.
    strip([block(RED, 60), block(BLUE, 60), block(GREEN, 60)], { height: 52, flexWrap: 'wrap' }),
    // Aspect ratio: a height derived from a width.
    Row({
      width: 180,
      height: 26,
      backgroundColor: '#eeeef2',
      flexDirection: 'column',
      children: Box({ width: 48, aspectRatio: 3, backgroundColor: GREEN }),
    }),
  ],
})

await draw('layout-flex', canvas, FORMATS)
