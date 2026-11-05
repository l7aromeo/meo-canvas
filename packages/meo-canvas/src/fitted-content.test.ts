import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Box, Image, Path } from './node.js'
import { Root } from './root.js'

/**
 * Content fitted to a box layout left fractional, as Chrome fits it: a picture
 * in the box as laid out and then snapped, a path's `viewBox` into the content
 * box before rounding. The Rust side is `fitted_content.rs` and
 * `chrome_view_box.rs`, which read the same `view-box.tsv`.
 */
describe('content fitted to a fractional box', () => {
  /** An 840x400 checkerboard of 60x50 cells: a fractional shift moves edges everywhere. */
  const art = async () => {
    const rows = Array.from({ length: 8 }, (_, row) =>
      Box({
        flexDirection: 'row',
        width: 840,
        height: 50,
        children: Array.from({ length: 14 }, (_, cell) => Box({ width: 60, height: 50, backgroundColor: (cell + row) % 2 === 0 ? '#3a6ec8' : '#e8b040' })),
      }),
    )
    const canvas = await Root({ width: 840, height: 400, flexDirection: 'column', children: rows })
    const png = Buffer.from(await canvas.toBuffer('png'))
    canvas.release()
    return png
  }

  it('draws cover as fill where the picture shares the box aspect', async () => {
    const bytes = await art()
    for (const scale of [1, 2.5]) {
      const drawn = async (objectFit: 'fill' | 'cover') => {
        const canvas = await Root({
          width: 370,
          height: 200,
          scale,
          gpu: false,
          display: 'block',
          backgroundColor: '#ffffff',
          children: Image({ src: { bytes }, width: 350, height: 350 / 2.1, objectFit }),
        })
        const raw = Buffer.from(await canvas.toBuffer('raw'))
        canvas.release()
        return raw
      }
      expect((await drawn('cover')).equals(await drawn('fill'))).toBe(true)
    }
  })

  it('maps a viewBox where Chrome maps it', async () => {
    const table = readFileSync(fileURLToPath(new URL('../../../crates/meo-canvas/tests/assets/chrome/view-box.tsv', import.meta.url)), 'utf8')
    const rows = table
      .split('\n')
      .filter(line => line.trim() !== '' && !line.startsWith('#'))
      .map(line => {
        const field = line.split('\t').map(Number)
        if (field.length !== 10 || field.some(Number.isNaN)) throw new Error(`\`${line}\` is not ten numbers`)
        const at = (index: number) => field[index] as number
        return { left: at(0), top: at(1), width: at(2), height: at(3), padding: at(4), border: at(5), a: at(6), d: at(7), e: at(8), f: at(9) }
      })
    expect(rows.some(row => row.height % 1 !== 0)).toBe(true)
    expect(rows.some(row => row.padding > 0 && row.border > 0)).toBe(true)

    // A bar reaching ten times the space's height down, so a scale off by a
    // sixth of a percent moves its far edge by pixels.
    const [width, height] = [400, 2100]
    const crossings = (n: number, coverage: (at: number) => number) => {
      const out: number[] = []
      for (let at = 0; at + 1 < n; at++) {
        const [here, next] = [coverage(at), coverage(at + 1)]
        if ((here - 0.5) * (next - 0.5) < 0) out.push(at + 0.5 + (0.5 - here) / (next - here))
      }
      return out
    }
    for (const { left, top, width: w, height: h, padding, border, a, d, e, f } of rows) {
      const canvas = await Root({
        width,
        height,
        gpu: false,
        positionType: 'relative',
        backgroundColor: '#ffffff',
        children: Path({
          d: 'M10 10 H90 V1000 H10 Z',
          viewBox: [0, 0, 100, 100],
          fill: '#ff0000',
          positionType: 'absolute',
          position: { left, top },
          width: w,
          height: h,
          padding,
          border,
          borderStyle: 'solid',
          borderColor: '#ffffff',
        }),
      })
      const raw = Buffer.from(await canvas.toBuffer('raw'))
      canvas.release()
      const coverage = (x: number, y: number) => 1 - (raw[(y * width + x) * 4 + 1] as number) / 255
      const want = [10 * a + e, 90 * a + e, 10 * d + f, 1000 * d + f]
      const [x0, x1, y0, y1] = want as [number, number, number, number]
      const across = crossings(width, x => coverage(x, Math.floor((y0 + y1) / 2)))
      const down = crossings(height, y => coverage(Math.floor((x0 + x1) / 2), y))
      const got = [...across, ...down]
      expect(got).toHaveLength(4)
      got.forEach((edge, index) => expect(Math.abs(edge - (want[index] as number))).toBeLessThan(0.25))
    }
  })
})
