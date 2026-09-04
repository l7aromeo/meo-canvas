// The README banners, drawn by meo-canvas.
//
// Run from a checkout:
//
//     just brand      # or: node tools/brand/banner.mjs
//
// **Drawn by the library rather than by hand, so the banner is proof rather
// than illustration.** If layout, text shaping, gradients, shadows, paths or
// the easing catalogue break, the banner breaks with them — and it exercises
// the animated-encode path at a frame count and size nothing else here does.
//
// What it shows is the thing this library is unusual for: **it computes
// motion.** Four cards, each drawing one curve from the easing catalogue as a
// stroked path, with a dot running that curve and a bar following it. The
// fourth is a spring, so the row covers a table lookup, a bezier, a step
// function and an integrated physical model — which is the whole animation
// surface in one picture.
//
// The motion is a triangle rather than a sawtooth: `t` runs 0 → 1 → 0 across
// the cycle, so the loop closes without a jump. A banner that snaps back on
// every repeat reads as a broken GIF.
//
// # Why four files
//
// GitHub honours `<picture>` and `prefers-color-scheme`; npm and crates.io
// strip `<source>` and render the `<img>`. So there is a dark one, a light one,
// and a theme-neutral one for the two registries — neutral meaning it reads on
// white and on dark, which rules out a near-white or near-black field. The
// still PNG is the first frame, for anywhere that will not animate.
//
// # Fonts
//
// No family is registered, so this draws with what the host has. The committed
// files were rendered on macOS; regenerating on another machine will set the
// type differently, which is a property of the picture rather than a defect.
// See `docs/assets/brand/README.md`.

import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { Box, Column, Path, Root, Row, Text, ease, spring } from '../../packages/meo-canvas/dist/index.js'

const HERE = dirname(fileURLToPath(import.meta.url))
const OUT = resolve(HERE, '../../docs/assets/brand')

const WIDTH = 1280
const HEIGHT = 468
/** Two seconds at 30fps. Long enough to read the motion, short enough to watch twice. */
const FPS = 30
const DURATION = 2

/** How far apart the four cards are in the loop, so they do not move in lockstep. */
const STAGGER = 0.12

/** Sample count for a curve drawn as a polyline. Sixty is smooth at this size. */
const CURVE_SAMPLES = 60

/** Four cards and three gaps fill the width inside the padding, so the row is
 * not a cluster with a third of the banner empty beside it. */
const CARD_WIDTH = (WIDTH - 56 * 2 - 16 * 3) / 4
const CARD_PADDING = 16
const PLOT_WIDTH = CARD_WIDTH - CARD_PADDING * 2
const PLOT_HEIGHT = 108
/** The curve is drawn in a square and scaled to the plot, so `d` is size-free. */
const VIEW_BOX = [0, 0, 100, 100]
/** Both paths fill the plot exactly, stacked. */
const PLOT_BOX = { positionType: 'absolute', position: { top: 0, left: 0 }, width: PLOT_WIDTH, height: PLOT_HEIGHT }

/** The travelling dot, as a circle in the curve's own coordinates. */
const dot = (x, y) => `M ${(x * 100).toFixed(2)},${y.toFixed(2)} m -3.2,0 a 3.2,3.2 0 1,0 6.4,0 a 3.2,3.2 0 1,0 -6.4,0`

/**
 * The four curves, chosen to span the kinds rather than to look different.
 *
 * A table lookup, a curve with overshoot, one that oscillates, and a spring
 * that is integrated rather than evaluated. Anything this library can animate
 * is one of those four shapes.
 */
const CARDS = [
  { name: 'outCubic', at: t => ease('outCubic', t) },
  { name: 'inOutBack', at: t => ease('inOutBack', t) },
  // `outElastic` was here and is not any more: its oscillation is dense enough
  // at this width to read as a rendering glitch rather than a curve. `outBounce`
  // covers the same family and its segments are legible at 264 pixels.
  { name: 'outBounce', at: t => ease('outBounce', t) },
  // Underdamped on purpose, so the spring is visibly a spring beside the
  // table-driven curves rather than a fourth ease-out.
  { name: 'spring', at: t => spring(t * 0.8, { stiffness: 180, damping: 12 }) },
]

/**
 * Light and dark and neutral are one drawing with the palette swapped, so the
 * three files cannot end up saying different things about the project.
 *
 * `neutral` is the one npm and crates.io show, and it is deliberately neither
 * end of the range: a panel dark enough to carry bright accents and light
 * enough not to become a hole in a white page.
 */
const THEMES = {
  dark: { field: '#0b0e14', panel: '#141926', edge: '#232a3d', ink: '#e8ecf4', muted: '#8b93a7', accent: '#f2aa4c', trace: '#3c465f' },
  light: { field: '#ffffff', panel: '#f4f6fa', edge: '#dde2ec', ink: '#111726', muted: '#5b6478', accent: '#c2701a', trace: '#c3cad8' },
  neutral: { field: '#161c2b', panel: '#1e2536', edge: '#39425c', ink: '#eef1f7', muted: '#98a1b6', accent: '#f2aa4c', trace: '#4a5470' },
}

/** The curve as SVG path data, in a 0..100 box with y already flipped. */
function trace(at) {
  const points = Array.from({ length: CURVE_SAMPLES + 1 }, (_, index) => {
    const t = index / CURVE_SAMPLES
    // Clamped for the drawing only: `outElastic` and `inOutBack` leave 0..1 by
    // design, and a path that wandered outside the box would be clipped into a
    // flat line that misrepresents the curve.
    const y = Math.max(-0.35, Math.min(1.35, at(t)))
    return `${(t * 100).toFixed(2)},${(100 - ((y + 0.35) / 1.7) * 100).toFixed(2)}`
  })
  return `M ${points.join(' L ')}`
}

/** One card: the curve, the dot on it, and a bar the same value drives. */
function card({ name, at }, phase, theme) {
  // 0 → 1 → 0 across the loop, so the repeat has no seam.
  const t = 1 - Math.abs(2 * ((phase % 1) + (phase < 0 ? 1 : 0)) - 1)
  const value = at(t)
  const dotY = 100 - ((Math.max(-0.35, Math.min(1.35, value)) + 0.35) / 1.7) * 100

  return Column({
    width: CARD_WIDTH,
    gap: 12,
    padding: CARD_PADDING,
    backgroundColor: theme.panel,
    borderRadius: 14,
    borderWidth: 1,
    borderColor: theme.edge,
    borderStyle: 'solid',
    children: [
      // **Relative, so the two paths inside have something to be absolute
      // against.** Without it they position against the page root, and a
      // `viewBox` scaled to 1280x420 draws the curve across the whole banner --
      // which is what the first render did, and what looking at it caught.
      Box({
        width: PLOT_WIDTH,
        height: PLOT_HEIGHT,
        positionType: 'relative',
        overflow: 'hidden',
        children: [
          Path({ d: trace(at), viewBox: VIEW_BOX, stroke: theme.trace, strokeWidth: 2.5, fill: 'none', ...PLOT_BOX }),
          Path({ d: dot(t, dotY), viewBox: VIEW_BOX, fill: theme.accent, ...PLOT_BOX }),
        ],
      }),
      // The bar is the same number as the dot, read a second way — which is
      // what a caller actually does with an easing value.
      Box({
        height: 6,
        borderRadius: 3,
        backgroundColor: theme.edge,
        children: [Box({ width: `${Math.max(0, Math.min(1, value)) * 100}%`, height: 6, borderRadius: 3, backgroundColor: theme.accent })],
      }),
      Text(name, { fontSize: 13, color: theme.muted, letterSpacing: 0.4 }),
    ],
  })
}

/** The whole banner at one point in the loop. */
function frame(page, theme) {
  return [
    Column({
      width: WIDTH,
      height: HEIGHT,
      padding: 56,
      gap: 34,
      backgroundColor: theme.field,
      gradient: {
        type: 'radial',
        at: { x: '18%', y: '0%' },
        stops: [
          { color: `${theme.accent}22`, offset: 0 },
          { color: `${theme.field}00`, offset: 1 },
        ],
      },
      children: [
        Column({
          gap: 10,
          children: [
            Text('meo-canvas', { fontSize: 58, fontWeight: 700, color: theme.ink, letterSpacing: -1.6 }),
            Text('Describe a layout; get image bytes back.', { fontSize: 21, color: theme.ink }),
            Text('Flexbox, CSS grid and text shaping in Rust — for Node and for Rust.', { fontSize: 16, color: theme.muted }),
          ],
        }),
        Row({
          gap: 16,
          boxShadow: { offsetX: 0, offsetY: 10, blur: 30, color: '#00000033' },
          children: CARDS.map((entry, index) => card(entry, page.cycle + index * STAGGER, theme)),
        }),
      ],
    }),
  ]
}

/** Renders one theme and writes its animated and still forms. */
async function draw(name, theme, files) {
  const canvas = await Root({ width: WIDTH, height: HEIGHT, duration: DURATION, fps: FPS, backgroundColor: theme.field, children: page => frame(page, theme) })
  const written = []
  for (const [file, format, options] of files) {
    const bytes = await canvas.toBuffer(format, options)
    writeFileSync(join(OUT, file), bytes)
    written.push(`${file} ${(bytes.length / 1024).toFixed(0)} KiB`)
  }
  canvas.release()
  process.stderr.write(`${name}: ${written.join(', ')}\n`)
  return written
}

/**
 * The drawing, separately from the command that writes it.
 *
 * Exported so a frame can be pulled at an arbitrary point in the loop without
 * re-implementing the scene beside it — which is how the seam at the wrap gets
 * checked, and how the first version's escaped paths were found. The command
 * half runs only when this file *is* the command.
 */
export { CARDS, DURATION, FPS, HEIGHT, THEMES, WIDTH, frame }

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  mkdirSync(OUT, { recursive: true })

  const animated = ['webp', { fps: FPS, loop: 0 }]
  await draw('dark', THEMES.dark, [['banner-dark.webp', ...animated]])
  await draw('light', THEMES.light, [['banner-light.webp', ...animated]])
  await draw('neutral', THEMES.neutral, [
    ['banner.webp', ...animated],
    ['banner.png', 'png', { page: 0 }],
  ])

  process.stderr.write(`${WIDTH}x${HEIGHT}, ${DURATION * FPS} frames, ${DURATION}s at ${FPS}fps\n`)
}
