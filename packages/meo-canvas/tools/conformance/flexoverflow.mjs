// Where a flex container puts lines and items that overflow it, for every
// `align-content` and `justify-content` value: CSS's fallback for an
// overflowing `normal` or `stretch` is `flex-start`, which a reversed axis puts
// at the far edge, and the other values each fall back their own way.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/flex-overflow.tsv')

/** The container every case is laid out in, and where it sits on the page. */
const BOX = { left: 60, top: 60, size: 40 }

/**
 * The lines cases: two children a line each, 30 across the cross axis, so two
 * lines need 60 of the 40 there is. `flex-shrink: 0` keeps them their size.
 */
const LINE = { main: 40, cross: 30 }

/** The items case: one child 47 along the main axis, 7 more than there is. */
const ITEM = { main: 47, cross: 20 }

/**
 * The wrapped case: that child alone on a line, then two that fit on the next,
 * so one line of the container overflows and the other has space to share.
 */
const WRAPPED = [
  { main: 47, cross: 10 },
  { main: 15, cross: 10 },
  { main: 15, cross: 10 },
]

/** `normal` is the property unset, which is how a scene says it. */
const ALIGN_CONTENT = ['normal', 'stretch', 'flex-start', 'flex-end', 'center', 'space-between', 'space-around', 'space-evenly']
const JUSTIFY_CONTENT = ['normal', 'flex-start', 'flex-end', 'center', 'space-between', 'space-around', 'space-evenly']

/** Both writing directions: `rtl` puts `start` on the right of a row and a column's cross axis. */
const DIRS = ['ltr', 'rtl']

const LINE_CASES = DIRS.flatMap(dir =>
  ['row', 'column'].flatMap(direction => ['wrap', 'wrap-reverse'].flatMap(wrap => ALIGN_CONTENT.map(value => ({ dir, direction, wrap, value })))),
)
const ITEM_CASES = DIRS.flatMap(dir =>
  ['row', 'row-reverse', 'column', 'column-reverse'].flatMap(direction => JUSTIFY_CONTENT.map(value => ({ dir, direction, wrap: 'nowrap', value }))),
)
const WRAPPED_CASES = ITEM_CASES.map(({ dir, direction, value }) => ({ dir, direction, wrap: 'wrap', value }))

const browser = await open()
try {
  const measure = (part, cases, property, children) =>
    browser.page.evaluate(
      ({ box, part, cases, property, children }) => {
        const out = []
        for (const { dir, direction, wrap, value } of cases) {
          document.body.innerHTML = ''
          const container = document.createElement('div')
          container.style.cssText =
            `position:absolute;left:${box.left}px;top:${box.top}px;display:flex;` +
            `width:${box.size}px;height:${box.size}px;direction:${dir};flex-direction:${direction};flex-wrap:${wrap};`
          if (value !== 'normal') container.style.setProperty(property, value)
          const row = direction.startsWith('row')
          for (const child of children) {
            const element = document.createElement('div')
            const [width, height] = row ? [child.main, child.cross] : [child.cross, child.main]
            element.style.cssText = `width:${width}px;height:${height}px;flex-shrink:0;`
            container.append(element)
          }
          document.body.append(container)

          const origin = container.getBoundingClientRect()
          for (const [index, element] of [...container.children].entries()) {
            const rect = element.getBoundingClientRect()
            out.push([part, dir, direction, wrap, value, index, rect.left - origin.left, rect.top - origin.top, rect.width, rect.height].join('\t'))
          }
        }
        return out
      },
      { box: BOX, part, cases, property, children },
    )

  const rows = [
    ...(await measure('lines', LINE_CASES, 'align-content', [LINE, LINE])),
    ...(await measure('items', ITEM_CASES, 'justify-content', [ITEM])),
    ...(await measure('wrapped', WRAPPED_CASES, 'justify-content', WRAPPED)),
  ]

  const header = [
    '# Chrome, through `just conformance`. Where a flex container puts what overflows it.',
    '#',
    `# A ${BOX.size}x${BOX.size} flex container at ${BOX.left},${BOX.top} on the page.`,
    `# \`lines\`: two children ${LINE.main} along the main axis and ${LINE.cross} across it, a line each`,
    `# under \`flex-wrap\`, so the lines need ${2 * LINE.cross} of the ${BOX.size} there is, for every \`align-content\`.`,
    `# \`items\`: one child ${ITEM.main} along the main axis and ${ITEM.cross} across, for every \`justify-content\`.`,
    `# \`wrapped\`: under \`flex-wrap: wrap\`, children ${WRAPPED.map(c => c.main).join(', ')} along the main axis and ${WRAPPED[0].cross} across,`,
    '# so the first is alone on an overflowing line and the next two share one that fits.',
    '# Every case in both writing directions, `dir`. Every child has `flex-shrink: 0`.',
    '# `normal` is the property unset.',
    '# Rectangles are the children relative to the container, from `getBoundingClientRect`.',
    '# Written by packages/meo-canvas/tools/conformance/flexoverflow.mjs.',
    '#',
    '# part\tdir\tdirection\twrap\tvalue\tchild\tx\ty\tw\th',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`flex overflow: ${LINE_CASES.length + ITEM_CASES.length + WRAPPED_CASES.length} cases, ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
