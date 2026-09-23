import { describe, expect, it } from 'vitest'
import { fileURLToPath } from 'node:url'

import { Root, Text } from './index.js'

/**
 * What `ellipsis` actually draws: only a render reaches the arena boundary, and each
 * case is read against another render rather than a pinned picture. `true` must draw
 * exactly what the literal marker draws, and `false` what leaving it out draws.
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
