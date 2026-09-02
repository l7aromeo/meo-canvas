/**
 * Grid: templates, spans, auto flow, track sizing and alignment.
 *
 * Every cell is a different colour so a track in the wrong place is visible as
 * a colour in the wrong place, rather than as a size a reader has to measure.
 */

import { Box, Grid, Root, type SceneNode } from '@l7aromeo/meo-canvas'

import { FORMATS, draw } from './write.js'

const RED = '#dc2828'
const BLUE = '#2850dc'
const GREEN = '#288c3c'
const GOLD = '#e6aa1e'

/** A filled cell. */
const cell = (colour: string, rest: Record<string, unknown> = {}): SceneNode => Box({ backgroundColor: colour, ...rest })

// Fixed tracks, so a column in the wrong place is a colour in the wrong place
// rather than a width to measure.
const fixed = Grid({
  width: 180,
  height: 80,
  gap: 4,
  gridTemplateColumns: [40, 60, '1fr'],
  gridTemplateRows: [30, '1fr'],
  children: [cell(RED), cell(BLUE), cell(GREEN), cell(GOLD), cell(RED), cell(BLUE)],
})

// A span: the first cell takes two columns, so the second row's cells sit under
// the tail of it.
const spanning = Grid({
  width: 180,
  height: 80,
  gap: 4,
  gridTemplateColumns: ['1fr', '1fr', '1fr'],
  gridTemplateRows: ['1fr', '1fr'],
  children: [cell(RED, { gridColumn: { start: 1, span: 2 } }), cell(BLUE), cell(GREEN, { gridRow: { start: 2, span: 1 } }), cell(GOLD)],
})

// Column-major auto flow: the same six cells fill downward first.
const flowed = Grid({
  width: 180,
  height: 80,
  gap: 4,
  gridAutoFlow: 'column',
  gridTemplateRows: ['1fr', '1fr'],
  gridAutoColumns: 52,
  children: [cell(RED), cell(BLUE), cell(GREEN), cell(GOLD), cell(RED), cell(BLUE)],
})

// Alignment inside the tracks: cells smaller than their cell.
const aligned = Grid({
  width: 180,
  height: 80,
  gap: 4,
  gridTemplateColumns: ['1fr', '1fr'],
  gridTemplateRows: ['1fr'],
  justifyContent: 'center',
  alignItems: 'center',
  children: [cell(RED, { width: 30, height: 20 }), cell(BLUE, { width: 30, height: 20 })],
})

const canvas = await Root({
  width: 400,
  height: 200,
  backgroundColor: '#ffffff',
  padding: 8,
  flexDirection: 'column',
  gap: 6,
  children: [Box({ gap: 6, children: [fixed, spanning] }), Box({ gap: 6, children: [flowed, aligned] })],
})

await draw('layout-grid', canvas, FORMATS)
