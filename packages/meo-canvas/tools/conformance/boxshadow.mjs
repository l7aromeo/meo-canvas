// Where an outer box-shadow's ink is allowed to land, and where it is not.
//
// CSS Backgrounds and Borders 3 §7.1.1: an outer shadow is drawn *outside* the
// border edge only — the border box is clipped out of it. So the element's own
// background never composites over its own shadow, and a translucent
// background can never reveal one.
//
// **The translucent row is the whole measurement.** Over an opaque background
// the two possible implementations agree exactly: painting the shadow under
// the box and then covering it, or clipping it out of the box, both leave the
// background's own colour at every point inside. An opaque case therefore pins
// nothing, and is measured here anyway so that the table says so rather than a
// comment claiming it.
//
// The `below` probe is the control the inside probes need. A renderer that
// fixed the inside reading by *not drawing the shadow at all* would satisfy
// every other row here; only a point outside the box can tell a clipped shadow
// from an absent one.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/box-shadow.tsv')

/** The cell every case is drawn in, and the opaque page under it. */
const CELL = { width: 80, height: 80 }
const PAGE = '#b01020'

/** The inner box: 40x40 inset 20 on every side, so the cell's padding is even. */
const BOX = { left: 20, top: 20, width: 40, height: 40 }

/** The two backgrounds. Half-alpha black is the one that can reveal a shadow. */
const BACKGROUNDS = {
  translucent: 'rgba(0, 0, 0, 0.5)',
  // The colour half-alpha black composites to over the page, so the two
  // background rows are the same picture where nothing is wrong.
  opaque: 'rgb(108, 15, 19)',
}

/** The shadow, small enough that its ink stays near the box it belongs to. */
const SHADOW = '0 1px 2px rgba(0, 0, 0, 0.5)'

/**
 * Three points: two inside the border box and one outside it.
 *
 * `inside` is the centre, far from every edge, which no blur of 2px reaches.
 * `inside top` sits 2px below the top edge, which is where an *inset* shadow's
 * ink is heaviest and where an outer one's would be if it leaked upward.
 * `below` sits 2px under the bottom edge, in the outer shadow's ink.
 */
const PROBES = [
  ['inside', BOX.left + BOX.width / 2, BOX.top + BOX.height / 2],
  ['inside top', BOX.left + BOX.width / 2, BOX.top + 2],
  ['below', BOX.left + BOX.width / 2, BOX.top + BOX.height + 2],
]

/** Each case: the background it fills with, and the shadow it casts. */
const CASES = [
  ['translucent', 'none', 'none'],
  ['translucent', 'outer', SHADOW],
  ['translucent', 'inset', `inset ${SHADOW}`],
  ['opaque', 'none', 'none'],
  ['opaque', 'outer', SHADOW],
]

const browser = await open()
try {
  const rows = []
  await browser.page.setViewportSize(CELL)

  for (const [background, shadow, css] of CASES) {
    await browser.page.evaluate(
      ({ cell, page, box, fill, css }) => {
        document.body.innerHTML = ''
        const ground = document.createElement('div')
        ground.style.cssText = `position:absolute;left:0;top:0;width:${cell.width}px;height:${cell.height}px;background:${page};`
        const inner = document.createElement('div')
        inner.style.cssText = `position:absolute;left:${box.left}px;top:${box.top}px;width:${box.width}px;height:${box.height}px;background:${fill};box-shadow:${css};`
        ground.append(inner)
        document.body.append(ground)
      },
      { cell: CELL, page: PAGE, box: BOX, fill: BACKGROUNDS[background], css },
    )
    await settle(browser.page)

    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...CELL } }))
    for (const [point, x, y] of PROBES) {
      const [r, g, b] = pixel(shot, x, y)
      rows.push([background, shadow, point, x, y, r, g, b].join('\t'))
    }
  }

  const header = [
    '# Chrome, through `just conformance`. Where an outer box-shadow may land.',
    '#',
    `# Cell ${CELL.width}x${CELL.height} of ${PAGE}, carrying a ${BOX.width}x${BOX.height} box at`,
    `# ${BOX.left},${BOX.top}. Backgrounds: translucent ${BACKGROUNDS.translucent},`,
    `# opaque ${BACKGROUNDS.opaque} -- the colour the translucent one composites to over`,
    '# the page, so the two agree wherever nothing is wrong.',
    `# Shadow: ${SHADOW}, and the same again with \`inset\`.`,
    '#',
    '# CSS Backgrounds and Borders 3 §7.1.1: an outer shadow is drawn outside the',
    '# border edge ONLY. The border box is clipped out of it, so the two `inside`',
    '# probes read the same with the shadow as without it -- on the TRANSLUCENT',
    '# background as well, which is the row that discriminates. Painting the shadow',
    '# beneath the box instead gives the translucent case a second coat of the',
    '# shadow colour: `1 - (1-0.5)^2 = 0.75` rather than 0.5.',
    '#',
    '# The opaque rows pin nothing about the clip and are here to say so: both',
    '# implementations agree under a background that hides whatever is beneath it.',
    '#',
    '# `below` is outside the box, in the outer shadow ink. It is what separates a',
    '# shadow that is correctly clipped from one that is not drawn at all.',
    '#',
    '# background\tshadow\tpoint\tx\ty\tr\tg\tb',
  ]
  await writeFile(DESTINATION, table([...header, ...rows]), 'utf8')
  process.stderr.write(`box-shadow clip: ${CASES.length} cases, ${rows.length} samples -> ${DESTINATION}\n`)
} finally {
  await browser.close()
}
