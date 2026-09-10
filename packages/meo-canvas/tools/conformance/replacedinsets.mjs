// What insets do to a replaced element, which is not what they do to a box.
//
// Reported as `l7aromeo/meo-canvas#92`: an `Image` with `inset: 0` and no width
// or height is stretched to its containing block. Chrome keeps its intrinsic
// size. A replaced element's `auto` width and height are its own dimensions,
// and CSS resolves the over-constraint by **dropping an inset** rather than by
// stretching the element -- CSS 2.2 §10.3.8 and §10.6.5.
//
// **Measured on a real `<img>`, and that is the whole reason this table
// exists.** A `div` and an `img` give different answers to the same
// declaration, so a harness that models an image as a plain box clears this
// wrongly -- which is how the defect reached the reporter: an inset-only
// workaround was checked in Chrome on a `<div>`, rendered correctly here, and
// would have been wrong in a browser. Every `img` row below is an `<img>`
// element with a real intrinsic size, and the `div` rows are here as the
// contrast rather than as a stand-in.
//
// **The `div` rows are controls that must not move.** A repair that stops
// insets sizing things breaks every non-replaced box, and only a table
// carrying both kinds can see that happen.
//
// The source is an SVG data URL with explicit `width` and `height`, so the
// intrinsic size is stated rather than decoded from pixels, and 60x40 is
// deliberately neither square nor the containing block's shape: a stretched
// box, an intrinsic box and a ratio-preserving box are three different numbers.
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
  // **No insets at all, and this is the row that pins the clamp.** It was
  // missing until the Rust side asked for it: every other row here has a
  // container the art either fits or is stretched into, and this one is the
  // case where a renderer that narrows an intrinsic extent to the space
  // offered reads 60x30 where Chrome reads 60x40.
  ['img no insets', 'img', '', 'absolute, no insets: intrinsic size against a shorter block'],
  // **The end inset alone, both axes.** The rule drops the END inset on an
  // axis where BOTH are set; a lone `right` or `bottom` is untouched by
  // construction. These two rows are what say the rule is scoped rather than
  // merely stated -- and they are what goes red if someone later simplifies it
  // to "drop the end inset for a replaced node", which reads like a tidy-up
  // and is the obvious wrong generalisation.
  ['img right 0 only', 'img', 'right:0;', 'a lone end inset positions and must not be dropped'],
  ['img bottom 0 only', 'img', 'bottom:0;', 'the same on the block axis'],
  ['img inset 0 fill', 'img', 'inset:0;object-fit:fill;', 'object-fit paints, it does not size'],
  ['img inset 0 contain', 'img', 'inset:0;object-fit:contain;', 'the same, and the reporter used fill'],
  ['img inset 0 cover', 'img', 'inset:0;object-fit:cover;', 'the same'],
  ['img inset 0 none', 'img', 'inset:0;object-fit:none;', 'the same'],
  ['img inset 0 scale-down', 'img', 'inset:0;object-fit:scale-down;', 'the same'],

  // **The rows that say the scope is a choice.** A `Text` node measures, like
  // an image, and is *not* replaced — so a rule keyed on "the measurer answered"
  // would have taken these with it. They stretch in Chrome and must keep
  // stretching here. The block-axis numbers are font metrics rather than the
  // property under test: this measures a 12px sans-serif line, and the same
  // scene on our side is two pixels taller for that reason and no other.
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
