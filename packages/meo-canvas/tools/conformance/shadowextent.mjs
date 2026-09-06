// How far an outer box-shadow's ink reaches, and in which directions.
//
// The companion to `boxshadow.mjs`, which asks where the ink may *not* go.
// This asks where it does go, and that is a number rather than an invariant:
// offset, blur and spread each move the edge of the ink by an amount CSS
// states and a browser is the authority on.
//
// **The box is white on a white page.** It is drawn and it is invisible, so
// every pixel that is not white is shadow -- there is no box edge, no
// antialiased rim and no background colour to subtract before the measurement
// starts. That is what lets an extent be read by scanning rather than by
// knowing where the box was.
//
// The reading is an **ink span against a stated threshold**, not a colour: two
// rasterisers do not agree on a Gaussian's bytes and never will, but they
// agree closely on where it has faded to nothing. The threshold is written
// into the table so a row can be re-derived rather than trusted.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/shadow-extent.tsv')
const PROFILE = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/shadow-profile.tsv')

/** The cell, and the box inside it. Both large enough for a 12px blur to fade. */
const CELL = { width: 160, height: 160 }
const BOX = { left: 55, top: 55, width: 50, height: 50 }

/** Ink is anything at least this far off the white page. */
const THRESHOLD = 6

/** Each case: what it is called, its `box-shadow`, and its `border-radius`. */
const CASES = [
  ['none', 'none', 0],
  // Offset, because with no offset at all the shadow sits entirely behind the
  // 50x50 box that casts it and every ray reads -1 -- the six values `none`
  // reads, from a different rule. 4 and 4 keep the edge hard while putting ink
  // where a ray can find it; the axes stay symmetric here and asymmetric in
  // `offset`, which is the row that catches a renderer swapping them.
  ['hard', '4px 4px 0 0 #000', 0],
  ['offset', '8px 4px 0 0 #000', 0],
  ['blur', '0 0 12px 0 #000', 0],
  ['spread', '0 0 0 6px #000', 0],
  ['blur-spread', '0 0 8px 4px #000', 0],
  ['radius-spread', '0 0 0 6px #000', 16],
  ['radius-blur', '0 0 10px 0 #000', 16],
  // Half-alpha, offset far enough that the band below the box is flat: what
  // the profile reads there is the shadow's own colour and nothing else. It is
  // the one case whose answer is arithmetic rather than a kernel -- half-alpha
  // black over white is 128 -- so a renderer that applies the alpha twice
  // reads 191 and is caught by a row it cannot blame on a rasteriser.
  ['alpha', '0 20px 0 0 rgba(0, 0, 0, 0.5)', 0],
]

/**
 * The rays scanned, from the box's own edges outward.
 *
 * The four sides are read at their midpoints, where no corner can reach. The
 * two diagonals leave from the corner *point* -- which is where a radius shows
 * itself, because a rounded corner pulls the shadow's own corner in with it
 * and a square one does not.
 */
const RAYS = [
  ['left', -1, 0],
  ['right', 1, 0],
  ['up', 0, -1],
  ['down', 0, 1],
  ['corner up-left', -1, -1],
  ['corner down-right', 1, 1],
]

/** Where a ray starts: on the border edge, at the midpoint or the corner. */
function start(dx, dy) {
  const right = BOX.left + BOX.width
  const bottom = BOX.top + BOX.height
  return [dx < 0 ? BOX.left : dx > 0 ? right - 1 : BOX.left + BOX.width / 2, dy < 0 ? BOX.top : dy > 0 ? bottom - 1 : BOX.top + BOX.height / 2]
}

/**
 * The cases whose ink is sampled step by step as well as scanned.
 *
 * An extent says where a Gaussian has faded out and says nothing about its
 * shape on the way there: a blur with the right reach and the wrong falloff
 * passes every span row. These are the two cases with a blur in them, read
 * down the ray from the bottom edge.
 */
const PROFILED = new Set(['blur', 'blur-spread', 'radius-blur', 'alpha'])

const browser = await open()
try {
  const rows = []
  const profile = []
  await browser.page.setViewportSize(CELL)

  for (const [name, shadow, radius] of CASES) {
    await browser.page.evaluate(
      ({ cell, box, shadow, radius }) => {
        document.body.innerHTML = ''
        const ground = document.createElement('div')
        ground.style.cssText = `position:absolute;left:0;top:0;width:${cell.width}px;height:${cell.height}px;background:#fff;`
        const inner = document.createElement('div')
        inner.style.cssText = `position:absolute;left:${box.left}px;top:${box.top}px;width:${box.width}px;height:${box.height}px;background:#fff;border-radius:${radius}px;box-shadow:${shadow};`
        ground.append(inner)
        document.body.append(ground)
      },
      { cell: CELL, box: BOX, shadow, radius },
    )
    await settle(browser.page)
    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...CELL } }))

    for (const [ray, dx, dy] of RAYS) {
      const [sx, sy] = start(dx, dy)
      // Walk outward from the edge and keep the last step that still carries
      // ink. `-1` means the very first step outside the box was already clear,
      // which is what `none` reads everywhere and what a shadow pointing the
      // other way reads on the side it does not reach.
      let last = -1
      for (let step = 1; step <= 40; step += 1) {
        const x = Math.round(sx + dx * step)
        const y = Math.round(sy + dy * step)
        if (x < 0 || y < 0 || x >= CELL.width || y >= CELL.height) break
        const [r, g, b] = pixel(shot, x, y)
        if (255 - Math.min(r, g, b) >= THRESHOLD) last = step
      }
      rows.push([name, ray, last].join('\t'))
    }

    if (PROFILED.has(name)) {
      // Straight down from the bottom edge's midpoint, where no corner and no
      // offset can reach: the ramp is the blur's own falloff and nothing else.
      const [sx, sy] = start(0, 1)
      for (let step = 1; step <= 16; step += 1) {
        const [r, g, b] = pixel(shot, Math.round(sx), Math.round(sy) + step)
        profile.push([name, 'down', step, r, g, b].join('\t'))
      }
    }
  }

  const header = [
    '# Chrome, through `just conformance`. How far an outer box-shadow reaches.',
    '#',
    `# Cell ${CELL.width}x${CELL.height} of #fff carrying a ${BOX.width}x${BOX.height} box of #fff at`,
    `# ${BOX.left},${BOX.top}. The box is white on a white page, so it is invisible and`,
    '# every pixel that is not white is shadow ink -- no box edge and no antialiased',
    '# rim to subtract before the scan starts.',
    '#',
    '# Each row is an ink span: the furthest whole step outside the border edge that',
    `# still carries ink, where ink is any channel at least ${THRESHOLD}/255 off white.`,
    '# `-1` means the first step outside the box was already clear.',
    '#',
    '# The four sides are read from their midpoints, where no corner reaches. The two',
    '# diagonals leave from the corner POINT, which is the only ray a border-radius',
    "# can move: a rounded corner pulls the shadow's corner in with it.",
    '#',
    "# A span, not a colour: two rasterisers do not agree on a Gaussian's bytes and",
    '# do agree closely on where it has faded out. Compare these with a tolerance,',
    '# and read the `none` row first -- if it is not -1 everywhere, the instrument is',
    '# the suspect and not the renderer.',
    '#',
    '# case\tray\tsteps',
  ]
  await writeFile(DESTINATION, table([...header, ...rows]), 'utf8')

  const profileHeader = [
    "# Chrome, through `just conformance`. The shape of a box-shadow's blur.",
    '#',
    '# The companion to `shadow-extent.tsv`, from the SAME rendered cells, so the',
    '# two cannot disagree about the scene they describe -- one walker renders each',
    '# case once and reads it twice.',
    '#',
    '# An extent says where a Gaussian has faded out and nothing about its shape on',
    '# the way there: a blur with the right reach and the wrong falloff passes every',
    '# span row. These rows are that shape.',
    '#',
    '# Read straight DOWN from the midpoint of the bottom edge, one pixel per step,',
    '# on the same white-box-on-white-page cell the spans use. No corner and no',
    "# offset reaches this ray, so the ramp is the blur's own falloff.",
    '#',
    '# Compare with a tolerance. Two engines that agree on sigma still differ by a',
    '# few units through the middle of the ramp, which is where a Gaussian is',
    '# steepest and where a one-pixel disagreement about the edge shows largest.',
    '#',
    '# case\tray\tstep\tr\tg\tb',
  ]
  await writeFile(PROFILE, table([...profileHeader, ...profile]), 'utf8')
  process.stderr.write(`shadow extent: ${CASES.length} cases, ${rows.length} rays -> ${DESTINATION}\n`)
  process.stderr.write(`shadow profile: ${profile.length} samples -> ${PROFILE}\n`)
} finally {
  await browser.close()
}
