// What a definite width does to a replaced element's `auto` height.
//
// Reported as `l7aromeo/meo-canvas#94`: an `<img>` with an intrinsic 60x40 and
// `width: 200` is 200x133 in a browser and was 200x40 here -- the intrinsic
// height arriving untouched, because block layout never told the leaf what its
// width was. A replaced element with one axis definite takes the other from
// its intrinsic ratio, which is CSS 2.2 §10.3.2 and §10.6.2 and is what
// `aspect-ratio: auto` means today.
//
// **The container's `display` is the axis this table exists for.** The same
// element in a flex container is a flex item, and a flex item with an `auto`
// cross size stretches to the line -- so 200x40 there is correct and 200x40 in
// block flow is the defect. Two sessions measured those two rows against each
// other and reported a divergence that was not one. **Every flex row here is a
// control that must not move**, and a repair scored without them can trade one
// for the other and look like progress.
//
// The absolute rows are the same kind of control from the other side: they are
// correct today, and they are the rows a repair reaching further than block
// flow would break first.
//
// **The `div` and `text` rows are the contrast.** A block-level box with
// `width: auto` fills its container and must keep filling it; a replaced one
// takes its intrinsic width. A rule keyed on "this node measures" rather than
// "this node is replaced" takes the text row with it, which is the mistake the
// sibling table caught once already.
//
// The art is 60x40, stated on the SVG root: neither square, nor the container's
// shape, nor a multiple of it, so a stretched box, an intrinsic box and a
// ratio-derived box are three different numbers in both axes.
import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/replaced-ratio.tsv')

/** The containing block. 200 wide, and shorter than the ratio would derive. */
const BLOCK = { width: 200, height: 40 }

/** The art: 60x40, intrinsic size declared on the SVG root. */
const ART = { width: 60, height: 40 }
const SRC = `data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="${ART.width}" height="${ART.height}"><rect width="100%" height="100%" fill="%23c00"/></svg>`,
)}`

/**
 * Every case: the container, the element, and why the row is here.
 *
 * `display` and `height` are the container's; `position`, `width` and `height2`
 * are the element's.
 */
const CASES = [
  ['block w200', 'img', 'block', '40px', 'static', '200px', 'auto', 'the defect: a definite width, and the height follows the ratio'],
  ['block w50%', 'img', 'block', '40px', 'static', '50%', 'auto', 'the same through a percentage, which resolves before the ratio'],
  ['block w auto', 'img', 'block', '40px', 'static', 'auto', 'auto', 'no width: the intrinsic size, NOT the container width'],
  ['block w auto h80', 'img', 'block', '40px', 'static', 'auto', '80px', 'a declared height derives the width, the ratio running the other way'],
  ['block tall auto w200', 'img', 'block', 'auto', 'static', '200px', 'auto', 'a content-sized container cannot change the answer'],
  ['block tall auto w auto', 'img', 'block', 'auto', 'static', 'auto', 'auto', 'and neither axis definite is still the intrinsic size'],

  ['flex w200', 'img', 'flex', '40px', 'static', '200px', 'auto', 'control: a flex item with an auto cross size STRETCHES to the line'],
  ['flex w50%', 'img', 'flex', '40px', 'static', '50%', 'auto', 'control: the same, and the width still resolves'],
  ['flex w auto', 'img', 'flex', '40px', 'static', 'auto', 'auto', 'control: shrink-to-fit on the main axis, stretched on the cross'],
  ['flex w auto h80', 'img', 'flex', '40px', 'static', 'auto', '80px', 'control: a declared cross size stops the stretch and the ratio applies'],
  ['flex tall auto w200', 'img', 'flex', 'auto', 'static', '200px', 'auto', 'control: with nothing to stretch to, the ratio settles it'],

  ['abs block w200', 'img', 'block', '40px', 'absolute', '200px', 'auto', 'control: out of flow, correct today'],
  ['abs block w auto', 'img', 'block', '40px', 'absolute', 'auto', 'auto', 'control: out of flow with neither axis definite'],
  ['abs flex w200', 'img', 'flex', '40px', 'absolute', '200px', 'auto', 'control: an out-of-flow child is not a flex item'],

  // **A declared height on the `div` rows, so both renderers can be compared
  // on them.** An empty `div` is zero tall, and a zero-tall box is a row
  // neither side can measure by ink -- the control would exist in the table
  // and in nothing that reads it.
  ['div block w auto', 'div', 'block', '40px', 'static', 'auto', '20px', 'control: a NON-replaced block box DOES fill its container'],
  ['div block w200', 'div', 'block', '40px', 'static', '200px', '20px', 'control: and agrees with itself when sized'],
  // The block-axis number here is a font metric rather than the property under
  // test -- a 12px sans-serif line -- so this row is the browser-side contrast
  // and is not compared against a render.
  ['text block w auto', 'text', 'block', '40px', 'static', 'auto', 'auto', 'control: a measured node that is not replaced still fills'],
]

const browser = await open()
try {
  await browser.page.setViewportSize({ width: 400, height: 400 })
  const rows = []

  for (const [key, tag, display, tall, position, width, height, note] of CASES) {
    await browser.page.evaluate(
      ({ block, src, tag, display, tall, position, width, height }) => {
        document.body.innerHTML = ''
        const parent = document.createElement('div')
        parent.style.cssText = `position:relative;display:${display};width:${block.width}px;height:${tall};background:#eee;`
        const element = document.createElement(tag === 'text' ? 'div' : tag)
        if (tag === 'img') element.src = src
        if (tag === 'text') {
          element.textContent = 'x'
          element.style.font = '12px sans-serif'
        }
        element.id = 'm'
        // `display: block` on the element itself, because every node in this
        // renderer's scene is a block-level box and an `<img>` is inline-level
        // in a browser. The header says what measuring it both ways found.
        element.style.cssText = `display:block;background:#00c;position:${position};width:${width};height:${height};${
          position === 'absolute' ? 'top:0;left:0;' : ''
        }`
        parent.append(element)
        document.body.append(parent)
      },
      { block: BLOCK, src: SRC, tag, display, tall, position, width, height },
    )
    // The decode has to finish before the intrinsic size exists at all: an
    // undecoded `<img>` measures 0x0, which would read as "Chrome does not
    // apply the ratio" for entirely the wrong reason.
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
      return { x: rect.left - parent.left, y: rect.top - parent.top, width: rect.width, height: rect.height }
    })
    rows.push(
      [key, box.x.toFixed(2), box.y.toFixed(2), box.width.toFixed(2), box.height.toFixed(2), `${display}/${tall}`, `${position} ${width} ${height}`, note].join(
        '\t',
      ),
    )
  }

  const written = table([
    '# What a definite width does to a replaced element’s `auto` height.',
    `# Container ${BLOCK.width} wide, art ${ART.width}x${ART.height} as an SVG data URL.`,
    '# Written by packages/meo-canvas/tools/conformance/replacedratio.mjs.',
    '#',
    '# A replaced element with one axis definite takes the other from its',
    '# intrinsic ratio -- CSS 2.2 §10.3.2 and §10.6.2.',
    '#',
    '# The container’s `display` is the axis this table exists for: a flex item',
    '# with an auto cross size stretches to the line, so 200x40 is correct in a',
    '# flex container and wrong in block flow. Every flex row and every absolute',
    '# row is a control that must not move.',
    '#',
    '# `div` and `text` are the contrast: a non-replaced box fills its container',
    '# and must keep filling it.',
    '#',
    '# **Every number here is a border box, read from getBoundingClientRect.**',
    '# What the browser paints inside that box is not recorded — an `<img>` is',
    '# the one element whose ink can sit anywhere within its box, and which of',
    '# those it does is `object-fit`. That question is `object-fit.tsv`, whose',
    '# walker reads pixels. A row here agreeing with a render says the boxes',
    '# agree, not that the pictures do.',
    '#',
    '# **And an `object-fit` row must not be added here.** The element box in',
    '# these rows shares the art’s aspect by construction: 60x40 art at width',
    '# 200 derives 200x133.33, which is the art’s own 1.5, because that is what',
    '# a ratio does. Nothing collapses today, since no row asks what a fit rule',
    '# draws — but a row that did would be unable to tell `fill`, `contain` and',
    '# `cover` apart, all three landing on one rectangle, at the very box where',
    '# that once hid a rule from a reader. `object-fit.tsv` is where such a row',
    '# goes; its cells are chosen not to share the art’s aspect.',
    '#',
    '# **The element carries `display: block`, and that was measured rather than',
    '# assumed.** An `<img>` is inline-level in a browser and every node in this',
    '# renderer’s scene is a block-level box, so the whole grid was taken both',
    '# ways: 32 cases, `block` against `inline`, identical in every one. The axis',
    '# is dropped because it discriminates nothing, not because nobody tried it.',
    '#',
    '# key\tx\ty\twidth\theight\tcontainer\telement\tnote',
    ...rows,
  ])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`replaced ratio: ${CASES.length} cases -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
