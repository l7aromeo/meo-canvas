import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Box } from './node.js'
import { Root } from './root.js'

/**
 * A `filter` or `backdropFilter` other than `'none'` contains absolute and
 * fixed descendants, as Chrome lays out `filter-containing-block.tsv`. The
 * Rust side is `chrome_filter_containing_block.rs`, reading the same table.
 */
describe('a filter as a containing block', () => {
  const PAGE = { width: 400, height: 300 }
  const table = readFileSync(fileURLToPath(new URL('../../../crates/meo-canvas/tests/assets/chrome/filter-containing-block.tsv', import.meta.url)), 'utf8')
  const rows = table
    .split('\n')
    .filter(line => line.trim() !== '' && !line.startsWith('#'))
    .map(line => {
      const [property, chain, position, left, top, width, height, ink] = line.split('\t')
      if (ink === undefined) throw new Error(`\`${line}\` does not have eight cells`)
      return {
        property: property as 'none' | 'filter' | 'backdrop-filter',
        chain: JSON.parse(chain as string) as string,
        position: position as 'absolute' | 'fixed',
        rect: [left, top, width, height].map(Number),
        drawn: ink === 'drawn',
      }
    })

  /** The child's red ink, more than half covered, as `[left, top, width, height]`. */
  const ink = (raw: Buffer) => {
    let [x0, y0, x1, y1] = [Infinity, Infinity, -1, -1]
    for (let y = 0; y < PAGE.height; y++) {
      for (let x = 0; x < PAGE.width; x++) {
        const at = (y * PAGE.width + x) * 4
        if ((raw[at] as number) >= 128 && (raw[at + 1] as number) < 128 && (raw[at + 2] as number) < 128) {
          ;[x0, y0, x1, y1] = [Math.min(x0, x), Math.min(y0, y), Math.max(x1, x), Math.max(y1, y)]
        }
      }
    }
    return x1 < 0 ? null : [x0, y0, x1 - x0 + 1, y1 - y0 + 1]
  }

  it('places and clips the child where Chrome does', async () => {
    expect(rows).toHaveLength(18)
    const wrong: string[] = []
    for (const row of rows) {
      for (const clip of [false, true]) {
        const effect = row.property === 'filter' ? { filter: row.chain } : row.property === 'backdrop-filter' ? { backdropFilter: row.chain } : {}
        const canvas = await Root({
          ...PAGE,
          gpu: false,
          display: 'block',
          backgroundColor: '#ffffff',
          children: [
            Box({ display: 'block', height: 50 }),
            Box({
              display: 'block',
              margin: { left: 30 },
              width: 100,
              height: 80,
              overflow: clip ? 'hidden' : 'visible',
              ...effect,
              children: Box({
                display: 'block',
                positionType: row.position,
                position: { left: 110, top: 10 },
                width: '50%',
                height: '25%',
                backgroundColor: '#ff0000',
              }),
            }),
          ],
        })
        const got = ink(Buffer.from(await canvas.toBuffer('raw')))
        canvas.release()
        const want = row.drawn || !clip ? row.rect : null
        if (JSON.stringify(got) !== JSON.stringify(want)) {
          wrong.push(`${row.property} ${JSON.stringify(row.chain)} ${row.position}, clip ${clip}: ${JSON.stringify(got)}, in Chrome ${JSON.stringify(want)}`)
        }
      }
    }
    expect(wrong).toEqual([])
  })
})
