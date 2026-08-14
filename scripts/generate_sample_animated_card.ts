import { Root, Column, Row, Box, Text, Style, track, sequence, springDuration, mix, easings } from '../src/index.js'
import path from 'path'
import fs from 'fs'

/**
 * Renders an animated stats card: one page per frame, described entirely with animation tracks.
 *
 * Nothing here is keyframed and nothing computes its own timing. Each moving value is a `track`
 * declared once and sampled per page, so the stagger, the easing and the colour blend are the
 * library's arithmetic rather than this script's.
 */

const WIDTH = 640
const HEIGHT = 320
const FPS = 24

const SERIES = [
  { label: 'Renders', value: 0.92, color: '#38bdf8' },
  { label: 'Cache hits', value: 0.74, color: '#a78bfa' },
  { label: 'Errors', value: 0.16, color: '#fb7185' },
]

const BAR_TRACK_WIDTH = WIDTH - 64
const BAR_HEIGHT = 14
const RING_SIZE = 54

/** Each bar eases to full over its own window, the next starting a beat after the one above. */
const growth = track({ from: 0, to: 1, duration: 0.75, delay: 0.1, stagger: 0.18, ease: 'outCubic' })

/** The ring's hue is a colour blend rather than hand-rolled `hsl()` arithmetic. */
const ringColor = track({ from: '#38bdf8', to: '#f472b6', duration: 1.4, ease: 'inOutSine' })
const ringFade = track({ from: 0.35, to: 1, duration: 1.4, ease: 'inOutSine' })

/** The ring also scales in on a spring, which overshoots the way an eased curve cannot. */
const RING_SPRING = { stiffness: 190, damping: 12 }
const ringScale = track({ from: 0.6, to: 1, spring: RING_SPRING })

/**
 * The delta badge does three things in a row, which is what a sequence is for: it drops in on a
 * spring, rests long enough to be read, then slides back out.
 */
const BADGE_TRAVEL = -28
const badgeOffset = sequence({
  from: BADGE_TRAVEL,
  steps: [
    { to: 0, spring: { stiffness: 200, damping: 15 } },
    { to: 0, duration: 0.5, hold: 0.35 },
    { to: BADGE_TRAVEL, duration: 0.3, ease: 'inCubic' },
  ],
  delay: 0.35,
})
const badgeFade = sequence({
  from: 0,
  steps: [
    { to: 1, duration: 0.35 },
    { to: 1, duration: 0.85, hold: 0.35 },
    { to: 0, duration: 0.3 },
  ],
  delay: 0.35,
})

/** Long enough for every staggered bar to finish, for the spring to settle, and for the badge to leave. */
const DURATION_SECONDS = Math.max(growth.totalDuration(SERIES.length), ringColor.duration, ringScale.duration, badgeOffset.duration)

const Bar = (series: (typeof SERIES)[number], page: Parameters<typeof growth.at>[0], index: number) => {
  const filled = series.value * growth.at(page, index)

  return Column({
    gap: 6,
    children: [
      Row({
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
            // The fill deepens as it grows, blended from the track's own progress.
            backgroundColor: mix('#334155', series.color, easings.outQuad(growth.at(page, index))),
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
      children: page =>
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
                    Row({
                      gap: 10,
                      alignItems: Style.Align.Center,
                      children: [
                        Text('Weekly report', { fontSize: 26, fontWeight: 'bold', color: '#f8fafc' }),
                        Box({
                          backgroundColor: '#134e4a',
                          borderRadius: 999,
                          padding: { Left: 10, Right: 10, Top: 3, Bottom: 3 },
                          opacity: badgeFade.at(page),
                          transform: { translateY: badgeOffset.at(page) },
                          children: [Text('+12%', { fontSize: 12, fontWeight: 'bold', color: '#5eead4' })],
                        }),
                      ],
                    }),
                    Text('meo-canvas · animated card', { fontSize: 13, color: '#64748b' }),
                  ],
                }),
                Box({
                  width: RING_SIZE,
                  height: RING_SIZE,
                  borderRadius: RING_SIZE / 2,
                  border: 4,
                  borderColor: ringColor.at(page),
                  opacity: ringFade.at(page),
                  transform: { scale: ringScale.at(page) },
                }),
              ],
            }),

            Column({ gap: 16, children: SERIES.map((series, i) => Bar(series, page, i)) }),

            Text(`frame ${page.index + 1} / ${page.count}`, { fontSize: 12, color: '#475569' }),
          ],
        }),
    })

    const outDir = path.join(process.cwd(), 'samples')
    if (!fs.existsSync(outDir)) {
      fs.mkdirSync(outDir)
    }

    const gifFile = path.join(outDir, 'sample_animated_card.gif')
    await canvas.toFile(gifFile, { fps: FPS, loop: 0 })

    console.log(`Animated card generated at: ${gifFile} (${canvas.pages.length} pages, ${DURATION_SECONDS.toFixed(2)}s)`)
    console.log(`  ring spring settles after ${springDuration(RING_SPRING).toFixed(2)}s`)
  } catch (e) {
    console.error(e)
  }
})()
