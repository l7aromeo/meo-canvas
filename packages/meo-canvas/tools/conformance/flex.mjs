// Where a flex container puts its children, for every pair of `justify-content`
// and `align-items`.
//
// Thirty combinations, three children each, ninety rows. The children have
// **unequal cross sizes and no height of their own**, which is what makes
// `stretch` a different picture from `flex-start`: an item with a height set
// is stretched to exactly the height it already had, and a matrix built that
// way reports one of its five alignments as a duplicate of another.
//
// Rectangles rather than pixels. `getBoundingClientRect` is what Chrome laid
// out, and the equivalent on our side is the bounding box of each child's own
// colour — the same question asked of two renderers that will never agree on
// an antialiased edge.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/flex-alignment.tsv')

/** The container every case is laid out in. */
const BOX = { width: 160, height: 80 }

/**
 * The three children, by width and by the height of the spacer inside them.
 *
 * The spacer is what gives a child an intrinsic height without giving it a
 * height: `align-items: stretch` then has something to change, and the three
 * differ from each other so `flex-start`, `center` and `flex-end` are three
 * pictures rather than one.
 */
const CHILDREN = [
  { width: 24, content: 20 },
  { width: 30, content: 32 },
  { width: 20, content: 44 },
]

const JUSTIFY = ['flex-start', 'flex-end', 'center', 'space-between', 'space-around', 'space-evenly']
const ALIGN = ['flex-start', 'flex-end', 'center', 'stretch', 'baseline']

/**
 * The wrapping cases, measured with **six** children in a box that fits three.
 *
 * Separate from the matrix because they need twice the children: with three
 * there is nothing to wrap, and a `wrap` that never wrapped would agree with
 * `nowrap` on every row of the matrix above.
 */
const WRAPS = ['nowrap', 'wrap', 'wrap-reverse']

/**
 * The box the wrapping cases use: narrow enough that six children cannot fit.
 *
 * The matrix's 160 fits all six on one line, so measured there `wrap` and
 * `nowrap` agree and the table would report a working property as untested.
 * 88 fits three of them.
 */
const WRAP_BOX = { width: 88, height: 56 }

const browser = await open()
try {
  const rows = await browser.page.evaluate(
    ({ box, children, justifies, aligns }) => {
      const out = []
      for (const justify of justifies) {
        for (const align of aligns) {
          document.body.innerHTML = ''
          const container = document.createElement('div')
          container.style.cssText = `position:absolute;left:0;top:0;display:flex;width:${box.width}px;height:${box.height}px;justify-content:${justify};align-items:${align};`
          for (const child of children) {
            const element = document.createElement('div')
            element.style.cssText = `width:${child.width}px;`
            const spacer = document.createElement('div')
            spacer.style.cssText = `height:${child.content}px;`
            element.append(spacer)
            container.append(element)
          }
          document.body.append(container)

          const origin = container.getBoundingClientRect()
          for (const [index, element] of [...container.children].entries()) {
            const rect = element.getBoundingClientRect()
            out.push(
              [
                justify,
                align,
                index,
                Math.round(rect.left - origin.left),
                Math.round(rect.top - origin.top),
                Math.round(rect.width),
                Math.round(rect.height),
              ].join('\t'),
            )
          }
        }
      }
      return out
    },
    { box: BOX, children: CHILDREN, justifies: JUSTIFY, aligns: ALIGN },
  )

  const wrapped = await browser.page.evaluate(
    ({ box, children, wraps }) => {
      const out = []
      for (const wrap of wraps) {
        document.body.innerHTML = ''
        const container = document.createElement('div')
        container.style.cssText = `position:absolute;left:0;top:0;display:flex;width:${box.width}px;height:${box.height}px;flex-wrap:${wrap};`
        // Six children in a box that fits three, so there is a second line.
        for (const child of [...children, ...children]) {
          const element = document.createElement('div')
          element.style.cssText = `width:${child.width}px;`
          const spacer = document.createElement('div')
          spacer.style.cssText = `height:${child.content}px;`
          element.append(spacer)
          container.append(element)
        }
        document.body.append(container)

        const origin = container.getBoundingClientRect()
        for (const [index, element] of [...container.children].entries()) {
          const rect = element.getBoundingClientRect()
          out.push(
            [
              wrap,
              'six-children',
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
    { box: WRAP_BOX, children: CHILDREN, wraps: WRAPS },
  )
  rows.push(...wrapped)

  const header = [
    '# Chrome, through `just conformance`. Where a flex container puts its children.',
    '#',
    `# Container ${BOX.width}x${BOX.height}. Three children ${CHILDREN.map(c => `${c.width}x${c.content}`).join(', ')},`,
    '# each sized by a spacer inside it rather than by a height of its own -- so',
    '# `align-items: stretch` has something to change. An item with its own height is',
    '# stretched to the height it already had, and a matrix built that way reports one',
    '# of its five alignments as a duplicate of another.',
    '#',
    '# Rectangles are relative to the container and rounded to whole pixels.',
    '#',
    `# The last rows are the wrapping cases: SIX children in a ${WRAP_BOX.width}x${WRAP_BOX.height} box,`,
    '# because with three there is nothing to wrap and a `wrap` that never wrapped',
    '# would agree with `nowrap` on every row of the matrix.',
    '#',
    '# justify\talign\tchild\tx\ty\tw\th',
  ]
  await writeFile(DESTINATION, table([...header, ...rows]), 'utf8')
  process.stderr.write(`flex alignment: ${JUSTIFY.length * ALIGN.length} cases, ${rows.length} rows -> ${DESTINATION}\n`)
} finally {
  await browser.close()
}
