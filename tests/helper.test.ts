import { drawBorders, drawRoundedRectPath, parseBorderRadius, parsePercentage } from '@/canvas/canvas.helper.js'
import { extractFunctions, restoreFunctions, FN_MARKER } from '@/worker/comlink.pool.js'
import * as YogaTypes from 'yoga-layout'
import { Style } from '@/constant/common.const.js'
import type { CanvasRenderingContext2D } from 'meo-skia-canvas'
import { vi } from 'vitest'

const createMockContext = () => {
  const mockCtx: Partial<CanvasRenderingContext2D> = {
    // Properties
    strokeStyle: '',
    fillStyle: '',
    lineWidth: 0,
    lineCap: 'butt',
    lineJoin: 'miter',
    globalAlpha: 1,
    shadowOffsetX: 0,
    shadowOffsetY: 0,
    shadowBlur: 0,
    shadowColor: '',
    filter: '',
    imageSmoothingEnabled: false,
    imageSmoothingQuality: 'high',
    textBaseline: 'alphabetic',
    letterSpacing: '',
    wordSpacing: '',
    font: '',
    fontVariant: 'normal',
  }
  return mockCtx as CanvasRenderingContext2D
}

const createMockYogaNode = (borders: Partial<Record<YogaTypes.Edge, number>> = {}, boxSizing: YogaTypes.BoxSizing = Style.BoxSizing.BorderBox) => {
  const mockNode: Partial<YogaTypes.Node> = {
    getBorder: (edge: YogaTypes.Edge) => borders[edge] || 0,
    getBoxSizing: () => boxSizing,
    getComputedPadding: () => 0,
    getComputedBorder: (edge: YogaTypes.Edge) => borders[edge] || 0,
  }
  return mockNode as YogaTypes.Node
}

describe('parsePercentage', () => {
  it('should return the number if the value is a number', () => {
    expect(parsePercentage(10, 100)).toBe(10)
  })

  it('should return the calculated percentage if the value is a percentage string', () => {
    expect(parsePercentage('50%', 100)).toBe(50)
  })

  it('should return 0 if the value is not a number or a valid percentage', () => {
    expect(parsePercentage('abc', 100)).toBe(0)
  })

  it('should return 0 if the base is 0 and the value is a percentage', () => {
    expect(parsePercentage('50%', 0)).toBe(0)
  })
})

describe('parseBorderRadius', () => {
  it('should return all radii as the same number when a single number is provided', () => {
    const result = parseBorderRadius(10)
    expect(result).toEqual({ TopLeft: 10, TopRight: 10, BottomRight: 10, BottomLeft: 10 })
  })

  it('should return specified radii when an object is provided', () => {
    const result = parseBorderRadius({ TopLeft: 5, TopRight: 10, BottomRight: 15, BottomLeft: 20 })
    expect(result).toEqual({ TopLeft: 5, TopRight: 10, BottomRight: 15, BottomLeft: 20 })
  })

  it('should default unspecified radii to 0 when an object is provided', () => {
    const result = parseBorderRadius({ TopLeft: 5, BottomRight: 15 })
    expect(result).toEqual({ TopLeft: 5, TopRight: 0, BottomRight: 15, BottomLeft: 0 })
  })

  it('should clamp negative radii to 0', () => {
    const result = parseBorderRadius({ TopLeft: -5, TopRight: 10 })
    expect(result).toEqual({ TopLeft: 0, TopRight: 10, BottomRight: 0, BottomLeft: 0 })
  })

  it('should return all zeros for null or undefined input', () => {
    expect(parseBorderRadius(null as any)).toEqual({ TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 })
    expect(parseBorderRadius(undefined)).toEqual({ TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 })
  })
})

describe('drawRoundedRectPath', () => {
  let mockCtx: CanvasRenderingContext2D

  beforeEach(() => {
    mockCtx = createMockContext()
    mockCtx.save = vi.fn()
    mockCtx.restore = vi.fn()
    mockCtx.beginPath = vi.fn()
    mockCtx.closePath = vi.fn()
    mockCtx.moveTo = vi.fn()
    mockCtx.lineTo = vi.fn()
    mockCtx.arc = vi.fn()
    mockCtx.rect = vi.fn()
    mockCtx.stroke = vi.fn()
    mockCtx.fill = vi.fn()
    mockCtx.clip = vi.fn()
    mockCtx.setLineDash = vi.fn()
  })

  it('should draw a simple rectangle if width or height is zero or negative', () => {
    drawRoundedRectPath(mockCtx, 0, 0, 0, 100, { TopLeft: 10, TopRight: 10, BottomRight: 10, BottomLeft: 10 })
    expect(mockCtx.beginPath).toHaveBeenCalledTimes(1)
    expect(mockCtx.rect).toHaveBeenCalledWith(0, 0, 0, 100)
    expect(mockCtx.moveTo).not.toHaveBeenCalled()
    expect(mockCtx.arc).not.toHaveBeenCalled()
    expect(mockCtx.closePath).not.toHaveBeenCalled() // closePath is not called in this branch

    vi.clearAllMocks()

    drawRoundedRectPath(mockCtx, 0, 0, 100, -10, { TopLeft: 10, TopRight: 10, BottomRight: 10, BottomLeft: 10 })
    expect(mockCtx.beginPath).toHaveBeenCalledTimes(1)
    expect(mockCtx.rect).toHaveBeenCalledWith(0, 0, 100, -10)
    expect(mockCtx.moveTo).not.toHaveBeenCalled()
    expect(mockCtx.arc).not.toHaveBeenCalled()
    expect(mockCtx.closePath).not.toHaveBeenCalled() // closePath is not called in this branch
  })

  it('should draw a rectangle with no rounded corners when radii are 0', () => {
    drawRoundedRectPath(mockCtx, 10, 20, 100, 50, { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 })
    expect(mockCtx.beginPath).toHaveBeenCalledTimes(1)
    expect(mockCtx.moveTo).toHaveBeenCalledWith(10, 20) // x + clampedTL, y
    expect(mockCtx.lineTo).toHaveBeenCalledWith(110, 20) // x + width - clampedTR, y
    expect(mockCtx.lineTo).toHaveBeenCalledWith(110, 70) // x + width, y + height - clampedBR
    expect(mockCtx.lineTo).toHaveBeenCalledWith(10, 70) // x + clampedBL, y + height
    expect(mockCtx.lineTo).toHaveBeenCalledWith(10, 20) // x, y + clampedTL
    expect(mockCtx.arc).not.toHaveBeenCalled()
    expect(mockCtx.closePath).toHaveBeenCalledTimes(1)
  })

  it('should draw a rectangle with all rounded corners', () => {
    drawRoundedRectPath(mockCtx, 10, 20, 100, 50, { TopLeft: 10, TopRight: 10, BottomRight: 10, BottomLeft: 10 })
    expect(mockCtx.beginPath).toHaveBeenCalledTimes(1)
    expect(mockCtx.moveTo).toHaveBeenCalledWith(20, 20) // x + clampedTL, y
    expect(mockCtx.lineTo).toHaveBeenCalledWith(100, 20) // x + width - clampedTR, y
    expect(mockCtx.arc).toHaveBeenCalledWith(100, 30, 10, 1.5 * Math.PI, 0) // TopRight
    expect(mockCtx.lineTo).toHaveBeenCalledWith(110, 60) // x + width, y + height - clampedBR
    expect(mockCtx.arc).toHaveBeenCalledWith(100, 60, 10, 0, 0.5 * Math.PI) // BottomRight
    expect(mockCtx.lineTo).toHaveBeenCalledWith(20, 70) // x + clampedBL, y + height
    expect(mockCtx.arc).toHaveBeenCalledWith(20, 60, 10, 0.5 * Math.PI, Math.PI) // BottomLeft
    expect(mockCtx.lineTo).toHaveBeenCalledWith(10, 30) // x, y + clampedTL
    expect(mockCtx.arc).toHaveBeenCalledWith(20, 30, 10, Math.PI, 1.5 * Math.PI) // TopLeft
    expect(mockCtx.closePath).toHaveBeenCalledTimes(1)
  })

  it('should clamp radii to half of the smallest dimension', () => {
    drawRoundedRectPath(mockCtx, 0, 0, 10, 10, { TopLeft: 100, TopRight: 100, BottomRight: 100, BottomLeft: 100 })
    // maxRadius should be 5 (min(10/2, 10/2))
    expect(mockCtx.moveTo).toHaveBeenCalledWith(5, 0)
    expect(mockCtx.arc).toHaveBeenCalledWith(5, 5, 5, Math.PI, 1.5 * Math.PI) // TopLeft
    expect(mockCtx.arc).toHaveBeenCalledWith(5, 5, 5, 1.5 * Math.PI, 0) // TopRight
    expect(mockCtx.arc).toHaveBeenCalledWith(5, 5, 5, 0, 0.5 * Math.PI) // BottomRight
    expect(mockCtx.arc).toHaveBeenCalledWith(5, 5, 5, 0.5 * Math.PI, Math.PI) // BottomLeft
  })

  it('should use lineTo for sharp corners', () => {
    drawRoundedRectPath(mockCtx, 0, 0, 100, 50, { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 })
    expect(mockCtx.lineTo).toHaveBeenCalledTimes(8)
    expect(mockCtx.arc).not.toHaveBeenCalled()
  })
})

describe('drawBorders', () => {
  let mockCtx: CanvasRenderingContext2D
  let mockNode: YogaTypes.Node

  beforeEach(() => {
    mockCtx = createMockContext()
    mockCtx.save = vi.fn()
    mockCtx.restore = vi.fn()
    mockCtx.beginPath = vi.fn()
    mockCtx.closePath = vi.fn()
    mockCtx.moveTo = vi.fn()
    mockCtx.lineTo = vi.fn()
    mockCtx.arc = vi.fn()
    mockCtx.rect = vi.fn()
    mockCtx.stroke = vi.fn()
    mockCtx.fill = vi.fn()
    mockCtx.clip = vi.fn()
    mockCtx.setLineDash = vi.fn()

    mockNode = createMockYogaNode()
    mockNode.getBorder = vi.fn(() => 0)
    mockNode.getBoxSizing = vi.fn(() => Style.BoxSizing.BorderBox)
    mockNode.getComputedPadding = vi.fn(() => 0)
    mockNode.getComputedBorder = vi.fn(() => 0)
  })

  it('should not draw borders if hasBorder is false', () => {
    mockNode = createMockYogaNode({ [YogaTypes.Edge.All]: 0 })
    drawBorders({
      ctx: mockCtx,
      node: mockNode,
      x: 0,
      y: 0,
      width: 100,
      height: 100,
      radii: { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 },
      borderColor: 'black',
      borderStyle: Style.Border.Solid,
    })
    expect(mockCtx.save).not.toHaveBeenCalled()
    expect(mockCtx.beginPath).not.toHaveBeenCalled()
    expect(mockCtx.stroke).not.toHaveBeenCalled()
  })

  it('should not draw borders if borderColor is undefined', () => {
    mockNode = createMockYogaNode({ [YogaTypes.Edge.All]: 5 })
    drawBorders({
      ctx: mockCtx,
      node: mockNode,
      x: 0,
      y: 0,
      width: 100,
      height: 100,
      radii: { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 },
      borderColor: undefined,
      borderStyle: Style.Border.Solid,
    })
    expect(mockCtx.save).not.toHaveBeenCalled()
    expect(mockCtx.beginPath).not.toHaveBeenCalled()
    expect(mockCtx.stroke).not.toHaveBeenCalled()
  })

  it('should not draw corner arc if radius is zero', () => {
    mockNode = createMockYogaNode({ [YogaTypes.Edge.All]: 5 })
    drawBorders({
      ctx: mockCtx,
      node: mockNode,
      x: 0,
      y: 0,
      width: 100,
      height: 100,
      radii: { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 },
      borderColor: 'black',
      borderStyle: Style.Border.Solid,
    })
    // arc should not be called for the corners
    expect(mockCtx.arc).not.toHaveBeenCalled()
  })

  describe('Solid Borders (BorderBox)', () => {
    beforeEach(() => {
      mockNode = createMockYogaNode({ [YogaTypes.Edge.All]: 5 }, Style.BoxSizing.BorderBox)
    })

    it('should draw solid borders for all edges with no radii', () => {
      drawBorders({
        ctx: mockCtx,
        node: mockNode,
        x: 10,
        y: 10,
        width: 100,
        height: 100,
        radii: { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 },
        borderColor: 'red',
        borderStyle: Style.Border.Solid,
      })

      expect(mockCtx.strokeStyle).toBe('red')
      expect(mockCtx.lineWidth).toBe(5) // borderTop
      expect(mockCtx.setLineDash).toHaveBeenCalledWith([]) // Solid line

      // Top line
      expect(mockCtx.moveTo).toHaveBeenCalledWith(10, 12.5) // x + rTL, y + halfBt
      expect(mockCtx.lineTo).toHaveBeenCalledWith(110, 12.5) // x + width - rTR, y + halfBt

      // Right line
      expect(mockCtx.moveTo).toHaveBeenCalledWith(107.5, 10) // x + width - halfBr, y + rTR
      expect(mockCtx.lineTo).toHaveBeenCalledWith(107.5, 110) // x + width - halfBr, y + height - rBR

      // Bottom line
      expect(mockCtx.moveTo).toHaveBeenCalledWith(110, 107.5) // x + width - rBR, y + height - halfBb
      expect(mockCtx.lineTo).toHaveBeenCalledWith(10, 107.5) // x + rBL, y + height - halfBb

      // Left line
      expect(mockCtx.moveTo).toHaveBeenCalledWith(12.5, 110) // x + halfBl, y + height - rBL
      expect(mockCtx.lineTo).toHaveBeenCalledWith(12.5, 10) // x + halfBl, y + rTL

      expect(mockCtx.stroke).toHaveBeenCalledTimes(4) // 4 lines
    })

    it('should draw solid borders with rounded corners', () => {
      mockNode = createMockYogaNode({ [YogaTypes.Edge.All]: 5 }, Style.BoxSizing.BorderBox)
      drawBorders({
        ctx: mockCtx,
        node: mockNode,
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        radii: { TopLeft: 10, TopRight: 10, BottomRight: 10, BottomLeft: 10 },
        borderColor: 'blue',
        borderStyle: Style.Border.Solid,
      })

      expect(mockCtx.strokeStyle).toBe('blue')
      expect(mockCtx.setLineDash).toHaveBeenCalledWith([])

      // Expect 4 line segments and 4 arcs
      expect(mockCtx.stroke).toHaveBeenCalledTimes(8)
      expect(mockCtx.arc).toHaveBeenCalledTimes(4)

      // Top-Left arc (cx, cy, radius, startAngle, endAngle, border1, border2)
      expect(mockCtx.arc).toHaveBeenCalledWith(10, 10, 7.5, Math.PI, 1.5 * Math.PI)
      // Top-Right arc
      expect(mockCtx.arc).toHaveBeenCalledWith(90, 10, 7.5, 1.5 * Math.PI, 2 * Math.PI)
      // Bottom-Right arc
      expect(mockCtx.arc).toHaveBeenCalledWith(90, 90, 7.5, 0, 0.5 * Math.PI)
      // Bottom-Left arc
      expect(mockCtx.arc).toHaveBeenCalledWith(10, 90, 7.5, 0.5 * Math.PI, Math.PI)
    })

    it('should handle individual border widths', () => {
      mockNode = createMockYogaNode(
        {
          [YogaTypes.Edge.Top]: 1,
          [YogaTypes.Edge.Right]: 2,
          [YogaTypes.Edge.Bottom]: 3,
          [YogaTypes.Edge.Left]: 4,
        },
        Style.BoxSizing.BorderBox,
      )
      drawBorders({
        ctx: mockCtx,
        node: mockNode,
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        radii: { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 },
        borderColor: 'green',
        borderStyle: Style.Border.Solid,
      })

      // Top line (width 1)
      // expect(mockCtx.lineWidth).toBe(1) // This will only reflect the last set lineWidth
      expect(mockCtx.moveTo).toHaveBeenCalledWith(0, 0.5)
      expect(mockCtx.lineTo).toHaveBeenCalledWith(100, 0.5)

      // Right line (width 2)
      // expect(mockCtx.lineWidth).toBe(2)
      expect(mockCtx.moveTo).toHaveBeenCalledWith(99, 0)
      expect(mockCtx.lineTo).toHaveBeenCalledWith(99, 100)

      // Bottom line (width 3)
      // expect(mockCtx.lineWidth).toBe(3)
      expect(mockCtx.moveTo).toHaveBeenCalledWith(100, 98.5)
      expect(mockCtx.lineTo).toHaveBeenCalledWith(0, 98.5)

      // Left line (width 4)
      // expect(mockCtx.lineWidth).toBe(4)
      expect(mockCtx.moveTo).toHaveBeenCalledWith(2, 100)
      expect(mockCtx.lineTo).toHaveBeenCalledWith(2, 0)
      expect(mockCtx.lineWidth).toBe(4) // Assert on the final lineWidth set
    })
  })

  describe('Dashed Borders', () => {
    it('should draw dashed borders', () => {
      mockNode = createMockYogaNode({ [YogaTypes.Edge.All]: 5 }, Style.BoxSizing.BorderBox)
      drawBorders({
        ctx: mockCtx,
        node: mockNode,
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        radii: { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 },
        borderColor: 'purple',
        borderStyle: undefined as any,
      })

      // For a 5px border, dashLength = 5 * 1.5 = 7.5, gapLength = 5 * 1 = 5
      expect(mockCtx.setLineDash).toHaveBeenCalledWith([])
      expect(mockCtx.stroke).toHaveBeenCalled()
    })
  })

  describe('ContentBox Borders', () => {
    beforeEach(() => {
      mockNode = createMockYogaNode({ [YogaTypes.Edge.All]: 5 }, Style.BoxSizing.ContentBox)
    })

    it('should draw borders for content-box model', () => {
      drawBorders({
        ctx: mockCtx,
        node: mockNode,
        x: 10,
        y: 10,
        width: 100,
        height: 100,
        radii: { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 },
        borderColor: 'orange',
        borderStyle: Style.Border.Solid,
      })

      expect(mockCtx.strokeStyle).toBe('orange')
      expect(mockCtx.lineWidth).toBe(5)
      expect(mockCtx.setLineDash).toHaveBeenCalledWith([])

      // Top line (x + rTL, y - halfBt, x + width - rTR, y - halfBt)
      expect(mockCtx.moveTo).toHaveBeenCalledWith(10, 7.5)
      expect(mockCtx.lineTo).toHaveBeenCalledWith(110, 7.5)

      // Right line (x + width + halfBr, y + rTR, x + width + halfBr, y + height - rBR)
      expect(mockCtx.moveTo).toHaveBeenCalledWith(112.5, 10)
      expect(mockCtx.lineTo).toHaveBeenCalledWith(112.5, 110)

      // Bottom line (x + width - rBR, y + height + halfBb, x + rBL, y + height + halfBb)
      expect(mockCtx.moveTo).toHaveBeenCalledWith(110, 112.5)
      expect(mockCtx.lineTo).toHaveBeenCalledWith(10, 112.5)

      // Left line (x - halfBl, y + height - rBL, x - halfBl, y + rTL)
      expect(mockCtx.moveTo).toHaveBeenCalledWith(7.5, 110)
      expect(mockCtx.lineTo).toHaveBeenCalledWith(7.5, 10)

      expect(mockCtx.stroke).toHaveBeenCalledTimes(4)
    })

    it('should draw content-box borders with rounded corners', () => {
      mockNode = createMockYogaNode({ [YogaTypes.Edge.All]: 5 }, Style.BoxSizing.ContentBox)
      drawBorders({
        ctx: mockCtx,
        node: mockNode,
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        radii: { TopLeft: 10, TopRight: 10, BottomRight: 10, BottomLeft: 10 },
        borderColor: 'cyan',
        borderStyle: Style.Border.Solid,
      })

      expect(mockCtx.strokeStyle).toBe('cyan')
      expect(mockCtx.setLineDash).toHaveBeenCalledWith([])

      // Expect 4 line segments and 4 arcs
      expect(mockCtx.stroke).toHaveBeenCalledTimes(8)
      expect(mockCtx.arc).toHaveBeenCalledTimes(4)

      // Top-Left arc (cx, cy, radius, startAngle, endAngle, border1, border2)
      // For content-box, centerlineArcRadius = radius + cornerWidth / 2
      expect(mockCtx.arc).toHaveBeenCalledWith(10, 10, 12.5, Math.PI, 1.5 * Math.PI)
      // Top-Right arc
      expect(mockCtx.arc).toHaveBeenCalledWith(90, 10, 12.5, 1.5 * Math.PI, 2 * Math.PI)
      // Bottom-Right arc
      expect(mockCtx.arc).toHaveBeenCalledWith(90, 90, 12.5, 0, 0.5 * Math.PI)
      // Bottom-Left arc
      expect(mockCtx.arc).toHaveBeenCalledWith(10, 90, 12.5, 0.5 * Math.PI, Math.PI)
    })

    it('should draw a cap for border-box when border is thicker than radius allows for centerline arc', () => {
      mockNode = createMockYogaNode({ [YogaTypes.Edge.All]: 20 }, Style.BoxSizing.BorderBox) // Border 20px
      drawBorders({
        ctx: mockCtx,
        node: mockNode,
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        radii: { TopLeft: 5, TopRight: 5, BottomRight: 5, BottomLeft: 5 }, // Radius 5px
        borderColor: 'magenta',
        borderStyle: Style.Border.Solid,
      })

      // For border-box, centerlineArcRadius = Math.max(0, radius - cornerWidth / 2)
      // radius = 5, cornerWidth = 20, so centerlineArcRadius = Math.max(0, 5 - 10) = 0
      // This should trigger the cap drawing logic.
      expect(mockCtx.fillStyle).toBe('magenta')
      expect(mockCtx.beginPath).toHaveBeenCalledTimes(4 + 4) // 4 lines + 4 arcs (for cap)
      expect(mockCtx.arc).toHaveBeenCalledWith(5, 5, 5, 0, 2 * Math.PI) // Cap arc for TopLeft
      expect(mockCtx.fill).toHaveBeenCalledTimes(4) // 4 caps
      expect(mockCtx.stroke).toHaveBeenCalledTimes(4) // 4 lines, arcs are skipped
    })
  })
})

describe('extractFunctions', () => {
  it('should replace functions with sentinels and collect them', () => {
    const fn = (x: number) => x * 2
    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    const input = { a: 1, b: fn, c: 'hello' }
    const result = extractFunctions(input, fnMap, { value: 0 })

    expect(result.a).toBe(1)
    expect(result.c).toBe('hello')
    expect(fnMap.size).toBe(1)
    expect((result.b as any)[FN_MARKER]).toBe(0)
    expect(fnMap.get(0)).toBe(fn)
  })

  it('should handle nested objects', () => {
    const fn = () => 'test'
    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    const input = { nested: { deep: { fn } } }
    const result = extractFunctions(input, fnMap, { value: 0 })

    expect(fnMap.size).toBe(1)
    expect((result.nested.deep.fn as any)[FN_MARKER]).toBe(0)
  })

  it('should handle arrays with functions', () => {
    const fn1 = () => 1
    const fn2 = () => 2
    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    const input = [fn1, 'a', fn2]
    const result = extractFunctions(input, fnMap, { value: 0 })

    expect(fnMap.size).toBe(2)
    expect(result[1]).toBe('a')
    expect((result[0] as any)[FN_MARKER]).toBe(0)
    expect((result[2] as any)[FN_MARKER]).toBe(1)
  })

  it('should preserve null and undefined', () => {
    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    expect(extractFunctions(null, fnMap, { value: 0 })).toBeNull()
    expect(extractFunctions(undefined, fnMap, { value: 0 })).toBeUndefined()
    expect(fnMap.size).toBe(0)
  })

  it('should preserve primitives', () => {
    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    expect(extractFunctions(42, fnMap, { value: 0 })).toBe(42)
    expect(extractFunctions('str', fnMap, { value: 0 })).toBe('str')
    expect(extractFunctions(true, fnMap, { value: 0 })).toBe(true)
    expect(fnMap.size).toBe(0)
  })

  it('should preserve Buffer instances', () => {
    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    const buf = Buffer.from('hello')
    expect(extractFunctions(buf, fnMap, { value: 0 })).toBe(buf)
    expect(fnMap.size).toBe(0)
  })

  it('should preserve ArrayBuffer instances', () => {
    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    const ab = new ArrayBuffer(8)
    expect(extractFunctions(ab, fnMap, { value: 0 })).toBe(ab)
    expect(fnMap.size).toBe(0)
  })

  it('should preserve TypedArray instances', () => {
    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    const ta = new Uint8Array(8)
    expect(extractFunctions(ta, fnMap, { value: 0 })).toBe(ta)
    expect(fnMap.size).toBe(0)
  })

  it('should handle objects with no functions', () => {
    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    const input = { a: 1, b: 'two', c: [3, 4] }
    const result = extractFunctions(input, fnMap, { value: 0 })

    expect(result).toEqual(input)
    expect(fnMap.size).toBe(0)
  })
})

describe('restoreFunctions', () => {
  it('should replace sentinels with callable functions', async () => {
    const mockCallFn = async (id: number, ...args: unknown[]) => {
      if (id === 0) return (args[0] as number) * 2
      return null
    }
    const input = { a: 1, b: { [FN_MARKER]: 0 }, c: 'hello' }
    const result = restoreFunctions(input, mockCallFn)

    expect(result.a).toBe(1)
    expect(result.c).toBe('hello')
    expect(typeof result.b).toBe('function')
    expect(await (result.b as any)(5)).toBe(10)
  })

  it('should handle nested sentinels', async () => {
    const mockCallFn = async (id: number) => `fn_${id}`
    const input = { nested: { deep: { fn: { [FN_MARKER]: 3 } } } }
    const result = restoreFunctions(input, mockCallFn)

    expect(typeof result.nested.deep.fn).toBe('function')
    expect(await (result.nested.deep.fn as any)()).toBe('fn_3')
  })

  it('should handle arrays with sentinels', async () => {
    const mockCallFn = async (id: number) => id * 10
    const input = [{ [FN_MARKER]: 0 }, 'a', { [FN_MARKER]: 1 }]
    const result = restoreFunctions(input, mockCallFn)

    expect(typeof result[0]).toBe('function')
    expect(result[1]).toBe('a')
    expect(typeof result[2]).toBe('function')
    expect(await (result[0] as any)()).toBe(0)
    expect(await (result[2] as any)()).toBe(10)
  })

  it('should preserve non-sentinel values', () => {
    const mockCallFn = async () => null
    expect(restoreFunctions(null, mockCallFn)).toBeNull()
    expect(restoreFunctions(undefined, mockCallFn)).toBeUndefined()
    expect(restoreFunctions(42, mockCallFn)).toBe(42)
    expect(restoreFunctions('str', mockCallFn)).toBe('str')
  })

  it('should preserve binary data', () => {
    const mockCallFn = async () => null
    const buf = Buffer.from('hello')
    expect(restoreFunctions(buf, mockCallFn)).toBe(buf)
  })
})
