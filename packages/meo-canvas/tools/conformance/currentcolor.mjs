// What `currentColor` paints as in Chrome, for each colour property the renderer
// has: each declaration is drawn with `currentColor` and again with each colour
// it could stand for, and the row names the ones whose screenshot is identical.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { FONT, open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/current-color.tsv')

/** The page: the element at (10, 10), with room around it for a shadow. */
const PAGE = { width: 48, height: 48 }

/** The element's own `color`, where a context sets one. */
const OWN = 'rgb(220, 40, 40)'

/** Its parent's `color`, where a context sets one. */
const PARENT = 'rgb(40, 80, 220)'

/** CSS's initial `color` outside a document's own styling: black. */
const INITIAL = 'rgb(0, 0, 0)'

/** Who sets `color`: the element, only its parent, or nobody. */
const CONTEXTS = {
  own: { parent: PARENT, own: OWN },
  inherited: { parent: PARENT, own: null },
  unset: { parent: null, own: null },
}

/**
 * Each case: the declaration under test, and the same declaration with a colour
 * written out. A case whose colour is omitted is the property's default. A
 * fourth field names what the declaration sits on instead of the div: `path`, a
 * square SVG path filling it, or `span`, a span holding its text.
 */
const CASES = [
  ['background-color', 'background-color: currentColor', c => `background-color: ${c}`],
  ['background-color upper case', 'background-color: CURRENTCOLOR', c => `background-color: ${c}`],
  ['border-color', 'border: 4px solid currentColor', c => `border: 4px solid ${c}`],
  ['border-color omitted', 'border: 4px solid', c => `border: 4px solid ${c}`],
  ['box-shadow', 'box-shadow: 4px 4px 0 currentColor', c => `box-shadow: 4px 4px 0 ${c}`],
  ['box-shadow colour omitted', 'box-shadow: 4px 4px 0', c => `box-shadow: 4px 4px 0 ${c}`],
  ['gradient stop', 'background-image: linear-gradient(to right, currentColor, white)', c => `background-image: linear-gradient(to right, ${c}, white)`],
  ['drop-shadow()', 'filter: drop-shadow(4px 4px 0 currentColor)', c => `filter: drop-shadow(4px 4px 0 ${c})`],
  ['drop-shadow() colour omitted', 'filter: drop-shadow(4px 4px 0)', c => `filter: drop-shadow(4px 4px 0 ${c})`],
  ['text-shadow', 'text-shadow: 2px 2px 0 currentColor', c => `text-shadow: 2px 2px 0 ${c}`],
  ['text-shadow colour omitted', 'text-shadow: 2px 2px 0', c => `text-shadow: 2px 2px 0 ${c}`],
  ['text stroke', '-webkit-text-stroke: 1px currentColor', c => `-webkit-text-stroke: 1px ${c}`],
  ['text stroke colour omitted', '-webkit-text-stroke: 1px', c => `-webkit-text-stroke: 1px ${c}`],
  ['text decoration colour omitted', 'text-decoration: underline', c => `text-decoration: underline ${c}`],
  ['color', 'color: currentColor', c => `color: ${c}`],
  ['color-mix()', 'background-color: color-mix(in srgb, currentColor 50%, white)', c => `background-color: color-mix(in srgb, ${c} 50%, white)`],
  ['relative colour', 'background-color: rgb(from currentColor r g b)', c => `background-color: rgb(from ${c} r g b)`],
  ['path fill', 'fill: currentColor', c => `fill: ${c}`, 'path'],
  ['path fill omitted', '', c => `fill: ${c}`, 'path'],
  ['span color', 'color: currentColor', c => `color: ${c}`, 'span'],
]

const browser = await open()
try {
  const { page } = browser
  await page.setViewportSize(PAGE)

  /** A screenshot of the element carrying `declaration` in `context`. */
  const shot = async (context, declaration, target) => {
    const parent = context.parent === null ? '' : `color: ${context.parent};`
    const own = context.own === null ? '' : `color: ${context.own};`
    const content = {
      undefined: 'Hg',
      path: '<svg width="24" height="24"><path id="t" d="M0 0H24V24H0Z"/></svg>',
      span: '<span id="t">Hg</span>',
    }[target]
    const onDiv = target === undefined ? declaration : ''
    const onTarget = target === undefined ? '' : `#t{${declaration}}`
    const markup =
      `<style>#p{position:absolute;inset:0;background:#fff;${parent}}` +
      `#m{position:absolute;left:10px;top:10px;width:24px;height:24px;` +
      `font:16px/24px '${FONT.family}';${own}${onDiv}}${onTarget}</style>` +
      `<div id="p"><div id="m">${content}</div></div>`
    await page.evaluate(html => {
      document.body.innerHTML = html
    }, markup)
    await settle(page)
    return page.screenshot({ clip: { x: 0, y: 0, ...PAGE } })
  }

  const rows = []
  for (const [contextName, context] of Object.entries(CONTEXTS)) {
    const candidates = [
      ['own', context.own],
      ['parent', context.parent],
      ['initial', INITIAL],
    ].filter(([, colour]) => colour !== null)
    for (const [name, declaration, written, target] of CASES) {
      const measured = await shot(context, declaration, target)
      const same = []
      for (const [candidate, colour] of candidates) {
        if (measured.equals(await shot(context, written(colour), target))) same.push(candidate)
      }
      if (measured.equals(await shot(context, '', target))) same.push('absent')
      rows.push([name, contextName, JSON.stringify(declaration), same.length === 0 ? 'none' : same.join(',')].join('\t'))
    }
  }

  const written = table([
    '# Chrome, through `just conformance`. What `currentColor` paints as, per property.',
    '# Written by packages/meo-canvas/tools/conformance/currentcolor.mjs.',
    '#',
    `# A 24px div at (10, 10) on a white 48px page, inside a parent. In context \`own\``,
    `# the div sets \`color: ${OWN}\` and its parent \`color: ${PARENT}\`; in`,
    '# `inherited` only the parent sets it; in `unset` neither does. Each declaration is',
    '# screenshotted, then again with each colour written in place of currentColor, and',
    '# again with the declaration absent: `same as` names those whose pixels are',
    '# identical, `own`, `parent`, `initial` (black) or `absent`, or `none`. The `path`',
    '# cases style a 24px square SVG path in the div, and `span color` a span holding',
    '# its text, rather than the div itself.',
    '#',
    '# case\tcontext\tdeclaration\tsame as',
    ...rows,
  ])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`current colour: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
