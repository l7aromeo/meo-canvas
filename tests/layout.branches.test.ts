import { BoxNode, normalizeDescriptorChildren } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import { Canvas } from 'meo-skia-canvas'
import type { BoxProps } from '@/canvas/canvas.type.js'

/** Lays a box out on a real canvas and renders it, which is what exercises the paint branches. */
async function paint(props: BoxProps, width = 200, height = 200) {
  const canvas = new Canvas(width, height)
  const ctx = canvas.getContext('2d')
  const node = new BoxNode({ width, height, ...props })
  node.processInitialChildren()
  node.node.calculateLayout(width, height, Style.Direction.LTR)
  await node.render(ctx, 0, 0)
  return { canvas, ctx, node }
}

describe('BoxNode — transforms', () => {
  it.each([
    ['a translation in pixels', { translateX: 10, translateY: 5 }],
    ['a translation in percentages', { translateX: '10%', translateY: '5%' }],
    ['a zero translation, which is skipped', { translateX: 0, translateY: 0, rotate: 10 }],
    ['a rotation', { rotate: 30 }],
    ['a uniform scale', { scale: 2 }],
    ['separate axis scales', { scaleX: 2, scaleY: 0.5 }],
    ['a scale of exactly one, which is skipped', { scale: 1, rotate: 5 }],
    ['scaleX only, with scaleY falling back', { scaleX: 1.5 }],
    ['scaleY only, with scaleX falling back', { scaleY: 1.5 }],
    ['an explicit origin', { rotate: 45, originX: '0%', originY: '0%' }],
    ['an origin in pixels', { rotate: 45, originX: 10, originY: 10 }],
    ['everything at once', { translateX: 4, translateY: 4, rotate: 15, scale: 1.2, originX: '25%', originY: '75%' }],
  ])('renders with %s', async (_label, transform) => {
    const { canvas } = await paint({ backgroundColor: '#0a0', transform: transform as any })
    expect(canvas.width).toBe(200)
  })

  it('skips the transform block entirely when every component is inert', async () => {
    const { canvas } = await paint({ backgroundColor: '#0a0', transform: {} as any })
    expect(canvas.width).toBe(200)
  })
})

describe('BoxNode — border radius', () => {
  it.each([
    ['a single number', 12],
    ['zero', 0],
    ['a negative number, clamped to zero', -8],
    ['every corner named', { TopLeft: 4, TopRight: 8, BottomRight: 12, BottomLeft: 16 }],
    ['only some corners named', { TopLeft: 10 }],
    ['a negative corner, clamped to zero', { TopLeft: -10, BottomRight: 6 }],
  ])('renders with %s', async (_label, borderRadius) => {
    const { canvas } = await paint({ backgroundColor: '#06c', borderRadius: borderRadius as any })
    expect(canvas.width).toBe(200)
  })
})

describe('BoxNode — outset shadows', () => {
  it('takes the fast path behind an opaque background', async () => {
    const { canvas } = await paint({
      backgroundColor: '#ffffff',
      boxShadow: [{ offsetX: 4, offsetY: 4, blur: 8, color: 'rgba(0,0,0,0.4)' }],
    })
    expect(canvas.width).toBe(200)
  })

  it.each([
    ['a translucent hex background', '#ffffff80'],
    ['an rgba background', 'rgba(255,255,255,0.5)'],
    ['a transparent background', 'transparent'],
    ['no background at all', undefined],
  ])('takes the offscreen path behind %s', async (_label, backgroundColor) => {
    const { canvas } = await paint({
      backgroundColor,
      boxShadow: [{ offsetX: 4, offsetY: 4, blur: 8, color: '#000' }],
    })
    expect(canvas.width).toBe(200)
  })

  it.each([
    ['a shadow with no blur, which falls back to the offset', { offsetX: 6, offsetY: 3 }],
    ['a shadow with no offsets at all', { blur: 5 }],
    ['a bare shadow', {}],
    ['a shadow with a positive spread', { blur: 4, spread: 6 }],
    ['a shadow with a negative spread', { blur: 4, spread: -6 }],
    ['a shadow with negative offsets', { offsetX: -6, offsetY: -6, blur: 3 }],
    ['a shadow with no colour', { blur: 4, offsetX: 2 }],
  ])('renders %s', async (_label, shadow) => {
    const { canvas } = await paint({ backgroundColor: '#ffffff', boxShadow: [shadow as any] })
    expect(canvas.width).toBe(200)
  })

  it('renders several outset shadows together', async () => {
    const { canvas } = await paint({
      backgroundColor: 'rgba(0,0,0,0.2)',
      boxShadow: [
        { offsetX: 2, offsetY: 2, blur: 4, color: '#f00' },
        { offsetX: -6, offsetY: 8, blur: 12, spread: 2, color: '#00f' },
      ],
    })
    expect(canvas.width).toBe(200)
  })
})

describe('BoxNode — inset shadows', () => {
  it.each([
    ['a plain inset shadow', { inset: true, blur: 6, color: '#000' }],
    ['an inset shadow with offsets', { inset: true, offsetX: 4, offsetY: -4, blur: 6 }],
    ['an inset shadow with a spread', { inset: true, blur: 4, spread: 5 }],
    ['an inset shadow with no blur', { inset: true, offsetX: 3 }],
    ['an inset shadow with a negative blur, clamped', { inset: true, blur: -4 }],
    ['an inset shadow with no colour', { inset: true, blur: 3 }],
  ])('renders %s', async (_label, shadow) => {
    const { canvas } = await paint({ backgroundColor: '#eee', boxShadow: [shadow as any] })
    expect(canvas.width).toBe(200)
  })

  it('renders inset and outset shadows on the same node', async () => {
    const { canvas } = await paint({
      backgroundColor: '#eee',
      boxShadow: [
        { blur: 6, color: '#000' },
        { inset: true, blur: 6, color: '#fff' },
      ],
    })
    expect(canvas.width).toBe(200)
  })

  it('draws no inset shadow into a box with no area', async () => {
    const { canvas } = await paint({ backgroundColor: '#eee', boxShadow: [{ inset: true, blur: 4 } as any] }, 0, 0)
    expect(canvas.width).toBe(0)
  })
})

describe('normalizeDescriptorChildren', () => {
  it.each([
    ['undefined', undefined],
    ['null', null],
    ['false', false],
    ['an empty array', []],
    ['an array of only falsy values', [null, undefined, false]],
  ])('returns undefined for %s', (_label, children) => {
    expect(normalizeDescriptorChildren(children as any)).toBeUndefined()
  })

  it('wraps a lone child in an array', () => {
    const child = { __type: 'Box' } as any
    expect(normalizeDescriptorChildren(child)).toEqual([child])
  })

  it('strips falsy entries from an array', () => {
    const child = { __type: 'Box' } as any
    expect(normalizeDescriptorChildren([child, null, false] as any)).toEqual([child])
  })
})
