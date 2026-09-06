// Where a grid puts its items, for each auto-placement flow.
//
// The four flows differ only when the grid has a **hole** to fill: an item
// that spans two tracks leaves a gap behind it, and `dense` goes back for that
// gap while `row` and `column` do not. A grid of uniform single-cell items
// places them identically under all four, so a table built that way reports
// two working keywords as one.
//
// Rectangles rather than pixels, as with the flex matrix: `getBoundingClientRect`
// is what Chrome laid out, and the equivalent on our side is the bounding box of
// each item's own colour.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/grid-placement.tsv')

/** The grid every case is laid out in: three columns, three rows, no gaps. */
const GRID = { width: 120, height: 90, columns: 3, rows: 3 }

/**
 * The items, in document order.
 *
 * The second spans **all three** columns, which is what leaves a hole for
 * `dense` to come back to: it cannot fit beside the first, so it starts a new
 * line and the two cells beside the first are left empty. A span of two would
 * have fitted in a three-column grid and left nothing behind -- and then the
 * dense flows draw the same picture as their plain counterparts, which is a
 * table reporting two working keywords as one.
 */
const ITEMS = [{ span: 'auto' }, { span: 'column 3' }, { span: 'auto' }, { span: 'auto' }, { span: 'row 2' }, { span: 'auto' }]

const FLOWS = ['row', 'column', 'row dense', 'column dense']

const browser = await open()
try {
  const rows = await browser.page.evaluate(
    ({ grid, items, flows }) => {
      const out = []
      for (const flow of flows) {
        document.body.innerHTML = ''
        const container = document.createElement('div')
        container.style.cssText = `position:absolute;left:0;top:0;display:grid;width:${grid.width}px;height:${grid.height}px;grid-template-columns:repeat(${grid.columns},1fr);grid-template-rows:repeat(${grid.rows},1fr);grid-auto-flow:${flow};`
        for (const item of items) {
          const element = document.createElement('div')
          element.style.cssText = item.span === 'column 3' ? 'grid-column:span 3;' : item.span === 'row 2' ? 'grid-row:span 2;' : ''
          container.append(element)
        }
        document.body.append(container)

        const origin = container.getBoundingClientRect()
        for (const [index, element] of [...container.children].entries()) {
          const rect = element.getBoundingClientRect()
          out.push(
            [
              flow.replace(' ', '-'),
              index,
              Math.round(rect.left - origin.left),
              Math.round(rect.top - origin.top),
              Math.round(rect.width),
              Math.round(rect.height),
            ].join('\t'),
          )
        }
      }
      return out
    },
    { grid: GRID, items: ITEMS, flows: FLOWS },
  )

  const header = [
    '# Chrome, through `just conformance`. Where a grid places its items.',
    '#',
    `# ${GRID.width}x${GRID.height}, ${GRID.columns} equal columns and ${GRID.rows} equal rows, no gaps.`,
    '# Six items in document order; the second spans all three columns and the fifth',
    '# spans two rows.',
    '#',
    '# THE SPANS ARE THE POINT. The four flows place uniform single-cell items',
    '# identically, so a table without a spanning item reports `dense` and its',
    '# non-dense counterpart as the same keyword: `dense` exists only to go back for',
    '# a hole, and an item that spans is what leaves one.',
    '#',
    '# flow\titem\tx\ty\tw\th',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`grid placement: ${FLOWS.length} flows, ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
