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

/**
 * The child's offsets, the fifth letter.
 *
 * `-` is the 120 rows measured by hand; `i` and `n` are new. Unequal numbers so
 * an implementation that swapped the axes fails, small enough that the child
 * stays partly inside a 60x40 clipper -- a row that reads "nothing visible" is
 * a row that has stopped discriminating -- and several times the walker's 1px
 * tolerance. The negative pair is here because it can only fail for a defect
 * somebody could actually write: a renderer that clamped an inset at zero
 * passes every positive row.
 */
const OFFSET = { '-': null, i: { left: 8, top: 6 }, n: { left: -8, top: -6 } }

const outOfFlow = kind => kind === 'absolute' || kind === 'fixed'

/** One row's CSS, built from its five-letter code. */
function scene(code) {
  const [o, p, c, t, i] = code
  const clipperKind = POSITION[p]
  const childKind = CHILD_POSITION[c]

  const clipperPlacement = outOfFlow(clipperKind)
    ? `left:${CLIPPER.outOfFlow.left}px;top:${CLIPPER.outOfFlow.top}px`
    : `margin:${CLIPPER.inFlow.top}px 0 0 ${CLIPPER.inFlow.left}px`
  // The offsets are *added to* the margin placement rather than replacing it,
  // so an `i` row is its `-` row plus two declarations and nothing else. They
  // are written on a `static` child as readily as on a `relative` one: that a
  // static box ignores them is the property the fifth letter exists to measure,
  // and withholding them there would assume the answer.
  const offset = OFFSET[i]
  const childPlacement = outOfFlow(childKind)
    ? `left:${CHILD.outOfFlow.left}px;top:${CHILD.outOfFlow.top}px`
    : `margin:${CHILD.inFlow.top}px 0 0 ${CHILD.inFlow.left}px${offset === null ? '' : `;left:${offset.left}px;top:${offset.top}px`}`

  return `<!doctype html><meta charset="utf-8"><style>
    html,body{margin:0;padding:0}
    /* The outer box's own place on the page is part of the scene, and the
       committed header does not record it: chrome_tables.rs:699 places it at
       40,40, and every fixed row resolves against the viewport -- so a page
       that puts it anywhere else moves the 36 rows with a fixed child. A
       backtick here
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
        // Offsets for in-flow children only: an out-of-flow child is already
        // placed by insets, so an offset row there would restate a scene the
        // table already carries.
        const offsets = outOfFlow(CHILD_POSITION[c]) ? ['-'] : ['-', 'i', 'n']
        for (const t of ['n', 't']) {
          for (const i of offsets) {
            const code = `${o}${p}${c}${t}${i}`
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
  }

  const header = [
    '# Chrome 151.0.7922.34, 2026-09-06: overflow x clipper position x child',
    '# position x transform x the child offsets. The 120 rows without offsets',
    '# were measured by hand on 2026-08-23 and are reproduced here unchanged.',
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
    '#       I: -=no offsets  i=left:8px;top:6px  n=left:-8px;top:-6px on the',
    '#          child, in flow only -- an out-of-flow child is placed by insets',
    '#          already. The `-` rows are the 120 measured by hand; the letter',
    '#          was added to them rather than the offsets being a separate file,',
    '#          so their values must not move.',
    '# outer sits at 40,40 on the page. That is not decoration: every `fixed`',
    '# row resolves against the viewport rather than against outer, so a page',
    '# that places outer anywhere else moves the 36 rows with a fixed child.',
    '# The first version of this tool put it at 0,0 and did exactly that.',
    '#',
    '# Two pairs of letters are known not to separate anywhere in the table,',
    '# and they are recorded rather than left for the next reader to notice:',
    "# the clipper's R and K agree in all 48 comparable rows, because nothing",
    '# scrolls, and a sticky box with nothing to stick to is a relative one;',
    '# h and s agree in all 80, because the scroll rows carry',
    '# scrollbar-width:none deliberately, so no gutter distinguishes them.',
    '# Neither is fixed by the offsets. An undocumented non-separating pair',
    "# and a documented one look identical, which is how the child's own",
    '# S-against-R pair survived 120 rows.',
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
