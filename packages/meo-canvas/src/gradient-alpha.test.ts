import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Box, Path } from './node.js'
import { Root } from './root.js'
import type { Gradient } from './index.js'

/**
 * How a gradient mixes stops that are not opaque, against `gradient-alpha.tsv`:
 * premultiplied for a box, the root and a mask, as CSS mixes them, so red to
 * `transparent` stays red; straight for a path's fill or stroke, as SVG mixes them. The Rust side is
 * `chrome_gradient_alpha.rs`, which also reads every colour page depth.
 */
describe('gradient alpha', () => {
  /** Chrome's dither and a byte's rounding, as the Rust side allows. */
  const TOLERANCE = 2

  const table = fileURLToPath(new URL('../../../crates/meo-canvas/tests/assets/chrome/gradient-alpha.tsv', import.meta.url))

  /** The ramps, by the name a case has after its role prefixes. */
  const RAMPS: Record<string, readonly string[]> = {
    opaque: ['rgb(255, 0, 0)', 'rgb(0, 0, 255)'],
    'zero same': ['rgb(255, 0, 0)', 'rgba(255, 0, 0, 0)'],
    transparent: ['rgb(255, 0, 0)', 'transparent'],
    'near zero': ['rgb(255, 0, 0)', 'rgba(255, 0, 0, 0.01)'],
    'zero other': ['rgb(255, 0, 0)', 'rgba(0, 0, 255, 0)'],
    'zero first': ['rgba(255, 0, 0, 0)', 'rgb(0, 0, 255)'],
    'through transparent': ['rgb(255, 0, 0)', 'transparent', 'rgb(0, 0, 255)'],
    // Under a mask only the alpha is read, so its colours are this ramp's.
    partial: ['rgb(255, 0, 0)', 'rgba(0, 0, 255, 0.2)'],
  }

  /** The gradient a case names: its ramp along the geometry its name says. */
  const gradient = (name: string): Gradient => {
    const ramp = ['root ', 'mask ', 'svg ', 'stroke ', 'radial ', 'conic '].reduce(
      (rest, prefix) => (rest.startsWith(prefix) ? rest.slice(prefix.length) : rest),
      name,
    )
    const colors = RAMPS[ramp]
    if (colors === undefined) throw new Error(`${name} is not a case this test draws`)
    if (name.includes('radial')) return { type: 'radial', colors }
    if (name.includes('conic')) return { type: 'conic', colors }
    return { type: 'linear', direction: 'to-right', colors }
  }

  /** The case's page: a box's background, the root's, a masked red box, or a path's fill or stroke. */
  const scene = (name: string, width: number, height: number) => {
    const box = { display: 'block', width, height } as const
    if (name.startsWith('root')) return { gradient: gradient(name) }
    if (name.startsWith('mask')) return { children: Box({ ...box, backgroundColor: '#ff0000', mask: { gradient: gradient(name) } }) }
    if (name.startsWith('svg stroke')) {
      return { children: Path({ ...box, d: `M0 ${height / 2} H${width}`, fill: 'none', stroke: gradient(name), lineWidth: height }) }
    }
    if (name.startsWith('svg')) return { children: Path({ ...box, d: `M0 0 H${width} V${height} H0 Z`, fill: gradient(name) }) }
    return { children: Box({ ...box, gradient: gradient(name) }) }
  }

  it('adds no colour of its own at a transparent stop', async () => {
    const rows = readFileSync(table, 'utf8')
      .split('\n')
      .filter(line => line.trim() !== '' && !line.startsWith('#'))
      .map(line => line.split('\t'))
    expect(rows.length).toBeGreaterThan(0)
    const drawn = new Map<string, Uint8Array>()
    const wrong: string[] = []
    for (const field of rows) {
      const text = (index: number) => field[index] as string
      const name = JSON.parse(text(0)) as string
      const [width, height, x, y] = [1, 2, 4, 5].map(index => Number(text(index))) as [number, number, number, number]
      let pixels = drawn.get(name)
      if (pixels === undefined) {
        const canvas = await Root({
          width,
          height,
          gpu: false,
          backgroundColor: '#ffffff',
          ...scene(name, width, height),
        })
        pixels = await canvas.toBuffer('raw')
        canvas.release()
        drawn.set(name, pixels)
      }
      const at = (y * width + x) * 4
      const ours = [0, 1, 2].map(channel => pixels[at + channel] as number)
      const chrome = [6, 7, 8].map(index => Number(text(index)))
      if (ours.some((value, channel) => Math.abs(value - (chrome[channel] as number)) > TOLERANCE)) {
        wrong.push(`${name} ${text(3)}: ours ${ours.join(',')}, chrome ${chrome.join(',')}`)
      }
    }
    expect(wrong).toEqual([])
  })
})
