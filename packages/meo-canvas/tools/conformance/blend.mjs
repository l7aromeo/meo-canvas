// What each blend mode does to one source over one backdrop.
//
// Unlike a blur, a blend mode is an **exact formula on channels** rather than
// a filter kernel, so these numbers are something another engine has to
// reproduce rather than approximate. That is why this table is worth asking
// for at all, and the argument is already written down in
// `fixtures/blend-modes/notes.json`.
//
// The backdrop is a **ramp**, and that is the whole design. On a flat backdrop
// several of the sixteen collapse onto each other: `multiply` and `darken`
// agree wherever the backdrop is lighter than the source, and `screen` and
// `lighten` agree wherever it is darker. A ramp puts both halves of each pair
// inside one cell, and the two sample points read one half each.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/blend-modes.tsv')

/** The cell every mode is drawn in. */
const BOX = { width: 56, height: 40 }

/** The source square, and where it sits in the cell. */
const SOURCE = { width: 36, height: 24, left: 10, top: 8, colour: '#4090c0' }

/** The backdrop ramp: dark on the left, light on the right. */
const BACKDROP = 'linear-gradient(90deg, #181838, #f0e0a0)'

/** Every mode CSS spells, in the order the scene numbers them. */
const MODES = [
  'normal',
  'multiply',
  'screen',
  'overlay',
  'darken',
  'lighten',
  'color-dodge',
  'color-burn',
  'hard-light',
  'soft-light',
  'difference',
  'exclusion',
  'hue',
  'saturation',
  'color',
  'luminosity',
]

/**
 * The two points each mode is read at, inside the source and derived from it.
 *
 * One where the backdrop under the source is dark and one where it is light —
 * the pair that stops `multiply` and `darken` reading the same, and `screen`
 * and `lighten` likewise.
 */
const PROBES = [
  ['over dark', SOURCE.left + 4, SOURCE.top + 12],
  ['over light', SOURCE.left + 32, SOURCE.top + 12],
]

const browser = await open()
try {
  const rows = []
  await browser.page.setViewportSize(BOX)

  for (const mode of ['none', ...MODES]) {
    await browser.page.evaluate(
      ({ box, source, backdrop, mode }) => {
        document.body.innerHTML = ''
        const cell = document.createElement('div')
        cell.style.cssText = `position:absolute;left:0;top:0;width:${box.width}px;height:${box.height}px;background-image:${backdrop};isolation:isolate;`
        if (mode !== 'none') {
          const square = document.createElement('div')
          square.style.cssText = `position:absolute;left:${source.left}px;top:${source.top}px;width:${source.width}px;height:${source.height}px;background:${source.colour};mix-blend-mode:${mode};`
          cell.append(square)
        }
        document.body.append(cell)
      },
      { box: BOX, source: SOURCE, backdrop: BACKDROP, mode },
    )

    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))
    for (const [point, x, y] of PROBES) {
      const [r, g, b] = pixel(shot, x, y)
      rows.push([mode, point, x, y, r, g, b].join('\t'))
    }
  }

  const header = [
    '# Chrome, through `just conformance`. Every blend mode over one ramp.',
    '#',
    `# Cell ${BOX.width}x${BOX.height} carrying ${BACKDROP}.`,
    `# Source ${SOURCE.width}x${SOURCE.height} of ${SOURCE.colour} at ${SOURCE.left},${SOURCE.top}, with`,
    '# `isolation: isolate` on the cell so the blend has the cell as its backdrop',
    '# rather than the page behind it.',
    '#',
    '# Read at two points INSIDE the source, one where the backdrop under it is dark',
    '# and one where it is light. On a flat backdrop `multiply` and `darken` agree',
    '# wherever the backdrop is lighter than the source, and `screen` and `lighten`',
    '# agree wherever it is darker -- the ramp and the two points are what keep those',
    '# four apart.',
    '#',
    '# The `none` row is the cell with NO source drawn, which is what `normal` has to',
    '# be read against: a mode that drew nothing would otherwise look like one that',
    '# let the backdrop through.',
    '#',
    '# mode\tpoint\tx\ty\tr\tg\tb',
  ]
  await writeFile(DESTINATION, table([...header, ...rows]), 'utf8')
  process.stderr.write(`blend modes: ${MODES.length + 1} cases, ${rows.length} samples -> ${DESTINATION}\n`)
} finally {
  await browser.close()
}
