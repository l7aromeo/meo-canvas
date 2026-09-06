// Where each `object-fit` rule puts a picture inside its box.
//
// Asked because the fixture asked: `fixtures/object-fit/notes.json` ends with
// "needs a Chrome number", and `object-fit` is one of the properties a browser
// answers exactly — five rules, five distinct rectangles, no interpolation
// argument in any of them.
//
// The source is the fixture's own picture: eight by four, with a **magenta
// column at x=0 and a cyan column at x=7**. That is what separates `fill` from
// `cover`, which both fill the box and differ only in what they cut: a
// symmetric picture reads the same stretched as cropped, and the first version
// of that fixture could not tell the two apart at all.

import { readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/object-fit.tsv')
const SOURCE = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/fit-marks.png')

/** The boxes every rule places its picture in.
 *
 * **72 is not enough on its own, and the fixture it produced could not fail for
 * one of its five rules.** CSS defines `scale-down` as the smaller of `none`
 * and `contain`, so wherever the picture already fits they are the same rule by
 * definition -- and an 8x4 source fits a 72x72 box. The two rows came out
 * byte-identical, correctly, and a renderer implementing `scale-down` as `none`
 * passed the table, its walker, and everything built on them.
 *
 * The boxes below 8 wide are where the two separate: `none` crops the picture
 * to the box, `scale-down` shrinks it to fit.
 *
 * **6 and not 4.** A 4x4 box halves an eight-pixel source, and this renderer
 * scales with a smoothing filter where Chrome is pinned to
 * `image-rendering: pixelated` -- so the magenta column arrives blended past
 * any tolerance that can still tell the source's four colours apart, and the
 * mark columns read `-` here against Chrome's `magenta`. The rectangles agree
 * at 4 exactly; it is the marks that stop being readable. 6 separates the two
 * rules in the rectangle AND in both marks, which is the row worth committing.
 */
const BOXES = [
  { width: 72, height: 72 },
  { width: 6, height: 6 },
]

/** What the box is painted with, so the picture's own extent can be found. */
const CELL = [240, 240, 240]

/** The colours at the picture's two edges, which is what tells the fits apart. */
const MAGENTA = [232, 40, 200]
const CYAN = [40, 200, 200]

const FITS = ['fill', 'contain', 'cover', 'none', 'scale-down']

const browser = await open()
try {
  const picture = await readFile(SOURCE)
  const source = `data:image/png;base64,${picture.toString('base64')}`
  const rows = []

  for (const BOX of BOXES) {
    await browser.page.setViewportSize(BOX)

    for (const fit of FITS) {
      await browser.page.evaluate(
        ({ box, source, fit }) => {
          document.body.innerHTML = ''
          const cell = document.createElement('div')
          cell.style.cssText = `position:absolute;left:0;top:0;width:${box.width}px;height:${box.height}px;background:#f0f0f0;overflow:hidden;`
          const image = document.createElement('img')
          image.style.cssText = `display:block;width:${box.width}px;height:${box.height}px;object-fit:${fit};image-rendering:pixelated;`
          image.src = source
          cell.append(image)
          document.body.append(cell)
        },
        { box: BOX, source, fit },
      )
      // Waited for by the harness rather than by this page: a shot taken before
      // decode measures an empty box and reports every fit as drawing nothing.
      await settle(browser.page)

      const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))
      let box = null
      for (let y = 0; y < BOX.height; y += 1) {
        for (let x = 0; x < BOX.width; x += 1) {
          const [r, g, b] = pixel(shot, x, y)
          if (r === CELL[0] && g === CELL[1] && b === CELL[2]) continue
          box = box === null ? [x, y, x, y] : [Math.min(box[0], x), Math.min(box[1], y), Math.max(box[2], x), Math.max(box[3], y)]
        }
      }

      const has = ink => {
        for (let y = 0; y < BOX.height; y += 1) {
          for (let x = 0; x < BOX.width; x += 1) {
            const [r, g, b] = pixel(shot, x, y)
            if (r === ink[0] && g === ink[1] && b === ink[2]) return true
          }
        }
        return false
      }

      const rect = box === null ? 'absent' : `${box[0]},${box[1]},${box[2] - box[0] + 1},${box[3] - box[1] + 1}`
      rows.push([fit, BOX.width, BOX.height, rect, has(MAGENTA) ? 'magenta' : '-', has(CYAN) ? 'cyan' : '-'].join('\t'))
    }
  }

  const header = [
    '# Chrome, through `just conformance`. Where each object-fit rule puts a picture.',
    '#',
    `# An <img> on a #f0f0f0 cell at ${BOXES.map(box => `${box.width}x${box.height}`).join(', ')}, source `,
    '# `crates/meo-canvas/tests/assets/fit-marks.png` — eight by four, magenta at its',
    '# own x=0 and cyan at x=7. Those two columns are what separate `fill` from',
    '# `cover`: both fill the box and differ only in what they CUT, so a symmetric',
    '# picture reads the same stretched as cropped.',
    '#',
    '# `rect` is the bounding box of everything that is not the cell colour, so a',
    '# letterboxed fit reports the picture rather than the box.',
    '#',
    '# `image-rendering: pixelated`, so an eight-pixel-wide source scaled to 72 keeps',
    '# its columns readable instead of blending them into their neighbours.',
    '#',
    '# **More than one box size, because at 72 the source fits and `scale-down` IS',
    '# `none`** -- the same rule by definition, not by coincidence. The boxes below',
    '# eight wide are where the two separate: `none` crops, `scale-down` shrinks.',
    '# A table with only the 72 rows cannot fail for `scale-down` at all.',
    '#',
    '# fit\tw\th\trect\tmagenta\tcyan',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`object fit: ${FITS.length} rules x ${BOXES.length} boxes -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
