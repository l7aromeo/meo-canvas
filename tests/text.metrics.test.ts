import { vi } from 'vitest'
import type { CanvasRenderingContext2D } from 'meo-skia-canvas'
import { invalidateTextMeasurements, measureText, textMeasurementCacheSize } from '@/canvas/text.metrics.js'

/** The cap the cache enforces, restated here so a change to it fails this file rather than leaks. */
const CACHE_LIMIT = 4096

/**
 * A context whose measurements are a pure function of the text, so a repeated answer is provably
 * the cache rather than a coincidence, and whose state is writable so a test can change the one
 * thing it cares about.
 */
const context = () => {
  const ctx = {
    font: '16px Roboto',
    letterSpacing: '0px',
    fontVariant: 'normal',
    textBaseline: 'alphabetic',
    direction: 'ltr',
    measureText: vi.fn((text: string) => ({
      width: text.length * 8,
      actualBoundingBoxAscent: 10,
      actualBoundingBoxDescent: 3,
    })),
  }
  return ctx as unknown as CanvasRenderingContext2D & { measureText: ReturnType<typeof vi.fn> }
}

beforeEach(() => {
  // Every test starts from a cache that cannot answer for it, which is what makes a call count
  // meaningful. The suite shares one process, so without this the first file to measure `"hello"`
  // decides what every later one observes.
  invalidateTextMeasurements()
})

describe('measureText', () => {
  it('returns what the renderer returned', () => {
    const ctx = context()
    expect(measureText(ctx, 'hello')).toEqual({ width: 40, actualBoundingBoxAscent: 10, actualBoundingBoxDescent: 3 })
  })

  it('asks the renderer once for a repeated question', () => {
    const ctx = context()

    const first = measureText(ctx, 'hello')
    const second = measureText(ctx, 'hello')

    expect(ctx.measureText).toHaveBeenCalledTimes(1)
    expect(second).toEqual(first)
  })

  it('answers a repeat identically, not merely equivalently', () => {
    const ctx = context()
    // Same object back, so a caller cannot mutate one measurement and leave the cache holding a
    // different one under the same key.
    expect(measureText(ctx, 'hello')).toBe(measureText(ctx, 'hello'))
  })

  it('measures each distinct string on its own', () => {
    const ctx = context()

    expect(measureText(ctx, 'a').width).toBe(8)
    expect(measureText(ctx, 'bb').width).toBe(16)
    expect(ctx.measureText).toHaveBeenCalledTimes(2)
  })

  /**
   * Each of these changes what the same string measures, so each has to miss. A key that dropped
   * one would serve the wrong geometry — the bug this cache could plausibly introduce.
   */
  describe('re-measures when the context changes', () => {
    it.each([
      ['font', 'font', '24px Roboto'],
      ['letterSpacing', 'letterSpacing', '2px'],
      ['fontVariant', 'fontVariant', 'small-caps'],
      ['textBaseline', 'textBaseline', 'top'],
      ['direction', 'direction', 'rtl'],
    ])('%s', (_label, property, changed) => {
      const ctx = context()
      measureText(ctx, 'hello')
      ;(ctx as unknown as Record<string, unknown>)[property] = changed

      measureText(ctx, 'hello')
      expect(ctx.measureText).toHaveBeenCalledTimes(2)
    })
  })

  it('re-measures everything once fonts are registered', () => {
    const ctx = context()
    measureText(ctx, 'hello')
    expect(ctx.measureText).toHaveBeenCalledTimes(1)

    // `16px Roboto` meant a fallback face a moment ago and means Roboto now, so the number taken
    // under the old set describes a font that is no longer being used.
    invalidateTextMeasurements()

    measureText(ctx, 'hello')
    expect(ctx.measureText).toHaveBeenCalledTimes(2)
  })

  it('does not disturb measurements already taken when fonts arrive', () => {
    const ctx = context()
    const before = measureText(ctx, 'hello')

    invalidateTextMeasurements()

    // A layout pass mid-flight keeps reading the numbers it was given; changing them underneath it
    // would move geometry that has already been positioned.
    expect(before).toEqual({ width: 40, actualBoundingBoxAscent: 10, actualBoundingBoxDescent: 3 })
  })

  it('stays bounded, so a process rendering endless text cannot grow it forever', () => {
    const ctx = context()
    const before = textMeasurementCacheSize()

    for (let i = 0; i < CACHE_LIMIT + 500; i++) {
      measureText(ctx, `line ${i}`)
    }

    expect(textMeasurementCacheSize()).toBeLessThanOrEqual(CACHE_LIMIT)
    expect(textMeasurementCacheSize()).toBeGreaterThan(before)
  })

  it('evicts the least recently used entry rather than the oldest', () => {
    const ctx = context()
    measureText(ctx, 'kept')

    // Re-asked partway through, so insertion order alone would still evict it and recency will not.
    for (let i = 0; i < CACHE_LIMIT; i++) {
      measureText(ctx, `filler ${i}`)
      if (i === CACHE_LIMIT / 2) measureText(ctx, 'kept')
    }

    ctx.measureText.mockClear()
    measureText(ctx, 'kept')
    expect(ctx.measureText).not.toHaveBeenCalled()
  })
})
