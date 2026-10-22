// How wide a wrapping text item is in each container (`l7aromeo/meo-canvas#162`).
// Its width is fit-content wherever it is not stretched, and that is the space on
// offer, never the widest line it wrapped to. The `widest` column is that line, so
// a row where the two coincide is visibly unable to tell them apart.
import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/flex-column-text.tsv')

/** The container's width, and the words that wrap inside it at the size below. */
const WIDTH = 200
const TEXT = 'one two three four five six seven eight'
const FONT = '20px Fixture'
const LINE_HEIGHT = 1.2

/**
 * Each container by the name the table keys it on: its CSS, and the item's own.
 * Every container is placed at the page origin, so its rect is the frame.
 */
const COLUMN = `width:${WIDTH}px;display:flex;flex-direction:column`
const CONTAINERS = {
  'column flex-start': { box: `${COLUMN};align-items:flex-start`, item: '' },
  'column flex-start, text right': { box: `${COLUMN};align-items:flex-start`, item: 'text-align:right' },
  'column center': { box: `${COLUMN};align-items:center`, item: '' },
  'column stretch': { box: `${COLUMN};align-items:stretch`, item: '' },
  'row flex-start': { box: `width:${WIDTH}px;display:flex;flex-direction:row;align-items:flex-start`, item: '' },
  block: { box: `width:${WIDTH}px;display:block`, item: '' },
  grid: { box: `width:${WIDTH}px;display:grid;grid-template-columns:${WIDTH}px;align-items:start`, item: '' },
  'absolute shrink-to-fit': { box: `max-width:${WIDTH}px;display:block`, item: '' },
}

/** The item's margins, by the name the table keys them on. */
const MARGINS = { '0 20 0 30': '0 20px 0 30px', none: '0' }

/** Why each container is here, in the row rather than in a comment above the table. */
const NOTES = {
  'column flex-start': 'the defect: fit-content on the cross axis',
  'column flex-start, text right': 'the line sits against the right edge of the fit-content box',
  'column center': 'fit-content too, then centred',
  'column stretch': 'control: stretched, so the measurement never sizes it',
  'row flex-start': 'control: sized on the main axis by the flex algorithm',
  block: 'control: a block fills the line',
  grid: 'control: `justify-items` is normal, which stretches; the scene has no `justify-items`',
  'absolute shrink-to-fit': 'the container is fit-content itself, and its block child fills it',
}

/** The page shot for ink: the container and its overflow, whatever the row. */
const SHOT = { x: 0, y: 0, width: 300, height: 120 }

/** A red channel below this is glyph, over the item's #cc0000 or the white page. */
const GLYPH = 100

const browser = await open()
try {
  const rows = []
  for (const [container, { box, item }] of Object.entries(CONTAINERS)) {
    for (const [margin, value] of Object.entries(MARGINS)) {
      await browser.page.evaluate(
        ({ box, item, value, text, font, lineHeight }) => {
          document.body.innerHTML = ''
          const frame = document.createElement('div')
          frame.id = 'box'
          frame.style.cssText = `position:absolute;left:0;top:0;${box}`
          const element = document.createElement('div')
          element.id = 'item'
          element.style.cssText = `margin:${value};font:${font};line-height:${lineHeight};color:#000;background:#cc0000;${item}`
          element.textContent = text
          frame.append(element)
          document.body.append(frame)
        },
        { box, item, value, text: TEXT, font: FONT, lineHeight: LINE_HEIGHT },
      )
      // Waited for by the harness rather than by the page: a measurement taken
      // before the face is applied reads the fallback's lines.
      await settle(browser.page)
      const measured = await browser.page.evaluate(() => {
        const origin = document.getElementById('box').getBoundingClientRect()
        const element = document.getElementById('item')
        const rect = element.getBoundingClientRect()
        // One client rect per line box the text was broken into.
        const range = document.createRange()
        range.selectNodeContents(element)
        const lines = [...range.getClientRects()]
        return {
          x: rect.left - origin.left,
          y: rect.top - origin.top,
          w: rect.width,
          h: rect.height,
          lines: lines.length,
          widest: Math.max(...lines.map(line => line.width)),
        }
      })
      const shot = read(await browser.page.screenshot({ clip: SHOT }))
      let [left, right] = [null, null]
      for (let x = 0; x < SHOT.width; x += 1) {
        for (let y = 0; y < SHOT.height; y += 1) {
          if (pixel(shot, x, y)[0] >= GLYPH) continue
          left = left ?? x
          right = x
          break
        }
      }
      rows.push(
        [
          container,
          margin,
          measured.x,
          measured.y,
          measured.w,
          measured.h,
          measured.lines,
          measured.widest.toFixed(3),
          `${left},${right}`,
          NOTES[container],
        ].join('\t'),
      )
    }
  }

  const header = [
    "# Chrome, through `just conformance`. A wrapping text item's width, per container.",
    `# A container, ${WIDTH}px or at most that, holding one item: \`${TEXT}\`,`,
    `# ${FONT}, line-height ${LINE_HEIGHT}, black on #cc0000.`,
    '# Written by packages/meo-canvas/tools/conformance/flexcolumntext.mjs.',
    '#',
    "# `x y w h` is the item's border box from `getBoundingClientRect`, relative to",
    '# the container. `lines` and `widest` are the line boxes the text broke into,',
    '# from a range over it: a row whose `widest` equals its `w` cannot tell',
    '# fit-content from the widest line, and every row here keeps them apart.',
    '# `ink` is the first and last column of glyph, black text over the box, where',
    '# a red channel under 100 counts: where the lines sit inside it.',
    '#',
    '# container\tmargin\tx\ty\tw\th\tlines\twidest\tink\tnote',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`flex column text: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
