// What Chrome keeps when a line does not fit, as a **string**.
//
// The question is which characters survive, not how many pixels they occupy.
// A word-boundary rule keeps `Flower of…` where a character rule keeps
// `Flower of Par…`, and those differ in content: a fixture pinning the string
// fails a wrong rule outright, where a fixture pinning a width fails it by a
// pixel or two and can be argued with.
//
// Measured with `measureText` rather than `text-overflow: ellipsis`, because
// Chrome does not expose the string that property drew. `measureText` is also
// what v1's `fillLineToWidth` uses, so this asks the browser the same question
// v1 asks it.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { FONT, open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/ellipsis.tsv')

/** The ellipsis a truncated line ends with. */
const MARKER = '…'

/**
 * The strings and widths to ask about.
 *
 * The first four are one string at four widths, so the answer moves through a
 * word boundary rather than sitting on one side of it: 90 is the width v1's
 * own example uses, and 60 and 120 bracket it. The fifth is a single word
 * longer than its box, where a word-boundary rule has nothing to fall back on
 * and has to cut mid-word or draw nothing.
 */
const CASES = [
  { text: 'Flower of Paradise', size: 16, width: 60 },
  { text: 'Flower of Paradise', size: 16, width: 90 },
  { text: 'Flower of Paradise', size: 16, width: 120 },
  { text: 'Flower of Paradise', size: 16, width: 150 },
  { text: 'Antidisestablishmentarianism', size: 16, width: 90 },
  { text: 'Flower of Paradise', size: 22, width: 90 },
]

const browser = await open()
try {
  const rows = await browser.page.evaluate(
    ({ cases, family, marker }) => {
      const canvas = document.createElement('canvas')
      const context = canvas.getContext('2d')

      return cases.map(({ text, size, width }) => {
        context.font = `${size}px "${family}"`
        const whole = context.measureText(text).width
        const ellipsis = context.measureText(marker).width

        // The longest prefix whose width, plus the marker's, fits. Walked one
        // character at a time rather than by word: what is being measured is
        // where Chrome's own metrics say the line has to stop, and a word rule
        // is one of the answers under test rather than part of the ruler.
        let kept = ''
        if (whole > width) {
          for (const character of text) {
            const candidate = kept + character
            if (context.measureText(candidate).width + ellipsis > width) break
            kept = candidate
          }
        }

        const drawn = whole <= width ? text : kept + marker
        return [
          // The whole string, quoted. An abbreviated one reads better and is
          // useless to a walker: the column is data, not a caption.
          JSON.stringify(text),
          size,
          width,
          whole.toFixed(2),
          JSON.stringify(drawn),
          context.measureText(drawn).width.toFixed(2),
        ].join('\t')
      })
    },
    { cases: CASES, family: FONT.family, marker: MARKER },
  )

  const header = [
    '# Chrome, through `just conformance`. What a line keeps when it does not fit.',
    '#',
    "# Measured with `measureText` on the repository's own face, which the page",
    '# asserts has loaded rather than assuming it. `text-overflow: ellipsis` is not',
    '# used: Chrome does not expose the string it drew, and the string is the whole',
    '# question -- a word-boundary rule and a character rule disagree about content,',
    '# not about width.',
    '#',
    '# text\tsize\twidth\tfull width\tdrawn\tdrawn width',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`ellipsis: ${rows.length} cases -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
