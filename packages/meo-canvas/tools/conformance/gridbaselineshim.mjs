// Where baseline-aligned grid items land, and how tall their row grows, when
// one item is shimmed down to share the other's baseline: the shim and the
// item's margins count toward the row, whatever the track's sizing says.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/grid-baseline-shim.tsv')

/**
 * The row track. `minmax(auto, 60px)` is the shape whose clamp moves and the
 * scene has no `minmax`, so these are the tracks it can state either side.
 */
const ROWS = ['auto', '60px', '30px']

/** A second item's top margin: none, or 10px more for the row to hold. */
const MARGINS = [0, 10]

const browser = await open()
try {
  const rows = await browser.page.evaluate(
    ({ tracks, margins }) => {
      const out = []
      for (const track of tracks) {
        for (const margin of margins) {
          document.body.innerHTML = ''
          const grid = document.createElement('div')
          grid.style.cssText =
            'position:absolute;left:0;top:0;width:100px;display:grid;' + `grid-template-columns:50px 50px;grid-template-rows:${track};align-items:baseline;`
          const items = ['display:grid;padding-top:25px;', `display:grid;padding-bottom:25px;margin-top:${margin}px;`].map(css => {
            const item = document.createElement('div')
            item.style.cssText = css
            const content = document.createElement('div')
            content.style.cssText = 'width:25px;height:25px;'
            item.append(content)
            return item
          })
          grid.append(...items)
          document.body.append(grid)

          const origin = grid.getBoundingClientRect()
          const rects = items.flatMap(item => {
            const { left, top, width, height } = item.getBoundingClientRect()
            return [left - origin.left, top - origin.top, width, height]
          })
          out.push([track, margin, origin.height, ...rects].join('\t'))
        }
      }
      return out
    },
    { tracks: ROWS, margins: MARGINS },
  )

  const header = [
    '# Chrome, through `just conformance`. Baseline-aligned grid items and the row they share.',
    '#',
    '# A 100px-wide grid at the origin, columns `50px 50px`, one row of the track named,',
    '# `align-items: baseline`. Each item is `display: grid` holding a 25x25 box: the',
    '# first with `padding-top: 25px`, the second with `padding-bottom: 25px` and the',
    "# `margin-top` named, so its baseline is shimmed down to the first's. `height` is",
    "# the grid's; the items' border boxes are relative to it, from `getBoundingClientRect`.",
    '# Written by packages/meo-canvas/tools/conformance/gridbaselineshim.mjs.',
    '#',
    '# track\tmargin-top\theight\ta-x\ta-y\ta-w\ta-h\tb-x\tb-y\tb-w\tb-h',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`grid baseline shim: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
