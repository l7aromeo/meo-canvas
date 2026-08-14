import { spring, springDuration } from '@/animate/spring.js'

const UNDERDAMPED = { stiffness: 170, damping: 12 }
const CRITICAL = { stiffness: 100, damping: 20, mass: 1 } // damping = 2*sqrt(k*m) exactly
const OVERDAMPED = { stiffness: 100, damping: 60, mass: 1 }

/** Samples a spring across its settling time. */
const sample = (config: Parameters<typeof spring>[1], count = 200) => {
  const end = springDuration(config)
  return Array.from({ length: count }, (_, i) => spring((i / (count - 1)) * end, config))
}

describe('spring', () => {
  it('starts at rest at the origin', () => {
    for (const config of [UNDERDAMPED, CRITICAL, OVERDAMPED]) {
      expect(spring(0, config)).toBeCloseTo(0, 6)
    }
  })

  it('settles on the target', () => {
    for (const config of [UNDERDAMPED, CRITICAL, OVERDAMPED]) {
      expect(spring(springDuration(config), config)).toBeCloseTo(1, 2)
      // Far past settling it must stay put, not drift or blow up.
      expect(spring(1000, config)).toBeCloseTo(1, 6)
    }
  })

  it('honours from and to', () => {
    const config = { ...CRITICAL, from: 20, to: 80 }
    expect(spring(0, config)).toBeCloseTo(20, 6)
    expect(spring(1000, config)).toBeCloseTo(80, 6)
  })

  it('overshoots when underdamped, and not otherwise', () => {
    expect(Math.max(...sample(UNDERDAMPED))).toBeGreaterThan(1)
    expect(Math.max(...sample(CRITICAL))).toBeLessThanOrEqual(1 + 1e-6)
    expect(Math.max(...sample(OVERDAMPED))).toBeLessThanOrEqual(1 + 1e-6)
  })

  it('approaches monotonically when critically damped or overdamped', () => {
    for (const config of [CRITICAL, OVERDAMPED]) {
      const values = sample(config)
      expect(values).toEqual([...values].sort((a, b) => a - b))
    }
  })

  it('settles fastest at critical damping, which is the point of it', () => {
    // Holding stiffness and mass fixed, so damping is the only variable. Critical damping is the
    // quickest approach that does not overshoot; more damping than that is slower, not faster.
    const stiffness = 100
    const under = springDuration({ stiffness, damping: 8, mass: 1 })
    const critical = springDuration({ stiffness, damping: 20, mass: 1 })
    const over = springDuration({ stiffness, damping: 60, mass: 1 })

    expect(critical).toBeLessThan(under)
    expect(critical).toBeLessThan(over)
  })

  it('moves faster with more stiffness', () => {
    const soft = springDuration({ stiffness: 60, damping: 15 })
    const stiff = springDuration({ stiffness: 400, damping: 15 })
    expect(stiff).toBeLessThan(soft)
  })

  it('takes an initial velocity', () => {
    // A push toward the target gets there sooner than a standing start.
    const pushed = spring(0.05, { ...CRITICAL, velocity: 10 })
    const still = spring(0.05, CRITICAL)
    expect(pushed).toBeGreaterThan(still)
  })

  it('stays finite everywhere, including at the damping boundary', () => {
    // ζ = 1 exactly is where the underdamped solution divides by zero if it is not special-cased.
    for (const config of [UNDERDAMPED, CRITICAL, OVERDAMPED, { stiffness: 100, damping: 20.0000001 }]) {
      for (let t = 0; t <= 5; t += 0.05) {
        expect(Number.isFinite(spring(t, config))).toBe(true)
      }
    }
  })

  it('is continuous across the critical boundary', () => {
    const t = 0.15
    const just_under = spring(t, { stiffness: 100, damping: 19.999, mass: 1 })
    const exactly = spring(t, CRITICAL)
    const just_over = spring(t, { stiffness: 100, damping: 20.001, mass: 1 })
    expect(just_under).toBeCloseTo(exactly, 3)
    expect(just_over).toBeCloseTo(exactly, 3)
  })

  it('treats mass as inertia', () => {
    const heavy = springDuration({ stiffness: 170, damping: 26, mass: 4 })
    const light = springDuration({ stiffness: 170, damping: 26, mass: 1 })
    expect(heavy).toBeGreaterThan(light)
  })

  it('never goes backwards in time', () => {
    expect(spring(-1, CRITICAL)).toBeCloseTo(0, 6)
  })

  it('rejects a configuration that cannot oscillate', () => {
    expect(() => spring(0.5, { stiffness: 0, damping: 10 })).toThrow(/stiffness/i)
    expect(() => spring(0.5, { stiffness: 100, damping: -1 })).toThrow(/damping/i)
    expect(() => spring(0.5, { stiffness: 100, damping: 10, mass: 0 })).toThrow(/mass/i)
  })
})

describe('springDuration', () => {
  it('reports a positive settling time', () => {
    expect(springDuration(UNDERDAMPED)).toBeGreaterThan(0)
  })

  it('reports a time the spring has arrived by and stays arrived after', () => {
    const config = UNDERDAMPED
    const settled = springDuration(config)

    // Arrived at the reported time, and still there afterwards. Checking a point *before* it would
    // prove nothing: an underdamped spring swings through its target on the way.
    for (const t of [settled, settled * 1.5, settled * 4]) {
      expect(Math.abs(spring(t, config) - 1)).toBeLessThan(0.01)
    }
  })

  it('does not report rest while the spring is still swinging', () => {
    const config = UNDERDAMPED
    const settled = springDuration(config)
    const overshoot = Math.max(...Array.from({ length: 400 }, (_, i) => spring((i / 399) * settled, config)))

    // The overshoot has to happen inside the reported window, not after it.
    expect(overshoot).toBeGreaterThan(1.01)
  })

  it('scales with the rest threshold', () => {
    expect(springDuration(UNDERDAMPED, { restDelta: 0.0001 })).toBeGreaterThan(springDuration(UNDERDAMPED, { restDelta: 0.1 }))
  })
})
