// What Chrome keeps when a line does not fit, as a string: a word rule keeps
// `Flower of…` where a character rule keeps `Flower of Par…`. Measured with
// `measureText`, since Chrome does not expose the string `text-overflow` drew.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { FONT, open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/ellipsis.tsv')

/** The ellipsis a truncated line ends with. */
const MARKER = '…'

/**
 * The strings and widths: one string at 60 to 150, so the answer crosses a word
 * boundary; a single word longer than its box, where a word rule must cut mid-word
 * or draw nothing; and the string again at 22px.
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
