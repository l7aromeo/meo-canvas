import { createRequire } from 'node:module'
import { readFileSync, writeFileSync } from 'node:fs'

import { describe, expect, it } from 'vitest'

import { Chart } from './chart.js'
import { Box } from './node.js'
import { encodeScene } from './arena.js'

/**
 * The same chart, built twice, compared as bytes.
 *
 * # Why bytes and why two implementations
 *
 * **For `Chart` there is no external adjudicator.** Chrome has no charts, v1 is
 * both baselines, and the arithmetic is the specification. So the strongest
 * check available is that **two independent implementations produce the same
 * scene** — and if they differ, one is wrong and the comparison says so
 * without either being trusted.
 *
 * # What this closes, and what it does not
 *
 * **It closes the port and not the geometry.** Both surfaces agreeing on a
 * wrong bar edge passes every byte of this file. What guards the numbers is
 * the rendering: `chart.render.test.ts` on this side and the equivalent on the
 * Rust side, which check the arithmetic against pixels rather than against
 * itself. **Three checks, three different questions, and none substitutes for
 * another.**
 *
 * # Why a comparison to four decimals is safe here
 *
 * The pie's path data is formatted with `toFixed(4)`, and the two languages
 * round halfway cases differently — JavaScript away from zero, Rust to even.
 * **A tie at four decimals needs an odd multiple of `1/20000`, and
 * `20000 = 2^5 x 625` is not a power of two, so no binary float lands on
 * one.** The only transcendentals in chart geometry are `sin` and `cos`, which
 * were measured bit-identical between V8 and libm in the animation work;
 * `exp`, the one known to differ, does not appear.
 *
 * # Why the asset is checked in rather than written here
 *
 * `ci` runs the Rust tests **before** the JavaScript ones. A test that wrote
 * the asset on every run would leave the Rust side comparing against whatever
 * the previous run produced — the stale-artifact trap, with the staleness
 * created by the suite itself. So the bytes are committed, both sides assert
 * against them, and a legitimate change to the chart is a deliberate
 * regeneration: `UPDATE_CHART_BYTES=1 npx vitest run chart.agreement`.
 */
const ASSET = new URL('../../../crates/meo-canvas/tests/assets/chart/bar-bytes.txt', import.meta.url).pathname

interface Addon {
  sceneBytes(slots: Float64Array, values: readonly (string | Buffer)[]): Buffer
}

function addon(): Addon {
  try {
    return createRequire(import.meta.url)('../meo-canvas.node') as Addon
  } catch (cause) {
    throw new Error('the addon is not built; run `just addon`. This is the only check here that reaches the byte codec.', { cause })
  }
}

/**
 * The chart both surfaces build.
 *
 * **Every option the Rust side has is switched on**, because an option left at
 * its default is one the comparison never sees: two implementations agree
 * trivially about a branch neither takes.
 */
const CHART = () =>
  // Wrapped, because `Root::new(200, 120)` on the Rust side contributes a page
  // root of its own — so a chart used directly as the page would be one node
  // short and every byte after the count would shift. **The first run differed
  // by exactly that node and by a font family I had set on one side only:
  // both were the harness, neither was an implementation.**
  // A bare wrapper, and **only the subtree from `bar chart` onward is
  // compared.** The two surfaces frame a page differently — `Root::new` on one
  // side, a page root passed to `encodeScene` on the other — and their default
  // page styles disagree in ways that are about the frame rather than about
  // the chart. Comparing the whole scene measured my harness; comparing from
  // the chart's own node measures the two chart implementations.
  Box({
    children: Chart({
      type: 'bar',
      fontFamily: 'Fixture',
      data: {
        labels: ['a', 'b'],
        datasets: [{ data: [1, 2], label: 'Sales', color: '#3366cc' }, { data: [2, 1] }],
      },
      options: {
        showLabels: true,
        showValues: true,
        showYAxis: true,
        grid: { show: true, color: '#e0e0e0' },
        labelFontSize: 11,
        valueFontSize: 10,
        yAxisFontSize: 9,
        labelColor: '#112233',
        valueColor: '#445566',
        yAxisColor: '#778899',
      },
    }),
  })

describe('the two chart implementations agree', () => {
  it('produces the bytes the Rust side is checked against', () => {
    const arena = encodeScene([CHART()], 200, 120, 1)
    const bytes = addon()
      .sceneBytes(
        arena.slots,
        arena.values.map(value => (typeof value === 'string' ? value : Buffer.from(value))),
      )
      .toString('hex')

    if (process.env['UPDATE_CHART_BYTES'] === '1') {
      writeFileSync(ASSET, `${bytes}\n`)
    }

    const committed = readFileSync(ASSET, 'utf8').trim()
    expect(bytes).toBe(committed)
  })

  // The control this file needs: bytes that differ when the chart differs.
  // Without it, a comparison against a constant would pass for a scene that
  // encoded to nothing.
  it('produces different bytes for a different chart', () => {
    const other = Chart({ type: 'bar', width: 200, height: 120, data: { labels: ['a'], datasets: [{ data: [1] }] } })
    const encode = (node: ReturnType<typeof Chart>) => {
      const arena = encodeScene([node], 200, 120, 1)
      return addon()
        .sceneBytes(
          arena.slots,
          arena.values.map(value => (typeof value === 'string' ? value : Buffer.from(value))),
        )
        .toString('hex')
    }
    expect(encode(other)).not.toBe(encode(CHART()))
  })
})
