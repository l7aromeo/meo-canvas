// What a percentage height resolves against when a ratio settles the parent
// (`l7aromeo/meo-canvas#91`). The renderer decides definiteness and hands taffy
// the answer; the rows that must not move -- a declared height wins, no ratio
// settles nothing -- guard against a repair that trades one wrong answer for another.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/aspect-ratio-percentage.tsv')

/** The page every case is measured in. */
const VIEWPORT = { width: 400, height: 400 }

/**
 * One scene per row, written out rather than built by a helper, since which box
 * settles which axis is the thing under test. `#m` is always the element measured.
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
    // Neither axis states a length: the parent's width is shrink-to-fit, the ratio
    // derives its height, and the child's `100%` resolves against it -- so a rule
    // demanding a declared width would paint nothing here.
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

  // The rows below measure the ratio box itself, not a percentage child: whether
  // the ratio derives a height at all when the width is an outcome of layout --
  // block-level auto -- rather than declared, a percentage or shrink-to-fit.
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
    // An author `min-height` and a ratio-derived height share one taffy slot, and
    // CSS takes the larger: 200 against the 35.28 the ratio implies, so this row
    // says which Chrome honours and which a compensation writing there must.
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
    // Content taller than the ratio implies: if Chrome takes the width back up
    // through the ratio, a compensation clearing the ratio is wrong and taffy's
    // 255 here is correct.
    note: 'shrink-to-fit ratio box whose content exceeds the derived height -- does the width follow',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85">
               <div style="width:30px;height:300px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-issue-97',
    // The shape `l7aromeo/meo-canvas#97` reports. `ratio-gt-one-shrink` is the
    // same scene at ratio 2.0; `ratio-with-no-definite-length`, named for the
    // issue in `KNOWN`, has a percentage child and fails a different way.
    note: 'the issue shape itself: a plain 30x10 child under ratio .85 on a shrink-to-fit parent',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-min-width-binds',
    // A `min-width` above the fit-content width binds the axis the compensation
    // pins; Chrome gives `100 x 117.64`. Four mutations of the pin's minimum clause
    // leave it green -- it records Chrome's corner and pairs with
    // `ratio-shrink-max-width-binds`, where a maximum does need the pin.
    note: 'a minimum above the fit-content width settles the inline axis, and the block size follows it',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85;min-width:100px">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-min-width-slack',
    // A present, slack minimum -- 20 under a fit-content 30 -- takes the unbounded
    // answer. A clause asking whether a minimum exists reports taffy's `20 x 24`
    // here; one asking whether the solved width is the minimum does not.
    note: 'a minimum below the fit-content width: present, not binding, and the answer is the unbounded one',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85;min-width:20px">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-min-width-just-under',
    // A slack minimum within a pixel of binding: `29px` under `29.98`, inside
    // `DERIVED_TOLERANCE`. The pin's clause compares exactly, since a tolerance
    // reports `29 x 34` on this row alone.
    note: 'a minimum one pixel under the fit-content width: still slack, and the answer is the unbounded one',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85;min-width:29px">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-min-over-max',
    // Both bounds written: CSS resolves the minimum last, so the box is 100 wide,
    // past its own `max-width`, at `100 x 117.64`. Pinned because it looks wrong;
    // like `ratio-shrink-min-width-binds` it records Chrome, and removing the
    // pin's minimum clause leaves it green.
    note: 'a minimum and a maximum both written: CSS resolves the minimum last, and the block size follows it',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85;min-width:100px;max-width:20px">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-max-width-binds',
    // A binding maximum: Chrome clamps the width and still derives the height,
    // `19.98 x 23.52` against taffy's `9 x 10`, so the pin is what matches it.
    // Pointing the clause at `max_size.width` instead of `min_size.width` turns
    // this row red.
    note: 'a maximum below the fit-content width: clamped, and the pin is still what reaches Chrome',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85;max-width:20px">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-under-definite-ratio-parent',
    // The ratio parent's width is declared, so nothing the child derives depends
    // on it: "an ancestor has a ratio" skips the child, "an ancestor's inline size
    // is also an outcome" compensates it. Only this row tells the two apart.
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
    key: 'ratio-far-above-one',
    // Ratios above 3. At 0.85, 2 and 3 the derivation and the content agree, so
    // sampling only those hides a derived height wrong by a factor from 4 up.
    note: 'a definite width well above ratio 3, where the derived height was wrong by a factor rather than absent',
    html: `<div style="width:30px">
             <div id="m" style="aspect-ratio:10">
               <div style="height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-far-above-one-vanishing',
    // At 100 the box comes out `0 x 0` rather than merely mis-sized, which a
    // caller meets as a missing element. 1e30 behaves the same and is left out:
    // the family starts at 4 and is not about extreme values.
    note: 'the same at a ratio far enough that the box rendered nothing at all, which a wrong size is not',
    html: `<div style="width:30px">
             <div id="m" style="aspect-ratio:100">
               <div style="height:10px"></div>
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
    key: 'ratio-definite-clipped',
    // CSS gives a ratio box an automatic minimum block size only under
    // `overflow: visible`, so clipped it should be 117.64. `overflow: clip` keeps
    // the 300 but `Overflow` has no `Clip` variant to reach it; text content shows
    // the floor is the content height at the used width.
    note: 'the same box clipped -- if the automatic minimum is what makes 300, this is 118',
    html: `<div style="width:100px">
             <div id="m" style="aspect-ratio:.85;overflow:hidden">
               <div style="height:300px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-definite-overflow-scroll',
    // The predicate is a set rather than a value: CSS removes the automatic
    // minimum for a non-`visible` overflow, and that is a reading until each
    // spelling is measured.
    note: 'overflow: scroll -- does it drop the automatic minimum the way hidden does',
    html: `<div style="width:100px">
             <div id="m" style="aspect-ratio:.85;overflow:scroll">
               <div style="height:300px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-definite-overflow-auto',
    // The predicate is a set rather than a value: CSS removes the automatic
    // minimum for a non-`visible` overflow, and that is a reading until each
    // spelling is measured.
    note: 'overflow: auto -- does it drop the automatic minimum the way hidden does',
    html: `<div style="width:100px">
             <div id="m" style="aspect-ratio:.85;overflow:auto">
               <div style="height:300px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-definite-clipped-author-min',
    // Clipping removes the automatic minimum; an author `min-height` survives and
    // transfers through the ratio to the inline axis, 200 x 0.85 = 170. Taffy has
    // one slot that behaves like the explicit kind: `l7aromeo/meo-canvas#104`.
    note: 'clipped and carrying an author min-height -- the explicit minimum survives and takes the width with it',
    html: `<div style="width:100px">
             <div id="m" style="aspect-ratio:.85;overflow:hidden;min-height:200px">
               <div style="height:300px"></div>
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
