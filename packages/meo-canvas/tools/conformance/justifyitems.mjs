// Where one item sits on the inline axis under every value of `justify-items` and
// `justify-self` (`l7aromeo/meo-canvas#171`), in grid, block and flex containers, box
// and image, `ltr` and `rtl`, overflowing, absolute, with its own direction and under a
// `legacy` ancestor, from `getBoundingClientRect`. Baseline groups: `justifybaseline.mjs`.
import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/justify-items.tsv')

/** The container: 200 wide, and a grid has one 200px column, so free space is 200 less the item. */
const CONTAINER = { width: 200, height: 100 }

/** The width a box item's content and an image take: 60 to fit the cell, 260 to overflow it by 60. */
const FITS = 60
const OVERFLOWS = 260

/** Every item is this tall: a box's content, and the image's intrinsic height. */
const HEIGHT = 20

/** An image of the given width with its intrinsic size on the SVG root, which the Rust test draws too. */
const src = width =>
  `data:image/svg+xml,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${HEIGHT}"><rect width="100%" height="100%" fill="#c00"/></svg>`,
  )}`

/** Every `<self-position>`, and each with an overflow keyword. */
const POSITIONS = ['center', 'start', 'end', 'self-start', 'self-end', 'flex-start', 'flex-end', 'left', 'right']
const OVERFLOW = POSITIONS.flatMap(position => [`safe ${position}`, `unsafe ${position}`])

/**
 * Every value `justify-self` takes, `auto` included, less `anchor-center`: anchor
 * positioning has no counterpart in a scene, which refuses the value.
 */
const SELF = ['auto', 'normal', 'stretch', 'baseline', 'first baseline', 'last baseline', ...POSITIONS, ...OVERFLOW]

/** Every value `justify-items` takes, `legacy` in each of its forms included. */
const ITEMS = [
  'normal',
  'stretch',
  'baseline',
  'first baseline',
  'last baseline',
  ...POSITIONS,
  ...OVERFLOW,
  'legacy',
  'legacy left',
  'legacy right',
  'legacy center',
  'left legacy',
]

/**
 * A case, its fields the table's columns, which its header explains; `-` is unset. `outer`
 * is the `justify-items` of each block wrapper between the page and the container,
 * outermost first, joined by `>`.
 */
const at = overrides => ({
  container: 'grid',
  dir: 'ltr',
  outer: '-',
  items: '-',
  self: '-',
  item: 'box',
  width: FITS,
  position: 'static',
  itemdir: '-',
  ...overrides,
})

const both = build => ['ltr', 'rtl'].flatMap(dir => ['box', 'img'].map(item => build(dir, item)))

const CASES = [
  // Every value of each property alone, in a grid cell.
  ...SELF.flatMap(self => both((dir, item) => at({ dir, item, self }))),
  ...ITEMS.flatMap(items => both((dir, item) => at({ dir, item, items }))),
  // `justify-self` overriding a container value, `normal` and `auto` included.
  ...['start', 'center', 'legacy right'].flatMap(items =>
    ['normal', 'auto', 'stretch', 'end'].flatMap(self => both((dir, item) => at({ dir, item, items, self }))),
  ),
  // A block child under every value of `justify-self`, and under its container's.
  ...SELF.flatMap(self => both((dir, item) => at({ container: 'block', dir, item, self }))),
  ...['center', 'end', 'left', 'right', 'stretch', 'last baseline', 'legacy center', 'legacy right'].flatMap(items =>
    both((dir, item) => at({ container: 'block', dir, item, items })),
  ),
  // A flex container ignores both.
  ...['center', 'right', 'end'].flatMap(self => both((dir, item) => at({ container: 'flex', dir, item, self }))),
  ...['center', 'legacy right'].flatMap(items => both((dir, item) => at({ container: 'flex', dir, item, items }))),
  // An item wider than its cell: where `safe` and `unsafe` part.
  ...['start', 'center', 'end', 'self-end', 'left', 'right', 'stretch', 'last baseline', ...OVERFLOW.filter(value => !value.includes('flex'))].flatMap(self =>
    both((dir, item) => at({ dir, item, self, width: OVERFLOWS })),
  ),
  ...['center', 'end', 'safe center', 'unsafe center', 'safe end', 'right', 'safe right'].flatMap(self =>
    both((dir, item) => at({ container: 'block', dir, item, self, width: OVERFLOWS })),
  ),
  // An absolutely positioned child, fitting and overflowing.
  ...['-', 'center', 'end', 'left', 'right', 'stretch', 'baseline', 'last baseline', 'safe center', 'unsafe end'].flatMap(self =>
    ['grid', 'block'].flatMap(container => both((dir, item) => at({ container, dir, item, self, position: 'absolute' }))),
  ),
  ...['center', 'safe center', 'unsafe center', 'end', 'safe end'].flatMap(self =>
    both((dir, item) => at({ dir, item, self, position: 'absolute', width: OVERFLOWS })),
  ),
  ...['center', 'end', 'left', 'right', 'baseline', 'last baseline', 'legacy right'].flatMap(items =>
    ['grid', 'block'].flatMap(container => both((dir, item) => at({ container, dir, item, items, position: 'absolute' }))),
  ),
  // An item stating the other direction: `self-*` follow it, `left` and `right` do not.
  ...['self-start', 'self-end', 'start', 'end', 'left', 'right', 'safe self-end'].flatMap(self =>
    ['grid', 'block'].flatMap(container => both((dir, item) => at({ container, dir, item, self, itemdir: dir === 'ltr' ? 'rtl' : 'ltr' }))),
  ),
  // `legacy` reaching a container through block wrappers, and what stops it.
  ...['legacy left', 'legacy right', 'legacy center', 'center', 'normal'].flatMap(outer =>
    ['-', 'legacy', 'normal', 'start', 'left'].flatMap(items =>
      ['grid', 'block'].flatMap(container => both((dir, item) => at({ container, dir, item, outer, items }))),
    ),
  ),
  ...['legacy right>-', 'legacy right>normal', 'legacy right>legacy', 'legacy right>legacy left', 'legacy center>-'].flatMap(outer =>
    ['grid', 'block'].flatMap(container => both((dir, item) => at({ container, dir, item, outer }))),
  ),
]

const browser = await open()
try {
  await browser.page.setViewportSize({ width: 600, height: 300 })
  const rows = []
  for (const item of CASES) {
    await browser.page.evaluate(
      ({ container, height, src, item }) => {
        document.body.innerHTML = ''
        const page = document.createElement('div')
        page.style.cssText = `position:absolute;left:0;top:0;width:600px;direction:${item.dir};`
        let parent = page
        if (item.outer !== '-') {
          for (const value of item.outer.split('>')) {
            const wrapper = document.createElement('div')
            wrapper.style.cssText = `display:block;width:${container.width}px;${value === '-' ? '' : `justify-items:${value};`}`
            parent.append(wrapper)
            parent = wrapper
          }
        }
        const box = document.createElement('div')
        box.id = 'container'
        const tracks = item.container === 'grid' ? `grid-template-columns:${container.width}px;` : ''
        const items = item.items === '-' ? '' : `justify-items:${item.items};`
        box.style.cssText = `display:${item.container};position:relative;width:${container.width}px;height:${container.height}px;${tracks}${items}`
        let child
        if (item.item === 'img') {
          child = document.createElement('img')
          child.src = src
          child.style.cssText = 'display:block;'
        } else {
          child = document.createElement('div')
          const inner = document.createElement('div')
          inner.style.cssText = `width:${item.width}px;height:${height}px;`
          child.append(inner)
          // A block child needs a width to have something to place; a grid item, a
          // flex item and an absolute one are sized by their content.
          if (item.container === 'block') child.style.width = `${item.width}px`
        }
        child.id = 'item'
        if (item.self !== '-') child.style.justifySelf = item.self
        if (item.position === 'absolute') child.style.position = 'absolute'
        if (item.itemdir !== '-') child.style.direction = item.itemdir
        box.append(child)
        parent.append(box)
        document.body.append(page)
        // A value the browser refused would measure as its initial value and record
        // a row nothing asked for.
        for (const [element, property, value] of [
          [box, 'justifyItems', item.items],
          [child, 'justifySelf', item.self],
        ]) {
          if (value !== '-' && element.style[property] === '') throw new Error(`the browser refused ${property}: ${value}`)
        }
      },
      { container: CONTAINER, height: HEIGHT, src: src(item.width), item },
    )
    await settle(browser.page)
    const [x, w, h] = await browser.page.evaluate(() => {
      const item = document.getElementById('item').getBoundingClientRect()
      const container = document.getElementById('container').getBoundingClientRect()
      return [item.left - container.left, item.width, item.height]
    })
    rows.push(
      [
        item.container,
        item.dir,
        item.outer,
        item.items,
        item.self,
        item.item,
        item.width,
        item.position,
        item.itemdir,
        x.toFixed(3),
        w.toFixed(3),
        h.toFixed(3),
      ].join('\t'),
    )
  }

  const header = [
    '# Chrome, through `just conformance`: where justify-items and justify-self place one item on the inline axis.',
    `# A ${CONTAINER.width}x${CONTAINER.height} relative container, \`grid\` with one ${CONTAINER.width}px column, \`block\` or \`flex\`, holding one item.`,
    `# \`box\` is a block holding content \`width\` wide (a block child is itself that wide); \`img\` is a \`width\`x${HEIGHT} SVG.`,
    "# `dir` is stated on the page and inherited; `itemdir` is the item's own direction, `-` for inherited; an absolute",
    `# item has no insets. \`outer\` is a chain of ${CONTAINER.width}-wide block wrappers between the page and the container,`,
    '# outermost first and joined by `>`, each naming its justify-items or `-` for unset. `-` elsewhere is unset.',
    '# Written by packages/meo-canvas/tools/conformance/justifyitems.mjs.',
    '#',
    '# x, w and h are the item from getBoundingClientRect, x from the container.',
    '#',
    '# container\tdir\touter\titems\tself\titem\twidth\tposition\titemdir\tx\tw\th',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`justify items: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
