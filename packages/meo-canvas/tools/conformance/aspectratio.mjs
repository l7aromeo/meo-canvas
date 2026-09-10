// What a percentage height resolves against when a ratio settles the parent.
//
// Reported as `l7aromeo/meo-canvas#91`: an in-flow `height: 100%` under a box
// sized by `width` and `aspect-ratio` came out zero, while the parent's own box
// was right. The renderer decides definiteness itself and hands taffy the
// answer, so the rule is ours; it knows a declared length and it knows opposing
// insets, and a ratio plus a definite length on the other axis is a third way
// that is not in it.
//
// **The rows that must not move are half the table.** This is the third repair
// to the same rule, and each time the failure mode has been trading one wrong
// answer for another: `ratio-and-declared-height` is 60 rather than 141 because
// a declared height wins outright, and `inflow-100-no-ratio` is nothing because
// without a ratio a content-sized parent still settles nothing. A repair that
// made every ratio parent definite breaks the first; one that made every parent
// definite breaks the second.
//
// Every height is `getBoundingClientRect().height` after layout, and the widths
// are here because `ratio-from-height` is the row where the derived axis is the
// *width* -- the rule is about either axis being definite, not about width.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/aspect-ratio-percentage.tsv')

/** The page every case is measured in. */
const VIEWPORT = { width: 400, height: 400 }

/**
 * One scene per row, written out rather than built by a helper.
 *
 * What varies is which box settles which axis, so a builder that assembled the
 * pair would hide the only thing under test. `#m` is always the element
 * measured.
 */
const CASES = [
  {
    key: 'ratio-parent-box',
    note: 'control -- the ratio settles the box itself, and already did',
    html: `<div id="m" style="width:120px;aspect-ratio:.85"></div>`,
  },
  {
    key: 'inflow-100-under-ratio',
    note: 'the defect: a whole-height child of a ratio-sized parent',
    html: `<div style="width:120px;aspect-ratio:.85">
             <div id="m" style="height:100%"></div>
           </div>`,
  },
  {
    key: 'inflow-50-under-ratio',
    note: 'the same, proportional rather than whole',
    html: `<div style="width:120px;aspect-ratio:.85">
             <div id="m" style="height:50%"></div>
           </div>`,
  },
  {
    key: 'abs-100-under-ratio',
    note: 'out of flow under the same parent',
    html: `<div style="position:relative;width:120px;aspect-ratio:.85">
             <div id="m" style="position:absolute;top:0;width:30px;height:100%"></div>
           </div>`,
  },
  {
    key: 'ratio-and-declared-height',
    note: 'control -- a declared height wins and the ratio does not fight it',
    html: `<div style="width:120px;height:60px;aspect-ratio:.85">
             <div id="m" style="height:100%"></div>
           </div>`,
  },
  {
    key: 'ratio-from-height',
    note: 'the other axis: height declared, width derived',
    html: `<div style="height:120px;aspect-ratio:.85">
             <div id="m" style="height:100%"></div>
           </div>`,
  },
  {
    key: 'min-height-200-under-ratio',
    note: 'a minimum resolves against it too',
    html: `<div style="width:120px;aspect-ratio:.85">
             <div id="m" style="height:20px;min-height:200%"></div>
           </div>`,
  },
  {
    key: 'max-height-25-under-ratio',
    note: 'and a maximum, which is the third kind build drops',
    html: `<div style="width:120px;aspect-ratio:.85">
             <div id="m" style="height:300px;max-height:25%"></div>
           </div>`,
  },
  {
    key: 'ratio-with-no-definite-length',
    // **The row that decides how wide the rule is.** Neither axis states a
    // length: the parent's width is shrink-to-fit from this child's 30, the
    // ratio derives the height from that, and the child's `100%` resolves
    // against it. So a ratio settles the block axis whatever the inline axis
    // was arrived at, and a repair demanding a declared width would leave this
    // painting nothing.
    note: 'neither axis declared -- the width is shrink-to-fit and it still resolves',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div style="aspect-ratio:.85">
               <div id="m" style="width:30px;height:100%"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-with-percentage-width',
    note: 'the width is itself a percentage, and the ratio still settles the height',
    html: `<div style="width:200px;height:50px">
             <div style="width:50%;aspect-ratio:.85">
               <div id="m" style="width:30px;height:100%"></div>
             </div>
           </div>`,
  },
  {
    key: 'inflow-100-no-ratio',
    note: 'control -- without the ratio the parent settles nothing',
    html: `<div style="width:120px">
             <div id="m" style="height:100%"></div>
           </div>`,
  },

  // **The rows below measure the ratio box itself rather than a percentage
  // child.** The eleven above ask what a percentage resolves against; these ask
  // whether the ratio derives a height at all when the width is an *outcome* of
  // layout rather than a declared length. That is a different question and the
  // table could not express it: every ratio box above has a width that is
  // declared, a percentage, or shrink-to-fit, and none is block-level auto.
  {
    key: 'block-auto-width-ratio',
    note: 'block-level, no width stated -- the width is the containing block and the height is derived from it',
    html: `<div style="width:200px">
             <div id="m" style="aspect-ratio:.85"></div>
           </div>`,
  },
  {
    key: 'nested-ratio-outer',
    note: 'two ratios deep, the outer measured -- shrink-to-fit around a ratio box',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:2">
               <div style="aspect-ratio:.85">
                 <div style="width:30px;height:10px"></div>
               </div>
             </div>
           </div>`,
  },
  {
    key: 'nested-ratio-inner',
    note: 'the same tree, the inner measured -- does the outer ratio change what the inner derives',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div style="aspect-ratio:2">
               <div id="m" style="aspect-ratio:.85">
                 <div style="width:30px;height:10px"></div>
               </div>
             </div>
           </div>`,
  },
  {
    key: 'column-flex-ratio-cross',
    note: 'a ratio item in a column flex container -- the width is the cross axis and stretches',
    html: `<div style="display:flex;flex-direction:column;width:200px">
             <div id="m" style="aspect-ratio:.85"></div>
           </div>`,
  },
  {
    key: 'grid-item-ratio',
    note: 'a ratio item in a grid track -- the width comes from the track',
    html: `<div style="display:grid;grid-template-columns:200px">
             <div id="m" style="aspect-ratio:.85"></div>
           </div>`,
  },
  {
    key: 'ratio-shrink-with-author-min-height',
    // **One field, two claimants.** An author's `min-height` and a height
    // derived from a ratio are the same taffy slot, and CSS takes the larger of
    // the two. 200 is larger than the 35.28 the ratio implies here, so this row
    // says which one Chrome honours when they disagree -- and a compensation
    // writing into that field has to answer the same question.
    note: 'an author min-height beside a ratio on a shrink-to-fit width -- the larger floor wins',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85;min-height:200px">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-50-child',
    // The proportional case under a shrink-to-fit parent. The whole-height one
    // is `ratio-with-no-definite-length`; without this, a compensation could
    // satisfy `100%` by any means and this table could not tell.
    note: 'half the height of a shrink-to-fit ratio parent, where the whole-height row is the KNOWN one',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div style="aspect-ratio:.85">
               <div id="m" style="width:30px;height:50%"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-taller-content',
    // **The row that decides whether the width may grow.** A shrink-to-fit
    // ratio box holding content taller than the ratio implies: if Chrome takes
    // the width back up through the ratio, then a compensation clearing the
    // ratio is wrong, and the 255 a raw-taffy probe produced here is correct
    // rather than a defect.
    note: 'shrink-to-fit ratio box whose content exceeds the derived height -- does the width follow',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85">
               <div style="width:30px;height:300px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-issue-97',
    // **The shape `l7aromeo/meo-canvas#97` actually reports**, which had no row
    // in this table until now. The closest was `ratio-gt-one-shrink`, the same
    // scene at ratio 2.0, and `ratio-with-no-definite-length` -- which carries
    // that issue's name in `KNOWN` -- has a percentage-height child and fails a
    // different way.
    note: 'the issue shape itself: a plain 30x10 child under ratio .85 on a shrink-to-fit parent',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-under-definite-ratio-parent',
    // **The control on what counts as entangled.** The parent carries a ratio
    // and a declared width, so its own width is not an outcome and nothing the
    // child derives depends on it. A predicate asking "does an ancestor have a
    // ratio" would skip the child here; one asking "is an ancestor's inline
    // size also an outcome" compensates it. Nothing else in this table
    // distinguishes the two readings.
    note: 'a shrink-to-fit ratio child inside a ratio parent whose width is declared -- the parent is not entangled',
    html: `<div style="width:200px;aspect-ratio:2;display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-content-just-under',
    note: 'content one pixel short of the derived height -- the derived one should win and the width should not move',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85">
               <div style="width:30px;height:34px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-content-just-over',
    note: 'content one pixel past it -- if the width moves here and not above, the threshold is the derived height itself',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85">
               <div style="width:30px;height:36px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-gt-one-shrink',
    note: 'a ratio above 1 with short content, so a direction rule cannot be an artefact of the ratio side of 1',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:2">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-gt-one-taller-content',
    note: 'the same ratio with content past the derived height',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:2">
               <div style="width:30px;height:40px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-with-taller-content',
    note: 'content taller than the ratio implies -- decides whether a derived height is a floor, a ceiling or an override',
    html: `<div style="width:100px">
             <div id="m" style="aspect-ratio:.85">
               <div style="height:300px"></div>
             </div>
           </div>`,
  },
]

const { page, close } = await open()
try {
  await page.setViewportSize(VIEWPORT)

  // No stamp line here: `table()` inserts it after the first, from the version
  // the browser reported when `open()` launched it. A tool that wrote its own
  // would produce two.
  const lines = [
    '# What a percentage height resolves against when a ratio settles the parent.',
    `# Chrome, viewport ${VIEWPORT.width}x${VIEWPORT.height}.`,
    '# Written by packages/meo-canvas/tools/conformance/aspectratio.mjs.',
    '# case\theight\twidth\tnote',
  ]

  for (const { key, note, html } of CASES) {
    await page.evaluate(markup => {
      document.body.innerHTML = markup
    }, html)
    await settle(page)
    const box = await page.evaluate(() => {
      const element = document.getElementById('m')
      if (element === null) throw new Error('the case has no #m to measure')
      const seen = element.getBoundingClientRect()
      return { height: seen.height, width: seen.width }
    })
    lines.push([key, box.height.toFixed(2), box.width.toFixed(2), note].join('\t'))
  }

  const written = table(lines)
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    console.log(`wrote ${DESTINATION}`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await close()
}
