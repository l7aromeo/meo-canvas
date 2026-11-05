import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { Box, Image, Root, type SceneNode, type Style } from './index.js'

/**
 * The box an image whose source fails takes, drawn through the addon: the failed rows
 * of `failed-image.tsv`, Chrome's `<img>` with no `alt` answering HTTP 522, read as the
 * placeholder's ink on a white page.
 */

// `fileURLToPath`, not `.pathname`: see `chart.agreement.test.ts`.
const TABLE = fileURLToPath(new URL('../../../crates/meo-canvas/tests/assets/chrome/failed-image.tsv', import.meta.url))

/** The page, square and as wide as the table's container. */
const PAGE = 300

/** The table's grid and absolute containers' height. */
const TALL = 200

/** The insets of the `absolute insets` rows. */
const INSET = 10

/** The status the table's failing source answered. */
const STATUS = 522

/** One cell: the container, and the image in it. */
function cell(container: string, size: Style): SceneNode {
  const url = { src: { url: 'https://origin.invalid/logo.png' } }
  switch (container) {
    case 'block':
      return Box({ width: PAGE, display: 'block', children: Image({ ...url, ...size }) })
    case 'flex column':
      return Box({ width: PAGE, flexDirection: 'column', alignItems: 'flex-start', children: Image({ ...url, ...size }) })
    case 'flex column stretch':
      return Box({ width: PAGE, flexDirection: 'column', children: Image({ ...url, ...size }) })
    case 'flex row':
      return Box({ width: PAGE, flexDirection: 'row', alignItems: 'flex-start', children: Image({ ...url, ...size }) })
    case 'grid':
      return Box({ width: PAGE, height: TALL, display: 'grid', gridTemplateColumns: [PAGE], children: Image({ ...url, ...size }) })
    case 'absolute':
    case 'absolute insets': {
      const insets = container === 'absolute insets' ? { position: { left: INSET, right: INSET } } : {}
      return Box({
        width: PAGE,
        height: TALL,
        display: 'block',
        positionType: 'relative',
        children: Image({ ...url, ...size, positionType: 'absolute', ...insets }),
      })
    }
    default:
      throw new Error(`a container this test does not build: ${container}`)
  }
}

/** The bounding box of every pixel that is not the page's white. */
async function inked(container: string, size: Style): Promise<[number, number]> {
  const canvas = await Root({ width: PAGE, height: PAGE, display: 'block', backgroundColor: '#ffffff', children: cell(container, size) })
  expect(canvas.warnings.map(warning => warning.failure)).toEqual(['status'])
  const raw = await canvas.toBuffer('raw')
  canvas.release()
  let [left, top, right, bottom] = [PAGE, PAGE, -1, -1]
  for (let y = 0; y < PAGE; y++) {
    for (let x = 0; x < PAGE; x++) {
      const at = (y * PAGE + x) * 4
      if (raw[at] !== 255 || raw[at + 1] !== 255 || raw[at + 2] !== 255) {
        left = Math.min(left, x)
        top = Math.min(top, y)
        right = Math.max(right, x)
        bottom = Math.max(bottom, y)
      }
    }
  }
  return right < 0 ? [0, 0] : [right - left + 1, bottom - top + 1]
}

describe('an image whose source fails', () => {
  it('draws its placeholder in the box Chrome gives a broken img with no alt', async () => {
    const real = globalThis.fetch
    globalThis.fetch = async () => new Response('origin timeout', { status: STATUS })
    try {
      const rows = readFileSync(TABLE, 'utf8')
        .split('\n')
        .filter(line => line.startsWith('failed\t'))
        .map(line => line.split('\t'))
      // Seven containers by four sizes: a table that lost its rows would pass.
      expect(rows).toHaveLength(28)
      const wrong: string[] = []
      for (const [, container, width, height, w, h] of rows) {
        const size: Style = { ...(width === '-' ? {} : { width: Number(width) }), ...(height === '-' ? {} : { height: Number(height) }) }
        const got = await inked(container ?? '', size)
        const want = [Math.round(Number(w)), Math.round(Number(h))]
        if (got[0] !== want[0] || got[1] !== want[1]) wrong.push(`${container} ${width}x${height}: ${got.join('x')}, Chrome ${want.join('x')}`)
      }
      expect(wrong).toEqual([])
    } finally {
      globalThis.fetch = real
    }
  })
})
