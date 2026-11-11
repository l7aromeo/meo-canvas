// The line box Chrome gives a line of text, `line-height: normal` and three
// multiples, on faces whose line gaps differ: a block's height, and a zero-height
// inline-block's top, which sits on the baseline. `chrome_line_height.rs` reads
// the table; Chrome's `normal` adds the face's line gap to its ascent and descent.

import { readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { FONT, open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/line-height-normal.tsv')
const FONTS_DIR = resolve(HERE, '../../../../crates/meo-canvas-core/tests/assets/fonts')

/**
 * The faces, by the family the test registers them under: the repository's own,
 * whose line gap is 0, and Poppins, whose hhea and typo metrics give 100 of 1000.
 */
const FACES = [
  { family: FONT.family, file: null },
  { family: 'Poppins', file: 'Poppins-Regular.ttf' },
]

/**
 * Sizes where the three parts of `normal` round differently from their sum:
 * at 13px on Poppins they are 13.65, 4.55 and 1.3, rounded apart to 14, 5 and 1.
 */
const SIZES = [12, 13, 15, 16, 17, 20, 25, 40]

/**
 * `null` is `normal`; the multiples test that the box is the content area plus
 * leading, and 1.5 makes a box that is not a whole pixel at an odd size.
 */
const LINE_HEIGHTS = [null, 1, 1.5, 2]

/** One line and two, since a short box per line accumulates down a paragraph. */
const LINES = [1, 2]

/** The text: a cap and a descender, as the report's own string. */
const TEXT = 'Ag'

/**
 * Lines holding a span of another face or size, at `normal`, as `[block family,
 * block size, span family, span size]`: each inline box carries its own
 * half-leading, so the line box is the union of the boxes, not the largest
 * ascent, descent and gap. 13px is where the two give the same box and 15 and
 * 25 are where they part; a 25px span in a 13px block asks it across sizes.
 */
const MIXED = [
  ...[13, 15, 25].flatMap(size => [
    [FONT.family, size, 'Poppins', size],
    ['Poppins', size, FONT.family, size],
  ]),
  ['Poppins', 13, 'Poppins', 25],
  [FONT.family, 13, FONT.family, 25],
  [FONT.family, 13, 'Poppins', 25],
  ['Poppins', 13, FONT.family, 25],
]

const browser = await open()
try {
  await browser.page.setViewportSize({ width: 800, height: 400 })
  for (const face of FACES) {
    if (face.file === null) continue
    const bytes = await readFile(resolve(FONTS_DIR, face.file))
    await browser.page.evaluate(
      async ({ family, data }) => {
        const loaded = await new FontFace(family, `url(data:font/ttf;base64,${data})`).load()
        document.fonts.add(loaded)
      },
      { family: face.family, data: bytes.toString('base64') },
    )
    const ready = await browser.page.evaluate(family => document.fonts.check(`16px "${family}"`), face.family)
    if (!ready) throw new Error(`${face.family} did not load`)
  }
  await settle(browser.page)

  /** The block's box and first baseline: one line or two of the text, and a span after the first. */
  const measure = (family, size, multiple, lines, span) =>
    browser.page.evaluate(
      ({ family, size, multiple, lines, span, text }) => {
        const block = ruler => {
          const div = document.createElement('div')
          div.style.cssText = `font:${size}px "${family}";line-height:${multiple === null ? 'normal' : multiple};width:400px`
          for (let line = 0; line < lines; line += 1) {
            if (line > 0) div.append(document.createElement('br'))
            div.append(text)
          }
          if (span !== null) {
            const inner = document.createElement('span')
            inner.style.cssText = `font:${span.size}px "${span.family}"`
            inner.textContent = text
            div.firstChild.after(inner)
          }
          let mark = null
          if (ruler) {
            mark = document.createElement('span')
            mark.style.cssText = 'display:inline-block;width:0;height:0'
            div.firstChild.after(mark)
          }
          document.body.append(div)
          const top = div.getBoundingClientRect().top
          const result = { height: div.getBoundingClientRect().height, baseline: mark === null ? null : mark.getBoundingClientRect().top - top }
          div.remove()
          return result
        }
        // The box from bare text, since the ruler can hold a short line box open.
        return { box: block(false).height, baseline: block(true).baseline }
      },
      { family, size, multiple, lines, span, text: TEXT },
    )
  const row = (family, multiple, size, lines, span, measured) =>
    [
      family,
      multiple === null ? 'normal' : multiple,
      size,
      lines,
      span === null ? '-' : span.family,
      span === null ? '-' : span.size,
      measured.box.toFixed(3),
      measured.baseline.toFixed(3),
    ].join('\t')

  const rows = []
  for (const face of FACES) {
    for (const multiple of LINE_HEIGHTS) {
      for (const size of SIZES) {
        for (const lines of LINES) {
          if (lines === 2 && multiple !== null) continue
          rows.push(row(face.family, multiple, size, lines, null, await measure(face.family, size, multiple, lines, null)))
        }
      }
    }
  }
  for (const [family, size, spanFamily, spanSize] of MIXED) {
    const span = { family: spanFamily, size: spanSize }
    rows.push(row(family, null, size, 1, span, await measure(family, size, null, 1, span)))
  }

  const header = [
    '# Chrome, through `just conformance`: line boxes at `line-height: normal` and three multiples.',
    `# The text is ${JSON.stringify(TEXT)}, on faces whose line gaps differ, from HTML elements.`,
    '# Written by packages/meo-canvas/tools/conformance/lineheightnormal.mjs.',
    '#',
    '# The block is 400px wide, its lines broken by <br>. `box` is its height; `baseline` is the',
    "# first line's baseline from the block's top, read from a zero-height inline-block. A row with",
    "# a span puts the text again, in `span_family` at `span_size`, after the first line's text.",
    '#',
    '# family\tline_height\tsize\tlines\tspan_family\tspan_size\tbox\tbaseline',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    console.log(`wrote ${DESTINATION}`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
