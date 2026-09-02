// The four colour functions as one API, rather than four that each work.
//
// `parseColor` exists so the other three are reachable: `mixColor` takes an
// `Rgba`, `interpolate` and `track` blend one, and `formatColor` writes one
// back. Every test that checks them separately passes with no way in, which is
// the state this file was written to end -- so each case here starts from a
// string and ends at a string, the way a caller does.

import { describe, expect, it } from 'vitest'

import { formatColor, mixColor } from './animate.js'
import { isColor, parseColor } from './color.js'

describe('a colour makes the round trip', () => {
  it('parses, mixes and formats without leaving the exported surface', () => {
    const from = parseColor('#f2aa4c')
    const to = parseColor('#2850dc')
    expect(from).not.toBeNull()
    expect(to).not.toBeNull()
    if (from === null || to === null) return

    // Halfway is the one point where a mix cannot be right by accident: it is
    // neither endpoint, and each channel has to move by its own amount.
    expect(formatColor(mixColor(from, to, 0))).toBe('#f2aa4c')
    expect(formatColor(mixColor(from, to, 1))).toBe('#2850dc')
    expect(formatColor(mixColor(from, to, 0.5))).toBe('#8d7d94')
  })

  it('reads every spelling the renderer reads', () => {
    // One parser is the whole reason this goes through the addon, so the check
    // is that three spellings of one colour give one answer.
    for (const css of ['#ff0000', 'rgb(255, 0, 0)', 'hsl(0 100% 50%)', 'red']) {
      const parsed = parseColor(css)
      expect(parsed, css).not.toBeNull()
      expect([parsed?.r, parsed?.g, parsed?.b], css).toEqual([255, 0, 0])
    }
  })

  it('keeps a colour outside the gamut outside it', () => {
    // The reason the channels are not clamped at the parse: a mix needs room to
    // overshoot, and clamping here would flatten it before any mix saw it.
    const wide = parseColor('color(srgb 1.25 0 0)')
    expect(wide).not.toBeNull()
    expect(wide?.r).toBeGreaterThan(255)
    expect(wide === null ? '' : formatColor(wide)).toContain('color(srgb')
  })

  it('answers null for something that is not a colour', () => {
    expect(parseColor('not-a-colour')).toBeNull()
    expect(isColor('not-a-colour')).toBe(false)
    expect(isColor('#f2aa4c')).toBe(true)
  })

  it('agrees with itself about what a colour is', () => {
    // A validator written beside a parser drifts from it. These are one parser,
    // and this is what says so.
    for (const css of ['#f2aa4c', 'rebeccapurple', 'rgba(1,2,3,0.5)', 'nonsense', '']) {
      expect(isColor(css), css).toBe(parseColor(css) !== null)
    }
  })
})
