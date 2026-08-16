import { cubicBezier, easings, resolveEasing, steps } from '@/animate/easing.js'

const NAMES = Object.keys(easings) as (keyof typeof easings)[]

describe('easings', () => {
  it('offers the standard catalogue', () => {
    // The families every animation library ships, in the in/out/inOut spellings people expect.
    for (const family of ['Quad', 'Cubic', 'Quart', 'Quint', 'Sine', 'Expo', 'Circ', 'Back', 'Elastic', 'Bounce']) {
      for (const direction of ['in', 'out', 'inOut']) {
        expect(easings).toHaveProperty(`${direction}${family}`)
      }
    }
    expect(easings).toHaveProperty('linear')
  })

  it.each(NAMES)('%s is pinned at both ends', name => {
    // An easing that does not start at 0 and end at 1 makes a track jump on its first and last page.
    expect(easings[name](0)).toBeCloseTo(0, 5)
    expect(easings[name](1)).toBeCloseTo(1, 5)
  })

  it.each(NAMES)('%s stays finite across the range', name => {
    for (let t = 0; t <= 1.0001; t += 0.05) {
      expect(Number.isFinite(easings[name](t))).toBe(true)
    }
  })

  it.each(NAMES.filter(n => !/Back|Elastic/.test(n)))('%s stays within 0..1', name => {
    // Back and Elastic overshoot by design; nothing else should.
    for (let t = 0; t <= 1.0001; t += 0.05) {
      expect(easings[name](t)).toBeGreaterThanOrEqual(-1e-6)
      expect(easings[name](t)).toBeLessThanOrEqual(1 + 1e-6)
    }
  })

  it('overshoots deliberately for back and elastic', () => {
    const backMax = Math.max(...Array.from({ length: 101 }, (_, i) => easings.outBack(i / 100)))
    expect(backMax).toBeGreaterThan(1)
  })

  it('eases rather than running linearly', () => {
    // outCubic is ahead of linear at the midpoint; inCubic is behind it.
    expect(easings.outCubic(0.5)).toBeGreaterThan(0.5)
    expect(easings.inCubic(0.5)).toBeLessThan(0.5)
  })

  it('rises monotonically for the non-overshooting curves', () => {
    for (const name of NAMES.filter(n => !/Back|Elastic|Bounce/.test(n))) {
      const values = Array.from({ length: 21 }, (_, i) => easings[name](i / 20))
      expect(values).toEqual([...values].sort((a, b) => a - b))
    }
  })

  it('clamps input outside the unit range', () => {
    expect(easings.outCubic(-1)).toBeCloseTo(0, 5)
    expect(easings.outCubic(2)).toBeCloseTo(1, 5)
  })
})

describe('cubicBezier', () => {
  it('matches linear when the control points are on the diagonal', () => {
    const linear = cubicBezier(0, 0, 1, 1)
    for (const t of [0, 0.25, 0.5, 0.75, 1]) {
      expect(linear(t)).toBeCloseTo(t, 3)
    }
  })

  it('reproduces the CSS ease-in-out curve', () => {
    const easeInOut = cubicBezier(0.42, 0, 0.58, 1)
    expect(easeInOut(0)).toBeCloseTo(0, 5)
    expect(easeInOut(0.5)).toBeCloseTo(0.5, 2)
    expect(easeInOut(1)).toBeCloseTo(1, 5)
    expect(easeInOut(0.25)).toBeLessThan(0.25)
  })

  it('solves a steep curve accurately', () => {
    // A near-vertical start is where a naive solver loses precision.
    const steep = cubicBezier(0.9, 0, 0.1, 1)
    expect(steep(0.5)).toBeCloseTo(0.5, 2)
    for (let t = 0; t <= 1; t += 0.1) {
      expect(steep(t)).toBeGreaterThanOrEqual(-1e-6)
      expect(steep(t)).toBeLessThanOrEqual(1 + 1e-6)
    }
  })
})

describe('steps', () => {
  it('quantises into n jumps', () => {
    const four = steps(4)
    expect(four(0)).toBe(0)
    expect(four(0.24)).toBeCloseTo(0)
    expect(four(0.26)).toBeCloseTo(0.25)
    expect(four(1)).toBe(1)
  })

  it('rejects a step count below one', () => {
    expect(() => steps(0)).toThrow(/at least 1/i)
  })
})

describe('resolveEasing', () => {
  it('accepts a name', () => {
    expect(resolveEasing('outCubic')(0.5)).toBeCloseTo(easings.outCubic(0.5), 10)
  })

  it('accepts a function', () => {
    const custom = (t: number) => t * t
    expect(resolveEasing(custom)(0.5)).toBe(0.25)
  })

  it('defaults to linear when nothing is given', () => {
    expect(resolveEasing(undefined)(0.3)).toBeCloseTo(0.3, 10)
  })

  it('names the mistake when the easing does not exist', () => {
    expect(() => resolveEasing('outCubicc' as never)).toThrow(/outCubicc/)
  })
})
