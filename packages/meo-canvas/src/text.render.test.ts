import { describe, expect, it } from 'vitest'
import { fileURLToPath } from 'node:url'

import { Box, Root, Text } from './index.js'

/**
 * What text draws through the addon, where only a render reaches the arena boundary.
 * `ellipsis` is read against another render rather than a pinned picture: `true`
 * must draw exactly what the literal marker draws, and `false` what leaving it out
 * draws. A wrapped item's width, and the lines a box rounded to a whole pixel
 * draws, are read against Chrome's.
 */

/** The test font, so a line breaks at the same place on every machine. */
// `fileURLToPath`, not `.pathname`: see `chart.agreement.test.ts`.
const FONT = fileURLToPath(new URL('../../../crates/meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf', import.meta.url))

/** A page wide enough for the marker and narrow enough to truncate. */
const PAGE = { width: 120, height: 40 }

/**
 * Renders one truncated line and returns its bytes, throwing rather than skipping
 * without the addon, as `chart.render.test.ts` does.
 */
async function ink(props: { readonly ellipsis?: boolean | string }): Promise<Buffer> {
  try {
    const canvas = await Root({
      width: PAGE.width,
      height: PAGE.height,
      backgroundColor: '#ffffff',
      fonts: [{ family: 'Fixture', paths: [FONT] }],
      children: Text('Flower of Paradise Lost', {
        width: 80,
        maxLines: 1,
        fontSize: 16,
        fontFamily: 'Fixture',
        color: '#000000',
        ...props,
      }),
    })
    return await canvas.toBuffer('raw')
  } catch (cause) {
    throw new Error('the addon is not built; run `just addon`. These are the only ellipsis checks that reach the renderer.', { cause })
  }
}

/** How many pixels of the page carry ink, which is what a marker adds. */
function dark(bytes: Buffer): number {
  let count = 0
  for (let at = 0; at < bytes.length; at += 4) if ((bytes[at] as number) < 128) count += 1
  return count
}

describe('what each spelling of `ellipsis` draws', () => {
  it('draws the default marker for `true`, byte for byte with the literal one', async () => {
    const written = await ink({ ellipsis: '…' })
    const asked = await ink({ ellipsis: true })
    expect(asked.equals(written)).toBe(true)
  })

  it('draws no marker for `false`, byte for byte with leaving it out', async () => {
    const absent = await ink({})
    const off = await ink({ ellipsis: false })
    const empty = await ink({ ellipsis: '' })
    expect(off.equals(absent)).toBe(true)
    expect(empty.equals(absent)).toBe(true)
  })

  it('draws something for the marker that it does not draw without one', async () => {
    // The control the two agreements need. A renderer that ignored `ellipsis`
    // entirely would satisfy both of them: every render would equal every
    // other. This is what says the marker reaches the page at all.
    const absent = await ink({})
    const asked = await ink({ ellipsis: true })
    expect(dark(asked)).toBeGreaterThan(dark(absent))
  })

  it('hands back a Node Buffer', async () => {
    // The addon returns a Neon `JsBuffer` and always has; the declaration used
    // to say `Uint8Array`, which was a false statement about the value. This
    // reads the value rather than the type, so a change to either is caught.
    const bytes = await ink({})
    expect(Buffer.isBuffer(bytes)).toBe(true)
  })
})

describe('a wrapping text item in a flex column', () => {
  // `chrome_flex_column_text.rs` pins every container against Chrome; this is the
  // one row proving the npm surface reaches the same width. Chrome 153: 30, 0, 150, 48.
  // The box is the background's exact colour, so black glyphs past it never count.
  it('is as wide as the space on offer, not its widest line', async () => {
    const canvas = await Root({
      width: 300,
      height: 200,
      display: 'block',
      backgroundColor: '#ffffff',
      fonts: [{ family: 'Fixture', paths: [FONT] }],
      children: Box({
        width: 200,
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'flex-start',
        children: Text('one two three four five six seven eight', {
          fontFamily: 'Fixture',
          fontSize: 20,
          lineHeight: 1.2,
          margin: { top: 0, right: 20, bottom: 0, left: 30 },
          color: '#000000',
          backgroundColor: '#cc0000',
        }),
      }),
    })
    const raw = canvas.toBufferSync('raw')
    canvas.release()
    let [left, top, right, bottom] = [Infinity, Infinity, -1, -1]
    for (let y = 0; y < 200; y += 1) {
      for (let x = 0; x < 300; x += 1) {
        const at = (y * 300 + x) * 4
        if (raw[at] !== 0xcc || raw[at + 1] !== 0 || raw[at + 2] !== 0) continue
        ;[left, top, right, bottom] = [Math.min(left, x), Math.min(top, y), Math.max(right, x), Math.max(bottom, y)]
      }
    }
    expect([left, top, right - left + 1, bottom - top + 1]).toEqual([30, 0, 150, 48])
  })
})

describe('a text box rounded to a whole pixel', () => {
  // `chrome_wrap_width.rs` pins the rows against Chrome; this is the one proving the
  // npm surface draws the same. Chrome 153: 137.2 wide is two lines, 48 tall.
  it('breaks where its layout broke it, not where the rounded box would', async () => {
    const canvas = await Root({
      width: 300,
      height: 120,
      display: 'block',
      backgroundColor: '#ffffff',
      fonts: [{ family: 'Fixture', paths: [FONT] }],
      children: Box({
        display: 'block',
        margin: { left: 30 },
        children: Text('one two three four five six seven eight', {
          width: 137.2,
          fontFamily: 'Fixture',
          fontSize: 20,
          lineHeight: 1.2,
          color: '#000000',
          backgroundColor: '#cc0000',
        }),
      }),
    })
    const raw = canvas.toBufferSync('raw')
    canvas.release()
    // The box from its background alone and the lines from the glyphs' own ink,
    // one band of inked rows per line, so a line drawn outside the box is a line.
    let [top, bottom] = [Infinity, -1]
    const inked: boolean[] = []
    for (let y = 0; y < 120; y += 1) {
      let row = false
      for (let x = 0; x < 300; x += 1) {
        const at = (y * 300 + x) * 4
        const [r, g, b] = [raw[at] ?? 0, raw[at + 1] ?? 0, raw[at + 2] ?? 0]
        if (r === 0xcc && g === 0 && b === 0) [top, bottom] = [Math.min(top, y), Math.max(bottom, y)]
        row ||= r < 100 && g < 100 && b < 100
      }
      inked.push(row)
    }
    const lines = inked.filter((row, y) => row && !(inked[y - 1] ?? false)).length
    expect([bottom - top + 1, lines]).toEqual([48, 2])
  })
})
