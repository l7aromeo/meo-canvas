// What a percentage height resolves against when the box is out of flow
// (`l7aromeo/meo-canvas#84`). The renderer decides definiteness itself; six of the
// fourteen rows are controls that must not move. Heights are the used
// `getBoundingClientRect().height`, and each row carries its declarations.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/absolute-percentage.tsv')

/** The page every case is measured in, so `fixed` has a stated viewport. */
const VIEWPORT = { width: 400, height: 400 }

/**
 * One scene per row, written out rather than built by a helper, since the
 * containing block is what varies. `#m` is always the element measured.
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
    // Which box was resolved against: a height on the in-between box separates
    // 59.98 for the grandparent from 20 for the parent. It also makes the parent
    // definite, so whether the percentage survives is the row above.
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

  // No stamp here: `table()` in `browser.mjs` writes it from the version the
  // launch recorded, and throws if called before `open()`.
  const lines = [
    '# What a percentage height resolves against when the box is out of flow.',
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
