// Whether `direction` reaches a box that does not state one (`l7aromeo/meo-canvas#165`).
// Every case puts the direction on the page, on the box, on both, or on neither, and
// reads what moves: a grid's tracks, a flex row's items, a line's `text-align`.
import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/direction-inherit.tsv')

/** The page the box cases sit on, and the box: 20px in from the left, two 30px slots. */
const PAGE = { width: 100, height: 20 }
const BOX = { margin: 20, width: 60, height: 20, slot: 30 }

/** The cell the text cases sit on, wider than the line so placement shows. */
const CELL = { width: 300, height: 60 }

/** What the text cell is painted with, so a line's own extent can be found. */
const PAPER = [255, 255, 255]

/**
 * The box cases: where the direction is stated, and why the row is here. `wrapper`
 * puts a block between the page and the box, stating `wrapperDirection` if any, and
 * `absolute` places the box against the page rather than in flow.
 */
const BOXES = [
  { name: 'grid under an rtl page', display: 'grid', page: 'rtl', box: null, note: 'the defect: the grid states nothing and inherits rtl' },
  { name: 'grid under an rtl grandparent', display: 'grid', page: 'rtl', box: null, wrapper: true, note: 'the same, through a block that states nothing' },
  { name: 'grid stating rtl', display: 'grid', page: null, box: 'rtl', note: 'control: its own rtl, which already matched' },
  { name: 'grid and page both rtl', display: 'grid', page: 'rtl', box: 'rtl', note: 'control: stated twice' },
  { name: 'grid with no direction anywhere', display: 'grid', page: null, box: null, note: 'control: the root default is ltr' },
  { name: 'grid stating ltr under an rtl page', display: 'grid', page: 'rtl', box: 'ltr', note: 'an explicit value overrides the inherited one' },
  {
    name: 'absolute grid in an rtl block on an ltr page',
    display: 'grid',
    page: null,
    box: null,
    wrapper: true,
    wrapperDirection: 'rtl',
    absolute: true,
    note: 'inherits from its parent, not from the page it is placed against',
  },
  { name: 'flex row under an rtl page', display: 'flex', page: 'rtl', box: null, note: 'the defect in a flex row: items start at the right' },
  { name: 'flex row stating rtl', display: 'flex', page: null, box: 'rtl', note: 'control: its own rtl' },
  { name: 'flex row with no direction anywhere', display: 'flex', page: null, box: null, note: 'control: ltr' },
]

/** A block child's size, placed by its parent's direction alone. */
const BLOCK = { width: 30, height: 20 }

/**
 * The block cases: a block child of the page, directly or through a block that
 * states nothing, measured by its own left edge.
 */
const BLOCKS = [
  { name: 'block child under an rtl page', page: 'rtl', wrapper: false, note: 'control: the page places its own child, which already matched' },
  { name: 'block child under an rtl grandparent', page: 'rtl', wrapper: true, note: 'the defect: the block between states nothing and inherits rtl' },
  { name: 'block child with no direction anywhere', page: null, wrapper: true, note: 'control: ltr' },
]

/** The text cases: one line of Fixture, aligned `start` or `end`. */
const LINES = [
  { name: 'start under an rtl page', page: 'rtl', line: null, align: 'start', note: 'the defect in paint: inherited rtl puts `start` at the right' },
  { name: 'end under an rtl page', page: 'rtl', line: null, align: 'end', note: 'and `end` at the left' },
  { name: 'start with no direction anywhere', page: null, line: null, align: 'start', note: 'control: `start` is the left under the root default' },
  { name: 'start stating ltr under an rtl page', page: 'rtl', line: 'ltr', align: 'start', note: 'an explicit value overrides the inherited one' },
]

const browser = await open()
try {
  const rows = []

  await browser.page.setViewportSize({ width: CELL.width, height: CELL.height })
  for (const item of BOXES) {
    const placed = await browser.page.evaluate(
      ({ page, box, item }) => {
        // Inlined: a function cannot cross into the page as an argument.
        const css = direction => (direction === null ? '' : `direction:${direction};`)
        document.body.innerHTML = ''
        const sheet = document.createElement('div')
        sheet.style.cssText = `position:absolute;left:0;top:0;display:block;width:${page.width}px;height:${page.height}px;background:#fff;${css(item.page)}`
        const container = document.createElement('div')
        const tracks = item.display === 'grid' ? `grid-template-columns:${box.slot}px ${box.slot}px;` : ''
        const placement = item.absolute ? `position:absolute;left:${box.margin}px;top:0;` : `margin-left:${box.margin}px;`
        container.style.cssText = `display:${item.display};${placement}width:${box.width}px;height:${box.height}px;${tracks}${css(item.box)}`
        for (const colour of ['#dc2828', '#2850dc']) {
          const child = document.createElement('div')
          const fixed = item.display === 'flex' ? `width:${box.slot}px;flex:none;` : ''
          child.style.cssText = `display:block;height:${box.height}px;${fixed}background:${colour};`
          container.append(child)
        }
        if (item.wrapper) {
          const wrapper = document.createElement('div')
          wrapper.style.cssText = `display:block;${css(item.wrapperDirection ?? null)}`
          wrapper.append(container)
          sheet.append(wrapper)
        } else {
          sheet.append(container)
        }
        document.body.append(sheet)
        const origin = sheet.getBoundingClientRect().left
        return [...container.children].map(child => Math.round(child.getBoundingClientRect().left - origin))
      },
      { page: PAGE, box: BOX, item },
    )
    rows.push([item.name, 'first,second', placed.join(','), item.note].join('\t'))
  }

  for (const item of BLOCKS) {
    const left = await browser.page.evaluate(
      ({ page, block, item }) => {
        const css = direction => (direction === null ? '' : `direction:${direction};`)
        document.body.innerHTML = ''
        const sheet = document.createElement('div')
        sheet.style.cssText = `position:absolute;left:0;top:0;display:block;width:${page.width}px;height:${page.height}px;background:#fff;${css(item.page)}`
        const child = document.createElement('div')
        child.style.cssText = `display:block;width:${block.width}px;height:${block.height}px;background:#dc2828;`
        if (item.wrapper) {
          const wrapper = document.createElement('div')
          wrapper.style.cssText = 'display:block;'
          wrapper.append(child)
          sheet.append(wrapper)
        } else {
          sheet.append(child)
        }
        document.body.append(sheet)
        return Math.round(child.getBoundingClientRect().left - sheet.getBoundingClientRect().left)
      },
      { page: PAGE, block: BLOCK, item },
    )
    rows.push([item.name, 'left', String(left), item.note].join('\t'))
  }

  for (const item of LINES) {
    await browser.page.evaluate(
      ({ cell, item }) => {
        const css = direction => (direction === null ? '' : `direction:${direction};`)
        document.body.innerHTML = ''
        const sheet = document.createElement('div')
        sheet.style.cssText = `position:absolute;left:0;top:0;display:block;width:${cell.width}px;height:${cell.height}px;background:#fff;${css(item.page)}`
        const line = document.createElement('div')
        line.textContent = 'abc'
        line.style.cssText = `font:32px/40px Fixture;color:#000;text-align:${item.align};${css(item.line)}`
        sheet.append(line)
        document.body.append(sheet)
      },
      { cell: CELL, item },
    )
    // Waited for by the harness rather than the page: a shot taken before the face
    // is applied measures the fallback and reports a plausible number.
    await settle(browser.page)
    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...CELL } }))
    let left = null
    let right = null
    for (let x = 0; x < CELL.width; x += 1) {
      let inked = false
      for (let y = 0; y < CELL.height && !inked; y += 1) {
        const [r, g, b] = pixel(shot, x, y)
        inked = r !== PAPER[0] || g !== PAPER[1] || b !== PAPER[2]
      }
      if (inked) {
        left = left === null ? x : left
        right = x
      }
    }
    rows.push([item.name, 'left,width', left === null ? 'absent' : `${left},${right - left + 1}`, item.note].join('\t'))
  }

  const header = [
    '# Chrome, through `just conformance`: whether `direction` reaches a box that does not state one.',
    `# Box cases: a ${PAGE.width}x${PAGE.height} page holding a ${BOX.width}x${BOX.height} box ${BOX.margin}px in from its left,`,
    `# with two ${BOX.slot}px slots -- grid tracks or fixed-width flex items -- red then blue.`,
    `# Block cases: a ${BLOCK.width}x${BLOCK.height} block child of the same page, directly or through a block.`,
    `# Text cases: a ${CELL.width}x${CELL.height} page holding one line "abc" of Fixture at 32px.`,
    '# Written by packages/meo-canvas/tools/conformance/directioninherit.mjs.',
    '#',
    "# `first,second` is each slot child's left edge from the page, in document",
    "# order; `left` is the block child's left edge; `left,width` is the inked",
    '# columns of the line. A direction is on the page, on the box, on both or on',
    '# neither, as the case says.',
    '#',
    '# case\tmeasure\tvalue\tnote',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`direction inherit: ${BOXES.length} box cases, ${BLOCKS.length} blocks, ${LINES.length} lines -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
