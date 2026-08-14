import { COLOR_CACHE_LIMIT, colorCacheSize, formatColor, isColor, mixColor, parseColor } from '@/animate/color.js'

/**
 * Colour parsing is delegated to the rendering engine rather than reimplemented, so these tests
 * assert that delegation holds across every family the engine accepts — including the ones a
 * hand-written CSS parser would quietly miss, like `lab()`, `oklch()` and `color(display-p3 …)`.
 */
describe('parseColor', () => {
  it.each([
    ['named', 'red', [255, 0, 0, 1]],
    ['named, uncommon', 'rebeccapurple', [102, 51, 153, 1]],
    ['hex short', '#f00', [255, 0, 0, 1]],
    ['hex long', '#ff0000', [255, 0, 0, 1]],
    ['rgb legacy', 'rgb(255,0,0)', [255, 0, 0, 1]],
    ['rgb modern', 'rgb(255 0 0)', [255, 0, 0, 1]],
    ['rgb percent', 'rgb(100% 0% 0%)', [255, 0, 0, 1]],
    ['hsl legacy', 'hsl(120,100%,50%)', [0, 255, 0, 1]],
    ['hsl modern', 'hsl(120 100% 50%)', [0, 255, 0, 1]],
    ['hwb', 'hwb(0 0% 0%)', [255, 0, 0, 1]],
    ['color(srgb)', 'color(srgb 1 0 0)', [255, 0, 0, 1]],
  ])('parses %s', (_name, css, expected) => {
    const { r, g, b, a } = parseColor(css as string)
    expect([r, g, b, a]).toEqual(expected)
  })

  it.each([
    ['hex with alpha', '#f00a', 170 / 255],
    ['rgba', 'rgba(255,0,0,0.5)', 128 / 255],
    ['modern slash alpha', 'rgb(255 0 0 / 50%)', 128 / 255],
  ])('parses alpha from %s', (_name, css, alpha) => {
    const parsed = parseColor(css as string)
    expect(parsed.r).toBe(255)
    expect(parsed.a).toBeCloseTo(alpha as number, 2)
  })

  it("resolves alpha at the engine's precision, not the string's", () => {
    // Worth stating outright. Alpha goes through two roundings on the way in: the engine stores it
    // as one of 256 levels, then reports it to three decimals. 0.12345 becomes 31/255, printed as
    // 0.122. A colour written with more precision than that does not come back unchanged.
    expect(parseColor('rgba(9, 9, 9, 0.12345)').a).toBe(0.122)
    expect(parseColor('rgba(9, 9, 9, 0.5)').a).toBe(0.502)
  })

  it('parses transparent as fully clear', () => {
    expect(parseColor('transparent')).toEqual({ r: 0, g: 0, b: 0, a: 0 })
  })

  it.each([
    ['lab', 'lab(50% 40 59.5)'],
    ['lch', 'lch(50% 70 40)'],
    ['oklab', 'oklab(0.5 0.1 0.1)'],
    ['oklch', 'oklch(0.7 0.2 30)'],
    ['display-p3', 'color(display-p3 1 0 0)'],
  ])('parses %s, the formats a hand-written parser would miss', (_name, css) => {
    const parsed = parseColor(css as string)
    // Converted by the engine, so the exact values are its business; what matters is that a real
    // colour comes back rather than a throw or a silent black.
    expect(parsed.a).toBe(1)
    expect(parsed.r + parsed.g + parsed.b).toBeGreaterThan(0)
  })

  it('rejects a string that is not a colour', () => {
    // The engine ignores an unparseable fillStyle rather than throwing, so a wrong colour would
    // otherwise surface as whatever was set last — silently rendering the wrong thing.
    expect(() => parseColor('bogus-not-a-color')).toThrow(/not a colour|not a color/i)
    expect(() => parseColor('')).toThrow()
  })

  it('reports what it can parse without throwing', () => {
    expect(isColor('red')).toBe(true)
    expect(isColor('oklch(0.7 0.2 30)')).toBe(true)
    expect(isColor('bogus-not-a-color')).toBe(false)
  })

  it('returns the same object shape for a repeated colour', () => {
    // Parsing goes through a cache; a cached hit must not be mutable shared state.
    const first = parseColor('#123456')
    first.r = 999
    expect(parseColor('#123456').r).toBe(0x12)
  })
})

describe('formatColor', () => {
  it('writes an opaque colour as hex', () => {
    expect(formatColor({ r: 255, g: 0, b: 0, a: 1 })).toBe('#ff0000')
  })

  it('writes a translucent colour as rgba', () => {
    expect(formatColor({ r: 255, g: 0, b: 0, a: 0.5 })).toBe('rgba(255, 0, 0, 0.5)')
  })

  it('round-trips through the engine', () => {
    const parsed = parseColor('rebeccapurple')
    expect(parseColor(formatColor(parsed))).toEqual(parsed)
  })

  it('clamps and rounds out-of-range channels', () => {
    expect(formatColor({ r: 300, g: -20, b: 127.6, a: 2 })).toBe('#ff0080')
  })
})

describe('mixColor', () => {
  it('returns the endpoints exactly', () => {
    expect(mixColor('#000000', '#ffffff', 0)).toBe('#000000')
    expect(mixColor('#000000', '#ffffff', 1)).toBe('#ffffff')
  })

  it('mixes halfway', () => {
    expect(mixColor('#000000', '#ffffff', 0.5)).toBe('#808080')
  })

  it('mixes across formats', () => {
    // A named colour and an oklch one have nothing in common syntactically; both parse to sRGB.
    expect(mixColor('red', 'oklch(0.7 0.2 30)', 0.5)).toMatch(/^#[0-9a-f]{6}$/)
  })

  it('interpolates alpha', () => {
    expect(mixColor('rgba(255,0,0,0)', 'rgba(255,0,0,1)', 0.5)).toBe('rgba(255, 0, 0, 0.5)')
  })

  it('clamps t outside 0..1', () => {
    expect(mixColor('#000000', '#ffffff', -1)).toBe('#000000')
    expect(mixColor('#000000', '#ffffff', 2)).toBe('#ffffff')
  })

  it('moves monotonically from one endpoint to the other', () => {
    const reds = [0, 0.25, 0.5, 0.75, 1].map(t => parseColor(mixColor('#000000', '#ff0000', t)).r)
    expect(reds).toEqual([...reds].sort((a, b) => a - b))
    expect(new Set(reds).size).toBe(reds.length)
  })
})

/**
 * The cache is module-level, so anything it keeps is kept for the life of the process. A render
 * that computes a colour per page — which is what chained `mix()` calls produce — would otherwise
 * add a permanent entry per frame and grow without bound on a long-running server.
 */
describe('the colour cache is bounded', () => {
  it('never grows past its limit, however many distinct colours it sees', () => {
    for (let i = 0; i < COLOR_CACHE_LIMIT * 3; i++) {
      parseColor(`rgba(1, 2, 3, ${(i / (COLOR_CACHE_LIMIT * 3)).toFixed(6)})`)
    }

    expect(colorCacheSize()).toBeLessThanOrEqual(COLOR_CACHE_LIMIT)
  })

  it('keeps a colour that is still being used, rather than evicting by age alone', () => {
    const hot = 'rgba(9, 9, 9, 0.12345)'
    parseColor(hot)

    // Push far more than the limit through, touching the hot colour along the way as an animation
    // would: its two endpoints are read on every single page.
    for (let i = 0; i < COLOR_CACHE_LIMIT * 2; i++) {
      parseColor(`rgba(4, 5, 6, ${(i / (COLOR_CACHE_LIMIT * 2)).toFixed(6)})`)
      parseColor(hot)
    }

    const parsed = parseColor(hot)
    expect([parsed.r, parsed.g, parsed.b]).toEqual([9, 9, 9])
    // Alpha survives only to 8-bit precision, which is the engine's, not this cache's.
    expect(parsed.a).toBeCloseTo(0.12345, 2)
    expect(colorCacheSize()).toBeLessThanOrEqual(COLOR_CACHE_LIMIT)
  })
})
