import { Canvas } from 'meo-skia-canvas'
import { contextScale, drawWithGradientMask, isGradientMask, maskFillRule, maskPath } from '@/canvas/mask.canvas.js'

const BOX = { x: 10, y: 20, width: 100, height: 60 }

/** A real context rather than a mock: these helpers read transforms and build paths through it. */
const context = (width = 200, height = 200) => new Canvas(width, height).getContext('2d')

describe('isGradientMask', () => {
  it('separates the kind that composites from the kinds that clip', () => {
    expect(isGradientMask({ gradient: { type: 'linear', direction: 'to-bottom', colors: ['#000'] } })).toBe(true)
    expect(isGradientMask({ shape: 'circle' })).toBe(false)
    expect(isGradientMask({ path: 'M 0 0 H 10' })).toBe(false)
    expect(isGradientMask('M 0 0 H 10')).toBe(false)
  })
})

describe('maskPath', () => {
  it('has no path for a gradient, which cannot be clipped', () => {
    expect(maskPath({ gradient: { type: 'linear', direction: 'to-bottom', colors: ['#000'] } }, BOX)).toBeNull()
  })

  it('inscribes a circle in the box, sized by its shorter side', () => {
    const bounds = maskPath({ shape: 'circle' }, BOX)!.bounds
    const radius = Math.min(BOX.width, BOX.height) / 2

    // Centred on the box, and as wide as it is tall — a circle in an oblong is still a circle.
    expect(bounds.left).toBeCloseTo(BOX.x + BOX.width / 2 - radius, 5)
    expect(bounds.top).toBeCloseTo(BOX.y + BOX.height / 2 - radius, 5)
    expect(bounds.width).toBeCloseTo(radius * 2, 5)
    expect(bounds.height).toBeCloseTo(radius * 2, 5)
  })

  it('stretches an ellipse to the whole box', () => {
    const bounds = maskPath({ shape: 'ellipse' }, BOX)!.bounds

    expect(bounds.left).toBeCloseTo(BOX.x, 5)
    expect(bounds.top).toBeCloseTo(BOX.y, 5)
    expect(bounds.width).toBeCloseTo(BOX.width, 5)
    expect(bounds.height).toBeCloseTo(BOX.height, 5)
  })

  it('reads path data in the node coordinates, and places it at the node', () => {
    // `0,0` in the path is the node's top-left corner, not the canvas's — otherwise the same mask
    // would mean something different depending on where layout put the node.
    const bounds = maskPath('M 0 0 H 50 V 30 H 0 Z', BOX)!.bounds

    expect(bounds.left).toBeCloseTo(BOX.x, 5)
    expect(bounds.top).toBeCloseTo(BOX.y, 5)
    expect(bounds.width).toBeCloseTo(50, 5)
    expect(bounds.height).toBeCloseTo(30, 5)
  })

  it('treats a bare string and the object form alike', () => {
    expect(maskPath('M 0 0 H 50 V 30 H 0 Z', BOX)!.d).toBe(maskPath({ path: 'M 0 0 H 50 V 30 H 0 Z' }, BOX)!.d)
  })
})

describe('maskFillRule', () => {
  it('defaults to nonzero, as the Canvas API does', () => {
    expect(maskFillRule('M 0 0 H 10')).toBe('nonzero')
    expect(maskFillRule({ path: 'M 0 0 H 10' })).toBe('nonzero')
    expect(maskFillRule({ shape: 'circle' })).toBe('nonzero')
  })

  it('passes evenodd through, which is what makes a hole in a path', () => {
    expect(maskFillRule({ path: 'M 0 0 H 10', fillRule: 'evenodd' })).toBe('evenodd')
  })
})

describe('contextScale', () => {
  it('reports 1 for an untouched context', () => {
    expect(contextScale(context())).toEqual({ x: 1, y: 1 })
  })

  it('reports the scale a render is drawing at', () => {
    // What `Root`'s `scale: 2` leaves on the context, and what an offscreen has to match or the
    // masked node comes out softer than everything beside it.
    const ctx = context()
    ctx.scale(2, 3)
    expect(contextScale(ctx)).toEqual({ x: 2, y: 3 })
  })

  it('reports magnitude rather than the cosine of a rotation', () => {
    const ctx = context()
    ctx.scale(2, 2)
    ctx.rotate(Math.PI / 4)

    const scale = contextScale(ctx)
    expect(scale.x).toBeCloseTo(2, 6)
    expect(scale.y).toBeCloseTo(2, 6)
  })

  it('never reports zero, which would size an offscreen at nothing', () => {
    const ctx = context()
    ctx.scale(0, 0)
    expect(contextScale(ctx)).toEqual({ x: 1, y: 1 })
  })
})

describe('drawWithGradientMask', () => {
  const gradient = { type: 'linear', direction: 'to-bottom', colors: ['#000000ff', '#00000000'] } as const

  it('draws through the mask and reports that it did', async () => {
    const ctx = context()
    const drawn = await drawWithGradientMask(
      ctx,
      gradient,
      BOX,
      async target => {
        target.fillStyle = '#ff0000'
        target.fillRect(BOX.x, BOX.y, BOX.width, BOX.height)
      },
      '[test]',
    )

    expect(drawn).toBe(true)

    const alphaAt = (offsetY: number) => ctx.getImageData(BOX.x + BOX.width / 2, BOX.y + offsetY, 1, 1).data[3]
    // Opaque where the gradient starts, gone where it ends.
    expect(alphaAt(1)).toBeGreaterThan(200)
    expect(alphaAt(BOX.height - 2)).toBeLessThan(40)
  })

  it('reports failure for a box with no area, rather than allocating nothing', async () => {
    const drawn = await drawWithGradientMask(context(), gradient, { ...BOX, width: 0 }, async () => {}, '[test]')
    expect(drawn).toBe(false)
  })

  it('reports failure and says why when the gradient cannot be built', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const drawn = await drawWithGradientMask(context(), { type: 'linear', direction: 'sideways' as never, colors: ['#000'] }, BOX, async () => {}, '[test]')

    expect(drawn).toBe(false)
    // The caller draws unmasked instead, so the message has to say the mask was dropped — not that
    // it fell back to a colour, which is the background's answer and meaningless here.
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('Mask ignored.'))
    warn.mockRestore()
  })
})
