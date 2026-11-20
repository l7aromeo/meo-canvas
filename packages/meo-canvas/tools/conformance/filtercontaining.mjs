// Whether a `filter` or `backdrop-filter` makes a box the containing block of
// its absolute and fixed descendants: the child's box, and whether the box's
// own `overflow: hidden` then clips it, read from the child's centre.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/filter-containing-block.tsv')

/** The page, which is the initial containing block and the viewport. */
const PAGE = { width: 400, height: 300 }

/**
 * The scene `chrome_filter_containing_block.rs` builds: a spacer, then the box
 * carrying the property with `overflow: hidden`, then its out-of-flow child.
 * The child's left inset puts it past the box's right edge when the box
 * contains it, so the clip shows only then.
 */
const SCENE = {
  spacer: 50,
  box: { left: 30, width: 100, height: 80 },
  child: { left: 110, top: 10, width: '50%', height: '25%' },
}

/**
 * Each property with the chains either side of the line: `none`, chains that
 * filter nothing and chains that filter, and one Chrome refuses.
 */
const CASES = [
  ['none', ''],
  ['filter', 'none'],
  ['filter', 'brightness(1)'],
  ['filter', 'blur(0px)'],
  ['filter', 'blur(2px)'],
  ['filter', 'blur(-1px)'],
  ['backdrop-filter', 'none'],
  ['backdrop-filter', 'brightness(1)'],
  ['backdrop-filter', 'blur(2px)'],
]

const POSITIONS = ['absolute', 'fixed']

const browser = await open()
try {
  await browser.page.setViewportSize(PAGE)
  const rows = []
  for (const [property, chain] of CASES) {
    for (const position of POSITIONS) {
      const rect = await browser.page.evaluate(
        ({ scene, property, chain, position }) => {
          document.body.style.background = 'white'
          document.body.innerHTML = ''
          const spacer = document.createElement('div')
          spacer.style.cssText = `height:${scene.spacer}px`
          const box = document.createElement('div')
          box.style.cssText = `margin-left:${scene.box.left}px;width:${scene.box.width}px;height:${scene.box.height}px;overflow:hidden;`
          if (property !== 'none') box.style.setProperty(property, chain)
          const child = document.createElement('div')
          child.style.cssText =
            `position:${position};left:${scene.child.left}px;top:${scene.child.top}px;` +
            `width:${scene.child.width};height:${scene.child.height};background:rgb(255,0,0)`
          box.append(child)
          document.body.append(spacer, box)
          const { left, top, width, height } = child.getBoundingClientRect()
          return { left, top, width, height }
        },
        { scene: SCENE, property, chain, position },
      )
      const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...PAGE } }))
      const [r, g, b] = pixel(shot, Math.floor(rect.left + rect.width / 2), Math.floor(rect.top + rect.height / 2))
      const ink = r === 255 && g === 0 && b === 0 ? 'drawn' : r === 255 && g === 255 && b === 255 ? 'clipped' : `${r},${g},${b}`
      rows.push([property, JSON.stringify(chain), position, rect.left, rect.top, rect.width, rect.height, ink].join('\t'))
    }
  }

  const header = [
    '# Chrome, through `just conformance`. Which properties make a containing block.',
    '#',
    `# A ${PAGE.width}x${PAGE.height} page holds a ${SCENE.spacer}px spacer, then a ${SCENE.box.width}x${SCENE.box.height} box at`,
    `# \`margin-left: ${SCENE.box.left}px\` with \`overflow: hidden\` carrying the property, then its child:`,
    `# \`left: ${SCENE.child.left}px; top: ${SCENE.child.top}px; width: ${SCENE.child.width}; height: ${SCENE.child.height}\`, absolute or fixed.`,
    "# The rect is the child's border box from `getBoundingClientRect`; `ink` is what",
    "# its centre shows: `drawn`, or `clipped` by the box's overflow, which only a",
    '# containing block applies. `chain` is a JSON string, set with `setProperty`.',
    '# Written by packages/meo-canvas/tools/conformance/filtercontaining.mjs.',
    '#',
    '# property\tchain\tposition\tleft\ttop\twidth\theight\tink',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`filter containing block: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
