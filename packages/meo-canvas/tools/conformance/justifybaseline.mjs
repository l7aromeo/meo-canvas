// Where grid items sharing a column sit on the inline axis under `justify-self:
// baseline` and `last baseline` (`l7aromeo/meo-canvas#171`): a horizontal item's
// baseline is synthesized from its border box, and a column's items align on it as a
// group. Read as rectangles from `getBoundingClientRect`.
import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/justify-baseline.tsv')

/** The grid: 200 wide, as one 200px column or two of 100. */
const WIDTH = 200

/** Every item is this tall. */
const HEIGHT = 10

/** An image of the given width with its intrinsic size on the SVG root, which the Rust test draws too. */
const src = width =>
  `data:image/svg+xml,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${HEIGHT}"><rect width="100%" height="100%" fill="#c00"/></svg>`,
  )}`

/**
 * One item: `kind` is `box` (a block `w` wide) or `img` (an image `w` wide); `ml`, `mr`,
 * `bl` and `br` are its left and right margins and borders; `col` and `row` its grid
 * lines, `-` for auto; `self` its `justify-self`, `-` for unset; `abs` takes it out of
 * flow.
 */
const item = (w, extra = {}) => ({ kind: 'box', w, ml: 0, mr: 0, bl: 0, br: 0, col: '-', row: '-', self: '-', abs: false, ...extra })

/** The spellings an item list takes in the table: `box:60 ml=10 self=last baseline`. */
const spell = it =>
  [
    `${it.kind}:${it.w}`,
    ...['ml', 'mr', 'bl', 'br'].filter(key => it[key] !== 0).map(key => `${key}=${it[key]}`),
    ...(it.col === '-' ? [] : [`col=${it.col}`]),
    ...(it.row === '-' ? [] : [`row=${it.row}`]),
    ...(it.abs ? ['abs'] : []),
    ...(it.self === '-' ? [] : [`self=${it.self}`]),
  ].join(' ')

const GROUP = ['baseline', 'last baseline']

/** A case: the grid's `tracks`, its `items` value, and its children. */
const CASES = GROUP.flatMap(value => [
  { tracks: '200', items: '-', children: [item(60, { self: value }), item(107, { self: value })] },
  { tracks: '200', items: '-', children: [item(60, { kind: 'img', self: value }), item(100, { kind: 'img', self: value })] },
  { tracks: '200', items: '-', children: [item(60, { self: value, ml: 10, mr: 5 }), item(100, { self: value, bl: 3, br: 9 })] },
  { tracks: '200', items: '-', children: [item(60, { self: value }), item(100, { self: 'start' })] },
  { tracks: '200', items: '-', children: [item(60, { self: value }), item(100, { self: value }), item(30, { self: value, ml: 20 })] },
  { tracks: '200', items: value, children: [item(60), item(100)] },
  { tracks: '200', items: value, children: [item(60), item(100, { self: 'normal' })] },
  { tracks: '200', items: '-', children: [item(60, { self: value }), item(100, { self: value, abs: true })] },
  { tracks: '200', items: '-', children: [item(60, { self: value, ml: 50 }), item(180, { self: value })] },
  { tracks: '100 100', items: '-', children: [item(60, { self: value }), item(30, { self: value }), item(40, { self: value })] },
  { tracks: '100 100', items: '-', children: [item(60, { self: value, col: '1 / 3' }), item(30, { self: value, col: '1', row: '2' })] },
  { tracks: '100 100', items: '-', children: [item(60, { self: value, col: '1 / 3' }), item(30, { self: value, col: '2', row: '2' })] },
  { tracks: '100 100', items: '-', children: [item(80, { self: value, col: '1' }), item(50, { self: value, col: '1' }), item(20, { self: value, col: '2' })] },
])

const browser = await open()
try {
  await browser.page.setViewportSize({ width: 600, height: 300 })
  const rows = []
  for (const dir of ['ltr', 'rtl']) {
    for (const scene of CASES) {
      await browser.page.evaluate(
        ({ width, height, dir, scene, sources }) => {
          document.body.innerHTML = ''
          const grid = document.createElement('div')
          grid.id = 'grid'
          const items = scene.items === '-' ? '' : `justify-items:${scene.items};`
          const columns = scene.tracks
            .split(' ')
            .map(track => `${track}px`)
            .join(' ')
          grid.style.cssText = `position:absolute;left:0;top:0;direction:${dir};display:grid;width:${width}px;grid-template-columns:${columns};${items}`
          scene.children.forEach((it, index) => {
            const child = document.createElement(it.kind === 'img' ? 'img' : 'div')
            if (it.kind === 'img') child.src = sources[index]
            else child.style.cssText = `width:${it.w}px;height:${height}px;`
            child.style.display = 'block'
            child.style.marginLeft = `${it.ml}px`
            child.style.marginRight = `${it.mr}px`
            child.style.borderLeft = `${it.bl}px solid`
            child.style.borderRight = `${it.br}px solid`
            if (it.col !== '-') child.style.gridColumn = it.col
            if (it.row !== '-') child.style.gridRow = it.row
            if (it.abs) child.style.position = 'absolute'
            if (it.self !== '-') child.style.justifySelf = it.self
            grid.append(child)
          })
          document.body.append(grid)
        },
        { width: WIDTH, height: HEIGHT, dir, scene, sources: scene.children.map(it => src(it.w)) },
      )
      await settle(browser.page)
      const placed = await browser.page.evaluate(() => {
        const grid = document.getElementById('grid').getBoundingClientRect()
        return [...document.getElementById('grid').children].map(child => {
          const box = child.getBoundingClientRect()
          return [box.left - grid.left, box.width]
        })
      })
      rows.push(
        [
          dir,
          scene.tracks,
          scene.items,
          scene.children.map(spell).join('; '),
          placed.map(([x]) => x.toFixed(3)).join(' '),
          placed.map(([, w]) => w.toFixed(3)).join(' '),
        ].join('\t'),
      )
    }
  }

  const header = [
    '# Chrome, through `just conformance`: where grid items sharing a column sit under baseline and last baseline.',
    `# A ${WIDTH}-wide grid with the columns \`tracks\` names in pixels and \`items\` as its justify-items, \`-\` for unset.`,
    `# Each child is \`box:W\` (a block W wide) or \`img:W\` (a Wx${HEIGHT} SVG), ${HEIGHT} tall, with \`ml\`, \`mr\`, \`bl\` and \`br\` its`,
    '# left and right margins and borders in pixels, `col` and `row` its grid lines, `abs` for absolutely positioned,',
    '# and `self` its justify-self. `dir` is stated on the grid.',
    '# Written by packages/meo-canvas/tools/conformance/justifybaseline.mjs.',
    '#',
    "# x and w list each child's border box from getBoundingClientRect, in document order, x from the grid.",
    '#',
    '# dir\ttracks\titems\tchildren\tx\tw',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`justify baseline: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
