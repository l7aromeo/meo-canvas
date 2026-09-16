import { createRequire } from 'node:module'

import { describe, expect, it } from 'vitest'

import { Chart, type BaseChartOptions } from './chart.js'
import { Box } from './node.js'
import { encodeScene } from './arena.js'

/**
 * Where the two chart surfaces deliberately spell a number differently.
 *
 * # The divergence
 *
 * A chart label is a number turned into text, and the two languages part
 * company at the top of the range: JavaScript switches to exponential notation
 * at `1e21` and Rust's `Display` never does. So a value of `1e21` is drawn as
 * `1e+21` here and as `1000000000000000000000` on the Rust surface, and every
 * other chart check is silent about it — `chart.differential.test.ts` stops
 * short of the threshold on purpose, and the pinned agreement charts carry
 * whole numbers far below it.
 *
 * **It is left open rather than fixed.** Closing it means implementing one
 * language's number formatting in the other from scratch, with no helper on
 * either side to reach for. That is more commitment than a label at `1e21`
 * earns.
 *
 * # Why it is pinned rather than written down
 *
 * A deviation recorded only in prose is indistinguishable from a defect, and
 * nothing tells the next reader whether it still holds. Pinned, it is
 * self-reporting: **the day somebody closes the gap, these tests fail, and that
 * failure is the notification.**
 *
 * Each assertion pins **the spelling on this side and the absence of the
 * other's**, so a change to either surface reddens a row and the message says
 * which one moved. A pin reading only "the two differ" would pass for two
 * surfaces that had both become wrong.
 *
 * # Two paths, two thresholds
 *
 * The y-axis divisions are rounded to two decimals before they are spelled and
 * the value labels are not, which moves the boundary by a decade: rounding
 * `1e21` yields `999999999999999900000`, below the exponential threshold, so
 * the axis agrees there and parts at `1e22`. The two controls are what make
 * that a measurement rather than a story — one path agreeing at a threshold
 * where the other does not is the evidence that these are two paths and not one
 * condition described twice.
 *
 * The Rust half of this pin is `crates/meo-canvas/tests/chart_number_spelling.rs`.
 */
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

/** A chart's encoded bytes, for one value and one label switched on. */
function encoded(value: number, options: BaseChartOptions): Buffer {
  const chart = Chart({ type: 'bar', data: { labels: ['a'], datasets: [{ data: [value] }] }, options })
  const arena = encodeScene([Box({ children: chart })], 200, 120, false, 1)
  const values = arena.values.map(entry => (typeof entry === 'string' ? entry : Buffer.from(entry)))
  return addon().sceneBytes(arena.slots, values)
}

/** The value drawn against the bar, which is written as the caller gave it. */
const valueLabel = (value: number) => encoded(value, { showValues: true })

/** The y-axis scale, whose divisions are rounded to two decimals first. */
const yAxisLabel = (value: number) => encoded(value, { showYAxis: true })

/**
 * Whether the scene carries `text` as a string of its own.
 *
 * Matched with its length prefix rather than as a loose substring, because
 * `1000000000000000000000` is a substring of `10000000000000000000000` and a
 * bare search would have one threshold's expectation satisfied by the other's
 * output.
 */
function spells(bytes: Buffer, text: string): boolean {
  const length = Buffer.alloc(4)
  length.writeUInt32LE(Buffer.byteLength(text))
  return bytes.includes(Buffer.concat([length, Buffer.from(text, 'utf8')]))
}

describe('a number at the top of the range is spelled differently on each surface', () => {
  it('writes a value label at 1e21 in exponential notation where the Rust surface writes it in full', () => {
    const bytes = valueLabel(1e21)
    expect(spells(bytes, '1e+21'), 'this surface no longer writes 1e21 in exponential notation').toBe(true)
    expect(
      spells(bytes, '1000000000000000000000'),
      'this surface now spells 1e21 the way the Rust one does; if the gap has been closed, delete this file',
    ).toBe(false)
  })

  it('writes the top y-axis division at 1e22 in exponential notation where the Rust surface writes it in full', () => {
    const bytes = yAxisLabel(1e22)
    expect(spells(bytes, '1e+22'), 'this surface no longer writes the top y-axis division at 1e22 in exponential notation').toBe(true)
    expect(spells(bytes, '10000000000000000000000'), 'this surface now spells the top y-axis division the way the Rust one does').toBe(false)
  })

  it('agrees with the Rust surface on a y-axis division at 1e21, a decade below where this path parts', () => {
    expect(
      spells(yAxisLabel(1e21), '999999999999999900000'),
      'the y-axis division at 1e21 no longer rounds below the exponential threshold, so the two paths no longer part company a decade apart',
    ).toBe(true)
  })

  it('agrees with the Rust surface on a value label at 1e20, a decade below the threshold', () => {
    expect(spells(valueLabel(1e20), '100000000000000000000'), 'a value a decade below the threshold is no longer written in full').toBe(true)
  })
})
