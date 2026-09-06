import { describe, expect, it } from 'vitest'

import { Box } from './node.js'
import { Root } from './root.js'
import type { Style } from './style.js'

/**
 * `borderStyle: 'none'` is the initial value, and it zeroes the used width.
 *
 * Through a real render rather than against `used_border` directly. A unit
 * test would supply the style itself and so could not show that the layout
 * pass receives it — and the layout half is the half no comparison of ink can
 * see, because a border that paints nothing while still reserving its width
 * moves the content and draws no evidence of having done so.
 */
describe('a border with no style', () => {
  const SIZE = 40

  /** Renders one box and reports its pixels and where its child landed. */
  const render = async (style: Style) => {
    const canvas = await Root({
      width: SIZE,
      height: SIZE,
      backgroundColor: '#ffffff',
      children: Box({
        width: SIZE,
        height: SIZE,
        borderColor: '#000000',
        ...style,
        children: Box({ width: 8, height: 8, backgroundColor: '#ff0000' }),
      }),
    })
    const raw = Buffer.from(await canvas.toBuffer('raw'))
    canvas.release()

    let ink = 0
    let child = -1
    for (let y = 0; y < SIZE; y += 1) {
      for (let x = 0; x < SIZE; x += 1) {
        const at = (y * SIZE + x) * 4
        const [r, g, b] = [raw[at] as number, raw[at + 1] as number, raw[at + 2] as number]
        if (r < 100 && g < 100 && b < 100) ink += 1
        if (child < 0 && r > 180 && g < 100 && b < 100) child = x
      }
    }
    return { raw, ink, child }
  }

  it('paints nothing and reserves nothing, exactly as a zero width does', async () => {
    const bare = await render({ border: 4 })
    const zero = await render({ border: 0 })

    // Byte-identical rather than equal in ink: a border that painted nothing
    // but still inset the content would match on the count and differ here.
    expect(bare.raw.equals(zero.raw)).toBe(true)
    expect(bare.ink).toBe(0)
    expect(bare.child).toBe(0)
  })

  it('agrees with a zero width at the corners too, not only on a flat edge', async () => {
    // B's caveat, tested rather than argued. The equivalence above is
    // structural — the gate is in `used_border`, so a `none` border reaches
    // the painter as four zero widths and every later stage sees the same
    // inputs a zero width produces. "By construction" is still a claim about
    // code, and a radius with uneven edges is where a join stops being the sum
    // of its parts, so it is the case that would break it if anything did.
    const corner: Style = { borderRadius: 12, border: { top: 6, right: 2, bottom: 6, left: 2 } }
    const bare = await render(corner)
    const zero = await render({ ...corner, border: 0 })

    expect(bare.raw.equals(zero.raw)).toBe(true)

    // And it is a real corner: naming `solid` on the same scene draws one.
    const solid = await render({ ...corner, borderStyle: 'solid' })
    expect(solid.ink).toBeGreaterThan(0)
    expect(solid.raw.equals(bare.raw)).toBe(false)
  })

  it('is what the width alone no longer decides — `solid` still draws', async () => {
    // The binding control. Without it the assertion above is satisfied by a
    // renderer that has stopped drawing borders entirely, which is a live
    // possibility because this change edits `used_border`, the one function
    // every border in the crate goes through.
    const solid = await render({ border: 4, borderStyle: 'solid' })
    const bare = await render({ border: 4 })

    expect(solid.ink).toBeGreaterThan(0)
    expect(solid.raw.equals(bare.raw)).toBe(false)
    // Reserved as well as painted: the child moves in by the border width.
    expect(solid.child).toBe(4)
  })
})
