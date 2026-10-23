// How many lines a text box breaks into at a width within a pixel of one of its
// lines: Chrome lays out in fractions of a pixel, so a box a hair wider than a
// line keeps it whole. Read as the box and the line boxes of a range over it.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { FONT, open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/wrap-width.tsv')

/** The paragraph, which breaks into two lines at 150 with its widest just over 137. */
const PARAGRAPH = 'one two three four five six seven eight'

/**
 * The cases: the text, where it sits, and the width it is given, `auto` for its
 * own. A stated width is placed at `left` in a block; an `auto` one is a flex
 * row's item, as wide as its content. The 137 rows straddle the paragraph's
 * widest line, the label's width has a fraction under a half, and 82 is just
 * under it.
 */
const CASES = [
  { name: 'paragraph at 137', text: PARAGRAPH, left: 30, width: '137px', note: 'control: narrower than its widest line, so a third line' },
  { name: 'paragraph at 137.2', text: PARAGRAPH, left: 30, width: '137.2px', note: 'the defect: a hair wider than its widest line, two lines' },
  { name: 'paragraph at 137.6', text: PARAGRAPH, left: 30, width: '137.6px', note: 'control: two lines however the box rounds' },
  { name: 'label in a flex row', text: 'seven eight', left: 0, width: 'auto', note: 'its own width, which a whole-pixel box rounds down' },
  { name: 'label in a flex row at 0.4', text: 'seven eight', left: 0.4, width: 'auto', note: 'control: the same label where the box rounds up' },
  { name: 'label at 82', text: 'seven eight', left: 0, width: '82px', note: 'a box really narrower than the label, by less than a pixel, breaks it' },
]

const browser = await open()
try {
  await browser.page.setViewportSize({ width: 300, height: 120 })
  const rows = []
  for (const item of CASES) {
    await browser.page.evaluate(
      ({ item, family }) => {
        const text = `font:20px "${family}";line-height:1.2;width:${item.width};flex:none`
        const holder = item.width === 'auto' ? 'display:flex;align-items:flex-start' : 'display:block'
        document.body.innerHTML = `<div style="${holder};margin-left:${item.left}px"><div id=t style='${text}'>${item.text}</div></div>`
      },
      { item, family: FONT.family },
    )
    await settle(browser.page)
    const measured = await browser.page.evaluate(() => {
      const box = document.getElementById('t')
      const range = document.createRange()
      range.selectNodeContents(box)
      const tops = new Set([...range.getClientRects()].map(rect => Math.round(rect.top)))
      const rect = box.getBoundingClientRect()
      return { width: rect.width, height: rect.height, lines: tops.size }
    })
    rows.push(
      [
        JSON.stringify(item.name),
        item.text === PARAGRAPH ? 'paragraph' : JSON.stringify(item.text),
        item.left,
        item.width,
        measured.width.toFixed(3),
        measured.height.toFixed(3),
        measured.lines,
        item.note,
      ].join('\t'),
    )
  }

  const lines = [
    '# Chrome, through `just conformance`: how many lines a box a pixel either side of a line breaks into.',
    `# Text is 20px ${FONT.family}, line-height 1.2; "paragraph" is "${PARAGRAPH}".`,
    '# A stated width is a block at `left`; `auto` is a flex row item as wide as its content.',
    '# Written by packages/meo-canvas/tools/conformance/wrapwidth.mjs.',
    '#',
    '# `width` and `height` are the box from `getBoundingClientRect`; `lines` counts the',
    '# distinct line tops of a range over the text.',
    '#',
    '# case\ttext\tleft\tstated\twidth\theight\tlines\tnote',
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
