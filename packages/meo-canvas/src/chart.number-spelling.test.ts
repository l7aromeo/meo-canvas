import { createRequire } from 'node:module'

import { describe, expect, it } from 'vitest'

import { Chart, type BaseChartOptions } from './chart.js'
import { Box } from './node.js'
import { encodeScene } from './arena.js'

/**
 * Where the two surfaces deliberately spell a number differently: JavaScript goes
 * exponential at `1e21` and Rust's `Display` never does. Pinned rather than closed,
 * so closing it reds a row; each asserts this side's spelling and the absence of
 * the other's. The Rust half is `crates/meo-canvas/tests/chart_number_spelling.rs`.
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
 * Whether the scene carries `text` as a string of its own, matched with its length
 * prefix: `1000000000000000000000` is a substring of `10000000000000000000000`.
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
