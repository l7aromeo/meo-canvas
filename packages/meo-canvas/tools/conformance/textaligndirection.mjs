// Where `text-align` puts a line, and which values move with the direction.
//
// Reported as `l7aromeo/meo-canvas#109`: `TextAlign::Start` is documented as
// flipping under a right-to-left direction and cannot. The enum is the
// specification the code fails -- `Start` says "at the inline start, which
// flips under a right-to-left direction" and `Left` says "at the left edge
// regardless of direction" -- and both reach the same arm.
//
// **The `ltr` rows are the control and they are half the table.** Under `ltr`,
// `start` must land where `left` does and `end` where `right` does. Without
// them a repair to the physical arms and a repair to nothing look identical:
// the `rtl` rows alone cannot say whether `left` stayed put because it is
// correct or because nothing touched it.
//
// `center` is here and proves nothing about either direction, deliberately. It
// is the same rectangle under both, so it cannot witness a flip -- and a row
// that cannot fail is worth keeping only when it says so, which its note does.
// Without it the next person adds it believing it covers the centred case.
import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/text-align-direction.tsv')

/** The cell every line is placed in, wider than the line so placement shows. */
const BOX = { width: 300, height: 60 }

/** What the cell is painted with, so the line's own extent can be found. */
const CELL = [255, 255, 255]

const ALIGNS = ['start', 'end', 'left', 'right', 'center']
const DIRECTIONS = ['ltr', 'rtl']

/** Why each row is here, in the row rather than in a comment above the table. */
const NOTES = {
  'ltr start': 'control -- must equal `left` under ltr, or a no-op reads as a fix',
  'ltr end': 'control -- must equal `right` under ltr, for the same reason',
  'ltr left': 'the physical arm, which no direction moves',
  'ltr right': 'the physical arm, which no direction moves',
  'ltr center': 'proves nothing about direction: the same rectangle under both',
  'rtl start': 'the defect: `start` is the inline start and flips to the right edge',
  'rtl end': 'and `end` flips to the left edge with it',
  'rtl left': 'unmoved, which is what separates it from `start`',
  'rtl right': 'unmoved, which is what separates it from `end`',
  'rtl center': 'proves nothing about direction: the same rectangle under both',
}

const browser = await open()
try {
  await browser.page.setViewportSize(BOX)
  const rows = []

  for (const direction of DIRECTIONS) {
    for (const align of ALIGNS) {
      await browser.page.evaluate(
        ({ box, align, direction }) => {
          document.body.style.margin = '0'
          document.body.innerHTML = ''
          const cell = document.createElement('div')
          cell.style.cssText = `position:absolute;left:0;top:0;width:${box.width}px;height:${box.height}px;background:#fff;`
          const line = document.createElement('div')
          line.textContent = 'abc'
          line.style.cssText = `font:32px/40px Fixture;color:#000;text-align:${align};direction:${direction};`
          cell.append(line)
          document.body.append(cell)
        },
        { box: BOX, align, direction },
      )
      // Waited for by the harness rather than by the page: a shot taken before
      // the face is applied measures the fallback and reports a plausible
      // number.
      await settle(browser.page)

      const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))
      let left = null
      let right = null
      for (let x = 0; x < BOX.width; x += 1) {
        let inked = false
        for (let y = 0; y < BOX.height && !inked; y += 1) {
          const [r, g, b] = pixel(shot, x, y)
          inked = r !== CELL[0] || g !== CELL[1] || b !== CELL[2]
        }
        if (inked) {
          left = left === null ? x : left
          right = x
        }
      }

      const extent = left === null ? 'absent' : `${left},${right - left + 1}`
      rows.push([direction, align, extent, NOTES[`${direction} ${align}`]].join('\t'))
    }
  }

  const header = [
    '# Chrome, through `just conformance`. Where `text-align` puts a line, and',
    '# which values move with `direction`.',
    `# An <img>-free cell of ${BOX.width}x${BOX.height} painted #ffffff, one line of Fixture at 32px.`,
    '# Written by packages/meo-canvas/tools/conformance/textaligndirection.mjs.',
    '#',
    '# `extent` is the inked columns as `left,width`, so a line that moved is a',
    '# different first column rather than a different size -- every row here is',
    '# the same three glyphs and only the placement varies.',
    '#',
    '# The `ltr` rows are the control: `start` must equal `left` and `end` must',
    '# equal `right` there, or a repair to the physical arms cannot be told from',
    '# a repair to nothing.',
    '#',
    '# direction\talign\textent\tnote',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`text align direction: ${ALIGNS.length} aligns x ${DIRECTIONS.length} directions -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
