/**
 * Pages: a twelve-frame sequence, and the formats only a sequence exercises.
 *
 * Every page is built from the same function, so what moves between them is
 * what `PageInfo` reports and nothing else. The three derived numbers each
 * drive one thing: `progress` a bar that ends full, `cycle` a rotation that
 * meets itself, `index` the counter that names the page.
 *
 * It writes the still formats as well as the paged ones. What a still format
 * does with twelve pages — write the first, refuse, or write something else —
 * is a thing worth knowing rather than a thing to avoid asking.
 */

import { Box, Root, Text, type PageInfo, type SceneNode } from '@l7aromeo/meo-canvas'

import { FORMATS, PAGED_FORMATS, draw } from './write.js'

/** The family this example registers, and the file behind it. */
const FONT = {
  family: 'Showcase',
  paths: ['../../crates/meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf'],
}

/** The ink every page draws in. */
const INK = '#2850dc'

/** One page of the sequence. */
const page = (info: PageInfo): SceneNode =>
  Box({
    width: '100%',
    height: '100%',
    padding: 12,
    flexDirection: 'column',
    gap: 10,
    gradient: {
      type: 'linear',
      direction: 180,
      stops: [
        { offset: 0, color: '#f6f6fa' },
        { offset: 1, color: '#e2e2ec' },
      ],
    },
    children: [
      // The page's own name, so a reader of one frame knows which it is
      // without counting.
      Text(`${info.index + 1} / ${info.count}`, { fontFamily: FONT.family, fontSize: 14, color: '#14141e' }),
      // `progress` spans the sequence inclusively: this bar is empty on the
      // first page and exactly full on the last.
      Box({
        width: '100%',
        height: 10,
        backgroundColor: '#d0d0dc',
        children: Box({ width: `${info.progress * 100}%`, height: '100%', backgroundColor: INK }),
      }),
      // `cycle` goes round: the last page is one step short of the first rather
      // than a copy of it, so a loop does not stutter.
      Box({
        width: 36,
        height: 36,
        margin: { top: 6, right: 0, bottom: 0, left: 60 },
        backgroundColor: INK,
        transform: { rotate: info.cycle * 360 },
      }),
    ],
  })

const canvas = await Root({
  width: 200,
  height: 120,
  backgroundColor: '#ffffff',
  fps: 12,
  pages: 12,
  fonts: [FONT],
  children: page,
})

await draw('pages', canvas, [...FORMATS, ...PAGED_FORMATS])
