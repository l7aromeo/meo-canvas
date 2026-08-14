import { Root, Column, Row, Box, Text, Style } from '../src/index.js'
import path from 'path'
import fs from 'fs'

/**
 * Renders an animated stats card: one page per frame, driven entirely by the page's `progress`.
 *
 * Nothing here is keyframed. Every moving value is a function of `progress`, which is what a page
 * builder is for — the sequence is described once and the renderer runs it per page.
 */

const WIDTH = 640
const HEIGHT = 320
const DURATION_SECONDS = 2
const FPS = 24

/** Bars grow to these fractions of the track, in order. */
const SERIES = [
  { label: 'Renders', value: 0.92, color: '#38bdf8' },
  { label: 'Cache hits', value: 0.74, color: '#a78bfa' },
  { label: 'Errors', value: 0.16, color: '#fb7185' },
]

/** Each bar starts a little after the one above it, so the row staggers in rather than moving as a block. */
const STAGGER = 0.12

/** Fraction of the sequence a single bar spends growing, leaving the tail of the animation settled. */
const GROW_SPAN = 0.55

/** Bar track spans the card, less the padding either side and the value column's own width. */
const BAR_TRACK_WIDTH = WIDTH - 64
const BAR_HEIGHT = 14

const RING_SIZE = 54
/** The ring sweeps from cyan through violet across the sequence. */
const RING_HUE_START = 200
const RING_HUE_SWEEP = 120

/** Smoothstep: eases in and out, so bars arrive without the dead stop a linear ramp gives. */
const ease = (t: number): number => {
  const clamped = Math.min(1, Math.max(0, t))
  return clamped * clamped * (3 - 2 * clamped)
}

/** Progress of one bar at a given point in the sequence, accounting for its stagger. */
const barProgress = (progress: number, index: number): number => ease((progress - index * STAGGER) / GROW_SPAN)

const Bar = (series: (typeof SERIES)[number], progress: number, index: number) => {
  const grown = barProgress(progress, index)
  const filled = series.value * grown

  return Column({
    gap: 6,
    children: [
      Row({
        // Width is explicit: a row sized to its content has no free space for SPACE_BETWEEN to
        // distribute, and the label and value end up touching.
        width: BAR_TRACK_WIDTH,
        justifyContent: Style.Justify.SpaceBetween,
        alignItems: Style.Align.Center,
        children: [
          Text(series.label, { fontSize: 15, color: '#cbd5f5' }),
          Text(`${Math.round(filled * 100)}%`, { fontSize: 15, fontWeight: 'bold', color: series.color }),
        ],
      }),
      Box({
        width: BAR_TRACK_WIDTH,
        height: BAR_HEIGHT,
        backgroundColor: '#1e293b',
        borderRadius: BAR_HEIGHT / 2,
        children: [
          Box({
            width: Math.max(BAR_HEIGHT, BAR_TRACK_WIDTH * filled),
            height: BAR_HEIGHT,
            backgroundColor: series.color,
            borderRadius: BAR_HEIGHT / 2,
          }),
        ],
      }),
    ],
  })
}

void (async () => {
  try {
    const canvas = await Root({
      width: WIDTH,
      height: HEIGHT,
      workerMode: false,
      duration: DURATION_SECONDS,
      fps: FPS,
      backgroundColor: '#0b1120',
      padding: 32,
      children: ({ progress, index, count }) =>
        Column({
          width: '100%',
          gap: 22,
          children: [
            Row({
              width: '100%',
              justifyContent: Style.Justify.SpaceBetween,
              alignItems: Style.Align.Center,
              children: [
                Column({
                  gap: 4,
                  children: [
                    Text('Weekly report', { fontSize: 26, fontWeight: 'bold', color: '#f8fafc' }),
                    Text('meo-canvas · animated card', { fontSize: 13, color: '#64748b' }),
                  ],
                }),
                // A ring that sweeps once through the sequence, so the header carries the timeline too.
                Box({
                  width: RING_SIZE,
                  height: RING_SIZE,
                  borderRadius: RING_SIZE / 2,
                  border: 4,
                  borderColor: `hsl(${Math.round(RING_HUE_START + ease(progress) * RING_HUE_SWEEP)}, 85%, 62%)`,
                  opacity: 0.35 + ease(progress) * 0.65,
                }),
              ],
            }),

            Column({ gap: 16, children: SERIES.map((series, i) => Bar(series, progress, i)) }),

            Text(`frame ${index + 1} / ${count}`, { fontSize: 12, color: '#475569' }),
          ],
        }),
    })

    const outDir = path.join(process.cwd(), 'samples')
    if (!fs.existsSync(outDir)) {
      fs.mkdirSync(outDir)
    }

    const gifFile = path.join(outDir, 'sample_animated_card.gif')
    await canvas.toFile(gifFile, { fps: FPS, loop: 0 })

    console.log(`Animated card generated at: ${gifFile} (${canvas.pages.length} pages)`)
  } catch (e) {
    console.error(e)
  }
})()
