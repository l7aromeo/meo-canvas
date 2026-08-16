import { interpolate, lerp, mapRange, mix } from '@/animate/interpolate.js'

describe('lerp', () => {
  it('returns the endpoints', () => {
    expect(lerp(10, 20, 0)).toBe(10)
    expect(lerp(10, 20, 1)).toBe(20)
  })

  it('blends between them', () => {
    expect(lerp(0, 100, 0.25)).toBe(25)
    expect(lerp(-50, 50, 0.5)).toBe(0)
  })

  it('extrapolates past the ends, which is what overshooting easings need', () => {
    // outBack returns more than 1; a clamped lerp would silently flatten the overshoot.
    expect(lerp(0, 100, 1.1)).toBeCloseTo(110)
    expect(lerp(0, 100, -0.1)).toBeCloseTo(-10)
  })
})

describe('mapRange', () => {
  it('rescales between ranges', () => {
    expect(mapRange(50, [0, 100], [0, 1])).toBe(0.5)
    expect(mapRange(0, [-1, 1], [0, 200])).toBe(100)
  })

  it('handles an inverted output range', () => {
    expect(mapRange(0.25, [0, 1], [100, 0])).toBe(75)
  })

  it('extrapolates by default and clamps on request', () => {
    expect(mapRange(150, [0, 100], [0, 1])).toBeCloseTo(1.5)
    expect(mapRange(150, [0, 100], [0, 1], { clamp: true })).toBe(1)
    expect(mapRange(-50, [0, 100], [0, 1], { clamp: true })).toBe(0)
  })

  it('returns the range start when the input range is empty rather than dividing by zero', () => {
    expect(mapRange(5, [3, 3], [10, 20])).toBe(10)
    expect(Number.isNaN(mapRange(5, [3, 3], [10, 20]))).toBe(false)
  })
})

describe('interpolate', () => {
  it('walks a keyframe track', () => {
    expect(interpolate(0, [0, 0.5, 1], [0, 100, 0])).toBe(0)
    expect(interpolate(0.25, [0, 0.5, 1], [0, 100, 0])).toBe(50)
    expect(interpolate(0.5, [0, 0.5, 1], [0, 100, 0])).toBe(100)
    expect(interpolate(0.75, [0, 0.5, 1], [0, 100, 0])).toBe(50)
  })

  it('holds outside the declared stops', () => {
    expect(interpolate(-1, [0, 1], [10, 20])).toBe(10)
    expect(interpolate(2, [0, 1], [10, 20])).toBe(20)
  })

  it('applies an easing within each segment', () => {
    const eased = interpolate(0.25, [0, 0.5, 1], [0, 100, 0], { ease: 'outCubic' })
    expect(eased).toBeGreaterThan(50)
  })

  it('interpolates colours when the values are colours', () => {
    expect(interpolate(0.5, [0, 1], ['#000000', '#ffffff'])).toBe('#808080')
  })

  it('accepts unevenly spaced stops', () => {
    expect(interpolate(0.9, [0, 0.8, 1], [0, 10, 20])).toBeCloseTo(15)
  })

  it('rejects mismatched stop and value counts', () => {
    expect(() => interpolate(0.5, [0, 1], [0, 1, 2])).toThrow(/same number/i)
  })

  it('rejects stops that are not ascending', () => {
    expect(() => interpolate(0.5, [0, 1, 0.5], [0, 1, 2])).toThrow(/ascending/i)
  })

  it('needs at least two stops', () => {
    expect(() => interpolate(0.5, [0], [0])).toThrow(/at least two/i)
  })
})

describe('mix', () => {
  it('blends numbers', () => {
    expect(mix(0, 10, 0.5)).toBe(5)
  })

  it('blends colours in any format the engine takes', () => {
    expect(mix('#000000', '#ffffff', 0.5)).toBe('#808080')
    expect(mix('red', 'blue', 0.5)).toMatch(/^#[0-9a-f]{6}$/)
    // An oklch just outside sRGB keeps its gamut through the blend, so the result is written as
    // `color(srgb …)` rather than clipped into hex.
    expect(mix('red', 'oklch(0.7 0.2 30)', 0.5)).toMatch(/^color\(srgb /)
  })

  it('blends arrays element-wise', () => {
    expect(mix([0, 10], [10, 20], 0.5)).toEqual([5, 15])
  })

  it('rejects a mismatch between the two ends', () => {
    expect(() => mix(0 as never, '#fff' as never, 0.5)).toThrow(/same type/i)
    expect(() => mix([0, 1] as never, [0] as never, 0.5)).toThrow(/same length/i)
  })
})
