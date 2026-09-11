// Which of two overlapping siblings Chrome paints on top.
//
// Four sections, each a complete cross-product, so the case list is
// recoverable from the table rather than remembered:
//
//     position          125  = 5 displays x 5 positions(a) x 5 positions(b)
//     z-positioned       75  = 3 displays x 5 za x 5 zb
//     z-static           75  = 3 displays x 5 za x 5 zb
//     stacking-context    6  = 3 displays x 2 za
//
// **The sample point comes from rectangles and the answer does not**, which is
// how this obeys the rule differently from every other tool here. There is no
// rectangle whose width is the answer: the answer is *which box is on top*. So
// the two boxes' own reported rectangles give the true intersection, its centre
// is the point, and `elementFromPoint` reads what is painted there. A reader who
// knows the rule will look for a rectangle being measured and should find this
// paragraph instead.
//
// **Every case is measured alone, and that is part of the measurement rather
// than housekeeping.** A `position: fixed` box resolves against the viewport,
// so a case left on the page can sit over a later case's intersection and
// answer for the wrong pair. Four cases did exactly that while the method was
// being worked out, which is why the body is emptied between them.
//
// **A `fixed` box is given no inset, and that is the other half of the same
// rule -- established by measurement rather than chosen.** Written with
// `left: 0; top: 0` it goes to the viewport's origin while its sibling sits at
// the parent's, and the two never overlap: 11 of the 45 `fixed` cases had no
// intersection to sample at all and this tool refused to write them. With the
// inset omitted the box keeps its static position, all 281 cases have an
// overlap, and the table reproduces the committed one exactly. So the absence
// of an inset here is not an oversight to be tidied up: putting one back
// removes eleven rows from a complete cross-product.
//
// **A pair that does not overlap fails the run rather than being dropped.**
// 281 rows is a complete cross-product; a table arriving with 277 has found
// something a person needs to see, and a tool that quietly emits fewer rows
// than it has cases is the failure every check in this directory exists to
// avoid.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/paint-order.tsv')

/** The scene, in the geometry `chrome_tables.rs` builds on our side. */
const SCENE = {
  parent: { width: 90, height: 60, left: 44, top: 44 },
  child: { width: 50, height: 34 },
  // Out of flow, B is offset from A; in flow it is pulled back over A along the
  // parent's own axis. Both give an overlap whose centre is unambiguous.
  outOfFlow: { b: { left: 24, top: 14 } },
  gridB: { marginTop: 14, marginLeft: 24 },
  flexB: { marginLeft: -26 },
  blockB: { marginTop: -20 },
  ink: { a: 'rgb(220, 40, 40)', b: 'rgb(40, 80, 220)', parent: 'rgb(238, 238, 238)' },
}

const DISPLAYS = ['block', 'flex', 'grid', 'inline-block', 'table-cell']
const POSITIONS = ['static', 'relative', 'absolute', 'fixed', 'sticky']
const Z = ['auto', '-1', '0', '1', '2']

/** Every case, in the order the sections are written. */
function cases() {
  const out = []
  for (const display of DISPLAYS) {
    for (const a of POSITIONS) {
      for (const b of POSITIONS) {
        out.push({ section: 'position', display, a, b, za: 'auto', zb: 'auto', parent_z: 'auto' })
      }
    }
  }
  for (const [section, position] of [
    ['z-positioned', 'relative'],
    ['z-static', 'static'],
  ]) {
    for (const display of DISPLAYS.slice(0, 3)) {
      for (const za of Z) {
        for (const zb of Z) {
          out.push({ section, display, a: position, b: position, za, zb, parent_z: 'auto' })
        }
      }
    }
  }
  for (const display of DISPLAYS.slice(0, 3)) {
    for (const za of ['-1', '1']) {
      out.push({ section: 'stacking-context', display, a: 'relative', b: 'relative', za, zb: 'auto', parent_z: '0' })
    }
  }
  return out
}

const browser = await open()
try {
  const answers = await browser.page.evaluate(
    ({ scene, list }) => {
      const out = []
      for (const row of list) {
        // Emptied per case: a `fixed` box left on the page resolves against the
        // viewport and can answer for a later pair.
        document.body.innerHTML = ''

        const wrapper = document.createElement('div')
        wrapper.style.cssText = `display:block;padding:${scene.parent.top}px 0 0 ${scene.parent.left}px;`

        const parent = document.createElement('div')
        parent.style.cssText =
          `position:relative;display:${row.display};width:${scene.parent.width}px;height:${scene.parent.height}px;` +
          `background:${scene.ink.parent};` +
          (row.parent_z === 'auto' ? '' : `z-index:${row.parent_z};`)

        const made = { A: null, B: null }
        for (const which of ['A', 'B']) {
          const isB = which === 'B'
          const position = isB ? row.b : row.a
          const z = isB ? row.zb : row.za
          const child = document.createElement('div')
          let css =
            `display:block;width:${scene.child.width}px;height:${scene.child.height}px;` +
            `position:${position};background:${isB ? scene.ink.b : scene.ink.a};` +
            (z === 'auto' ? '' : `z-index:${z};`)
          if (position === 'absolute') {
            const at = isB ? scene.outOfFlow.b : { left: 0, top: 0 }
            css += `left:${at.left}px;top:${at.top}px;`
          } else if (row.display === 'grid') {
            // **Line 1 explicitly, not `span 1` alone.** A bare span is
            // auto-placed, so the two children take different cells and never
            // overlap; `GridPlacement::spanning(1, 1)` on our side names the
            // line, and this is the same statement in CSS.
            css += 'grid-row:1 / span 1;grid-column:1 / span 1;'
            if (isB) css += `margin:${scene.gridB.marginTop}px 0 0 ${scene.gridB.marginLeft}px;`
          } else if (isB) {
            css += row.display === 'flex' ? `margin-left:${scene.flexB.marginLeft}px;` : `margin-top:${scene.blockB.marginTop}px;`
          }
          child.style.cssText = css
          parent.append(child)
          made[which] = child
        }

        wrapper.append(parent)
        document.body.append(wrapper)

        const a = made.A.getBoundingClientRect()
        const b = made.B.getBoundingClientRect()
        const x0 = Math.max(a.left, b.left)
        const y0 = Math.max(a.top, b.top)
        const x1 = Math.min(a.right, b.right)
        const y1 = Math.min(a.bottom, b.bottom)
        if (x1 - x0 < 2 || y1 - y0 < 2) {
          out.push({ row, error: `the two boxes do not overlap: A ${JSON.stringify(a)}, B ${JSON.stringify(b)}` })
          continue
        }

        const seen = document.elementFromPoint((x0 + x1) / 2, (y0 + y1) / 2)
        const top = seen === made.A ? 'A' : seen === made.B ? 'B' : seen === parent ? 'P' : null
        if (top === null) {
          out.push({ row, error: `the overlap centre is none of A, B or the parent: ${seen === null ? 'nothing' : seen.tagName}` })
          continue
        }
        out.push({ row, top })
      }
      return out
    },
    { scene: SCENE, list: cases() },
  )

  const broken = answers.filter(one => one.error !== undefined)
  if (broken.length > 0) {
    for (const one of broken)
      process.stderr.write(`  ${one.row.section} ${one.row.display} ${one.row.a}:${one.row.za} vs ${one.row.b}:${one.row.zb} -- ${one.error}\n`)
    process.stderr.write(
      `\n${broken.length} of ${answers.length} cases could not be measured. Every one is a case with no answer rather than ` +
        'an answer of nothing, and a table written without them would be shorter than the cross-product it claims to be.\n',
    )
    process.exit(1)
  }

  const rows = answers.map(({ row, top }) => [row.section, row.display, row.a, row.b, row.za, row.zb, row.parent_z, top].join('\t'))

  const header = [
    '# Chrome, through `just conformance`. Which of two overlapping siblings paints on top.',
    '#',
    `# A ${SCENE.parent.width}x${SCENE.parent.height} relative parent at ${SCENE.parent.left},${SCENE.parent.top}, holding two`,
    `# ${SCENE.child.width}x${SCENE.child.height} children that overlap. \`top\` is A, B, or P for the parent's own`,
    '# background, read by `elementFromPoint` at the centre of the two reported',
    '# rectangles. Each case is measured on an emptied page: a `fixed` box left',
    '# behind resolves against the viewport and answers for the wrong pair.',
    '#',
    `# ${DISPLAYS.length} displays x ${POSITIONS.length}x${POSITIONS.length} positions = 125`,
    `# + 3 displays x ${Z.length}x${Z.length} z on relative children = 75`,
    `# + 3 displays x ${Z.length}x${Z.length} z on static children = 75`,
    '# + 3 displays x 2 z under a parent that stacks = 6',
    `# = ${cases().length} rows, four complete cross-products with no gaps.`,
    '#',
    '# THE COLUMN LINE BELOW IS ASSERTED BY THE READER, not skipped. A tab-separated',
    '# table is positional, so a column emitted in a different order would be read as',
    '# a different field and the test would stay green measuring something else.',
    '#',
    '# section\tdisplay\ta\tb\tza\tzb\tparent_z\ttop',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`paint order: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
