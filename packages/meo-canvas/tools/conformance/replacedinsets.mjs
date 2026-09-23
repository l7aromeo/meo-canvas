// What insets do to a replaced element (`l7aromeo/meo-canvas#92`): its `auto` size
// is its own, and CSS 2.2 §10.3.8 and §10.6.5 drop an inset rather than stretch it.
// Measured on a real `<img>` of intrinsic 60x40, since a `div` answers differently;
// the `div` rows are controls that must keep stretching.
import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/replaced-insets.tsv')

/** The containing block: wider and shorter than the art, so stretching shows. */
const BLOCK = { width: 200, height: 30 }

/** The art: 60x40, intrinsic size declared on the SVG root. */
const ART = { width: 60, height: 40 }
const SRC = `data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="${ART.width}" height="${ART.height}"><rect width="100%" height="100%" fill="%23c00"/></svg>`,
)}`

/** Every case: the element, the declarations under test, and why it is here. */
const CASES = [
  ['img inset 0', 'img', 'inset:0;', 'the defect: no width or height, insets alone'],
  ['img inset 0 + size 100%', 'img', 'inset:0;width:100%;height:100%;', 'an explicit size wins over the intrinsic one'],
  ['img size 100%', 'img', 'width:100%;height:100%;', 'control: sizing without insets must not move'],
  ['div inset 0', 'div', 'inset:0;', 'control: a non-replaced box DOES stretch to its insets'],
  ['div inset 0 + size 100%', 'div', 'inset:0;width:100%;height:100%;', 'control: and agrees with itself when sized'],
  ['img top 0 only', 'img', 'top:0;', 'one inset cannot over-constrain anything'],
  ['img left 0 right 0', 'img', 'left:0;right:0;', 'opposing insets on one axis: which one is dropped'],
  ['img top 0 bottom 0', 'img', 'top:0;bottom:0;', 'the same question on the other axis'],
  // No insets: pins the clamp. A renderer narrowing an intrinsic extent to the
  // space offered reads 60x30 where Chrome reads 60x40.
  ['img no insets', 'img', '', 'absolute, no insets: intrinsic size against a shorter block'],
  // A lone end inset, both axes: the end inset is dropped only where both are set,
  // and these go red if the rule is generalised to every replaced node.
  ['img right 0 only', 'img', 'right:0;', 'a lone end inset positions and must not be dropped'],
  ['img bottom 0 only', 'img', 'bottom:0;', 'the same on the block axis'],
  // These five record the border box, not where `object-fit` puts the picture
  // inside it -- that is `object-fit.tsv`. They show a rule that changes the box.
  ['img inset 0 fill', 'img', 'inset:0;object-fit:fill;', 'the box under fill; the picture is object-fit.tsv'],
  ['img inset 0 contain', 'img', 'inset:0;object-fit:contain;', 'the same, and the reporter used fill'],
  ['img inset 0 cover', 'img', 'inset:0;object-fit:cover;', 'the same'],
  ['img inset 0 none', 'img', 'inset:0;object-fit:none;', 'the same'],
  ['img inset 0 scale-down', 'img', 'inset:0;object-fit:scale-down;', 'the same'],

  // A `Text` node measures but is not replaced, so it stretches in Chrome and must
  // here. Its block-axis numbers are font metrics: our 12px line is two pixels taller.
  ['text inset 0', 'text', 'inset:0;', 'a non-replaced measured node stretches both axes'],
  ['text top 0 bottom 0', 'text', 'top:0;bottom:0;', 'the block axis stretches, the inline one shrink-fits'],
  ['text no insets', 'text', '', 'content size, and the height here is a font metric'],
]

const browser = await open()
try {
  await browser.page.setViewportSize({ width: 400, height: 200 })
  const rows = []

  for (const [key, tag, declarations, note] of CASES) {
    await browser.page.evaluate(
      ({ block, src, tag, declarations }) => {
        document.body.innerHTML = ''
        const parent = document.createElement('div')
        parent.style.cssText = `position:relative;width:${block.width}px;height:${block.height}px;background:#eee;`
        const element = document.createElement(tag === 'text' ? 'div' : tag)
        if (tag === 'img') element.src = src
        if (tag === 'text') {
          element.textContent = 'x'
          element.style.font = '12px sans-serif'
        }
        element.id = 'm'
        element.style.cssText = `position:absolute;background:#00c;${declarations}`
        parent.append(element)
        document.body.append(parent)
      },
      { block: BLOCK, src: SRC, tag, declarations },
    )
    // The decode has to finish before the intrinsic size exists at all: an
    // undecoded `<img>` measures 0x0 and would read as "Chrome does not
    // stretch it" for entirely the wrong reason.
    await browser.page.evaluate(async () => {
      const element = document.getElementById('m')
      if (element instanceof HTMLImageElement) await element.decode()
    })
    await settle(browser.page)

    const box = await browser.page.evaluate(() => {
      const element = document.getElementById('m')
      if (element === null) throw new Error('the case has no #m to measure')
      const rect = element.getBoundingClientRect()
      const parent = element.parentElement?.getBoundingClientRect() ?? rect
      // Parent-relative, so the row says where in the containing block it
      // landed rather than where the page happened to put the block.
      return { x: rect.left - parent.left, y: rect.top - parent.top, width: rect.width, height: rect.height }
    })
    rows.push([key, box.x.toFixed(2), box.y.toFixed(2), box.width.toFixed(2), box.height.toFixed(2), declarations, note].join('\t'))
  }

  const written = table([
    '# What insets do to a replaced element, measured on a real `<img>`.',
    `# Containing block ${BLOCK.width}x${BLOCK.height}, art ${ART.width}x${ART.height} as an SVG data URL.`,
    '# Written by packages/meo-canvas/tools/conformance/replacedinsets.mjs.',
    '#',
    '# A replaced element’s `auto` width and height are its own dimensions, and',
    '# CSS drops an inset rather than stretching it -- CSS 2.2 §10.3.8, §10.6.5.',
    '# The `div` rows are the contrast and must keep stretching.',
    '#',
    '#',
    '# `x` and `y` are parent-relative, and they are what says WHICH inset was',
    '# dropped when both on an axis were given: the element sits against the',
    '# one that was honoured.',
    '#',
    '# **Every number here is a border box, read from getBoundingClientRect.**',
    '# What the browser paints INSIDE that box is not recorded and cannot be:',
    '# the five `object-fit` rows say what each rule does to the element’s box,',
    '# not where it puts the picture. That question is `object-fit.tsv`, whose',
    '# walker reads pixels, and it takes three separators rather than one: the',
    '# rectangle for `contain`; the magenta and cyan columns for `cover` against',
    '# `fill`, which share a rectangle and differ in that only `cover` crops the',
    '# art’s corner marks out of the cell; and a cell narrower than the art for',
    '# `scale-down` against `none`, identical at 72x72 and different at 6x6.',
    '#',
    '# case\tx\ty\twidth\theight\tdeclarations\tnote',
    ...rows,
  ])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`replaced insets: ${CASES.length} cases -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
