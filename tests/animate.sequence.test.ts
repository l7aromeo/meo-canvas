import { sequence } from '@/animate/sequence.js'
import { springDuration } from '@/animate/spring.js'
import type { PageInfo } from '@/canvas/canvas.type.js'

const FPS = 30
const pageAt = (time: number, count = 120): PageInfo => ({
  index: Math.round(time * FPS),
  count,
  progress: count > 1 ? Math.round(time * FPS) / (count - 1) : 0,
  cycle: Math.round(time * FPS) / count,
  time,
})

describe('sequence', () => {
  it('runs its steps one after another', () => {
    const move = sequence({
      from: 0,
      steps: [
        { to: 100, duration: 1 },
        { to: 50, duration: 1 },
      ],
    })

    expect(move.at(pageAt(0))).toBe(0)
    expect(move.at(pageAt(0.5))).toBeCloseTo(50)
    expect(move.at(pageAt(1))).toBeCloseTo(100)
    expect(move.at(pageAt(1.5))).toBeCloseTo(75)
    expect(move.at(pageAt(2))).toBeCloseTo(50)
  })

  it('each step starts where the previous one ended', () => {
    const move = sequence({
      from: 10,
      steps: [
        { to: 20, duration: 1 },
        { to: 30, duration: 1 },
      ],
    })

    // No jump at the seam: the value is continuous across the boundary.
    expect(move.at(pageAt(0.999))).toBeCloseTo(20, 1)
    expect(move.at(pageAt(1.001))).toBeCloseTo(20, 1)
  })

  it('holds at the ends', () => {
    const move = sequence({ from: 0, steps: [{ to: 100, duration: 1 }] })

    expect(move.at(pageAt(-5))).toBe(0)
    expect(move.at(pageAt(99))).toBe(100)
  })

  it('pauses between steps for a hold', () => {
    const move = sequence({
      from: 0,
      steps: [
        { to: 100, duration: 1, hold: 0.5 },
        { to: 0, duration: 1 },
      ],
    })

    expect(move.at(pageAt(1))).toBeCloseTo(100)
    // Held for the whole gap, then starts back down.
    expect(move.at(pageAt(1.25))).toBeCloseTo(100)
    expect(move.at(pageAt(1.5))).toBeCloseTo(100)
    expect(move.at(pageAt(2))).toBeCloseTo(50)
    expect(move.at(pageAt(2.5))).toBeCloseTo(0)
  })

  it('waits out an overall delay', () => {
    const move = sequence({ from: 0, steps: [{ to: 100, duration: 1 }], delay: 0.5 })

    expect(move.at(pageAt(0.25))).toBe(0)
    expect(move.at(pageAt(1))).toBeCloseTo(50)
    expect(move.at(pageAt(1.5))).toBeCloseTo(100)
  })

  it('staggers a row of items through the same sequence', () => {
    const move = sequence({
      from: 0,
      steps: [{ to: 100, duration: 0.5 }],
      stagger: 0.25,
    })

    const t = pageAt(0.5)
    expect(move.at(t, 0)).toBeCloseTo(100)
    expect(move.at(t, 1)).toBeLessThan(100)
    expect(move.at(t, 2)).toBe(0)
  })

  it('eases each step independently', () => {
    const eased = sequence({
      from: 0,
      steps: [
        { to: 100, duration: 1, ease: 'outCubic' },
        { to: 200, duration: 1 },
      ],
    })

    // The first step is eased and so is ahead of linear; the second is not.
    expect(eased.at(pageAt(0.5))).toBeGreaterThan(50)
    expect(eased.at(pageAt(1.5))).toBeCloseTo(150)
  })

  it('drives a step with a spring, taking the duration from the physics', () => {
    const config = { stiffness: 180, damping: 14 }
    const move = sequence({ from: 0, steps: [{ to: 100, spring: config }] })

    expect(move.duration).toBeCloseTo(springDuration(config), 5)
    expect(move.at(pageAt(0))).toBeCloseTo(0)
    expect(move.at(pageAt(move.duration))).toBeCloseTo(100, 0)
  })

  it('sequences colours', () => {
    const shift = sequence({
      from: '#000000',
      steps: [
        { to: '#ffffff', duration: 1 },
        { to: '#ff0000', duration: 1 },
      ],
    })

    expect(shift.at(pageAt(0.5))).toBe('#808080')
    expect(shift.at(pageAt(1))).toBe('#ffffff')
    expect(shift.at(pageAt(2))).toBe('#ff0000')
  })

  it('reports the window it occupies', () => {
    const move = sequence({
      from: 0,
      steps: [
        { to: 1, duration: 0.5, hold: 0.25 },
        { to: 2, duration: 0.5 },
      ],
      delay: 0.1,
      stagger: 0.2,
    })

    expect(move.duration).toBeCloseTo(0.1 + 0.5 + 0.25 + 0.5)
    expect(move.totalDuration(3)).toBeCloseTo(0.1 + 0.5 + 0.25 + 0.5 + 0.4)
  })

  it('needs at least one step', () => {
    expect(() => sequence({ from: 0, steps: [] })).toThrow(/at least one step/i)
  })

  it('needs a duration or spring on every step', () => {
    expect(() => sequence({ from: 0, steps: [{ to: 1 }] })).toThrow(/duration/i)
  })

  it('refuses a step whose spring carries its own from or to', () => {
    // The step's `to` and the previous value define the range; a second pair on the spring would be
    // silently dropped otherwise.
    expect(() => sequence({ from: 0, steps: [{ to: 1, spring: { stiffness: 100, from: 9 } }] })).toThrow(/from|to/i)
    expect(() => sequence({ from: 0, steps: [{ to: 1, spring: { stiffness: 100, to: 9 } }] })).toThrow(/from|to/i)
  })

  it('rejects a step that is both sprung and eased', () => {
    expect(() => sequence({ from: 0, steps: [{ to: 1, duration: 1, spring: { stiffness: 100 }, ease: 'outCubic' }] })).toThrow(/spring.*ease|ease.*spring/i)
  })

  it('rejects negative timings', () => {
    expect(() => sequence({ from: 0, steps: [{ to: 1, duration: -1 }] })).toThrow(/duration/i)
    expect(() => sequence({ from: 0, steps: [{ to: 1, duration: 1, hold: -1 }] })).toThrow(/hold/i)
    expect(() => sequence({ from: 0, steps: [{ to: 1, duration: 1 }], delay: -1 })).toThrow(/delay/i)
    expect(() => sequence({ from: 0, steps: [{ to: 1, duration: 1 }], stagger: -1 })).toThrow(/stagger/i)
  })

  it('matches a single-step sequence to the equivalent track', async () => {
    const { track } = await import('@/animate/track.js')
    const asTrack = track({ from: 0, to: 1, duration: 1, ease: 'outCubic', delay: 0.2 })
    const asSequence = sequence({ from: 0, steps: [{ to: 1, duration: 1, ease: 'outCubic' }], delay: 0.2 })

    for (const t of [0, 0.3, 0.7, 1.2, 5]) {
      expect(asSequence.at(pageAt(t))).toBeCloseTo(asTrack.at(pageAt(t)) as number, 10)
    }
  })
})
