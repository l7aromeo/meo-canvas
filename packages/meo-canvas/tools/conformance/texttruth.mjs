// Chrome's text numbers at 16px on the repository's face, from HTML elements:
// a run's width is a `white-space: pre` span's, and a line's box and baseline
// are a block's height and a zero-height inline-block's top, which sits on the
// baseline. `chrome_text_truth.rs` holds each as a constant and reads this table.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { FONT, open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/text-truth.tsv')

/** The em size every number is measured at. */
const SIZE = 16

/**
 * The runs whose widths the test compares, each at a letter spacing. The
 * sixteen-character run is measured spaced and unspaced, since spacing is one
 * unit per character and sixteen tells that from one per run.
 */
const WIDTHS = [
  ['abcdefghijklmnop', 0],
  ['abcdefghijklmnop', 2],
  [' ', 0],
  ['a', 0],
  ['Hxgp quick', 0],
  ['brown fox', 0],
  ['jumps over', 0],
  ['the lazy dog', 0],
  ['Hxgpquickbrown', 0],
  ['Hxgp quick brown', 0],
]

/**
 * The line boxes: the face's own string and three others at `normal`, which must
 * give one box, then the test's own string at `normal` and three multiples, `null`
 * being `normal`.
 */
const LINES = [
  [null, 'Ag|``'],
  [null, 'x'],
  [null, 'ABCDEFG'],
  [null, 'gjpqy'],
  [null, 'Hxgp'],
  [1, 'Hxgp'],
  [2, 'Hxgp'],
  [0.5, 'Hxgp'],
]

const browser = await open()
try {
  await browser.page.setViewportSize({ width: 800, height: 200 })
  const rows = []
  for (const [text, spacing] of WIDTHS) {
    const measured = await browser.page.evaluate(
      ({ text, spacing, family, size }) => {
        const run = word => {
          const span = document.createElement('span')
          span.style.cssText = `font:${size}px "${family}";white-space:pre;letter-spacing:${spacing}px`
          span.textContent = word
          document.body.append(span)
          const width = span.getBoundingClientRect().width
          span.remove()
          return width
        }
        const words = text.split(' ')
        // The words and the gaps measured apart, which is how the test sums
        // them: a run that equals its whole is not shaped across a space.
        const parts = text.length > 1 && words.length > 1 ? words.reduce((sum, word) => sum + run(word), 0) + (words.length - 1) * run(' ') : null
        return { whole: run(text), parts }
      },
      { text, spacing, family: FONT.family, size: SIZE },
    )
    rows.push(['width', JSON.stringify(text), spacing, measured.whole.toFixed(3), measured.parts === null ? '-' : measured.parts.toFixed(3)].join('\t'))
  }

  await settle(browser.page)
  for (const [multiple, text] of LINES) {
    const measured = await browser.page.evaluate(
      ({ multiple, text, family, size }) => {
        const block = ruler => {
          const div = document.createElement('div')
          div.style.cssText = `font:${size}px "${family}";line-height:${multiple === null ? 'normal' : multiple}`
          div.append(text)
          let mark = null
          if (ruler) {
            mark = document.createElement('span')
            mark.style.cssText = 'display:inline-block;width:0;height:0'
            div.append(mark)
          }
          document.body.append(div)
          const top = div.getBoundingClientRect().top
          const result = { height: div.getBoundingClientRect().height, baseline: mark === null ? null : mark.getBoundingClientRect().top - top }
          div.remove()
          return result
        }
        // The box from a bare line, since the ruler can hold a short line box open.
        return { box: block(false).height, baseline: block(true).baseline }
      },
      { multiple, text, family: FONT.family, size: SIZE },
    )
    rows.push(['line', multiple === null ? 'normal' : multiple, JSON.stringify(text), measured.box.toFixed(3), measured.baseline.toFixed(3)].join('\t'))
  }

  const lines = [
    `# Chrome, through \`just conformance\`: text widths and line boxes at ${SIZE}px ${FONT.family}, from HTML elements.`,
    '# Written by packages/meo-canvas/tools/conformance/texttruth.mjs.',
    '#',
    "# `width` rows: the text, its letter spacing, a `white-space: pre` span's width, and the",
    '# sum of its words and spaces measured apart (`-` for one word).',
    "# `line` rows: the line height, the text, the bare line box's height, and the baseline",
    '# from its top, read from a zero-height inline-block.',
    '#',
    '# kind\tkey\targument\tvalue\tsecond',
    ...rows,
  ]
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
