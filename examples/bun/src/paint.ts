/**
 * Paint: fills, gradients, borders, shadows, compositing, transforms, masks.
 *
 * Every cell is the same box with one property changed, so a cell that looks
 * like its neighbour is a property that did nothing.
 *
 * The last row is `backgroundImage`, which was spellable here and nowhere on
 * the Rust half until `Style` grew a setter for it.
 */

import { Box, Root, type Gradient, type GradientStop, type SceneNode } from 'meo-canvas'

import { FORMATS, draw } from './write.js'

/** The side of every cell. */
const SIDE = 72

/** The two colours most cells ramp between. */
const FROM = '#2850dc'
const TO = '#f2b02c'

/** A cell of the one size every cell is. */
const cell = (rest: Record<string, unknown> = {}): SceneNode => Box({ width: SIDE, height: SIDE, backgroundColor: '#eeeef2', ...rest })

/** A ramp from one colour to another, at the ends. */
const ends: readonly GradientStop[] = [
  { offset: 0, color: FROM },
  { offset: 1, color: TO },
]

/** Three colours spread evenly, which is what a bare colour list means. */
const three: readonly GradientStop[] = [
  { offset: 0, color: FROM },
  { offset: 0.5, color: '#ffffff' },
  { offset: 1, color: TO },
]

/** The gradient a compositing cell puts behind its square. */
const backdrop: Gradient = { type: 'linear', direction: 135, stops: three }

/** A cell whose gradient gives a blend or a backdrop something to read. */
const over = (child: SceneNode): SceneNode => cell({ gradient: backdrop, children: child })

/** The square every compositing cell puts over its gradient. */
const inner = (rest: Record<string, unknown> = {}): SceneNode => Box({ width: 40, height: 40, margin: 16, backgroundColor: FROM, ...rest })

/** A cell filled edge to edge, so a mask's edge is the only edge in it. */
const filled = (rest: Record<string, unknown> = {}): SceneNode => cell({ backgroundColor: FROM, ...rest })

/** One row of cells. */
const row = (children: readonly SceneNode[]): SceneNode => Box({ gap: 8, children: [...children] })

/** The picture the background-image row paints. */
const STRIP = '../../crates/meo-canvas/tests/assets/strip.png'

/** A cell painted with the strip, under one repeat, size and offset. */
const tiled = (repeat: 'repeat' | 'no-repeat' | 'repeat-x', size: 'cover' | undefined, position: { x: number; y: number }): SceneNode =>
  cell({
    backgroundImage: { src: STRIP, repeat, ...(size === undefined ? {} : { size }), position },
  })

const canvas = await Root({
  width: 408,
  height: 568,
  backgroundColor: '#ffffff',
  padding: 8,
  flexDirection: 'column',
  gap: 8,
  children: [
    // Fills: a flat colour, then the three gradient shapes, then the one
    // direction an angle cannot say.
    row([
      cell({ backgroundColor: FROM }),
      cell({ gradient: { type: 'linear', direction: 135, stops: ends } }),
      cell({ gradient: { type: 'linear', direction: ['0%', '0%', '50%', '100%'], stops: three } }),
      cell({ gradient: { type: 'radial', at: { x: '30%', y: '30%' }, stops: ends } }),
      // `colors` rather than `stops`: this surface spaces a bare colour list
      // evenly, and the Rust half writes the same three offsets by hand. The
      // two must land on the same bytes or the spacing is this surface's own.
      cell({ gradient: { type: 'conic', from: 90, colors: [FROM, '#ffffff', TO] } }),
    ]),
    // Borders: one colour, four colours, the two dashed styles, and a radius,
    // which is the only one that changes the box's shape.
    row([
      cell({ border: 6, borderColor: FROM }),
      cell({ border: 6, borderColor: { top: FROM, right: TO, bottom: '#228844', left: '#cc4422' } }),
      cell({ border: 4, borderColor: FROM, borderStyle: 'dashed' }),
      cell({ border: 4, borderColor: FROM, borderStyle: 'dotted' }),
      cell({
        backgroundColor: FROM,
        borderRadius: { topLeft: 4, topRight: 16, bottomRight: 28, bottomLeft: 0 },
      }),
    ]),
    // Shadows: outside, inside, spread, coloured, and two at once.
    row([
      cell({ backgroundColor: FROM, boxShadow: { offsetX: 4, offsetY: 4, blur: 6 } }),
      cell({ backgroundColor: FROM, boxShadow: { inset: true, offsetX: 4, offsetY: 4, blur: 8 } }),
      cell({ backgroundColor: FROM, boxShadow: { blur: 2, spread: 6 } }),
      cell({ backgroundColor: FROM, boxShadow: { offsetY: 6, blur: 10, color: TO } }),
      cell({
        backgroundColor: FROM,
        boxShadow: [
          { offsetX: -6, blur: 4, color: TO },
          { offsetX: 6, blur: 4, color: '#228844' },
        ],
      }),
    ]),
    // Compositing: each cell holds a smaller square over a gradient, so there
    // is something behind for a blend or a backdrop to read.
    row([
      over(inner({ opacity: 0.4 })),
      over(inner({ mixBlendMode: 'multiply' })),
      over(inner({ mixBlendMode: 'difference' })),
      over(inner({ filter: 'blur(3px)' })),
      over(inner({ backgroundColor: '#ffffff40', backdropFilter: 'grayscale(1)' })),
    ]),
    // Transforms, all about the same box: rotation, scale, movement, and the
    // same rotation about a corner rather than the centre.
    row([
      over(inner({ transform: { rotate: 20 } })),
      over(inner({ transform: { scaleX: 1.6, scaleY: 0.6 } })),
      over(inner({ transform: { translateX: 10, translateY: -8 } })),
      over(inner({ transform: { rotate: 20, originX: '0%', originY: '0%' } })),
      // Dithering shows on a shallow ramp rather than a steep one.
      cell({
        dither: true,
        gradient: {
          type: 'linear',
          direction: 90,
          stops: [
            { offset: 0, color: '#303036' },
            { offset: 1, color: '#34343a' },
          ],
        },
      }),
    ]),
    // Masks: the two named shapes, a path, a gradient fade, and the same
    // gradient on a cell with a border, so a mask's effect on the border is
    // visible rather than assumed.
    row([
      filled({ mask: { shape: 'circle' } }),
      // Not square, because an ellipse inscribed in a square box IS the circle
      // beside it: on a 72 by 72 cell the two arms draw the same pixels and the
      // picture says they are one keyword.
      cell({
        children: Box({
          width: 72,
          height: 44,
          margin: { top: 14, right: 0, bottom: 0, left: 0 },
          backgroundColor: FROM,
          mask: { shape: 'ellipse' },
        }),
      }),
      filled({ mask: { path: 'M36 4 L68 68 L4 68 Z', fillRule: 'nonzero' } }),
      filled({
        mask: {
          gradient: {
            type: 'linear',
            direction: 90,
            stops: [
              { offset: 0, color: '#000000ff' },
              { offset: 1, color: '#00000000' },
            ],
          },
        },
      }),
      filled({ border: 6, borderColor: TO, mask: { shape: 'circle' } }),
    ]),
    // A background image, and the three things that travel with it. The picture
    // is eight by four, so a tile is small enough that the repeat is a pattern
    // rather than one stretched copy.
    //
    // All five cells draw the same thing today: the picture is stretched to the
    // box and the repeat, the size and the offset are ignored. Left in rather
    // than reduced to one cell — five cells that should differ and do not is the
    // showcase saying which parts work.
    row([
      tiled('repeat', undefined, { x: 0, y: 0 }),
      tiled('no-repeat', undefined, { x: 0, y: 0 }),
      tiled('repeat-x', undefined, { x: 0, y: 0 }),
      tiled('no-repeat', 'cover', { x: 0, y: 0 }),
      // The offset of the first tile, which only a repeat that does not start
      // at the corner can show.
      tiled('repeat', undefined, { x: 6, y: 10 }),
    ]),
  ],
})

await draw('paint', canvas, FORMATS)
