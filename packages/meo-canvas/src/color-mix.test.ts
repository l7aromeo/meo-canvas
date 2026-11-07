import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { mixColor } from './animate.js'
import { parseColor } from './color.js'
import type { Rgba } from './animate.js'

/**
 * Two colours mixed partway through an animation, against `color-mix.tsv`:
 * Chrome mixes with the alpha multiplied in, so a transparent end adds no
 * colour. The Rust side is `chrome_color_mix.rs`, reading the same table.
 */
describe('colour mixing', () => {
  /** Each side rounding a half differently, as the Rust side allows. */
  const TOLERANCE = 1

  const table = fileURLToPath(new URL('../../../crates/meo-canvas/tests/assets/chrome/color-mix.tsv', import.meta.url))

  /** The colour drawn over an opaque page of `page` on every channel. */
  const over = (colour: Rgba, page: number): number[] => {
    const alpha = Math.min(1, Math.max(0, colour.a))
    return [colour.r, colour.g, colour.b].map(channel => Math.min(255, Math.max(0, channel)) * alpha + page * (1 - alpha))
  }

  it('mixes as Chrome does, alpha multiplied in', () => {
    const rows = readFileSync(table, 'utf8')
      .split('\n')
      .filter(line => line.trim() !== '' && !line.startsWith('#'))
      .map(line => line.split('\t'))
    expect(rows.length).toBeGreaterThan(0)
    const wrong: string[] = []
    for (const field of rows) {
      const text = (index: number) => field[index] as string
      const [from, to] = [parseColor(JSON.parse(text(1)) as string), parseColor(JSON.parse(text(2)) as string)]
      if (from === null || to === null) throw new Error(`unparseable colour in ${field.join(' | ')}`)
      const mixed = mixColor(from, to, Number(text(3)))
      const ours = [...over(mixed, 255), ...over(mixed, 0)]
      const chrome = [4, 5, 6, 7, 8, 9].map(index => Number(text(index)))
      if (ours.some((value, index) => Math.abs(value - (chrome[index] as number)) > TOLERANCE)) {
        wrong.push(`${text(0)} ${text(1)} -> ${text(2)} at ${text(3)}: ours ${ours.map(v => v.toFixed(1)).join(',')}, chrome ${chrome.join(',')}`)
      }
    }
    expect(wrong).toEqual([])
  })
})
