// What `box-sizing` moves, across three parent display types: a complete 72-row
// cross-product. A third of the rows cannot tell the sizings apart and are the
// control. The content width is a grandchild filling the child, since a sample
// comes from a reported rectangle, never a computed style.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/box-sizing.tsv')

/** The host every child sits in, and the height every child takes. */
const HOST = { width: 200, height: 40 }

const PARENTS = ['block', 'flex', 'grid']
const SIZINGS = ['content-box', 'border-box']
const WIDTHS = ['100px', '50%', 'auto']
const BORDERS = [0, 6]
const PADDINGS = [0, 10]

const browser = await open()
try {
  const rows = await browser.page.evaluate(
    ({ host, parents, sizings, widths, borders, paddings }) => {
      const out = []
      for (const parent of parents) {
        for (const sizing of sizings) {
          for (const width of widths) {
            for (const border of borders) {
              for (const padding of paddings) {
                document.body.innerHTML = ''
                const container = document.createElement('div')
                container.style.cssText = `position:relative;display:${parent};width:${host.width}px;`

                const child = document.createElement('div')
                // `auto` is the absence of a width rather than a value, which
                // is what makes it the row that cannot discriminate: a child
                // saying nothing about its width is sized by its parent under
                // either sizing.
                child.style.cssText =
                  `display:block;height:${host.height}px;box-sizing:${sizing};` +
                  `border:${border}px solid #141414;padding:${padding}px;` +
                  (width === 'auto' ? '' : `width:${width};`)

                // A grandchild filling the content box, so its rectangle is the
                // content width -- a laid-out box rather than a computed `width`.
                const inner = document.createElement('div')
                inner.style.cssText = 'width:100%;height:100%;'
                child.append(inner)
                container.append(child)
                document.body.append(container)

                out.push(
                  [
                    parent,
                    sizing,
                    width,
                    border,
                    padding,
                    Math.round(child.getBoundingClientRect().width),
                    Math.round(inner.getBoundingClientRect().width),
                  ].join('\t'),
                )
              }
            }
          }
        }
      }
      return out
    },
    { host: HOST, parents: PARENTS, sizings: SIZINGS, widths: WIDTHS, borders: BORDERS, paddings: PADDINGS },
  )

  const header = [
    '# Chrome, through `just conformance`. What `box-sizing` moves.',
    '#',
    `# A ${HOST.width}px host in each of ${PARENTS.join(', ')}; the child is ${HOST.height}px tall,`,
    "# `display: block`, with the row's sizing, width, border and padding. `outer` is",
    "# the child's own border box; `content` is a grandchild at 100% of it.",
    '#',
    `# ${PARENTS.length} parents x ${SIZINGS.length} sizings x ${WIDTHS.length} widths x ${BORDERS.length} borders x ${PADDINGS.length} paddings`,
    `# = ${PARENTS.length * SIZINGS.length * WIDTHS.length * BORDERS.length * PADDINGS.length} rows, a complete cross-product with no gaps.`,
    '#',
    '# THE COLUMN LINE BELOW IS ASSERTED BY THE READER, not skipped. A tab-separated',
    '# table is positional, so a column emitted in a different order would be read as',
    '# a different field and the test would stay green measuring something else.',
    '#',
    '# parent\tsizing\twidth\tborder\tpadding\touter\tcontent',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`box sizing: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
