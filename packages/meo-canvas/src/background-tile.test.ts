import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Box } from './node.js'
import { Root } from './root.js'
import type { BackgroundImage, BackgroundRepeat, BackgroundSize, Dimension, Length } from './index.js'

/**
 * Where a background picture's tiles land, against `background-tile.tsv`: rounded
 * on an axis that does not repeat, fractional on one that does. The Rust side is
 * `chrome_background_tile.rs`, reading the same table the same way.
 */
describe('background tiles', () => {
  const asset = (name: string) => fileURLToPath(new URL(`../../../crates/meo-canvas/tests/assets/${name}`, import.meta.url))
  const picture = readFileSync(asset('split-tile.png'))
  const [WIDTH, HEIGHT, AT] = [420, 220, 10]

  /** One length as the table spells it, CSS's. */
  const length = (text: string): Length => (text.endsWith('%') ? text : Number(text.replace('px', ''))) as Length
  const size = (text: string): BackgroundSize => {
    if (text === 'contain' || text === 'cover') return text
    const [width = 'auto', height = width] = text.split(' ')
    const axis = (value: string): Dimension | undefined => (value === 'auto' ? undefined : length(value))
    const [w, h] = [axis(width), axis(height)]
    return { ...(w === undefined ? {} : { width: w }), ...(h === undefined ? {} : { height: h }) }
  }
  const crossings = (n: number, value: (at: number) => number) => {
    const out: number[] = []
    for (let at = 0; at + 1 < n; at++) {
      const [here, next] = [value(at), value(at + 1)]
      if ((here - 0.5) * (next - 0.5) < 0) out.push(at + 0.5 + (0.5 - here) / (next - here) - AT)
    }
    return out
  }

  it('lands where Chrome puts them', async () => {
    const lines = readFileSync(asset('chrome/background-tile.tsv'), 'utf8')
      .split('\n')
      .filter(line => line.trim() !== '' && !line.startsWith('#'))
    expect(lines.length).toBeGreaterThan(0)
    for (const line of lines) {
      const field = line.split('\t')
      const text = (index: number) => field[index] as string
      const [x = '0px', y = x] = text(4).split(' ')
      const background: BackgroundImage = {
        src: { bytes: picture },
        size: size(text(3)),
        position: { x: length(x), y: length(y) },
        repeat: text(5) as BackgroundRepeat,
      }
      const canvas = await Root({
        width: WIDTH,
        height: HEIGHT,
        gpu: false,
        positionType: 'relative',
        backgroundColor: '#ffffff',
        children: Box({
          display: 'block',
          positionType: 'absolute',
          position: { left: AT, top: AT },
          width: Number(text(1)),
          height: Number(text(2)),
          backgroundColor: '#ffffff',
          backgroundImage: background,
        }),
      })
      const raw = Buffer.from(await canvas.toBuffer('raw'))
      canvas.release()
      const channel = (px: number, py: number, index: number) => (raw[(py * WIDTH + px) * 4 + index] as number) / 255
      const [row, column] = [Number(text(6)) + AT, Number(text(7)) + AT]
      const got = [
        crossings(WIDTH, px => 1 - channel(px, row, 1)),
        crossings(WIDTH, px => 1 - channel(px, row, 2)),
        crossings(HEIGHT, py => 1 - channel(column, py, 1)),
        crossings(HEIGHT, py => 1 - channel(column, py, 2)),
      ]
      const want = [8, 9, 10, 11].map(index => text(index).split(',').filter(Boolean).map(Number))
      got.forEach((ours, axis) => {
        const theirs = want[axis] as number[]
        expect(ours, `${text(0)} axis ${axis}`).toHaveLength(theirs.length)
        ours.forEach((edge, index) => expect(Math.abs(edge - (theirs[index] as number)), `${text(0)} axis ${axis}`).toBeLessThanOrEqual(0.2))
      })
    }
  })
})
