// Where a transformed box inks, which is what the order of `translate`, `rotate`
// and `scale` decides: CSS translates last, so the move is neither turned nor
// scaled. Every row is spelled as the individual properties and as one
// `transform` list, and the two must agree.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'
import { read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/transform-order.tsv')

/** The page, and the box at its padding: room on every side for either order. */
const PAGE = 100
const BOX = { at: 20, side: 20, colour: [0x28, 0x50, 0xdc] }

/**
 * Each case's transform, as the walker builds it: lengths in `px` or `%`,
 * degrees, factors. Rotations are quarter turns and lengths whole pixels, so
 * the ink is an axis-aligned rectangle of exact pixels and both orders land on
 * the canvas, apart; the coincident `translate 20px 0, rotate 90` is kept as
 * the issue's row beside one that moves.
 */
const CASES = [
  { name: 'none', translate: ['0px', '0px'], rotate: 0, scale: [1, 1], origin: ['0px', '0px'] },
  { name: 'translate alone', translate: ['20px', '0px'], rotate: 0, scale: [1, 1], origin: ['0px', '0px'] },
  { name: 'translate, rotate', translate: ['20px', '0px'], rotate: 90, scale: [1, 1], origin: ['0px', '0px'] },
  { name: 'translate both, rotate', translate: ['20px', '10px'], rotate: 90, scale: [1, 1], origin: ['0px', '0px'] },
  { name: 'translate, scale', translate: ['20px', '0px'], rotate: 0, scale: [2, 2], origin: ['0px', '0px'] },
  { name: 'percentage translate, scale', translate: ['50%', '25%'], rotate: 0, scale: [2, 2], origin: ['0px', '0px'] },
  { name: 'all three about an origin', translate: ['10px', '-5px'], rotate: 90, scale: [2, 1], origin: ['25%', '50%'] },
]

/** The two spellings of one case, which Chrome must resolve alike. */
function spellings({ translate, rotate, scale, origin }) {
  const [tx, ty] = translate
  return [
    `translate: ${tx} ${ty}; rotate: ${rotate}deg; scale: ${scale[0]} ${scale[1]}`,
    `transform: translate(${tx}, ${ty}) rotate(${rotate}deg) scale(${scale[0]}, ${scale[1]})`,
  ].map(css => `transform-origin: ${origin[0]} ${origin[1]}; ${css}`)
}

/**
 * The box's ink: the bounding rectangle of its colour, refused unless every
 * pixel inside is that colour, everything else is the white page, and the
 * rectangle is clear of the canvas edge, where a clipped reading looks whole.
 */
function ink(image, css) {
  let [left, top, right, bottom, count] = [PAGE, PAGE, -1, -1, 0]
  for (let y = 0; y < PAGE; y++) {
    for (let x = 0; x < PAGE; x++) {
      const at = (y * image.width + x) * image.channels
      const rgb = [...image.bytes.subarray(at, at + 3)]
      if (rgb.every((channel, index) => channel === BOX.colour[index])) {
        count++
        ;[left, top, right, bottom] = [Math.min(left, x), Math.min(top, y), Math.max(right, x), Math.max(bottom, y)]
      } else if (!rgb.every(channel => channel === 255)) {
        throw new Error(`${css}: (${x}, ${y}) is ${rgb}, neither the box nor the page`)
      }
    }
  }
  if (left <= 0 || top <= 0 || right >= PAGE - 1 || bottom >= PAGE - 1) {
    throw new Error(`${css}: the ink ${[left, top, right, bottom]} reaches the canvas edge`)
  }
  if (count !== (right - left + 1) * (bottom - top + 1)) {
    throw new Error(`${css}: ${count} pixels inked in a ${right - left + 1}x${bottom - top + 1} rectangle`)
  }
  return [left, top, right, bottom]
}

const browser = await open()
try {
  await browser.page.setViewportSize({ width: PAGE, height: PAGE })
  const lines = [
    '# Chrome, through `just conformance`. Where a transformed box inks.',
    `# ${PAGE}x${PAGE} white page, a ${BOX.side}x${BOX.side} #2850dc block at its ${BOX.at}px padding.`,
    '# Each row agrees as the individual properties and as one `transform` list.',
    '# Written by packages/meo-canvas/tools/conformance/transformorder.mjs.',
    '# The ink columns are inclusive pixel indices.',
    '# case\ttranslate x\ttranslate y\trotate\tscale x\tscale y\torigin x\torigin y\tleft\ttop\tright\tbottom',
  ]
  for (const row of CASES) {
    const rectangles = []
    for (const css of spellings(row)) {
      await browser.page.evaluate(
        ({ css, box, page }) => {
          document.body.setAttribute('style', `background: #fff; padding: ${box.at}px; width: ${page - 2 * box.at}px; height: ${page - 2 * box.at}px`)
          document.body.innerHTML = `<div style="width: ${box.side}px; height: ${box.side}px; background: #2850dc; ${css}"></div>`
        },
        { css, box: BOX, page: PAGE },
      )
      const shot = await browser.page.screenshot({ clip: { x: 0, y: 0, width: PAGE, height: PAGE } })
      rectangles.push(ink(read(shot), css))
    }
    if (rectangles[0].join() !== rectangles[1].join()) {
      throw new Error(`${row.name}: the properties ink ${rectangles[0]} and the list ${rectangles[1]}`)
    }
    lines.push([JSON.stringify(row.name), ...row.translate, row.rotate, ...row.scale, ...row.origin, ...rectangles[0]].join('\t'))
  }

  const written = table(lines)
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    console.log(`wrote ${DESTINATION}`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
