// Where a clipped child ends up, and what covers it, across overflow x clipper
// position x child position x transform.
//
// **This table was measured by hand and committed.** `just conformance` runs
// eleven tools and none of them wrote it, so its 120 rows could not be
// re-measured against a browser bump the way every other table can -- which is
// what `just ci` proved of the others when the backend moved and they came back
// byte-identical.
//
// The first thing this has to do is reproduce those 120 rows exactly. A row
// that differs is a disagreement between this tool and the header's prose, and
// the prose is what the hand measurement was taken from; either the tool is
// wrong or the description was, and both have to be settled before a single new
// row is added.
import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/overflow-position.tsv')

/** The scene, exactly as the committed header describes it. */
const OUTER = { width: 200, height: 120 }
/** Where the outer box sits on the page, which `fixed` rows resolve against. */
const OUTER_AT = { top: 40, left: 40 }
const CLIPPER = { width: 60, height: 40, inFlow: { top: 20, left: 20 }, outOfFlow: { top: 20, left: 20 } }
const CHILD = { width: 50, height: 40, inFlow: { top: 20, left: 30 }, outOfFlow: { top: 20, left: 30 } }

/** The three points, relative to the outer box. */
const PROBES = [
  [60, 45],
  [88, 50],
  [60, 68],
]

const OVERFLOW = { v: 'visible', h: 'hidden', s: 'scroll' }
const POSITION = { S: 'static', R: 'relative', A: 'absolute', K: 'sticky', F: 'fixed' }
const CHILD_POSITION = { S: 'static', R: 'relative', A: 'absolute', F: 'fixed' }

const outOfFlow = kind => kind === 'absolute' || kind === 'fixed'

/** One row's CSS, built from its four-letter code. */
function scene(code) {
  const [o, p, c, t] = code
  const clipperKind = POSITION[p]
  const childKind = CHILD_POSITION[c]

  const clipperPlacement = outOfFlow(clipperKind)
    ? `left:${CLIPPER.outOfFlow.left}px;top:${CLIPPER.outOfFlow.top}px`
    : `margin:${CLIPPER.inFlow.top}px 0 0 ${CLIPPER.inFlow.left}px`
  const childPlacement = outOfFlow(childKind)
    ? `left:${CHILD.outOfFlow.left}px;top:${CHILD.outOfFlow.top}px`
    : `margin:${CHILD.inFlow.top}px 0 0 ${CHILD.inFlow.left}px`

  return `<!doctype html><meta charset="utf-8"><style>
    html,body{margin:0;padding:0}
    /* The outer box's own place on the page is part of the scene, and the
       committed header does not record it: chrome_tables.rs:699 places it at
       40,40, and every fixed row resolves against the viewport -- so a page
       that puts it anywhere else moves 36 of the 120 rows. A backtick here
       would end this template literal, which is why the citation is bare. */
    #page{padding:${OUTER_AT.top}px 0 0 ${OUTER_AT.left}px}
    #outer{position:relative;width:${OUTER.width}px;height:${OUTER.height}px}
    #clipper{width:${CLIPPER.width}px;height:${CLIPPER.height}px;overflow:${OVERFLOW[o]};position:${clipperKind};${clipperPlacement}${
      o === 's' ? ';scrollbar-width:none' : ''
    }${t === 't' ? ';transform:translateZ(0)' : ''}}
    #child{width:${CHILD.width}px;height:${CHILD.height}px;position:${childKind};${childPlacement}}
  </style><div id="page"><div id="outer"><div id="clipper"><div id="child"></div></div></div></div>`
}

const browser = await open()
try {
  const rows = []
  for (const o of Object.keys(OVERFLOW)) {
    for (const p of Object.keys(POSITION)) {
      for (const c of Object.keys(CHILD_POSITION)) {
        for (const t of ['n', 't']) {
          const code = `${o}${p}${c}${t}`
          await browser.page.setContent(scene(code))
          await settle(browser.page)

          const measured = await browser.page.evaluate(points => {
            const outer = document.getElementById('outer').getBoundingClientRect()
            const child = document.getElementById('child').getBoundingClientRect()
            const at = ([x, y]) => {
              const found = document.elementFromPoint(outer.left + x, outer.top + y)
              return found === null ? 'b' : found.id === 'child' ? 'c' : found.id === 'clipper' ? 'l' : found.id === 'outer' ? 'o' : 'b'
            }
            return {
              rect: [Math.round(child.left - outer.left), Math.round(child.top - outer.top), Math.round(child.width), Math.round(child.height)].join(','),
              abc: points.map(at).join(''),
            }
          }, PROBES)

          rows.push(`${code} ${measured.rect} ${measured.abc}`)
        }
      }
    }
  }

  const header = [
    '# Chrome 2026-08-23. overflow x clipper position x child position x transform.',
    '# Scene exactly as MC Agent Zero specified: outer 200x120 position:relative;',
    '# clipper 60x40 overflow:<O> position:<P>, placed by margin 20,20 when in flow',
    '# and by left/top 20,20 when out of it; child 50x40 position:<C>, placed by',
    '# margin 20,30 when in flow and by left/top 20,30 when out of it. `scroll` rows',
    '# carry scrollbar-width:none.',
    '#',
    '# key   O: v=visible h=hidden s=scroll',
    '#       P: S=static R=relative A=absolute K=sticky F=fixed',
    '#       C: S=static R=relative A=absolute F=fixed',
    '#       T: n=none t=transform:translateZ(0) on the clipper',
    '# outer sits at 40,40 on the page. That is not decoration: every `fixed`',
    '# row resolves against the viewport rather than against outer, so a page',
    '# that places outer anywhere else moves 36 of these 120 rows. The first',
    '# version of this tool put it at 0,0 and did exactly that.',
    '#',
    "# rect  the child's box minus outer's, x,y,w,h",
    '# abc   elementFromPoint at (60,45), (88,50), (60,68) relative to outer:',
    '#       c=child l=clipper o=outer b=body',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`overflow x position: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
