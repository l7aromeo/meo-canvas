// Whether a flex item with a definite base size collapses, or its content
// dictates the line.
//
// **The pair CSS provides for "ignore my content, take your size from the
// line" is a definite `flex-basis` with a definite minimum**, and the reported
// case writes both as zero. Zero is one cell of a family: measured here,
// Chrome collapses at `flex-basis: 1px` and `120px` and at `min-height: 1px`
// and `0%`, so a reader — or a predicate — that takes the zeros literally is
// wrong about five neighbours of the row that was filed.
//
// **`flex-basis: 0%` is not `flex-basis: 0`, and that is the row that says
// why.** A percentage basis against a container with no definite cross size
// resolves as `auto`, so the content dictates and both engines agree. The rule
// this table is about is **definiteness**, not the number, and the `0%` row is
// what separates the two readings.
//
// **The container's cross size is the other half.** An explicit length or a
// definite ancestor makes the collapse work; a percentage height does not,
// because it resolves against an indefinite parent and stays indefinite. So
// `align-self: flex-start` — no stretch at all — is in here as the row that
// refuses the obvious description: the trigger is an indefinite cross size
// rather than stretch.
//
// **The number measured is the OUTER box's cross extent**, because that is
// what a caller sees: a row whose height a sibling should have set, reported
// at the tall item's height instead. `1024` means the content dictated; `200`
// means the item collapsed and the sibling won.
//
// **The mirror is measured under a shrink-to-fit parent, and the reason is not
// tidiness.** A block element's `width: auto` fills where `height: auto` fits,
// so the naive width-direction mirror measures the viewport rather than the
// shape — it read 1280 on a 1280-wide page, which is a property of the markup.
// Wrapped in a flex parent at `align-items: flex-start` the width is
// shrink-to-fit, which is the question the height case asks.
//
// `l7aromeo/meo-canvas#145`. Reproduced against taffy 0.14.0 in twenty lines
// with no code of this repository in the picture, so the divergence is
// upstream's rather than the conversion's.
import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/flex-basis-collapse.tsv')

/** The row whose height a 300x200 sibling should set. */
const OUTER = 'display:flex;flex-direction:row;width:600px'
/** The column with no cross size of its own: the shape that diverges. */
const COLUMN = 'display:flex;flex-direction:column;flex-grow:1'
/** The pair that says "take your size from the line". */
const ITEM = 'flex-grow:1;flex-basis:0;min-height:0'
/** Tall enough that dictating is unmistakable against the sibling's 200. */
const TALL = '<div style="width:100px;height:1024px"></div>'

// key, outer, column, item, content, note
const CASES = [
  ['baseline', OUTER, COLUMN, ITEM, TALL, 'the reported shape: the pair collapses the item and the sibling sets the row'],

  // How the container gets its cross size.
  ['container explicit', OUTER, `${COLUMN};height:200px`, ITEM, TALL, 'a stated length makes it definite, and this is the row that works upstream too'],
  ['container percentage', OUTER, `${COLUMN};height:100%`, ITEM, TALL, 'a percentage against an indefinite parent stays indefinite'],
  ['container definite ancestor', `${OUTER};height:200px`, COLUMN, ITEM, TALL, 'the definiteness can come from above rather than from the container'],
  ['container align-self start', OUTER, `${COLUMN};align-self:start`, ITEM, TALL, 'no stretch, and the collapse still happens: stretch is not the trigger'],
  ['container align-self flex-start', OUTER, `${COLUMN};align-self:flex-start`, ITEM, TALL, 'the same, spelled the older way'],

  // What the basis is.
  ['basis auto', OUTER, COLUMN, 'flex-grow:1;min-height:0', TALL, 'control: an auto basis is the content, so the content dictates'],
  ['basis 1px', OUTER, COLUMN, 'flex-grow:1;flex-basis:1px;min-height:0', TALL, 'definite and not zero: it collapses, so the rule is not about zero'],
  ['basis 120px', OUTER, COLUMN, 'flex-grow:1;flex-basis:120px;min-height:0', TALL, 'definite and larger than the line: it still collapses'],
  ['basis 0%', OUTER, COLUMN, 'flex-grow:1;flex-basis:0%;min-height:0', TALL, 'a percentage basis against an indefinite container is auto, not zero'],

  // What the minimum is.
  ['min auto', OUTER, COLUMN, 'flex-grow:1;flex-basis:0', TALL, 'control: the automatic minimum floors the item at its content'],
  ['min 1px', OUTER, COLUMN, 'flex-grow:1;flex-basis:0;min-height:1px', TALL, 'definite and not zero: it collapses'],
  ['min 0%', OUTER, COLUMN, 'flex-grow:1;flex-basis:0;min-height:0%', TALL, 'a percentage minimum of zero is definite and collapses'],

  // What is inside the item.
  ['content nested', OUTER, COLUMN, ITEM, `<div>${TALL}</div>`, 'the tall thing one container deeper'],
  ['content empty', OUTER, COLUMN, ITEM, '', 'control: nothing to dictate, so nothing to collapse'],

  // Depth and display.
  ['item one level deeper', OUTER, COLUMN, ITEM, TALL, 'the collapsing item is a grandchild of the column'],
  [
    'container grid',
    OUTER,
    `display:grid;flex-grow:1`,
    ITEM,
    TALL,
    'control: the compensation must not reach a grid item -- `flex-basis` does not apply to one, so there is no §9.2 hypothetical main size to write',
  ],

  // Combinations a real scene actually contains. Each shares this defect's
  // mechanism and reaches it from a different side, which is the point: the
  // rows above vary one property, and callers do not.
  ['combo overflow hidden', OUTER, COLUMN, `${ITEM};overflow:hidden`, TALL, 'a scroll container has no automatic minimum, so §4.5 disappears'],
  ['combo overflow scroll', OUTER, COLUMN, `${ITEM};overflow:scroll`, TALL, 'the same, and grouped with hidden rather than with clip'],
  [
    'combo overflow hidden, min auto',
    OUTER,
    COLUMN,
    'flex-grow:1;flex-basis:0;overflow:hidden',
    TALL,
    'the basis alone, where overflow has removed the floor instead',
  ],
  ['combo max above min', OUTER, COLUMN, `${ITEM};max-height:400px`, TALL, 'a maximum that does not bind, beside the pair'],
  [
    'combo max below min',
    OUTER,
    COLUMN,
    'flex-grow:1;flex-basis:0;min-height:300px;max-height:100px',
    TALL,
    'CSS resolves the minimum last, so 300 wins over the 100',
  ],
  [
    'combo shrink definite basis',
    OUTER,
    COLUMN,
    'flex-shrink:1;flex-basis:400px;min-height:0',
    TALL,
    'shrinking below a definite basis is §9.7 from the other side',
  ],
  ['combo container gap', OUTER, `${COLUMN};gap:24px`, ITEM, TALL, 'a gap changes the free space the item resolves against'],
  ['combo container padding', OUTER, `${COLUMN};padding:16px`, ITEM, TALL, 'padding does the same and is on the container rather than the item'],
  ['combo border-box padding', OUTER, `${COLUMN};padding:16px;box-sizing:border-box`, ITEM, TALL, 'and with the padding inside the stated size'],
  ['combo item ratio', OUTER, COLUMN, `${ITEM};aspect-ratio:1`, TALL, 'a ratio transfers a main size into a cross size that is itself indefinite'],
  ['combo container wrap', OUTER, `${COLUMN};flex-wrap:wrap`, ITEM, TALL, 'a wrapped line takes its cross size from the items rather than the container'],
  ['combo nested percentage', OUTER, `${COLUMN};height:100%`, `${ITEM};height:100%`, TALL, 'a percentage at each level, none of which resolves'],

  // The scope control on the other axis.
  [
    'row-direction mirror',
    'display:flex;flex-direction:column;height:600px',
    'display:flex;flex-direction:row;flex-grow:1',
    'flex-grow:1;flex-basis:0;min-width:0',
    '<div style="width:1024px;height:100px"></div>',
    'control: the container’s main axis is inline and sized by max-content, which is §9.9 Intrinsic Sizes -- unimplemented in taffy (`DioxusLabs/taffy#351`) and a different computation from this one',
  ],
]

const browser = await open()
try {
  await browser.page.setViewportSize({ width: 1400, height: 1400 })
  const rows = []
  for (const [key, outer, column, item, content, note] of CASES) {
    const mirror = key === 'row-direction mirror'
    const deeper = key === 'item one level deeper'
    await browser.page.evaluate(
      ({ outer, column, item, content, mirror, deeper }) => {
        const sibling = mirror ? '<div style="width:200px;height:300px"></div>' : '<div style="width:300px;height:200px"></div>'
        const inner = `<div style="${item}">${content}</div>`
        const wrapped = deeper ? `<div style="display:flex;flex-direction:column;flex-grow:1">${inner}</div>` : inner
        const box = `<div id="c" style="${outer};background:#eee">${sibling}<div style="${column}">${wrapped}</div></div>`
        // A block element's `width: auto` fills where `height: auto` fits, so
        // the mirror needs a parent that shrinks to fit or it reports the page.
        document.body.innerHTML = mirror ? `<div style="display:flex;align-items:flex-start">${box}</div>` : box
      },
      { outer, column, item, content, mirror, deeper },
    )
    await settle(browser.page)
    const box = await browser.page.evaluate(() => {
      const element = document.getElementById('c')
      if (element === null) throw new Error('the case has no #c to measure')
      const rect = element.getBoundingClientRect()
      return { width: rect.width, height: rect.height }
    })
    rows.push([key, (mirror ? box.width : box.height).toFixed(2), mirror ? 'width' : 'height', note].join('\t'))
  }

  const written = table([
    '# Whether a flex item with a definite base size collapses into its line.',
    '# Outer row 600 wide with a 300x200 sibling; the column has no cross size',
    '# of its own; the item holds 1024 of content.',
    '# Written by packages/meo-canvas/tools/conformance/flexbasiscollapse.mjs.',
    '#',
    '# **No row here records a divergence.** Every one of the thirty matches',
    '# Chrome: eighteen because the compensation makes them, twelve because they',
    '# already did -- and those twelve are what says it does not over-reach.',
    '#',
    '# The number is the OUTER box on the axis named: 1024 means the content',
    '# dictated it, 200 means the item collapsed and the sibling set it.',
    '#',
    '# `l7aromeo/meo-canvas#145`. Reproduced in taffy 0.14.0 alone, so the',
    '# divergence is upstream rather than in this repository’s conversion.',
    '#',
    '# **`overflow` deletes the minimum half, and this codebase has now been',
    '# told that twice.** A scroll container has no automatic minimum, so a',
    '# definite basis suffices alone -- `combo overflow hidden, min auto`',
    '# collapses where `min auto` does not. `scroll` behaves as `hidden`,',
    '# grouped by the minimum they give an item rather than by whether they',
    '# clip, which is what `l7aromeo/meo-canvas#107` found in the negative',
    '# margin family and is the same grouping arriving in this one.',
    '#',
    '# **The pair is a definite basis with a definite minimum, and neither has',
    '# to be zero** — `basis 1px`, `basis 120px`, `min 1px` and `min 0%` all',
    '# collapse. `basis 0%` does not, because a percentage against an',
    '# indefinite container resolves as auto: the rule is definiteness rather',
    '# than the number, and those two rows are what tell them apart.',
    '#',
    '# key\textent\taxis\tnote',
    ...rows,
  ])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`flex basis collapse: ${CASES.length} cases -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
