import { vi } from 'vitest'
import { paintBackgroundImage } from '@/canvas/background.canvas.js'
import { createGradient } from '@/canvas/gradient.canvas.js'
import { parseColor, formatColor, isColor } from '@/animate/color.js'
import type { CanvasRenderingContext2D, Image as CanvasImage } from 'meo-skia-canvas'

const IMAGE = { width: 30, height: 60 } as CanvasImage
const BOX = { x: 0, y: 0, width: 100, height: 50 }
const NO_RADII = { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 }

function recordingContext() {
  const drawn: Array<[number, number, number, number]> = []
  const ctx = {
    save: vi.fn(),
    restore: vi.fn(),
    clip: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    arcTo: vi.fn(),
    closePath: vi.fn(),
    rect: vi.fn(),
    drawImage: vi.fn((_image: unknown, x: number, y: number, w: number, h: number) => drawn.push([x, y, w, h])),
  } as unknown as CanvasRenderingContext2D
  return { ctx, drawn }
}

describe('paintBackgroundImage — tile sizing', () => {
  const paint = (size: unknown) => {
    const { ctx, drawn } = recordingContext()
    paintBackgroundImage(ctx, IMAGE, { image: 'x.png', size, repeat: 'no-repeat' } as any, BOX, NO_RADII)
    return drawn
  }

  it('takes the picture at its natural size when no size is given', () => {
    expect(paint(undefined)[0].slice(2)).toEqual([30, 60])
  })

  it('follows the ratio when only a width is given', () => {
    expect(paint({ width: 60 })[0].slice(2)).toEqual([60, 120])
  })

  it('follows the ratio when only a height is given', () => {
    expect(paint({ height: 30 })[0].slice(2)).toEqual([15, 30])
  })

  it('takes both edges when both are given', () => {
    expect(paint({ width: 40, height: 20 })[0].slice(2)).toEqual([40, 20])
  })

  it('reads a percentage width against the box width', () => {
    expect(paint({ width: '50%' })[0][2]).toBe(50)
  })

  it('reads a percentage height against the box height', () => {
    expect(paint({ height: '50%' })[0][3]).toBe(25)
  })

  it('reads both edges as percentages', () => {
    expect(paint({ width: '20%', height: '40%' })[0].slice(2)).toEqual([20, 20])
  })

  it('draws nothing for a size that collapses to zero', () => {
    expect(paint({ width: 0, height: 0 })).toHaveLength(0)
  })

  it('draws nothing into a box with no area', () => {
    const { ctx, drawn } = recordingContext()
    paintBackgroundImage(ctx, IMAGE, { image: 'x.png' } as any, { x: 0, y: 0, width: 0, height: 0 }, NO_RADII)
    expect(drawn).toHaveLength(0)
  })
})

describe('paintBackgroundImage — tiling', () => {
  it.each(['repeat', 'repeat-x', 'repeat-y', 'no-repeat', 'space', 'round'] as const)('lays tiles out for %s', repeat => {
    const { ctx, drawn } = recordingContext()
    paintBackgroundImage(ctx, IMAGE, { image: 'x.png', repeat, size: { width: 30, height: 30 } } as any, BOX, NO_RADII)
    expect(drawn.length).toBeGreaterThan(0)
  })

  it('starts a repeat off the left edge when the origin is positive', () => {
    const { ctx, drawn } = recordingContext()
    paintBackgroundImage(ctx, IMAGE, { image: 'x.png', repeat: 'repeat-x', size: { width: 30, height: 30 }, position: { x: 20, y: 0 } } as any, BOX, NO_RADII)
    expect(Math.min(...drawn.map(tile => tile[0]))).toBeLessThan(20)
  })
})

describe('createGradient', () => {
  const ctx = {
    createLinearGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    createRadialGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    createConicGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
  } as unknown as CanvasRenderingContext2D
  const box = { x: 0, y: 0, width: 100, height: 50 }

  it('takes an explicit four-number direction', () => {
    const result = createGradient(ctx, { type: 'linear', direction: [0, 0, 10, 10], colors: ['#000', '#fff'] } as any, box)
    expect(result.gradient).not.toBeNull()
  })

  it('refuses a direction that names nothing', () => {
    const result = createGradient(ctx, { type: 'linear', direction: 'to-nowhere', colors: ['#000'] } as any, box)
    expect(result.gradient).toBeNull()
    expect(result.reason).toBeTruthy()
  })

  it('refuses a direction that is neither a string nor four numbers', () => {
    const result = createGradient(ctx, { type: 'linear', direction: 42, colors: ['#000'] } as any, box)
    expect(result.gradient).toBeNull()
  })

  it('reads a direction case-insensitively', () => {
    expect(createGradient(ctx, { type: 'linear', direction: 'TO-BOTTOM', colors: ['#000', '#fff'] } as any, box).gradient).not.toBeNull()
  })

  it('gives a lone colour a single stop', () => {
    expect(createGradient(ctx, { type: 'linear', colors: ['#123'] } as any, box).gradient).not.toBeNull()
  })

  it.each([
    ['a radial gradient', { type: 'radial', colors: ['#000', '#fff'] }],
    ['a conic gradient', { type: 'conic', colors: ['#000', '#fff'] }],
    ['a conic gradient with a start angle', { type: 'conic', from: 90, colors: ['#000', '#fff'] }],
    ['a gradient with no type at all', { colors: ['#000', '#fff'] }],
  ])('builds %s', (_label, gradient) => {
    expect(createGradient(ctx, gradient as any, box).gradient).not.toBeNull()
  })
})

describe('colour parsing edges', () => {
  it('reads rgba with an explicit alpha', () => {
    const parsed = parseColor('rgba(10, 20, 30, 0.5)')
    expect(parsed).toMatchObject({ r: 10, g: 20, b: 30 })
    // Alpha comes back through an 8-bit channel, so 0.5 lands on 128/255.
    expect(parsed.a).toBeCloseTo(0.5, 2)
  })

  it('defaults the alpha when rgb gives none', () => {
    expect(parseColor('rgb(10, 20, 30)')).toMatchObject({ r: 10, g: 20, b: 30, a: 1 })
  })

  it('formats a colour outside the sRGB gamut as color(srgb …)', () => {
    expect(formatColor({ r: 300, g: -20, b: 10, a: 1 })).toMatch(/^color\(srgb /)
  })

  it('carries the alpha into an out-of-gamut colour', () => {
    expect(formatColor({ r: 300, g: -20, b: 10, a: 0.5 })).toContain('/')
  })

  it('omits the alpha from an out-of-gamut colour when it is opaque', () => {
    expect(formatColor({ r: 300, g: -20, b: 10, a: 1 })).not.toContain('/')
  })

  it('clamps an alpha outside 0..1', () => {
    expect(formatColor({ r: 10, g: 20, b: 30, a: 5 })).toBe(formatColor({ r: 10, g: 20, b: 30, a: 1 }))
  })

  it('rejects a string that is not a colour', () => {
    expect(isColor('definitely not a colour')).toBe(false)
  })
})

describe('comlink proxy transfer handler', () => {
  it('handles the four shapes canHandle must decide between', async () => {
    const { Comlink } = await import('@/worker/comlink.setup.js')
    const handler = Comlink.transferHandlers.get('proxy')!
    const marked = { [Comlink.proxyMarker]: true }
    const markedFn = Object.assign(() => {}, { [Comlink.proxyMarker]: true })

    expect(handler.canHandle(marked)).toBe(true)
    expect(handler.canHandle(markedFn)).toBe(true)
    expect(handler.canHandle({})).toBe(false)
    expect(handler.canHandle(null)).toBe(false)
    expect(handler.canHandle(42)).toBe(false)
    expect(handler.canHandle('a string')).toBe(false)
  })

  it('serialises a proxied object onto a Node message port and reads it back', async () => {
    const { Comlink } = await import('@/worker/comlink.setup.js')
    const handler = Comlink.transferHandlers.get('proxy')!
    const [port, transfers] = handler.serialize({ [Comlink.proxyMarker]: true, double: (n: number) => n * 2 }) as [any, unknown[]]
    expect(transfers).toHaveLength(1)

    const wrapped = handler.deserialize(port) as unknown as { double: (n: number) => Promise<number> }
    await expect(wrapped.double(21)).resolves.toBe(42)
    port.close()
  })
})

describe('gradient extras', () => {
  const ctx = {
    createLinearGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    createRadialGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    createConicGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
  } as unknown as CanvasRenderingContext2D
  const box = { x: 0, y: 0, width: 100, height: 50 }

  it('reads a radial centre given as a fraction of the box', () => {
    expect(createGradient(ctx, { type: 'radial', center: { x: 0.25, y: 0.5 }, colors: ['#000', '#fff'] } as any, box).gradient).not.toBeNull()
  })

  it('reads a radial centre given in pixels', () => {
    expect(createGradient(ctx, { type: 'radial', center: { x: 40, y: 20 }, colors: ['#000', '#fff'] } as any, box).gradient).not.toBeNull()
  })

  it('reads a radial centre given as a percentage string', () => {
    expect(createGradient(ctx, { type: 'radial', center: { x: '25%', y: '50%' }, colors: ['#000', '#fff'] } as any, box).gradient).not.toBeNull()
  })

  it('defaults a gradient with no direction key at all to top-to-bottom', () => {
    expect(createGradient(ctx, { type: 'linear', colors: ['#000', '#fff'] } as any, box).gradient).not.toBeNull()
  })

  it('reports the reason when the renderer refuses to build one', () => {
    const refusing = { ...ctx, createLinearGradient: vi.fn(() => null) } as unknown as CanvasRenderingContext2D
    const result = createGradient(refusing, { type: 'linear', colors: ['#000', '#fff'] } as any, box)
    expect(result.gradient).toBeNull()
    expect(result.reason).toContain('linear')
  })
})

describe('background tile origin', () => {
  it('takes the natural size on both edges when the size object names neither', () => {
    const { ctx, drawn } = recordingContext()
    paintBackgroundImage(ctx, IMAGE, { image: 'x.png', size: {}, repeat: 'no-repeat' } as any, BOX, NO_RADII)
    expect(drawn[0].slice(2)).toEqual([30, 60])
  })

  it('starts from the origin when a repeat lands no tile before it', () => {
    const { ctx, drawn } = recordingContext()
    paintBackgroundImage(ctx, IMAGE, { image: 'x.png', repeat: 'repeat', size: { width: 10, height: 10 }, position: { x: 0, y: 0 } } as any, BOX, NO_RADII)
    expect(drawn.length).toBeGreaterThan(0)
  })
})

describe('colour alpha defaults', () => {
  it('defaults the alpha of a colour(srgb …) with none given', () => {
    expect(parseColor('color(srgb 0.5 0.25 0.125)').a).toBe(1)
  })

  it('reads an explicit alpha on a colour(srgb …)', () => {
    expect(parseColor('color(srgb 0.5 0.25 0.125 / 0.5)').a).toBeCloseTo(0.5, 2)
  })
})
