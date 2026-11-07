// Whether Chrome keeps a `filter` or `backdrop-filter` chain and whether it
// stacks: a `z-index: -1` child shows over its parent's background only when
// the parent does. Read with `elementFromPoint` at the child's centre, beside
// the declared value, empty where Chrome refused it, and the computed one.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/filter-stacking.tsv')

/** The scene, in the geometry `chrome_filter_stacking.rs` builds on our side. */
const SCENE = {
  parent: { width: 40, height: 30, colour: '#dc2828' },
  child: { at: 5, side: 20, colour: '#2850dc' },
}

const PROPERTIES = ['filter', 'backdrop-filter']

/**
 * Chains either side of the line: none, the drawable ones, identity filters
 * that still stack, and the malformed ones a parser has to refuse.
 */
const CHAINS = [
  'none',
  'NONE',
  ' none ',
  '/* c */ none',
  'grayscale(1)',
  'blur(2px)',
  'blur(0px)',
  'opacity(1)',
  'drop-shadow(2px 2px 0 blue)',
  'grayscale(1) blur(2px)',
  'blur(4 px)',
  'blur(-1px)',
  '',
  'blur(2px), grayscale(1)',
  'blur(',
  'grayscale(1) blur(',
  'brightness(-1)',
  'blur(2)',
]

const browser = await open()
try {
  const answers = await browser.page.evaluate(
    ({ scene, properties, chains }) => {
      const out = []
      for (const property of properties) {
        for (const chain of chains) {
          document.body.innerHTML = ''
          const parent = document.createElement('div')
          parent.style.cssText = `position:relative;width:${scene.parent.width}px;height:${scene.parent.height}px;` + `background:${scene.parent.colour};`
          // `setProperty`, as the API takes a value alone: in `cssText` an unclosed
          // `grayscale(1) blur(` swallows the `;` after it and is refused, where
          // set alone its block closes and Chrome keeps `grayscale(1) blur()`.
          parent.style.setProperty(property, chain)
          const child = document.createElement('div')
          child.style.cssText =
            `position:absolute;left:${scene.child.at}px;top:${scene.child.at}px;` +
            `width:${scene.child.side}px;height:${scene.child.side}px;` +
            `background:${scene.child.colour};z-index:-1;`
          parent.append(child)
          document.body.append(parent)

          const centre = scene.child.at + scene.child.side / 2
          const seen = document.elementFromPoint(centre, centre)
          const top = seen === child ? 'child' : seen === parent ? 'parent' : null
          const declared = parent.style.getPropertyValue(property)
          const computed = getComputedStyle(parent).getPropertyValue(property)
          out.push({ property, chain, declared, computed, top })
        }
      }
      return out
    },
    { scene: SCENE, properties: PROPERTIES, chains: CHAINS },
  )

  const lost = answers.filter(one => one.top === null)
  if (lost.length > 0) {
    for (const one of lost) process.stderr.write(`  ${one.property}: ${JSON.stringify(one.chain)} hit neither box\n`)
    process.exit(1)
  }

  const header = [
    '# Chrome, through `just conformance`. Whether a filter chain stacks.',
    '#',
    `# A ${SCENE.parent.width}x${SCENE.parent.height} relative ${SCENE.parent.colour} parent carrying the chain, holding a`,
    `# ${SCENE.child.side}x${SCENE.child.side} ${SCENE.child.colour} child, absolute at ${SCENE.child.at},${SCENE.child.at}, \`z-index: -1\`. \`top\` is`,
    "# what `elementFromPoint` finds at the child's centre: the child when the",
    '# parent stacks, the parent when the child is hoisted beneath it.',
    '# Each chain is set with `style.setProperty`; `declared` is what the inline',
    '# style kept, empty where Chrome refused the value. `chain`, `declared` and',
    '# `computed` are JSON strings.',
    '# Written by packages/meo-canvas/tools/conformance/filterstacking.mjs.',
    '#',
    '# property\tchain\tdeclared\tcomputed\ttop',
  ]
  const rows = answers.map(({ property, chain, declared, computed, top }) =>
    [property, ...[chain, declared, computed].map(cell => JSON.stringify(cell)), top].join('\t'),
  )
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`filter stacking: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
