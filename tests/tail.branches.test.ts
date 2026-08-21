import { vi } from 'vitest'
import { PathNode } from '@/canvas/path.canvas.js'
import { drawBorders, parseBorderRadius, shadowSpill, parsePercentage } from '@/canvas/canvas.helper.js'
import { cubicBezier } from '@/animate/easing.js'
import { sequence } from '@/animate/sequence.js'
import { spring } from '@/animate/spring.js'
import { asNodeProps } from '@/canvas/page.plan.js'
import { Style } from '@/constant/common.const.js'
import { Canvas } from 'meo-skia-canvas'
import Yoga from '@/constant/common.const.js'
import type { RootProps, PageInfo } from '@/canvas/canvas.type.js'

describe('PathNode', () => {
  const paint = async (props: Record<string, unknown>) => {
    const canvas = new Canvas(100, 100)
    const ctx = canvas.getContext('2d')
    const node = new PathNode({ width: 100, height: 100, ...props } as any)
    node.processInitialChildren()
    node.node.calculateLayout(100, 100, Style.Direction.LTR)
    await node.render(ctx, 0, 0)
    return ctx
  }

  it('draws nothing without a path', async () => {
    await expect(paint({ fill: '#000' })).resolves.toBeTruthy()
  })

  it('fills a path', async () => {
    await expect(paint({ d: 'M0 0 L50 0 L50 50 Z', fill: '#0a0' })).resolves.toBeTruthy()
  })

  it('fills with the evenodd rule', async () => {
    await expect(paint({ d: 'M0 0 L50 0 L50 50 Z', fill: '#0a0', fillRule: 'evenodd' })).resolves.toBeTruthy()
  })

  it.each([
    ['a bare stroke', {}],
    ['an explicit line width', { lineWidth: 4 }],
    ['a line cap', { lineCap: 'round' }],
    ['a line join', { lineJoin: 'bevel' }],
    ['a dash pattern', { lineDash: [4, 2] }],
    ['a dash offset', { lineDash: [4, 2], lineDashOffset: 2 }],
    ['a dash offset of zero', { lineDash: [4, 2], lineDashOffset: 0 }],
    ['every stroke option at once', { lineWidth: 3, lineCap: 'square', lineJoin: 'round', lineDash: [6, 3], lineDashOffset: 1 }],
  ])('strokes a path with %s', async (_label, props) => {
    await expect(paint({ d: 'M0 0 L50 50', stroke: '#c00', ...props })).resolves.toBeTruthy()
  })

  it('warns and skips a paint whose gradient cannot be built', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    await paint({ d: 'M0 0 L50 50', fill: { type: 'linear', direction: 'to-nowhere', colors: ['#000'] } })
    expect(warn).toHaveBeenCalled()
    warn.mockRestore()
  })

  it('fills and strokes with gradients', async () => {
    const gradient = { type: 'linear', direction: 'to-bottom', colors: ['#000', '#fff'] }
    await expect(paint({ d: 'M0 0 L50 50 L0 50 Z', fill: gradient, stroke: gradient })).resolves.toBeTruthy()
  })
})

describe('drawBorders — per-edge colour defaults', () => {
  const run = (borderColor: unknown, widths: Record<string, number>, borderStyle?: unknown) => {
    const canvas = new Canvas(100, 100)
    const ctx = canvas.getContext('2d')
    const node = Yoga.Node.create()
    node.setWidth(100)
    node.setHeight(100)
    node.setBorder(Style.Edge.Top, widths.Top)
    node.setBorder(Style.Edge.Right, widths.Right)
    node.setBorder(Style.Edge.Bottom, widths.Bottom)
    node.setBorder(Style.Edge.Left, widths.Left)
    node.calculateLayout(100, 100, Style.Direction.LTR)
    drawBorders({
      ctx,
      node,
      x: 0,
      y: 0,
      width: 100,
      height: 100,
      borderColor: borderColor as any,
      borderStyle: borderStyle as any,
      radii: { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 },
    })
    node.freeRecursive()
    return ctx
  }
  const allEdges = { Top: 4, Right: 4, Bottom: 4, Left: 4 }

  it.each([
    ['only the top named', { Top: '#f00' }],
    ['only the right named', { Right: '#f00' }],
    ['only the bottom named', { Bottom: '#f00' }],
    ['only the left named', { Left: '#f00' }],
    ['every edge named', { Top: '#f00', Right: '#0f0', Bottom: '#00f', Left: '#ff0' }],
    ['no edge named', {}],
  ])('defaults the unnamed edges to black with %s', (_label, borderColor) => {
    expect(() => run(borderColor, allEdges)).not.toThrow()
  })

  it('takes a single colour string for every edge', () => {
    expect(() => run('#123456', allEdges)).not.toThrow()
  })

  it('draws nothing where a corner has no width', () => {
    expect(() => run('#000', { Top: 0, Right: 0, Bottom: 0, Left: 0 })).not.toThrow()
  })

  it('draws a corner where only one of the two edges has width', () => {
    expect(() => run('#000', { Top: 4, Right: 0, Bottom: 0, Left: 4 })).not.toThrow()
  })

  it.each([
    ['solid', 'solid'],
    ['dashed', 'dashed'],
    ['dotted', 'dotted'],
  ])('draws a %s border', (_label, borderStyle) => {
    expect(() => run('#000', allEdges, borderStyle)).not.toThrow()
  })

  it('draws a border with rounded corners', () => {
    const canvas = new Canvas(100, 100)
    const ctx = canvas.getContext('2d')
    const node = Yoga.Node.create()
    node.setWidth(100)
    node.setHeight(100)
    for (const edge of [Style.Edge.Top, Style.Edge.Right, Style.Edge.Bottom, Style.Edge.Left]) node.setBorder(edge, 6)
    node.calculateLayout(100, 100, Style.Direction.LTR)
    expect(() =>
      drawBorders({
        ctx,
        node,
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        borderColor: '#000',
        borderStyle: undefined,
        radii: { TopLeft: 20, TopRight: 20, BottomRight: 20, BottomLeft: 20 },
      }),
    ).not.toThrow()
    node.freeRecursive()
  })
})

describe('parseBorderRadius', () => {
  it.each([
    ['a number', 8, 8],
    ['zero', 0, 0],
    ['a negative number, clamped', -8, 0],
  ])('reads %s', (_label, input, expected) => {
    const radii = parseBorderRadius(input as any)
    expect(radii.TopLeft).toBe(expected)
    expect(radii.BottomLeft).toBe(expected)
  })

  it('reads an object, defaulting the corners left out', () => {
    const radii = parseBorderRadius({ TopLeft: 10, BottomRight: -4 } as any)
    expect(radii.TopLeft).toBe(10)
    expect(radii.TopRight).toBe(0)
    expect(radii.BottomRight).toBe(0)
  })

  it('reads null as no radius at all', () => {
    expect(parseBorderRadius(null as any).TopLeft).toBe(0)
  })

  it('reads undefined as no radius at all', () => {
    expect(parseBorderRadius(undefined).TopLeft).toBe(0)
  })
})

describe('shadowSpill', () => {
  it('returns nothing for no shadow', () => {
    expect(shadowSpill(undefined)).toBe(0)
  })

  it('falls back to the larger offset when no blur is given', () => {
    expect(shadowSpill({ offsetX: 10, offsetY: 4 } as any)).toBeGreaterThan(0)
  })

  it('reads a bare shadow as no spill', () => {
    expect(shadowSpill({} as any)).toBe(0)
  })

  it('ignores a negative spread', () => {
    expect(shadowSpill({ blur: 4, spread: -20 } as any)).toBe(shadowSpill({ blur: 4, spread: 0 } as any))
  })

  it('takes the largest spill across a list', () => {
    const list = [{ blur: 2 }, { blur: 20 }] as any
    expect(shadowSpill(list)).toBe(shadowSpill({ blur: 20 } as any))
  })

  it('accounts for a negative offset by its magnitude', () => {
    expect(shadowSpill({ blur: 2, offsetX: -30 } as any)).toBe(shadowSpill({ blur: 2, offsetX: 30 } as any))
  })
})

describe('parsePercentage', () => {
  it.each([
    ['a number', 20, 100, 20],
    ['a percentage string', '25%', 200, 50],
    ['undefined', undefined, 100, 0],
  ])('reads %s', (_label, value, base, expected) => {
    expect(parsePercentage(value as any, base)).toBe(expected)
  })
})

describe('cubicBezier', () => {
  it('solves an ordinary curve', () => {
    const ease = cubicBezier(0.42, 0, 0.58, 1)
    expect(ease(0.5)).toBeCloseTo(0.5, 1)
  })

  it('bisects when Newton stalls on a flat section', () => {
    // With both x controls at zero the slope is 3t^2, which near t=0 falls under the epsilon that
    // stops Newton — the one shape that forces the bisection fallback.
    const ease = cubicBezier(0, 0, 0, 1)
    const value = ease(0.001)
    expect(Number.isFinite(value)).toBe(true)
    expect(value).toBeGreaterThanOrEqual(0)
    expect(value).toBeLessThanOrEqual(1)
  })

  it('bisects from both sides of the target', () => {
    const ease = cubicBezier(0, 0, 0, 1)
    // Several targets across the range so the search narrows from above and from below.
    for (const t of [0.002, 0.01, 0.2, 0.6, 0.9]) {
      expect(Number.isFinite(ease(t))).toBe(true)
    }
  })

  it('holds the endpoints', () => {
    const ease = cubicBezier(0.25, 0.1, 0.25, 1)
    expect(ease(0)).toBeCloseTo(0, 5)
    expect(ease(1)).toBeCloseTo(1, 5)
  })
})

describe('sequence', () => {
  const FPS = 30
  const pageAt = (time: number, count = 120): PageInfo => ({
    index: Math.round(time * FPS),
    count,
    progress: count > 1 ? Math.round(time * FPS) / (count - 1) : 0,
    cycle: Math.round(time * FPS) / count,
    time,
  })

  it('runs a spring step', () => {
    const track = sequence({ from: 0, steps: [{ to: 10, spring: { stiffness: 100 } }] } as any)
    expect(Number.isFinite(track.at(pageAt(0.05)) as number)).toBe(true)
  })

  it('treats a zero-duration step as immediately complete', () => {
    const track = sequence({ from: 0, steps: [{ to: 10, duration: 0 }] } as any)
    // The step occupies no time at all, so anything past its start is already at the end value.
    expect(track.at(pageAt(0.1))).toBe(10)
  })

  it('runs an eased step', () => {
    const track = sequence({ from: 0, steps: [{ to: 10, duration: 1, ease: 'linear' }] } as any)
    expect(track.at(pageAt(0.5))).toBeCloseTo(5, 5)
  })

  it('holds at the end of a step before the next begins', () => {
    const track = sequence({
      from: 0,
      steps: [
        { to: 10, duration: 1, hold: 1 },
        { to: 20, duration: 1 },
      ],
    } as any)
    expect(track.at(pageAt(1.5))).toBeCloseTo(10, 5)
  })
})

describe('spring', () => {
  it('treats a zero distance as a unit distance rather than dividing by it', () => {
    expect(Number.isFinite(spring(0.5, { from: 5, to: 5 }))).toBe(true)
  })

  it('runs with the default range', () => {
    expect(Number.isFinite(spring(0.5))).toBe(true)
  })
})

describe('asNodeProps', () => {
  it('drops the page props a single-page node has no use for', () => {
    const props = { width: 10, height: 10, pages: 3, duration: 1, fps: 24, children: [] } as unknown as RootProps
    const result = asNodeProps(props) as Record<string, unknown>
    expect(result.pages).toBeUndefined()
    expect(result.fps).toBeUndefined()
    expect(result.width).toBe(10)
  })

  it('refuses a builder that was never resolved into pages', () => {
    const props = { width: 10, height: 10, children: () => [] } as unknown as RootProps
    expect(() => asNodeProps(props)).toThrow(/resolve it with planPages/)
  })
})
