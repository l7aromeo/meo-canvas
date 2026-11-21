import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Box, Image } from './node.js'
import { Root } from './root.js'
import type { SceneNode } from './node.js'

/**
 * A `filter` covers the node's whole rendering, as Chrome paints
 * `filter-whole.tsv`. The Rust side, `chrome_filter_whole.rs`, walks every
 * case; this walks one per mechanism, so the npm surface is held to the same
 * rows rather than only to the crate's bytes.
 */
describe('a filter over the whole rendering', () => {
  const PAGE = 240
  const assets = new URL('../../../crates/meo-canvas/tests/assets/', import.meta.url)
  const table = readFileSync(fileURLToPath(new URL('chrome/filter-whole.tsv', assets)), 'utf8')
  const disc = readFileSync(fileURLToPath(new URL('filter-disc.png', assets)))

  const rows = table
    .split('\n')
    .filter(line => line.trim() !== '' && !line.startsWith('#'))
    .map(line => {
      const [name, x, y, r, g, b] = line.split('\t')
      if (b === undefined) throw new Error(`\`${line}\` is short`)
      return { name: name as string, at: [Number(x), Number(y)] as const, rgb: [Number(r), Number(g), Number(b)] }
    })

  const image = (style: object = {}) => Image({ src: { bytes: disc }, display: 'block', width: 80, height: 80, ...style })
  const block = (width: number, height: number, color: string, style: object = {}) => Box({ display: 'block', width, height, backgroundColor: color, ...style })
  const placed = (left: number, top: number, width: number, height: number, color: string) =>
    block(width, height, color, { positionType: 'absolute', position: { left, top } })
  const cell = (width: number, height: number, style: object, children: SceneNode | SceneNode[]) => Box({ display: 'block', width, height, ...style, children })

  /** One case per mechanism, built as `chrome_filter_whole.rs` builds it. */
  const cases: Record<string, SceneNode> = {
    'image shadow past its box': cell(160, 80, {}, image({ filter: 'drop-shadow(80px 0px 0px red)' })),
    'one shadow under both children': cell(120, 100, { positionType: 'relative', filter: 'drop-shadow(-30px -30px 0px red)' }, [
      placed(0, 0, 60, 40, 'rgb(0,160,0)'),
      placed(40, 40, 40, 40, 'rgb(0,0,255)'),
    ]),
    'shadow turned with its box': cell(
      200,
      200,
      {},
      cell(40, 40, { margin: 60, transform: { rotate: 90 }, filter: 'drop-shadow(30px 0px 0px red)' }, block(40, 40, 'rgb(0,0,255)')),
    ),
    'a transformed child inside the group': cell(
      240,
      200,
      {},
      cell(
        40,
        40,
        { margin: { top: 80 }, filter: 'drop-shadow(100px 0px 0px red)' },
        block(80, 20, 'rgb(0,0,255)', { transform: { translateX: 60, rotate: 90, scale: 2 } }),
      ),
    ),
    'the mask after the filter': cell(
      160,
      80,
      {},
      cell(160, 80, { mask: { path: 'M0 0H80V80H0Z' }, filter: 'drop-shadow(80px 0px 0px red)' }, block(80, 80, 'rgb(0,0,255)')),
    ),
    'the filter as a backdrop root': cell(
      80,
      80,
      { positionType: 'relative', backgroundColor: 'rgb(255,0,0)' },
      cell(80, 80, { positionType: 'absolute', position: { left: 0, top: 0 }, filter: 'brightness(1)' }, cell(80, 80, { backdropFilter: 'invert(1)' }, [])),
    ),
    'icon tint': cell(
      80,
      80,
      { overflow: 'hidden' },
      cell(80, 80, { positionType: 'relative', position: { left: -80 }, filter: 'drop-shadow(80px 0px 0px red)' }, image()),
    ),
  }

  it('paints each mechanism where Chrome does', async () => {
    const wrong: string[] = []
    for (const [name, scene] of Object.entries(cases)) {
      const sampled = rows.filter(row => row.name === name)
      expect(sampled.length).toBeGreaterThan(0)
      const canvas = await Root({ width: PAGE, height: PAGE, gpu: false, display: 'block', backgroundColor: '#ffffff', children: scene })
      const raw = Buffer.from(await canvas.toBuffer('raw'))
      canvas.release()
      for (const { at, rgb } of sampled) {
        const index = (at[1] * PAGE + at[0]) * 4
        const got = [raw[index], raw[index + 1], raw[index + 2]] as number[]
        if (got.some((channel, i) => Math.abs(channel - (rgb[i] as number)) > 3)) {
          wrong.push(`${name} at ${at.join(',')}: ${got.join(',')}, in Chrome ${rgb.join(',')}`)
        }
      }
    }
    expect(wrong).toEqual([])
  })
})
