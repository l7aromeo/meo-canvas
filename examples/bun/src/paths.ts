/**
 * Paths: fill rules, caps, joins, dashes, and a path drawn by stroke alone.
 *
 * Every cell is the same size and its path is written in the cell's own
 * coordinates, so a difference on the page is a difference in one property
 * rather than in where the shape was put.
 *
 * No gradient here, because this surface cannot spell one on a path: `PathPaint`
 * is a colour or `'none'`, while the scene's own paint has a gradient arm the
 * Rust half reaches. Left out rather than worked around.
 */

import { Box, Path, Root, type SceneNode } from 'meo-canvas'

import { FORMATS, draw } from './write.js'

/**
 * A five-pointed star, whose arms overlap in the middle.
 *
 * The overlap is the whole point: it is the one region the two fill rules
 * disagree about, so a reader sees the rule rather than reads about it.
 */
const STAR = 'M32 4 L40 24 L60 24 L44 36 L50 56 L32 44 L14 56 L20 36 L4 24 L24 24 Z'

/** A stroke long enough that its ends are a small part of it. */
const SEGMENT = 'M8 32 L56 32'

/** One corner, so a join has something to be drawn at. */
const CHEVRON = 'M10 52 L32 12 L54 52'

/** A cell of the one size every cell is. */
const cell = (path: SceneNode): SceneNode => Box({ width: 64, height: 64, backgroundColor: '#eeeef2', children: path })

/** The star, filled by one rule. */
const star = (fillRule: 'nonzero' | 'evenodd'): SceneNode => Path({ d: STAR, width: 64, height: 64, fill: '#2850dc', fillRule })

/** A thick horizontal segment, so a cap is a visible fraction of it. */
const capped = (lineCap: 'butt' | 'round' | 'square'): SceneNode =>
  Path({ d: SEGMENT, width: 64, height: 64, fill: 'none', stroke: '#228844', lineWidth: 14, lineCap })

/** A corner, so a join is a visible fraction of it. */
const joined = (lineJoin: 'bevel' | 'round' | 'miter'): SceneNode =>
  Path({ d: CHEVRON, width: 64, height: 64, fill: 'none', stroke: '#cc4422', lineWidth: 12, lineJoin })

/** A segment under a dash pattern. */
const dashed = (lineDash: readonly number[], lineDashOffset: number): SceneNode =>
  Path({
    d: SEGMENT,
    width: 64,
    height: 64,
    fill: 'none',
    stroke: '#111118',
    lineWidth: 6,
    lineDash,
    lineDashOffset,
  })

const canvas = await Root({
  width: 360,
  height: 300,
  backgroundColor: '#ffffff',
  padding: 8,
  flexDirection: 'column',
  gap: 6,
  children: [
    // The two fill rules on the one shape they disagree about, then the same
    // shape with no fill at all — which is the only way to see that `'none'`
    // means unpainted rather than black.
    Box({
      gap: 6,
      children: [
        cell(star('nonzero')),
        cell(star('evenodd')),
        cell(Path({ d: STAR, width: 64, height: 64, fill: 'none', stroke: '#2850dc', lineWidth: 2 })),
        // A path with neither fill nor stroke set draws its default, which SVG
        // says is black.
        cell(Path({ d: STAR, width: 64, height: 64 })),
      ],
    }),
    Box({ gap: 6, children: [cell(capped('butt')), cell(capped('round')), cell(capped('square'))] }),
    Box({ gap: 6, children: [cell(joined('bevel')), cell(joined('round')), cell(joined('miter'))] }),
    Box({
      gap: 6,
      children: [
        // Solid, the same pattern, and the same pattern begun part-way through
        // — so the offset is read against the dash it moves.
        cell(dashed([], 0)),
        cell(dashed([10, 6], 0)),
        cell(dashed([10, 6], 8)),
      ],
    }),
  ],
})

await draw('paths', canvas, FORMATS)
