// What a ratio does to a grown flex item's cross size.
//
// Reported as `l7aromeo/meo-canvas#123`: an item with `flex-grow: 1` and
// `aspect-ratio: 1` in a column comes out with no width at all, where the ratio
// should turn its grown height into one. taffy applies the transferred size
// only where the item already has a cross contribution of its own;
// `DioxusLabs/taffy#804` is the upstream defect and
// `crates/meo-canvas-core/tests/taffy_flex_ratio.rs` pins what taffy does.
//
// **Every row names the item's own `display`, and it is our axis rather than
// CSS's.** A block item with content gets the derivation from taffy and a flex
// item with the same content does not -- and Chrome gives `248x248` for all
// four combinations of display and content. So a row that differs by the item's
// display is evidence about taffy, and the rows here say which they used so
// that nobody re-derives it.
//
// **The construction is the reader's axis, not this file's.** A scene built
// through `Box()` is a flex container and one assembled from `Node` values is a
// block one, which cost a control in the sweep behind this table -- `item has
// content` agreed only as a hand-assembled block item. The browser has no such
// distinction, so it is the Rust side that builds each row both ways.
//
// **The container is 440x264 with `box-sizing: border-box` and 8px of padding**,
// so the content box is 424x248 in every row and the numbers below are all
// derived from those two.
import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/flex-ratio-cross.tsv')

/** The container every row shares. Its content box is 424x248. */
const COLUMN = 'display:flex;flex-direction:column;width:440px;height:264px;padding:8px;box-sizing:border-box'
/** The item under test, before whatever the row adds. */
const ITEM = 'flex-grow:1;aspect-ratio:1'
/** A child with a stated size, for the rows that need a contribution. */
const CHILD = '<div style="width:30px;height:10px"></div>'

const CASES = [
  // The reported shape, and the alignment axis it turned out not to depend on.
  ['center', `${COLUMN};align-items:center`, ITEM, '', 'the reported shape'],
  ['flex-start', `${COLUMN};align-items:flex-start`, ITEM, '', 'predicted to differ from centre and does not'],
  ['flex-end', `${COLUMN};align-items:flex-end`, ITEM, '', 'nor does this'],
  ['baseline', `${COLUMN};align-items:baseline`, ITEM, '', 'nor this'],
  // The ratio's own value, including one whose derivation overflows the line.
  ['ratio 0.5', `${COLUMN};align-items:center`, 'flex-grow:1;aspect-ratio:0.5', '', 'below 1'],
  ['ratio 2', `${COLUMN};align-items:center`, 'flex-grow:1;aspect-ratio:2', '', 'above 1, and wider than the container'],
  ['ratio 8', `${COLUMN};align-items:center`, 'flex-grow:1;aspect-ratio:8', '', 'the derivation overflows by 1560'],
  // Growth that is not the whole line.
  ['two siblings', `${COLUMN};align-items:center`, ITEM, 'sibling', 'each takes half the free space'],
  // The four causes of one condition: nothing, padding, a border, content.
  ['item padding', `${COLUMN};align-items:center`, `${ITEM};padding:10px`, '', 'the padding is a contribution and is not the ratio'],
  ['item border', `${COLUMN};align-items:center`, `${ITEM};border:5px solid #000`, '', 'so is a border'],
  ['item flex with content', `${COLUMN};align-items:center`, `${ITEM};display:flex`, 'child', 'a contribution narrower than the derivation'],
  ['item block with content', `${COLUMN};align-items:center`, `${ITEM};display:block`, 'child', 'the one taffy gets right, and only as a block'],
  // Clipping, on each of the two boxes.
  ['item clips', `${COLUMN};align-items:center`, `${ITEM};overflow:hidden`, '', 'the item is a scroll container'],
  ['container clips', `${COLUMN};align-items:center;overflow:hidden`, ITEM, '', 'the container is'],
  ['wrap', `${COLUMN};align-items:center;flex-wrap:wrap`, ITEM, '', 'taffy carries !is_wrap nearby and this does not need it'],
  // Controls: rows nothing should move.
  ['control no grow', `${COLUMN};align-items:center`, 'aspect-ratio:1', '', 'control: nothing grown, nothing to derive from'],
  ['control no ratio', `${COLUMN};align-items:center`, 'flex-grow:1', '', 'control: nothing to transfer'],
  [
    'control auto container',
    `display:flex;flex-direction:column;width:440px;padding:8px;box-sizing:border-box;align-items:center`,
    ITEM,
    '',
    'control: nothing to grow into',
  ],
  ['control height 100%', `${COLUMN};align-items:center`, 'height:100%;aspect-ratio:1', '', 'control: a percentage main size already reaches the ratio'],
  [
    'control row container',
    `display:flex;flex-direction:row;width:440px;height:264px;padding:8px;box-sizing:border-box;align-items:center`,
    ITEM,
    '',
    'control: the axes swap and it is already right',
  ],
  [
    'control stretch no main',
    `display:flex;flex-direction:column;width:440px;padding:8px;box-sizing:border-box;align-items:stretch`,
    ITEM,
    '',
    'control: DioxusLabs/taffy#1081 family, and we agree',
  ],
  // The tolerance, either side of it.
  [
    'derived-tolerance-rounds',
    `${COLUMN};align-items:center`,
    'height:100%;aspect-ratio:0.333333',
    '',
    'a correct row 0.67 from main x ratio: must read as equal',
  ],
  ['derived-tolerance-diverges', `${COLUMN};align-items:center`, `${ITEM};display:flex`, 'child', 'a wrong row 218 from it: must not'],
  // The pin's inequality, either side of it.
  ['pin-fires-min-width', `${COLUMN};align-items:center`, `${ITEM};min-width:300px`, '', 'width/ratio >= height: the pin takes it, not this'],
  ['pin-quiet-empty', `${COLUMN};align-items:center`, ITEM, '', 'width/ratio < height: this takes it, not the pin'],
  // Deliberately not compensated, and a divergence deliberately pinned.
  ['uncompensated stretch', `${COLUMN};align-items:stretch`, ITEM, '', 'the derivation overflows its line; upstream has not shipped it'],
  ['known max-width', `${COLUMN};align-items:center`, `${ITEM};max-width:100px`, '', 'a maximum binding the cross axis does not transfer back here'],
]

const browser = await open()
try {
  await browser.page.setViewportSize({ width: 1400, height: 1400 })
  const rows = []
  for (const [key, container, item, extra, note] of CASES) {
    await browser.page.evaluate(
      ({ container, item, extra, child }) => {
        const inner = extra === 'child' ? child : ''
        const sibling = extra === 'sibling' ? `<div style="${item}"></div>` : ''
        document.body.innerHTML =
          `<div id="c" style="${container};background:#eee">` + `<div id="m" style="${item};background:#00c">${inner}</div>${sibling}</div>`
      },
      { container, item, extra, child: CHILD },
    )
    await settle(browser.page)
    const box = await browser.page.evaluate(() => {
      const element = document.getElementById('m')
      if (element === null) throw new Error('the case has no #m to measure')
      const rect = element.getBoundingClientRect()
      return { width: rect.width, height: rect.height }
    })
    const display = item.includes('display:flex') ? 'flex' : item.includes('display:block') ? 'block' : 'default'
    rows.push([key, box.width.toFixed(2), box.height.toFixed(2), display, note].join('\t'))
  }

  const written = table([
    '# What a ratio does to a grown flex item’s cross size.',
    '# Container 440x264, border-box, 8px padding, so a 424x248 content box.',
    '# Written by packages/meo-canvas/tools/conformance/flexratiocross.mjs.',
    '#',
    '# `l7aromeo/meo-canvas#123`, upstream `DioxusLabs/taffy#804`. taffy applies',
    '# the transferred size only where the item has a cross contribution of its',
    '# own; Chrome derives it regardless.',
    '#',
    '# **The `display` column is the item’s own, and it is our axis rather than',
    '# CSS’s** — Chrome gives the same answer for all four combinations of',
    '# display and content, and taffy does not. A row differing by it is evidence',
    '# about taffy.',
    '#',
    '# key\twidth\theight\tdisplay\tnote',
    ...rows,
  ])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`flex ratio cross: ${CASES.length} cases -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
