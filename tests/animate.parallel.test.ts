import { parallel } from '@/animate/parallel.js'
import { track } from '@/animate/track.js'
import { sequence } from '@/animate/sequence.js'
import type { PageInfo } from '@/canvas/canvas.type.js'

const FPS = 30
const pageAt = (time: number, count = 120): PageInfo => ({
  index: Math.round(time * FPS),
  count,
  progress: count > 1 ? Math.round(time * FPS) / (count - 1) : 0,
  time,
})

const fade = track({ from: 0, to: 1, duration: 1 })
const slide = track({ from: -20, to: 0, duration: 0.5, delay: 0.25 })
const tint = track({ from: '#000000', to: '#ffffff', duration: 2 })

describe('parallel', () => {
  it('samples every member at once', () => {
    const group = parallel({ fade, slide })
    const at = group.at(pageAt(0.5))

    expect(at.fade).toBeCloseTo(0.5)
    expect(at.slide).toBeCloseTo(-10)
  })

  it('keeps each member on its own timing', () => {
    const group = parallel({ fade, slide })

    // `slide` waits out its delay while `fade` is already moving.
    expect(group.at(pageAt(0.1)).slide).toBe(-20)
    expect(group.at(pageAt(0.1)).fade).toBeGreaterThan(0)
  })

  it('reports the longest member as its duration', () => {
    // fade ends at 1s, slide at 0.75s, tint at 2s.
    expect(parallel({ fade, slide }).duration).toBeCloseTo(1)
    expect(parallel({ fade, slide, tint }).duration).toBeCloseTo(2)
  })

  it('reports the longest staggered member as its total duration', () => {
    const staggered = track({ from: 0, to: 1, duration: 0.5, stagger: 0.3 })
    const group = parallel({ fade, staggered })

    // Three staggered items run to 0.5 + 0.6 = 1.1, past fade's 1s.
    expect(group.totalDuration(3)).toBeCloseTo(1.1)
    // With one item the stagger contributes nothing, so fade is longest again.
    expect(group.totalDuration(1)).toBeCloseTo(1)
  })

  it('passes an index through to every member', () => {
    const staggered = track({ from: 0, to: 1, duration: 0.5, stagger: 0.3 })
    const group = parallel({ fade, staggered })

    // At 0.4s the first item is 0.4 into its 0.5s window; the second started 0.3s later and so is
    // only 0.1 in, and the third has not begun.
    expect(group.at(pageAt(0.4), 0).staggered).toBeCloseTo(0.8)
    expect(group.at(pageAt(0.4), 1).staggered).toBeCloseTo(0.2)
    expect(group.at(pageAt(0.4), 2).staggered).toBe(0)
    // A member without a stagger is unaffected by the index.
    expect(group.at(pageAt(0.4), 1).fade).toBeCloseTo(0.4)
  })

  it('mixes tracks and sequences, which share a shape', () => {
    const legs = sequence({
      from: 0,
      steps: [
        { to: 10, duration: 0.5 },
        { to: 0, duration: 0.5 },
      ],
    })
    const group = parallel({ fade, legs })

    expect(group.at(pageAt(0.5)).legs).toBeCloseTo(10)
    expect(group.duration).toBeCloseTo(1)
  })

  it('carries whatever type each member animates', () => {
    const group = parallel({ fade, tint })
    const at = group.at(pageAt(1))

    expect(typeof at.fade).toBe('number')
    expect(at.tint).toBe('#808080')
  })

  it('nests, since a group is sampled the same way a track is', () => {
    const inner = parallel({ fade, slide })
    const outer = parallel({ inner, tint })

    expect(outer.at(pageAt(0.5)).inner.fade).toBeCloseTo(0.5)
    expect(outer.duration).toBeCloseTo(2)
  })

  it('needs at least one member', () => {
    expect(() => parallel({})).toThrow(/at least one/i)
  })
})
