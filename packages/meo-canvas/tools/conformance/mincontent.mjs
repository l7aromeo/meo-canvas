// Whether a truncation changes an element's **intrinsic** width, in Chrome.
//
// The claim under test is that it does not: `text-overflow: ellipsis` and
// `-webkit-line-clamp` describe what is drawn once the used width is already
// below what the content wants, and CSS Sizing 3 §5.1 derives min-content from
// the content alone. If that is right, a clamped element and a plain one
// report the same `min-content`, and a flex item floored at that value cannot
// be squeezed down to its ellipsis.
//
// **The pair is the measurement.** A single element's min-content is
// consistent with any rule at all, so every string is measured four ways -- the
// two truncations Chrome actually has, each against its own control -- and what
// is read off is whether the two halves of a pair agree:
//
//   plain      wrapping, no truncation. The baseline.
//   ellipsis   `white-space: nowrap; overflow: hidden; text-overflow: ellipsis`
//   nowrap     the same **without** `text-overflow`. The ellipsis control:
//              `nowrap` raises min-content to the whole run on its own, so a
//              pair that omitted this would credit that to the marker.
//   clamp      `-webkit-line-clamp: 1`, which truncates while still wrapping
//              and so is the analogue of this renderer's `maxLines`.
//
// A `min-content` that equals its own `max-content` in every row would be a
// suite measuring nothing, which is why `Flower of Paradise` is here: it wraps
// at spaces, so its two intrinsic widths genuinely differ and a rule that
// collapsed to either end is visible in the table.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { FONT, open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/min-content.tsv')

/**
 * The strings to ask about, and the size to ask at.
 *
 * `HP` is the reported case: two letters, no break opportunity, and the label
 * that collapsed to an ellipsis in a row with room to spare. `Flower of
 * Paradise` is the discriminating one -- min-content is `Paradise` and
 * max-content is the whole run, and those differ by 56 pixels. The single long
 * word is the third shape: nowhere to break *and* wider than any container it
 * would be put in, where a rule that floors at min-content has to admit an
 * overflow rather than shrink.
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
