// Where a gradient's ramp has got to, at points named in pixels, read from a
// screenshot since the question is what Chrome painted. Every row carries its box,
// its point and its raw channels, so a disagreement can be re-derived.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/gradient-truth.tsv')

/** The box every case is painted in: wider than it is tall, on purpose. */
const BOX = { width: 88, height: 56 }

/**
 * How far a sample is inset from its edge, and recorded: one pixel, since a corner
 * read at the corner is half-covered by antialiasing.
 */
const INSET = 1

/**
 * The ramp every case uses: two greyscale stops, black to white, so one channel is
 * the position along it and no inverse of an interpolation is needed.
 */
const RAMP = '#000000, #ffffff'

/** Every gradient to ask about, as CSS spells it. */
const CASES = [
  ['linear 0deg', `linear-gradient(0deg, ${RAMP})`],
  ['linear 90deg', `linear-gradient(90deg, ${RAMP})`],
  ['linear 180deg', `linear-gradient(180deg, ${RAMP})`],
  ['linear 270deg', `linear-gradient(270deg, ${RAMP})`],
  ['linear 30deg', `linear-gradient(30deg, ${RAMP})`],
  ['linear to right', `linear-gradient(to right, ${RAMP})`],
  ['linear to bottom', `linear-gradient(to bottom, ${RAMP})`],
  ['radial default', `radial-gradient(${RAMP})`],
  ['radial circle', `radial-gradient(circle, ${RAMP})`],
  ['radial ellipse', `radial-gradient(ellipse, ${RAMP})`],
  ['radial at 25% 75%', `radial-gradient(at 25% 75%, ${RAMP})`],
  ['conic from 0deg', `conic-gradient(from 0deg, ${RAMP})`],
  ['conic from 90deg', `conic-gradient(from 90deg, ${RAMP})`],
  ['conic at 25% 25%', `conic-gradient(at 25% 25%, ${RAMP})`],
]

/**
 * The points each case is read at, derived from the box: corners and mid-edges,
 * since a rectangle's corners are equidistant from its centre and cannot tell a
 * circle from an ellipse.
 */
function points({ width, height }) {
  const left = INSET
  const right = width - 1 - INSET
  const top = INSET
  const bottom = height - 1 - INSET
  const midX = Math.floor(width / 2)
  const midY = Math.floor(height / 2)
  return [
    ['top-left', left, top],
    ['top-right', right, top],
    ['bottom-left', left, bottom],
    ['bottom-right', right, bottom],
    ['mid-left', left, midY],
    ['mid-right', right, midY],
    ['mid-top', midX, top],
    ['mid-bottom', midX, bottom],
    ['centre', midX, midY],
  ]
}

const browser = await open()
try {
  const rows = []
  for (const [name, css] of CASES) {
    await browser.page.setViewportSize(BOX)
    await browser.page.evaluate(
      ({ css, box }) => {
        document.body.innerHTML = ''
        const element = document.createElement('div')
        element.style.cssText = `position:absolute;left:0;top:0;width:${box.width}px;height:${box.height}px;background-image:${css};`
        document.body.append(element)
      },
      { css, box: BOX },
    )

    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))
    if (shot.width !== BOX.width || shot.height !== BOX.height) {
      throw new Error(`the screenshot is ${shot.width}x${shot.height}, not the ${BOX.width}x${BOX.height} asked for`)
    }

    for (const [point, x, y] of points(BOX)) {
      const [r, g, b] = pixel(shot, x, y)
      rows.push([name, BOX.width, BOX.height, point, x, y, r, g, b, (r / 255).toFixed(3), css].join('\t'))
    }
  }

  const header = [
    '# Chrome, through `just conformance`. Where a gradient ramp has got to.',
    '#',
    '# The ramp is #000000 to #ffffff, so `t` IS the red channel over 255 -- no',
    '# inverse of an interpolation, which is the arithmetic a gradient table must',
    '# not depend on.',
    '#',
    `# Samples are inset ${INSET} pixel from the edge they belong to, and the inset is`,
    '# recorded because a corner read at the corner is half antialiasing and one read',
    '# three pixels in is a different question. Mid-edges are here because THE FOUR',
    '# CORNERS OF A RECTANGLE ARE EQUIDISTANT FROM ITS CENTRE: a corner-only table',
    '# cannot tell a circle from an ellipse, which is half of what these cases ask.',
    '#',
    '# CHROME DITHERS ITS GRADIENTS, AND THIS TABLE ALREADY SHOWS IT: `linear',
    '# 0deg` reads 126 at mid-left and 125 at mid-right, two points that are',
    '# analytically identical on a vertical ramp, and `180deg` reads 130 against',
    '# 129. Dither is a per-pixel offset from a pattern tied to device',
    '# coordinates and to a Skia build, so it is not reproducible across',
    '# renderers and not worth matching -- our own surface does not dither by',
    '# default. A consumer of this table must therefore NEVER assert equality',
    '# between two samples at the same `t`, and must carry at least one unit of',
    '# tolerance per channel against an undithered surface. Both are measurable',
    '# in the rows below rather than taken on trust.',
    '#',
    '# case\tw\th\tpoint\tx\ty\tr\tg\tb\tt\tcss',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`gradients: ${CASES.length} cases, ${rows.length} samples -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
