import { frameAtTime } from '@/canvas/image.frames.js'

/** A four-frame source with uneven timing, which is what a real GIF looks like. */
const DELAYS = [100, 400, 100, 400]
const TOTAL = 1000

describe('frameAtTime', () => {
  it('holds the first frame before the source has advanced', () => {
    expect(frameAtTime(DELAYS, 0)).toBe(0)
    expect(frameAtTime(DELAYS, 0.099)).toBe(0)
  })

  it('advances on each frame boundary, honouring uneven delays', () => {
    expect(frameAtTime(DELAYS, 0.1)).toBe(1)
    expect(frameAtTime(DELAYS, 0.49)).toBe(1)
    expect(frameAtTime(DELAYS, 0.5)).toBe(2)
    expect(frameAtTime(DELAYS, 0.6)).toBe(3)
  })

  it('loops back around by default', () => {
    // One full cycle later the source is where it started.
    expect(frameAtTime(DELAYS, TOTAL / 1000)).toBe(0)
    expect(frameAtTime(DELAYS, TOTAL / 1000 + 0.1)).toBe(1)
    expect(frameAtTime(DELAYS, (TOTAL * 3) / 1000 + 0.6)).toBe(3)
  })

  it('holds the last frame when told not to loop', () => {
    expect(frameAtTime(DELAYS, 5, { loop: false })).toBe(DELAYS.length - 1)
    // and still animates before it gets there
    expect(frameAtTime(DELAYS, 0.1, { loop: false })).toBe(1)
  })

  it('treats a time before zero as the start', () => {
    expect(frameAtTime(DELAYS, -3)).toBe(0)
  })

  it('stays on the only frame of a still image', () => {
    expect(frameAtTime([0], 12)).toBe(0)
    expect(frameAtTime([], 12)).toBe(0)
  })

  it('does not divide by zero when every delay is zero', () => {
    // Some encoders write 0ms delays; a browser substitutes a default rather than spinning.
    const frame = frameAtTime([0, 0, 0], 1)
    expect(Number.isInteger(frame)).toBe(true)
    expect(frame).toBeGreaterThanOrEqual(0)
    expect(frame).toBeLessThan(3)
  })

  it('never reports a frame the source does not have', () => {
    for (let t = 0; t < 4; t += 0.017) {
      const frame = frameAtTime(DELAYS, t)
      expect(frame).toBeGreaterThanOrEqual(0)
      expect(frame).toBeLessThan(DELAYS.length)
    }
  })
})
