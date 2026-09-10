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
const VECTOR = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/fit-marks.svg')

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
 *
 * **100x200 is the widest aspect disagreement this picture allows.** 1:2 against
 * a 2:1 source, where the square boxes are 1:1 against it. The disagreement is
 * what the table needs: where a box shares the picture's aspect, `fill`,
 * `contain` and `cover` are one rectangle, and a renderer that letterboxes what
 * it should stretch passes every row. Measured rather than supposed -- a 200x100
 * box against this source agrees on all five rules both with that defect present
 * and with it fixed, so it would have been a row that cannot go red.
 */
const BOXES = [
  { width: 72, height: 72 },
  { width: 6, height: 6 },
  { width: 100, height: 200 },
]

/** What the box is painted with, so the picture's own extent can be found. */
const CELL = [240, 240, 240]

/** The colours at the picture's two edges, which is what tells the fits apart. */
const MAGENTA = [232, 40, 200]
const CYAN = [40, 200, 200]

const FITS = ['fill', 'contain', 'cover', 'none', 'scale-down']

/** The two source kinds, because the rule is one and they disagreed under it.
 *
 * **A table with one source kind cannot state the finding.** `object-fit` was
 * covered here before this column existed and was never wrong -- it walked
 * `fit-marks.png` and only that, so the property was guarded for one of the two
 * kinds a caller can supply and the uncovered one is the kind that diverged.
 *
 * The vector is the same picture rather than a similar one: rasterised at its
 * own 8x4 it is byte-identical to the bitmap on all thirty-two pixels, checked
 * before it was committed. That is what makes a disagreement in these rows a
 * disagreement about placement -- if the two arts differed, every row would and
 * none of it would mean anything.
 *
 * Raster first and in the order it already had, so the rows this table carried
 * before are the same rows in the same places with one column added.
 */
const SOURCES = [
  { kind: 'raster', path: SOURCE, type: 'image/png' },
  { kind: 'svg', path: VECTOR, type: 'image/svg+xml' },
]

const browser = await open()
try {
  const rows = []

  for (const KIND of SOURCES) {
    const picture = await readFile(KIND.path)
    const source = `data:${KIND.type};base64,${picture.toString('base64')}`

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
        rows.push([fit, BOX.width, BOX.height, rect, has(MAGENTA) ? 'magenta' : '-', has(CYAN) ? 'cyan' : '-', KIND.kind].join('\t'))
      }
    }
  }

  const header = [
    '# Chrome, through `just conformance`. Where each object-fit rule puts a picture.',
    '#',
    `# An <img> on a #f0f0f0 cell at ${BOXES.map(box => `${box.width}x${box.height}`).join(', ')}, from`,
    '# `crates/meo-canvas/tests/assets/fit-marks.png` — eight by four, magenta at its',
    '# own x=0 and cyan at x=7. Those two columns are what separate `fill` from',
    '# `cover`: both fill the box and differ only in what they CUT, so a symmetric',
    '# picture reads the same stretched as cropped.',
    '#',
    '# **`source` is the kind, and it is the column this table was missing.** The',
    '# same picture is supplied twice: as that PNG and as `fit-marks.svg`, which',
    '# rasterises at its own 8x4 to the identical thirty-two pixels. A renderer can',
    '# place one correctly and the other not, which is what happened here, and a',
    '# table with no source column cannot say so.',
    '#',
    "# **No box shares the picture's aspect, and that is what makes the table",
    '# able to fail.** The picture is 2:1; two boxes are square and the third is',
    '# 1:2. Where a box and the picture agree on aspect, `fill`, `contain` and',
    '# `cover` are one rectangle and a renderer that letterboxes what it should',
    '# stretch passes every row -- measured, not supposed: a 200x100 box against',
    '# this 8x4 picture agrees on all five rules whether that defect is present',
    '# or absent. 100x200 is the widest disagreement this picture allows.',
    '#',
    '# **`fill` and `contain` on the same rectangle in every `svg` row is Chrome',
    '# being right, not two modes collapsing.** `object-fit` sizes the replaced',
    '# element; the document then lays itself out inside that viewport under its',
    '# own `preserveAspectRatio`, which defaults to `xMidYMid meet`. So the',
    '# stretch never reaches the drawing and the ink lands where `contain` puts',
    '# it. A bitmap carries no such rule and is stretched, which is why the',
    '# `raster` rows differ there and the `svg` rows do not. The two kinds are',
    '# the same thirty-two pixels -- `fit-marks.svg` rasterises at its own 8x4',
    '# identical to the PNG -- so this is a difference in placement, not in art.',
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
    '# fit\tw\th\trect\tmagenta\tcyan\tsource',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`object fit: ${FITS.length} rules x ${BOXES.length} boxes x ${SOURCES.length} sources -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
