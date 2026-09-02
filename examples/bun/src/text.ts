/**
 * Text: size, weight, style, decoration, alignment, spacing and markup.
 *
 * One string repeated with one property changed at a time. A property that does
 * nothing is a line that looks like the one above it, which is what a showcase
 * is for: `textDecoration` and a centred or right `textAlign` both drew exactly
 * that, and both draw now. The two rows that still repeat their neighbour are
 * `textStroke` and `paintOrder`, which the binding underneath cannot express —
 * its text style carries a colour and no stroke width.
 */

import { Box, RichText, Root, Text, type SceneNode } from 'meo-canvas'

import { FORMATS, draw } from './write.js'

/** The family this example registers, and the file behind it. */
const FONT = {
  family: 'Showcase',
  paths: ['../../crates/meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf'],
}

/** The same words every line draws. */
const WORDS = 'Hxgp quick 0123'

/** One line at the family the example registers. */
const line = (text: string, rest: Record<string, unknown> = {}): SceneNode => Text(text, { fontFamily: FONT.family, fontSize: 16, color: '#14141e', ...rest })

const column = (children: readonly SceneNode[]): SceneNode =>
  Box({ width: 184, padding: 4, flexDirection: 'column', gap: 3, backgroundColor: '#f6f6f8', children: [...children] })

const canvas = await Root({
  width: 400,
  height: 300,
  backgroundColor: '#ffffff',
  padding: 8,
  gap: 6,
  fonts: [FONT],
  children: [
    column([
      line(WORDS, { fontSize: 11 }),
      line(WORDS, { fontSize: 22 }),
      line(WORDS, { fontWeight: 'bold' }),
      line(WORDS, { fontStyle: 'italic' }),
      line(WORDS, { textDecoration: 'underline' }),
      line(WORDS, { textDecoration: 'line-through' }),
      line(WORDS, { letterSpacing: 3 }),
      line(WORDS, { wordSpacing: 12 }),
    ]),
    column([
      line(WORDS, { textAlign: 'center' }),
      line(WORDS, { textAlign: 'right' }),
      line(WORDS, { lineHeight: 2 }),
      // Markup: the parser turns the tags into runs, so the bold word is one
      // segment and the coloured one another.
      line('plain <b>bold</b> <color=#dc2828>red</color>'),
      // Rich text built from runs rather than parsed, which is the other way to
      // reach the same shape.
      RichText(
        [
          { text: 'two ', style: {} },
          { text: 'runs', style: { fontWeight: 'bold' } },
        ],
        {
          fontFamily: FONT.family,
          fontSize: 16,
          color: '#14141e',
        },
      ),
      // A paragraph clamped to one line, with what replaces the rest.
      line('this line is far too long to fit in the width it is given', { maxLines: 1, ellipsis: true }),
      // A shadow and a stroke, which are paint rather than layout.
      line(WORDS, { textShadow: { offsetX: 2, offsetY: 2, blur: 2, color: '#1414288c' } }),
      line(WORDS, { fontSize: 20, color: '#ffffff', textStroke: { width: 1, color: '#14141e' } }),
      // The same stroke painted over the fill rather than under it, which is
      // only legible against the line above.
      line(WORDS, {
        fontSize: 20,
        color: '#ffffff',
        textStroke: { width: 1, color: '#14141e' },
        paintOrder: 'stroke',
      }),
      // Where a short line sits in the space its height leaves.
      line(WORDS, { lineHeight: 2, verticalAlign: 'bottom' }),
    ]),
  ],
})

await draw('text', canvas, FORMATS)
