/**
 * Images: every fit, both source kinds, position, borders and radius.
 *
 * One picture eight pixels wide and four tall, drawn into square boxes — so
 * every fit resolves to a visibly different rectangle rather than to a
 * difference a reader has to measure.
 *
 * The last two cells currently draw the same bytes as a plain one: a border and
 * a corner radius on an image node paint nothing. Left in rather than removed.
 */

import { readFile } from 'node:fs/promises'

import { Box, Image, Root, type SceneNode } from 'meo-canvas'

import { FORMATS, draw } from './write.js'

/** The picture every cell draws. */
const STRIP = '../../crates/meo-canvas/tests/assets/strip.png'

/** A clipped cell holding one image. */
const cell = (image: SceneNode): SceneNode => Box({ width: 64, height: 64, overflow: 'hidden', backgroundColor: '#eeeef2', children: image })

/** The picture at a fit, filling its cell. */
const fitted = (objectFit: 'fill' | 'contain' | 'cover' | 'none' | 'scale-down'): SceneNode => Image({ src: STRIP, width: 64, height: 64, objectFit })

const bytes = new Uint8Array(await readFile(STRIP))

const canvas = await Root({
  width: 400,
  height: 168,
  backgroundColor: '#ffffff',
  padding: 8,
  flexDirection: 'column',
  gap: 6,
  children: [
    // Every fit, in one row, so they are read against each other.
    Box({
      gap: 6,
      children: [fitted('fill'), fitted('contain'), fitted('cover'), fitted('none'), fitted('scale-down')].map(cell),
    }),
    Box({
      gap: 6,
      alignItems: 'center',
      children: [
        // The same picture from bytes rather than from a path: two source kinds
        // that must draw the same thing.
        cell(Image({ src: { bytes }, width: 64, height: 64, objectFit: 'contain' })),
        // `objectPosition` moves the picture inside its box, which is only
        // visible where the fit leaves room.
        cell(Image({ src: STRIP, width: 64, height: 64, objectFit: 'none', objectPosition: ['0%', '0%'] })),
        cell(Image({ src: STRIP, width: 64, height: 64, objectFit: 'none', objectPosition: ['100%', '100%'] })),
        // An image is a box: it should take a border and a radius like one.
        cell(Image({ src: STRIP, width: 64, height: 64, objectFit: 'cover', border: 4, borderStyle: 'solid', borderColor: '#2850dc' })),
        cell(Image({ src: STRIP, width: 64, height: 64, objectFit: 'cover', borderRadius: 20 })),
      ],
    }),
  ],
})

await draw('images', canvas, FORMATS)
