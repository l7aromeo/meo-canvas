// Where a background picture's tiles land: Chrome rounds a tile's position and
// size to whole pixels on an axis it does not repeat along, and keeps both
// fractional on one it does. Read as the half-coverage crossings of the
// picture's edges and its red half, along one row and one column of the box.

import { readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/background-tile.tsv')
const SOURCE = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/split-tile.png')

/** The page, and where the box sits on it. */
const PAGE = { width: 420, height: 220 }
const AT = 10

/**
 * Each case: box size, `background-size`, `background-position`, repeat. The
 * percentage rows are the issue's; the pixel rows split position from size and
 * a half from a fraction; the repeating rows keep their phase; space and round
 * are controls.
 */
const CASES = [
  ['percent', 300, 100, '50% 25%', '50% 50%', 'no-repeat'],
  ['percent odd', 350, 167, '50% 25%', '50% 50%', 'no-repeat'],
  ['percent fractional box', 300.4, 100.6, '50% 25%', '50% 50%', 'no-repeat'],
  ['pixels', 300, 100, '40.4px 20.3px', '10.3px 10.6px', 'no-repeat'],
  ['pixel half', 300, 100, '40px 20px', '10.5px 10.5px', 'no-repeat'],
  ['negative half', 300, 100, '40px 20px', '-10.5px -10.5px', 'no-repeat'],
  ['negative percent', 300, 100, '41px 21px', '-10% -10%', 'no-repeat'],
  ['contain', 333, 150, 'contain', '50% 50%', 'no-repeat'],
  ['cover', 333, 150, 'cover', '50% 50%', 'no-repeat'],
  ['repeat-x pixels', 300, 100, '33.3px 20px', '0px 0px', 'repeat-x'],
  ['repeat-x percent', 301, 100, '11% auto', '0% 50%', 'repeat-x'],
  ['repeat-x phase', 300, 100, '33.3px 20px', '50% 0px', 'repeat-x'],
  ['repeat both', 300, 100, '33.3px 20.3px', '10.3px 10.6px', 'repeat'],
  ['space', 300, 100, '40px 20px', '0px 0px', 'space'],
  ['round', 300, 100, '33.3px 20.3px', '0px 0px', 'round'],
]

/** Every place `value` crosses a half along `n` samples, interpolated, in box px. */
function crossings(n, value) {
  const out = []
  for (let at = 0; at + 1 < n; at++) {
    const [here, next] = [value(at), value(at + 1)]
    if ((here - 0.5) * (next - 0.5) < 0) out.push(+(at + 0.5 + (0.5 - here) / (next - here) - AT).toFixed(3))
  }
  return out
}

const picture = (await readFile(SOURCE)).toString('base64')
const browser = await open()
try {
  await browser.page.setViewportSize(PAGE)
  const rows = []
  for (const [name, width, height, size, position, repeat] of CASES) {
    await browser.page.setContent(
      `<!doctype html><style>html,body{margin:0;background:#fff}</style><div style="position:absolute;left:${AT}px;top:${AT}px;width:${width}px;height:${height}px;background:#fff url(data:image/png;base64,${picture}) ${position}/${size} ${repeat}"></div>`,
    )
    await settle(browser.page)
    const shot = read(await browser.page.screenshot())
    const channel = (x, y, index) => shot.bytes[(y * shot.width + x) * shot.channels + index] / 255
    // Ink is red or blue, both with no green; red is the one with no blue.
    const ink = (x, y) => 1 - channel(x, y, 1)
    const red = (x, y) => 1 - channel(x, y, 2)
    // The row through the middle of the inked rows, and a column two pixels
    // inside the first inked one on it: inside the first tile on both axes.
    const inked = [...Array(PAGE.height).keys()].filter(y => [...Array(PAGE.width).keys()].some(x => ink(x, y) > 0.5))
    if (inked.length === 0) throw new Error(`${name}: nothing inked`)
    const row = Math.floor(((inked[0] ?? 0) + (inked.at(-1) ?? 0)) / 2)
    const firstInk = [...Array(PAGE.width).keys()].find(x => ink(x, row) > 0.5)
    if (firstInk === undefined) throw new Error(`${name}: no ink on row ${row}`)
    const column = firstInk + 2
    rows.push(
      [
        JSON.stringify(name),
        width,
        height,
        size,
        position,
        repeat,
        row - AT,
        column - AT,
        crossings(PAGE.width, x => ink(x, row)).join(','),
        crossings(PAGE.width, x => red(x, row)).join(','),
        crossings(PAGE.height, y => ink(column, y)).join(','),
        crossings(PAGE.height, y => red(column, y)).join(','),
      ].join('\t'),
    )
  }

  const written = table([
    '# Chrome, through `just conformance`. Where background tiles land.',
    `# A ${PAGE.width}x${PAGE.height} white page, the box at ${AT},${AT} on white, the picture`,
    '# `crates/meo-canvas/tests/assets/split-tile.png`: 40x20, its left half red and',
    '# its right half blue.',
    '# Written by packages/meo-canvas/tools/conformance/backgroundtile.mjs.',
    '#',
    '# Positions are box pixels: the half-coverage crossings of ink and of red',
    '# along the row `row` and the column `column`, both inside the first tile.',
    '# On an axis it does not repeat along, Chrome rounds the tile, a half up,',
    '# or the part of it the box shows where the tile crosses the box edge; on',
    '# an axis it repeats along, the tile keeps its fractional size and phase.',
    '# name\twidth\theight\tsize\tposition\trepeat\trow\tcolumn\tink x\tred x\tink y\tred y',
    ...rows,
  ])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    console.log(`wrote ${DESTINATION}`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
