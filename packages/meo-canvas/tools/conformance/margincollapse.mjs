// How tall a block is when its first child's margins collapse through that
// child, for each edge the block puts between them: a bottom padding or border
// keeps the margins out of its height, a top one stops the collapse. Read from
// the block's border box and the top of the sibling after it.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/margin-collapse.tsv')

/** The containing block's width, and the height of the sibling after the block. */
const WIDTH = 50
const SIBLING = 10

/** The block's edges: `none`, or one side of padding or border, or both borders. */
const EDGES = {
  none: '',
  'border-bottom': 'border-bottom:10px solid;',
  'padding-bottom': 'padding-bottom:10px;',
  'border-top': 'border-top:10px solid;',
  'padding-top': 'padding-top:10px;',
  'border-both': 'border-top:10px solid;border-bottom:10px solid;',
}

/** The empty first child's top and bottom margins. */
const MARGINS = [
  [10, 10],
  [10, 0],
  [0, 10],
  [10, 20],
  [20, 10],
]

/** What follows the empty child inside the block: nothing, or a 10px box. */
const AFTER = ['none', 'box']

const browser = await open()
try {
  const rows = await browser.page.evaluate(
    ({ width, sibling, edges, margins, after }) => {
      const out = []
      for (const [edge, css] of Object.entries(edges)) {
        for (const [top, bottom] of margins) {
          for (const then of after) {
            document.body.innerHTML = ''
            const root = document.createElement('div')
            root.style.cssText = `position:absolute;left:0;top:0;width:${width}px;`
            const block = document.createElement('div')
            block.style.cssText = css
            const empty = document.createElement('div')
            empty.style.cssText = `margin-top:${top}px;margin-bottom:${bottom}px;`
            block.append(empty)
            if (then === 'box') {
              const box = document.createElement('div')
              box.style.cssText = 'height:10px;'
              block.append(box)
            }
            const next = document.createElement('div')
            next.style.cssText = `height:${sibling}px;`
            root.append(block, next)
            document.body.append(root)

            const origin = root.getBoundingClientRect()
            const rect = block.getBoundingClientRect()
            out.push([edge, top, bottom, then, rect.top - origin.top, rect.height, next.getBoundingClientRect().top - origin.top].join('\t'))
          }
        }
      }
      return out
    },
    { width: WIDTH, sibling: SIBLING, edges: EDGES, margins: MARGINS, after: AFTER },
  )

  const header = [
    '# Chrome, through `just conformance`. A block whose first child collapses through.',
    '#',
    `# A ${WIDTH}px-wide root at the origin holds the block, then a ${SIBLING}px sibling. The block`,
    '# holds an empty first child with the margins named, then nothing or a 10px box,',
    "# and has the edges named, each 10px. `top` and `height` are the block's border",
    "# box and `next` the sibling's top, relative to the root, from `getBoundingClientRect`.",
    '# Written by packages/meo-canvas/tools/conformance/margincollapse.mjs.',
    '#',
    '# edges\tmargin-top\tmargin-bottom\tafter\ttop\theight\tnext',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`margin collapse: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
