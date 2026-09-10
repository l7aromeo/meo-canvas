// What a percentage height resolves against when the box is out of flow.
//
// Reported as `l7aromeo/meo-canvas#84`: a percentage height on an absolutely
// positioned box came out zero, and the same third written as `top`/`bottom`
// came out right. The renderer decides definiteness itself and hands taffy the
// result, so this is a question about our rule rather than about the layout
// engine, and the rule was written from the flex parent alone.
//
// **The controls are the point.** This is a fix to a fix: the arm next to the
// one that is wrong is right, and has a measured Chrome number behind it -- a
// `min-height: 200%` child of an absolutely positioned, content-sized box is
// 20, not 40. A repair that makes out-of-flow boxes definite everywhere trades
// one wrong answer for another, and only a table carrying both kinds of row
// can see that happen. Six of the fourteen rows below must not move.
//
// Every height is `getBoundingClientRect().height`, which is the used height
// after layout rather than the declaration. The declarations are in the table
// too, because a row is only readable if you can see the scene it came from.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/absolute-percentage.tsv')

/** The page every case is measured in, so `fixed` has a stated viewport. */
const VIEWPORT = { width: 400, height: 400 }

/**
 * One scene per row, written out rather than built by a helper.
 *
 * The question is *which box is the containing block*, so a helper that
 * assembled the ancestor chain would hide the only thing being varied. `#m` is
 * always the element measured.
 */
const CASES = [
  {
    key: 'abs-percent-content-cb',
    note: 'containing block is a content-sized relative ancestor',
    html: `<div style="position:relative;display:flex;flex-direction:column;align-items:flex-start">
             <div style="width:50px;height:120px"></div>
             <div id="m" style="position:absolute;top:0;width:30px;height:33.33%"></div>
           </div>`,
  },
  {
    key: 'abs-percent-declared-cb',
    note: 'control -- the ancestor states its height',
    html: `<div style="position:relative;height:120px">
             <div id="m" style="position:absolute;top:0;width:30px;height:33.33%"></div>
           </div>`,
  },
  {
    key: 'abs-percent-abs-cb',
    note: 'containing block is itself an auto-height absolute box',
    html: `<div style="position:relative;height:300px">
             <div style="position:absolute;top:0;width:60px">
               <div style="height:90px"></div>
               <div id="m" style="position:absolute;top:0;width:30px;height:33.33%"></div>
             </div>
           </div>`,
  },
  {
    key: 'abs-percent-grandparent-cb',
    note: 'the nearest positioned ancestor is not the parent',
    // The in-between box is left to its content, so it is zero tall and the
    // only thing that can produce a band here is a percentage that resolved
    // against the grandparent. This row asks *whether* it resolved.
    html: `<div style="position:relative;display:flex;flex-direction:column;align-items:flex-start">
             <div style="width:50px;height:120px"></div>
             <div style="width:70px">
               <div id="m" style="position:absolute;top:0;width:30px;height:33.33%"></div>
             </div>
           </div>`,
  },
  {
    key: 'abs-percent-grandparent-sized',
    note: 'the same chain with the in-between box 60 tall',
    // **The other half of the question, and it needs its own row.** Giving the
    // in-between box a height separates *which* box was resolved against --
    // 59.98 for the grandparent, whose content is now 180, against 20 for the
    // parent -- but it also makes that parent definite, so this row cannot
    // also ask whether the percentage survived at all. One row per question:
    // the row above asks *whether*, this one asks *which*.
    html: `<div style="position:relative;display:flex;flex-direction:column;align-items:flex-start">
             <div style="width:50px;height:120px"></div>
             <div style="width:70px;height:60px">
               <div id="m" style="position:absolute;top:0;width:30px;height:33.33%"></div>
             </div>
           </div>`,
  },
  {
    key: 'abs-insets-box',
    note: 'the box itself, sized by top and bottom',
    html: `<div style="position:relative;height:120px">
             <div id="m" style="position:absolute;top:33.33%;bottom:33.34%;width:30px"></div>
           </div>`,
  },
  {
    key: 'abs-insets-child',
    note: 'a full-height child of that box',
    html: `<div style="position:relative;height:120px">
             <div style="position:absolute;top:33.33%;bottom:33.34%;width:30px">
               <div id="m" style="height:100%"></div>
             </div>
           </div>`,
  },
  {
    key: 'abs-top-only-child',
    note: 'control -- one inset is not a height, so the box is content-sized',
    html: `<div style="position:relative;height:120px">
             <div style="position:absolute;top:10px;width:30px">
               <div style="height:25px"></div>
               <div id="m" style="height:100%"></div>
             </div>
           </div>`,
  },
  {
    key: 'abs-declared-over-insets-child',
    note: 'control -- a declared height wins over the insets',
    html: `<div style="position:relative;height:120px">
             <div style="position:absolute;top:0;bottom:0;height:50px;width:30px">
               <div id="m" style="height:100%"></div>
             </div>
           </div>`,
  },
  {
    key: 'abs-minheight-200',
    note: 'control -- an auto-height absolute box is indefinite for its children',
    html: `<div style="display:flex;flex-direction:column;height:120px;align-items:flex-start">
             <div style="position:absolute;width:200px;align-self:flex-start">
               <div id="m" style="width:30px;height:20px;min-height:200%"></div>
             </div>
           </div>`,
  },
  {
    key: 'inflow-percent-content-cb',
    note: 'control -- in flow, a content-sized parent resolves nothing',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div style="width:50px;height:120px"></div>
             <div id="m" style="width:30px;height:33.33%"></div>
           </div>`,
  },
  {
    key: 'fixed-percent-positioned-ancestor',
    note: 'a positioned ancestor does not capture a fixed box',
    html: `<div style="position:relative;display:flex;flex-direction:column;align-items:flex-start">
             <div style="width:50px;height:120px"></div>
             <div id="m" style="position:fixed;top:0;width:30px;height:33.33%"></div>
           </div>`,
  },
  {
    key: 'fixed-percent-no-ancestor',
    note: 'the viewport, stated',
    html: `<div style="display:flex;flex-direction:column;align-items:flex-start">
             <div style="width:50px;height:120px"></div>
             <div id="m" style="position:fixed;top:0;width:30px;height:33.33%"></div>
           </div>`,
  },
  {
    key: 'fixed-percent-transformed',
    note: 'a transform does capture one, and states its height',
    html: `<div style="transform:translateZ(0);width:80px;height:150px">
             <div id="m" style="position:fixed;top:0;width:30px;height:33.33%"></div>
           </div>`,
  },
  {
    key: 'fixed-percent-transformed-auto',
    note: 'the same capture, with the ancestor sized by its content',
    html: `<div style="transform:translateZ(0);width:80px">
             <div style="height:150px"></div>
             <div id="m" style="position:fixed;top:0;width:30px;height:33.33%"></div>
           </div>`,
  },
]

const { page, close } = await open()
try {
  await page.setViewportSize(VIEWPORT)

  // **Asked of the browser that is about to take the measurements**, rather
  // than written down beside them. A stamp that survives regeneration by being
  // retyped is worse than no stamp, because it is a claim about provenance that
  // provenance no longer backs: the next person runs `just conformance`, sees
  // the line vanish, and pastes back a version that may name a browser which
  // produced none of the rows beneath it.
  //
  // The other fourteen tables carry a hand-written stamp and their tools do not
  // emit one, so those lines do not survive their own regeneration. That is
  // tracked separately. This is the shape they should take rather than this
  // being the odd one out.
  const version = page.context().browser()?.version() ?? 'unknown'

  const lines = [
    '# What a percentage height resolves against when the box is out of flow.',
    `# Measured on Chrome Headless Shell ${version}, the chromium Playwright pins here.`,
    `# Chrome, viewport ${VIEWPORT.width}x${VIEWPORT.height}.`,
    '# Written by packages/meo-canvas/tools/conformance/abspositioned.mjs.',
    '# case\theight\tnote',
  ]

  for (const { key, note, html } of CASES) {
    await page.evaluate(markup => {
      document.body.innerHTML = markup
    }, html)
    await settle(page)
    const height = await page.evaluate(() => {
      const element = document.getElementById('m')
      if (element === null) throw new Error('the case has no #m to measure')
      return element.getBoundingClientRect().height
    })
    lines.push([key, height.toFixed(2), note].join('\t'))
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
