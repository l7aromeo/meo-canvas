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
    key: 'ratio-shrink-min-width-binds',
    // **A binding minimum on the axis the compensation pins.** The pin exists
    // for a width the content settled; a `min-width` above that settles it
    // instead, and Chrome derives the block size from the bound width --
    // `100 x 117.64` rather than the `29.98 x 35.28` the content alone gives.
    // `l7aromeo/meo-canvas#126` is the same condition on a grown flex item.
    //
    // **This row asserts nothing about the compensation and is kept anyway.**
    // Four mutations of the pin's minimum clause leave it green: the clause as
    // written, the clause removed, the clause reading `.is_some()` instead of
    // the solved width, and the comparison inverted. Pinning a bound width and
    // leaving it alone both reach Chrome on a shrink-to-fit box, so there is
    // no repair it separates -- `ratio-shrink-min-width-slack` below is the
    // row that does that. What it is, is Chrome's measured answer for the
    // fourth corner of minimum-or-maximum by binds-or-slack, which is what
    // this file is for before it is an assertion that we match.
    //
    // **The test for keeping it is what the remaining rows could no longer
    // say.** It is half a contrast with `ratio-shrink-max-width-binds`: a
    // minimum and a maximum bind the same axis and Chrome splits them, taffy
    // reaching `100 x 117.64` unaided where it gives `9 x 10` against
    // `19.98 x 23.52`. Delete this row and that one stands alone asserting
    // that a maximum needs the pin, with nothing on the page saying why a
    // minimum does not -- which is the reading `bounded_ratio_box` exists to
    // stop. The next person meets one row rather than a square, so the pair
    // is named here.
    note: 'a minimum above the fit-content width settles the inline axis, and the block size follows it',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85;min-width:100px">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-min-width-slack',
    // **The row that tells the two candidate repairs apart.** A minimum is
    // present and does not decide the width -- 20 under a fit-content 30 -- so
    // Chrome's answer is the unbounded one. A clause asking whether the node
    // *carries* a minimum skips the pin here and reports taffy's `20 x 24`; a
    // clause asking whether the solved width *is* the minimum leaves the row
    // alone. Without it, presence and outcome pass every row in this file.
    note: 'a minimum below the fit-content width: present, not binding, and the answer is the unbounded one',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85;min-width:20px">
               <div style="width:30px;height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-shrink-max-width-binds',
    // **The control, and it is the row that keeps the pin.** A maximum binds
    // the same axis with the same provenance and Chrome does not treat it the
    // same: it clamps the width and the block size still follows the ratio, so
    // taffy's own answer here is `9 x 10` and the pin is what reaches Chrome's
    // `19.98 x 23.52`. A clause written about author bounds in general rather
    // than about the minimum would take this row down with it.
    note: 'a maximum below the fit-content width: clamped, and the pin is still what reaches Chrome',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div id="m" style="aspect-ratio:.85;max-width:20px">
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
    key: 'ratio-far-above-one',
    // **The family nothing else in this table reaches.** Every other row sits
    // at 0.85, 2 or 3, and at 3 the derivation and the content are both 10 --
    // so a table sampling 0.85, 2 and 3 reports perfect agreement across a
    // family that was broken from 4 upward. That coincidence is why the
    // existing coverage found nothing, and it is the reason to measure here
    // rather than to add a fourth point near the ones that already agree.
    note: 'a definite width well above ratio 3, where the derived height was wrong by a factor rather than absent',
    html: `<div style="width:30px">
             <div id="m" style="aspect-ratio:10">
               <div style="height:10px"></div>
             </div>
           </div>`,
  },
  {
    key: 'ratio-far-above-one-vanishing',
    // **The other end of the same family, and a different kind of failure.**
    // At 10 the box came back 3 tall -- a wrong size. At 100 it came back
    // `0 x 0` -- an absent box, which a caller meets as a missing element
    // rather than a misplaced one. One row cannot stand for both.
    //
    // 1e30 behaves as this does and is deliberately absent: it would pin
    // nothing extra and would suggest the family is about extreme values when
    // it starts at 4.
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
    // **Does the automatic minimum survive clipping?** CSS gives a box with an
    // aspect ratio an automatic minimum block size only while `overflow` is
    // `visible`. If that is what puts `ratio-with-taller-content` at 300, then
    // clipping should remove it and Chrome should report the 118 the ratio
    // implies -- which is the number this renderer already gives, and a
    // compensation that always floors would be wrong for every clipped box.
    // Three further spellings were measured and are not rows here, because no
    // scene can reach them. `overflow: clip` gives **300** where `hidden`,
    // `scroll` and `auto` give 117.64 -- it is the one non-visible value that
    // establishes no scroll container, and `Overflow` in this renderer has no
    // `Clip` variant to express it. And text content at 100 wide gives 120 with
    // the ratio and 120 without, against 160 at its min-content width, which is
    // what says the floor is the content's height at the used width rather than
    // its min-content height.
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
    // **The row that separates CSS's two minimums.** Clipping removes the
    // automatic one; an author's `min-height` survives it and, unlike the
    // automatic one, transfers back through the ratio into the inline axis --
    // 200 x 0.85 = 170, on a box whose containing block is 100 wide. That
    // asymmetry is the whole of `l7aromeo/meo-canvas#104`'s upstream half:
    // taffy has one slot and it behaves like the explicit kind.
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
