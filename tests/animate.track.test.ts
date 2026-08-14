import { track } from '@/animate/track.js'
import { springDuration } from '@/animate/spring.js'
import type { PageInfo } from '@/canvas/canvas.type.js'

const FPS = 30

/** A page at a given time, which is what a track samples against. */
const pageAt = (time: number, count = 60): PageInfo => ({
  index: Math.round(time * FPS),
  count,
  progress: count > 1 ? Math.round(time * FPS) / (count - 1) : 0,
  time,
})

describe('track', () => {
  it('runs from its start value to its end value over its duration', () => {
    const fade = track({ from: 0, to: 1, duration: 1 })

    expect(fade.at(pageAt(0))).toBeCloseTo(0)
    expect(fade.at(pageAt(0.5))).toBeCloseTo(0.5)
    expect(fade.at(pageAt(1))).toBeCloseTo(1)
  })

  it('holds at both ends outside its window', () => {
    const fade = track({ from: 10, to: 20, duration: 1 })

    expect(fade.at(pageAt(-5))).toBe(10)
    expect(fade.at(pageAt(99))).toBe(20)
  })

  it('waits out its delay before moving', () => {
    const fade = track({ from: 0, to: 1, duration: 1, delay: 0.5 })

    expect(fade.at(pageAt(0.25))).toBe(0)
    expect(fade.at(pageAt(0.5))).toBeCloseTo(0)
    expect(fade.at(pageAt(1))).toBeCloseTo(0.5)
    expect(fade.at(pageAt(1.5))).toBeCloseTo(1)
  })

  it('applies an easing', () => {
    const eased = track({ from: 0, to: 1, duration: 1, ease: 'outCubic' })
    const linear = track({ from: 0, to: 1, duration: 1 })

    expect(eased.at(pageAt(0.5))).toBeGreaterThan(linear.at(pageAt(0.5)))
  })

  it('staggers by index', () => {
    const grow = track({ from: 0, to: 1, duration: 0.5, stagger: 0.2 })

    // At the moment the first item finishes, the later ones are still on their way.
    const t = pageAt(0.5)
    expect(grow.at(t, 0)).toBeCloseTo(1)
    expect(grow.at(t, 1)).toBeLessThan(1)
    expect(grow.at(t, 2)).toBeLessThan(grow.at(t, 1))
  })

  it('treats a missing index as the first item', () => {
    const grow = track({ from: 0, to: 1, duration: 0.5, stagger: 0.2 })
    expect(grow.at(pageAt(0.25))).toBe(grow.at(pageAt(0.25), 0))
  })

  it('interpolates colours as readily as numbers', () => {
    const shift = track({ from: '#000000', to: '#ffffff', duration: 1 })
    expect(shift.at(pageAt(0.5))).toBe('#808080')
  })

  it('reports the window it occupies, so a render can be sized by it', () => {
    const grow = track({ from: 0, to: 1, duration: 0.5, delay: 0.25, stagger: 0.2 })

    expect(grow.duration).toBeCloseTo(0.75)
    // Three staggered items finish two stagger steps after the first.
    expect(grow.totalDuration(3)).toBeCloseTo(0.75 + 0.4)
  })

  describe('driven by a spring', () => {
    const config = { stiffness: 180, damping: 14 }

    it('starts at rest and settles on the target', () => {
      const bounce = track({ from: 0, to: 100, spring: config })

      expect(bounce.at(pageAt(0))).toBeCloseTo(0)
      expect(bounce.at(pageAt(bounce.duration))).toBeCloseTo(100, 0)
    })

    it('takes its duration from the physics', () => {
      const bounce = track({ from: 0, to: 1, spring: config })
      expect(bounce.duration).toBeCloseTo(springDuration(config), 5)
    })

    it('still honours delay and stagger', () => {
      const bounce = track({ from: 0, to: 1, spring: config, delay: 0.5, stagger: 0.1 })

      expect(bounce.at(pageAt(0.25))).toBe(0)
      expect(bounce.at(pageAt(0.55), 0)).not.toBe(0)
      expect(bounce.at(pageAt(0.55), 3)).toBe(0)
    })

    it('overshoots, which an eased track would not', () => {
      const bounce = track({ from: 0, to: 1, spring: config })
      const peak = Math.max(...Array.from({ length: 200 }, (_, i) => bounce.at(pageAt((i / 199) * bounce.duration)) as number))
      expect(peak).toBeGreaterThan(1)
    })

    it('cannot also take an easing', () => {
      expect(() => track({ from: 0, to: 1, spring: config, ease: 'outCubic', duration: 1 })).toThrow(/spring.*ease|ease.*spring/i)
    })
  })

  it('needs a duration unless a spring supplies one', () => {
    expect(() => track({ from: 0, to: 1 })).toThrow(/duration/i)
  })

  it('rejects a negative duration, delay or stagger', () => {
    expect(() => track({ from: 0, to: 1, duration: -1 })).toThrow(/duration/i)
    expect(() => track({ from: 0, to: 1, duration: 1, delay: -1 })).toThrow(/delay/i)
    expect(() => track({ from: 0, to: 1, duration: 1, stagger: -1 })).toThrow(/stagger/i)
  })

  it('holds a zero-duration track at its end value', () => {
    const instant = track({ from: 0, to: 1, duration: 0 })
    expect(instant.at(pageAt(0))).toBe(1)
  })
})
