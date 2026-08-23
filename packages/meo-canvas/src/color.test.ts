import { createRequire } from 'node:module'

import { describe, expect, it } from 'vitest'

import { Box, Root } from './index.js'

/**
 * The colour half of the addon.
 *
 * **The point of this file is that there is only one parser.** Every colour
 * this package accepts is read by `meo-canvas-core`, and `parseColor` exports
 * that same reading rather than a second one — so a caller who asks what a
 * string means gets the answer the renderer will act on.
 */
interface ColorAddon {
  parseColor(css: string): { r: number; g: number; b: number; a: number } | null
  isColor(css: string): boolean
}

function addon(): ColorAddon {
  try {
    return createRequire(import.meta.url)('../meo-canvas.node') as ColorAddon
  } catch (cause) {
    throw new Error('the addon is not built; run `just addon`. This file is the only check that the two surfaces read one parser.', { cause })
  }
}

/** What the renderer actually paints for a colour string, as `r, g, b`. */
async function painted(css: string): Promise<[number, number, number]> {
  const canvas = await Root({
    width: 4,
    height: 4,
    backgroundColor: '#ffffff',
    children: Box({ width: 4, height: 4, backgroundColor: css }),
  })
  const raw = await canvas.toBuffer('raw')
  return [raw[0] as number, raw[1] as number, raw[2] as number]
}

describe('one parser serves both surfaces', () => {
  // **This is the measurement the design rests on.** The alternative was a
  // colour parser in TypeScript; the argument against it was that a second
  // implementation drifts. That argument is only worth anything if the one
  // implementation is demonstrably the one the renderer uses — so this asks
  // `parseColor` what a string means and then asks the renderer to draw it.
  it.each([
    ['#3366cc', [51, 102, 204]],
    ['rgb(1 2 3)', [1, 2, 3]],
    ['hsl(0 100% 50%)', [255, 0, 0]],
    ['rebeccapurple', [102, 51, 153]],
  ])('agrees with the renderer about %s', async (css, expected) => {
    const parsed = addon().parseColor(css)
    expect(parsed).not.toBeNull()
    expect([Math.round(parsed!.r), Math.round(parsed!.g), Math.round(parsed!.b)]).toEqual(expected)
    expect(await painted(css)).toEqual(expected)
  })

  // **The case a second parser would have got wrong.** `csscolorparser` has no
  // `color()` branch at all — 0.8.3 is its newest release — so this string is
  // read by the pre-pass rather than by the library, and it is the only CSS
  // syntax that can name a colour outside the gamut.
  it('reads color(srgb ...), which the library underneath cannot', async () => {
    const parsed = addon().parseColor('color(srgb 0.2 0.4 0.6)')
    expect(parsed).not.toBeNull()
    expect([Math.round(parsed!.r), Math.round(parsed!.g), Math.round(parsed!.b)]).toEqual([51, 102, 153])
    expect(await painted('color(srgb 0.2 0.4 0.6)')).toEqual([51, 102, 153])
  })

  // The parse is unclamped and the paint is not, which is the whole reason
  // `parse_channels` and `parse_color` are two functions. If they ever agreed
  // everywhere, one of them would be pointless.
  it('hands back an out-of-gamut colour unclamped, and paints it clamped', async () => {
    const parsed = addon().parseColor('color(srgb 1.25 -0.1 0.5)')
    expect(parsed!.r).toBeGreaterThan(255)
    expect(parsed!.g).toBeLessThan(0)
    // The renderer stores four bytes, so what reaches the page is clamped.
    const [r, g] = await painted('color(srgb 1.25 -0.1 0.5)')
    expect(r).toBe(255)
    expect(g).toBe(0)
  })

  // **Alpha is the channel that does not scale.** `r`, `g` and `b` come back
  // in 0-255 and alpha in 0-1, as v1 spells them. A parse scaling all four
  // alike would report an opaque colour as `a: 255`, and every caller
  // comparing against `1` would read that as transparent — a wrong value that
  // looks legitimate rather than one that fails.
  it('keeps alpha in 0-1 while the other three are in 0-255', () => {
    expect(addon().parseColor('#000000')!.a).toBe(1)
    expect(addon().parseColor('rgba(0, 0, 0, 0.5)')!.a).toBeCloseTo(0.5, 6)
    expect(addon().parseColor('#ffffff')!.r).toBe(255)
  })

  // Refused by name rather than as an unparseable string: reading `display-p3`
  // numbers as sRGB would draw a wrong colour in silence, and a caller can act
  // on "this space is unsupported" where they cannot act on "bad syntax".
  it('refuses a colour space it does not support, rather than guessing', () => {
    expect(addon().parseColor('color(display-p3 1 0 0)')).toBeNull()
    expect(addon().isColor('color(display-p3 1 0 0)')).toBe(false)
    expect(addon().isColor('color(srgb 1 0 0)')).toBe(true)
  })

  // Two functions that can disagree about one string are a defect waiting for
  // the first caller who uses both, so `isColor` is defined as the parse
  // succeeding rather than as its own check.
  it('answers isColor exactly when parseColor answers', () => {
    for (const css of ['#3366cc', 'rebeccapurple', 'color(srgb 1 0 0)', 'color(display-p3 1 0 0)', 'not-a-colour', '']) {
      expect(addon().isColor(css)).toBe(addon().parseColor(css) !== null)
    }
  })
})
