// What an `<svg>`'s `viewBox` maps to: the transform Chrome builds from it,
// read with `getScreenCTM`, for boxes at fractional sizes and positions and with
// padding and a border. Read as numbers rather than ink, since Chrome's path
// antialiasing steps in quarter pixels down the page and would hide a tenth.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/view-box.tsv')

/** The coordinate space every case draws in. */
const VIEW_BOX = '0 0 100 100'

/**
 * Border boxes, `box-sizing: border-box`. The bare integer box is the control;
 * a fractional one tells a fit from the rounded box, a padded one a fit into the
 * border box. Borders are whole: Chrome floors a fractional width, another rule.
 */
const CASES = [
  { left: 10, top: 10, width: 300, height: 200, padding: 0, border: 0 },
  { left: 10, top: 10, width: 350, height: 166.6667, padding: 0, border: 0 },
  { left: 10.3, top: 10.6, width: 350, height: 166.6667, padding: 0, border: 0 },
  { left: 10, top: 10, width: 300.4, height: 100.6, padding: 0, border: 0 },
  { left: 10, top: 10, width: 300, height: 200, padding: 7, border: 3 },
  { left: 10.3, top: 10.6, width: 350, height: 166.6667, padding: 7.3, border: 3 },
]

const browser = await open()
try {
  const lines = [
    '# Chrome, through `just conformance`. The transform an <svg viewBox> maps its drawing with.',
    `# viewBox="${VIEW_BOX}", xMidYMid meet, absolutely placed, box-sizing: border-box.`,
    '# Written by packages/meo-canvas/tools/conformance/viewbox.mjs.',
    '# a and d are the scales, e and f the translation, from getScreenCTM in CSS px.',
    '# left\ttop\twidth\theight\tpadding\tborder\ta\td\te\tf',
  ]
  for (const box of CASES) {
    const ctm = await browser.page.evaluate(
      ({ box, viewBox }) => {
        document.body.innerHTML = `<svg viewBox="${viewBox}" style="position:absolute;display:block;box-sizing:border-box;left:${box.left}px;top:${box.top}px;width:${box.width}px;height:${box.height}px;padding:${box.padding}px;border:${box.border}px solid #fff"><path d="M10 10 H90 V90 H10 Z"/></svg>`
        const matrix = document.querySelector('svg').getScreenCTM()
        return [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f]
      },
      { box, viewBox: VIEW_BOX },
    )
    const [a, b, c, d, e, f] = ctm
    if (b !== 0 || c !== 0) {
      throw new Error(`${JSON.stringify(box)}: a skewed matrix ${ctm}, which meet never builds`)
    }
    lines.push([box.left, box.top, box.width, box.height, box.padding, box.border, a, d, e, f].join('\t'))
  }

  const written = table(lines)
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    console.log(`wrote ${DESTINATION}`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
