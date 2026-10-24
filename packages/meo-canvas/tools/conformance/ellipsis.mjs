// What Chrome keeps when a line does not fit, as a string: a word rule keeps
// `Flower of…` where a character rule keeps `Flower of Par…`. Chrome does not
// expose the string `text-overflow` drew, so its render is matched byte for byte
// against every candidate drawn plainly, and exactly one must match.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { FONT, open, settle, table } from './browser.mjs'
import { read } from './png.mjs'

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

/** The page every render is clipped from, room for any candidate's overflow. */
const PAGE = { width: 400, height: 60 }

/**
 * Every string `text-overflow` could have drawn: the text whole, and each prefix,
 * a trailing space included, with the marker after it.
 */
function candidates(text) {
  const characters = [...text]
  return [text, ...characters.map((_, index) => characters.slice(0, index).join('') + MARKER)]
}

/** One line at the page's origin, and the pixels of its `width`-wide clip. */
async function render(page, { text, size, width }, truncate) {
  await page.evaluate(
    ({ text, size, width, truncate, family }) => {
      const clamp = truncate ? `width:${width}px;overflow:hidden;text-overflow:ellipsis;` : ''
      document.body.innerHTML = ''
      const line = document.createElement('div')
      line.style.cssText = `position:absolute;left:0;top:0;white-space:nowrap;font:${size}px "${family}";line-height:1.5;color:#000;${clamp}`
      line.textContent = text
      document.body.append(line)
    },
    { text, size, width, truncate, family: FONT.family },
  )
  await settle(page)
  return read(await page.screenshot({ clip: { x: 0, y: 0, width, height: Math.ceil(size * 1.5) } }))
}

/** Whether two clips are the same pixels. */
function same(left, right) {
  return left.bytes.length === right.bytes.length && left.bytes.every((byte, index) => byte === right.bytes[index])
}

/** The first column at which two clips of one size differ, or `-`. */
function firstDifference(left, right) {
  const stride = left.width * left.channels
  for (let x = 0; x < left.width; x += 1) {
    for (let at = x * left.channels; at < left.bytes.length; at += stride) {
      for (let channel = 0; channel < 3; channel += 1) {
        if (left.bytes[at + channel] !== right.bytes[at + channel]) return x
      }
    }
  }
  return '-'
}

const browser = await open()
try {
  await browser.page.setViewportSize(PAGE)
  const rows = []
  for (const item of CASES) {
    const truncated = await render(browser.page, item, true)
    const matches = []
    for (const candidate of candidates(item.text)) {
      if (same(truncated, await render(browser.page, { ...item, text: candidate }, false))) matches.push(candidate)
    }
    if (matches.length !== 1) {
      throw new Error(`${JSON.stringify(item.text)} at ${item.size}px in ${item.width}: ${matches.length} matches, ${JSON.stringify(matches)}`)
    }
    const [drawn] = matches

    // The marker's ink edge: where the drawn string first parts from its prefix alone.
    let marker = '-'
    if (drawn.endsWith(MARKER)) {
      marker = firstDifference(truncated, await render(browser.page, { ...item, text: drawn.slice(0, -MARKER.length) }, false))
    }
    const widths = await browser.page.evaluate(
      ({ texts, size, family }) =>
        texts.map(text => {
          const span = document.createElement('span')
          span.style.cssText = `font:${size}px "${family}";white-space:pre`
          span.textContent = text
          document.body.append(span)
          const width = span.getBoundingClientRect().width
          span.remove()
          return width
        }),
      { texts: [item.text, drawn], size: item.size, family: FONT.family },
    )
    rows.push(
      [
        // The whole string, quoted: the column is data for a walker, not a caption.
        JSON.stringify(item.text),
        item.size,
        item.width,
        widths[0].toFixed(2),
        JSON.stringify(drawn),
        widths[1].toFixed(2),
        matches.length,
        marker,
      ].join('\t'),
    )
  }

  const header = [
    '# Chrome, through `just conformance`. What a line keeps when it does not fit.',
    '#',
    "# A `text-overflow: ellipsis` line on the repository's own face, matched byte for",
    '# byte against the text whole and every prefix plus the marker, drawn plainly in',
    '# the same clip; exactly one matches. Widths are `white-space: pre` spans; the',
    "# marker's ink edge is the first column where the line parts from its prefix alone.",
    '#',
    '# text\tsize\twidth\tfull width\tdrawn\tdrawn width\tmatches\tmarker ink from',
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
