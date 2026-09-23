// Whether a truncation changes an element's intrinsic width in Chrome (CSS Sizing 3
// §5.1 says not). Each string is measured as `plain`, `ellipsis`, `nowrap` -- the
// ellipsis control, since `nowrap` alone raises min-content -- and `clamp`, the
// analogue of `maxLines`; the pairs are what is read.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { FONT, open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/min-content.tsv')

/**
 * The strings to ask about, and the size: `HP`, the reported label with no break
 * opportunity; `Flower of Paradise`, whose min- and max-content differ by 56
 * pixels; and one long word wider than any container, which must overflow.
 */
const CASES = [
  { text: 'HP', size: 12 },
  { text: 'Flower of Paradise', size: 16 },
  { text: 'Antidisestablishmentarianism', size: 16 },
]

/**
 * The four styles each string is measured under.
 *
 * Written as declarations rather than as a flag the page interprets, so the
 * table records what was actually set on the element.
 */
const VARIANTS = [
  { name: 'plain', css: '' },
  { name: 'nowrap', css: 'white-space: nowrap; overflow: hidden;' },
  { name: 'ellipsis', css: 'white-space: nowrap; overflow: hidden; text-overflow: ellipsis;' },
  { name: 'clamp', css: 'display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 1; overflow: hidden;' },
]

const browser = await open()
try {
  await browser.page.evaluate(
    ({ cases, variants, family }) => {
      const host = document.createElement('div')
      // Floated so each probe shrink-wraps to the width being asked for
      // rather than to the body's. A probe left in flow reports the
      // viewport's width for every row, which is the failure mode that makes
      // a whole table agree and mean nothing.
      host.id = 'probes'
      for (const { text, size } of cases) {
        for (const { name, css } of variants) {
          for (const sizing of ['min-content', 'max-content']) {
            const probe = document.createElement('div')
            probe.dataset.key = `${text}|${size}|${name}|${sizing}`
            probe.setAttribute('style', `float: left; clear: both; font: ${size}px "${family}"; width: ${sizing}; ${css}`)
            probe.textContent = text
            host.append(probe)
          }
        }
      }
      document.body.append(host)
    },
    { cases: CASES, variants: VARIANTS, family: FONT.family },
  )

  // The font and any other asset, proved usable before a single rectangle is
  // read. A probe measured against the fallback face reports a plausible
  // number for the wrong font.
  await settle(browser.page)

  const rows = await browser.page.evaluate(() => {
    const measured = new Map()
    for (const probe of document.querySelectorAll('#probes > div')) {
      // The border box Chrome resolved, not a number we computed from
      // metrics: what is under test is the browser's own sizing.
      measured.set(probe.dataset.key, probe.getBoundingClientRect().width)
    }
    return [...measured].map(([key, width]) => [key, width])
  })

  const widths = new Map(rows)
  const lines = [
    '# What Chrome reports as an element’s intrinsic width, by truncation.',
    '# Written by packages/meo-canvas/tools/conformance/mincontent.mjs.',
    '# text\tsize\tvariant\tmin-content\tmax-content',
  ]
  for (const { text, size } of CASES) {
    for (const { name } of VARIANTS) {
      const min = widths.get(`${text}|${size}|${name}|min-content`)
      const max = widths.get(`${text}|${size}|${name}|max-content`)
      if (min === undefined || max === undefined) {
        throw new Error(`no rectangle for ${text} at ${size} as ${name}`)
      }
      lines.push(
        [
          // The whole string, quoted: the column is data for a walker, not a
          // caption for a reader.
          JSON.stringify(text),
          size,
          name,
          min.toFixed(2),
          max.toFixed(2),
        ].join('\t'),
      )
    }
  }

  const written = table(lines)
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    console.log(`wrote ${DESTINATION}`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
