import { createRequire } from 'node:module'

import { describe, expect, it } from 'vitest'

import { Box, Root } from './index.js'

/**
 * The colour half of the addon: `parseColor` exports `meo-canvas-core`'s own reading,
 * so a caller asking what a string means gets the answer the renderer acts on.
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
  // The measurement the design rests on: `parseColor` is asked what a string means
  // and the renderer is asked to draw it, so the one parser is shown to be the
  // renderer's.
  it.each([
    ['#3366cc', [51, 102, 204]],
    ['rgb(1 2 3)', [1, 2, 3]],
    ['hsl(0 100% 50%)', [255, 0, 0]],
    ['rebeccapurple', [102, 51, 153]],
    // Chrome's bytes for both, from `color-strings.tsv`.
    ['hsl(30 60 40)', [163, 102, 41]],
    ['lab(67.5345% -8.6911 -41.6019)', [98, 172, 239]],
  ])('agrees with the renderer about %s', async (css, expected) => {
    const parsed = addon().parseColor(css)
    expect(parsed).not.toBeNull()
    expect([Math.round(parsed!.r), Math.round(parsed!.g), Math.round(parsed!.b)]).toEqual(expected)
    expect(await painted(css)).toEqual(expected)
  })

  // `color()` has no legacy form, so a parser that knows only CSS 3 refuses it.
  it('reads color(srgb ...)', async () => {
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

  // Alpha is the channel that does not scale: `r`, `g` and `b` are 0-255 and alpha
  // 0-1. Scaling all four alike would report an opaque colour as `a: 255`, which a
  // caller comparing against `1` would read as transparent.
  it('keeps alpha in 0-1 while the other three are in 0-255', () => {
    expect(addon().parseColor('#000000')!.a).toBe(1)
    expect(addon().parseColor('rgba(0, 0, 0, 0.5)')!.a).toBeCloseTo(0.5, 6)
    expect(addon().parseColor('#ffffff')!.r).toBe(255)
  })

  // A wide-gamut colour reads back outside sRGB, unclamped for animation, and
  // paints as the clamp Chrome paints; a space CSS does not name is refused.
  it('converts a wide-gamut space and refuses an unknown one', async () => {
    const parsed = addon().parseColor('color(display-p3 1 0 0)')
    expect(parsed?.r).toBeGreaterThan(255)
    expect(await painted('color(display-p3 1 0 0)')).toEqual([255, 0, 0])
    expect(addon().parseColor('color(bananas 1 0 0)')).toBeNull()
    expect(addon().isColor('color(bananas 1 0 0)')).toBe(false)
  })

  // The reader answers black for it; CSS means the node's text colour, which a
  // string parsed alone has none of.
  it('refuses currentColor rather than reading it as black', () => {
    for (const css of ['currentColor', ' CURRENTCOLOR ', 'color-mix(in srgb, currentColor 50%, white)']) {
      expect(addon().parseColor(css)).toBeNull()
      expect(addon().isColor(css)).toBe(false)
    }
  })

  // Two functions that can disagree about one string are a defect waiting for
  // the first caller who uses both, so `isColor` is defined as the parse
  // succeeding rather than as its own check.
  it('answers isColor exactly when parseColor answers', () => {
    for (const css of ['#3366cc', 'rebeccapurple', 'color(srgb 1 0 0)', 'color(display-p3 1 0 0)', 'color(bananas 1 0 0)', 'not-a-colour', '']) {
      expect(addon().isColor(css)).toBe(addon().parseColor(css) !== null)
    }
  })
})
