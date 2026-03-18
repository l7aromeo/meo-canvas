import { drawBorders, drawRoundedRectPath, parseBorderRadius, parsePercentage, WorkerPreProcessor } from '@/canvas/canvas.helper.js'
import type { CanvasElement, RootProps } from '@/canvas/canvas.type.js'
import * as YogaTypes from 'yoga-layout'
import { Style } from '@/constant/common.const.js'
import type { CanvasRenderingContext2D } from 'skia-canvas'
import { jest } from '@jest/globals'

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
    mockCtx.save = jest.fn()
    mockCtx.restore = jest.fn()
    mockCtx.beginPath = jest.fn()
    mockCtx.closePath = jest.fn()
    mockCtx.moveTo = jest.fn()
    mockCtx.lineTo = jest.fn()
    mockCtx.arc = jest.fn()
    mockCtx.rect = jest.fn()
    mockCtx.stroke = jest.fn()
    mockCtx.fill = jest.fn()
    mockCtx.clip = jest.fn()
    mockCtx.setLineDash = jest.fn()
  })

  it('should draw a simple rectangle if width or height is zero or negative', () => {
    drawRoundedRectPath(mockCtx, 0, 0, 0, 100, { TopLeft: 10, TopRight: 10, BottomRight: 10, BottomLeft: 10 })
    expect(mockCtx.beginPath).toHaveBeenCalledTimes(1)
    expect(mockCtx.rect).toHaveBeenCalledWith(0, 0, 0, 100)
    expect(mockCtx.moveTo).not.toHaveBeenCalled()
    expect(mockCtx.arc).not.toHaveBeenCalled()
    expect(mockCtx.closePath).not.toHaveBeenCalled() // closePath is not called in this branch

    jest.clearAllMocks()

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
    mockCtx.save = jest.fn()
    mockCtx.restore = jest.fn()
    mockCtx.beginPath = jest.fn()
    mockCtx.closePath = jest.fn()
    mockCtx.moveTo = jest.fn()
    mockCtx.lineTo = jest.fn()
    mockCtx.arc = jest.fn()
    mockCtx.rect = jest.fn()
    mockCtx.stroke = jest.fn()
    mockCtx.fill = jest.fn()
    mockCtx.clip = jest.fn()
    mockCtx.setLineDash = jest.fn()

    mockNode = createMockYogaNode()
    mockNode.getBorder = jest.fn(() => 0)
    mockNode.getBoxSizing = jest.fn(() => Style.BoxSizing.BorderBox)
    mockNode.getComputedPadding = jest.fn(() => 0)
    mockNode.getComputedBorder = jest.fn(() => 0)
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

describe('WorkerPreProcessor', () => {
  const CHART_COLORS = ['#FF6384', '#36A2EB', '#FFCE56', '#4BC0C0', '#9966FF', '#FF9F40', '#C9CBCF']

  const makeRootProps = (children?: any): RootProps => ({
    width: 800,
    height: 600,
    children,
  })

  const makeCartesianChartDesc = (type: 'bar' | 'line', options: Record<string, any> = {}): CanvasElement => ({
    __type: 'Chart',
    props: {
      type,
      data: {
        labels: ['A', 'B', 'C'],
        datasets: [{ label: 'DS1', data: [10, 20, 30] }],
      },
      options,
    } as any,
  })

  const makePieChartDesc = (type: 'pie' | 'doughnut', options: Record<string, any> = {}): CanvasElement => ({
    __type: 'Chart',
    props: {
      type,
      data: [
        { label: 'Slice1', value: 10, color: '#AAA' },
        { label: 'Slice2', value: 20 },
      ],
      options,
    } as any,
  })

  describe('process()', () => {
    it('should return props unchanged (minus functions) when there are no children', () => {
      const fn = () => 'hello'
      const props = makeRootProps()
      ;(props as any).someFn = fn
      const result = WorkerPreProcessor.process(props)

      expect(result.width).toBe(800)
      expect(result.height).toBe(600)
      expect((result as any).someFn).toBeUndefined()
    })

    it('should recursively process non-Chart children and strip functions', () => {
      const boxDesc: CanvasElement = {
        __type: 'Box',
        props: { someFn: () => 'x' } as any,
        children: [{ __type: 'Box', props: { anotherFn: () => 'y' } as any }],
      }
      const props = makeRootProps([boxDesc])
      const result = WorkerPreProcessor.process(props)

      const children = result.children as CanvasElement[]
      expect(children).toHaveLength(1)
      expect((children[0] as any).props.someFn).toBeUndefined()
      expect(((children[0] as any).children[0] as any).props.anotherFn).toBeUndefined()
    })

    it('should pre-compute _preComputedXAxisLabels from xAxisLabelFormatter for bar chart', () => {
      const chartDesc = makeCartesianChartDesc('bar', {
        xAxisLabelFormatter: (v: string, i: number) => `${v}-${i}`,
      })
      const props = makeRootProps([chartDesc])
      const result = WorkerPreProcessor.process(props)

      const children = result.children as CanvasElement[]
      const chartProps = (children[0] as any).props
      expect(chartProps.options._preComputedXAxisLabels).toEqual(['A-0', 'B-1', 'C-2'])
      expect(chartProps.options.xAxisLabelFormatter).toBeUndefined()
    })

    it('should pre-compute _preComputedYAxisLabels from yAxisLabelFormatter for bar chart', () => {
      const chartDesc = makeCartesianChartDesc('bar', {
        yAxisLabelFormatter: (v: number) => `${v}k`,
      })
      const props = makeRootProps([chartDesc])
      const result = WorkerPreProcessor.process(props)

      const children = result.children as CanvasElement[]
      const chartProps = (children[0] as any).props
      // maxValue = 30, ticks: 30, 24, 18, 12, 6, 0
      expect(chartProps.options._preComputedYAxisLabels).toEqual(['30k', '24k', '18k', '12k', '6k', '0k'])
      expect(chartProps.options.yAxisLabelFormatter).toBeUndefined()
    })

    it('should pre-compute _preComputedLegendItems from renderLegendItem for bar chart', () => {
      const legendNode: CanvasElement = { __type: 'Text', text: 'legend', props: {} }
      const chartDesc = makeCartesianChartDesc('bar', {
        renderLegendItem: (_props: any) => legendNode,
      })
      const props = makeRootProps([chartDesc])
      const result = WorkerPreProcessor.process(props)

      const children = result.children as CanvasElement[]
      const chartProps = (children[0] as any).props
      expect(chartProps.options._preComputedLegendItems).toHaveLength(1)
      expect(chartProps.options._preComputedLegendItems[0]).toEqual(legendNode)
      expect(chartProps.options.renderLegendItem).toBeUndefined()
    })

    it('should use generateColor palette for legend items without color (bar)', () => {
      const chartDesc: CanvasElement = {
        __type: 'Chart',
        props: {
          type: 'bar',
          data: {
            labels: ['A'],
            datasets: [
              { label: 'DS1', data: [10] },
              { label: 'DS2', data: [20] },
            ],
          },
          options: {
            renderLegendItem: ({ color }: any) => ({ __type: 'Text', text: color }),
          },
        } as any,
      }
      const props = makeRootProps([chartDesc])
      const result = WorkerPreProcessor.process(props)

      const children = result.children as CanvasElement[]
      const items = (children[0] as any).props.options._preComputedLegendItems
      expect(items[0]).toEqual({ __type: 'Text', text: CHART_COLORS[0] })
      expect(items[1]).toEqual({ __type: 'Text', text: CHART_COLORS[1] })
    })

    it('should pre-compute _preComputedLegendItems from renderLegendItem for pie chart', () => {
      const chartDesc = makePieChartDesc('pie', {
        renderLegendItem: ({ item, color }: any) => ({ __type: 'Text', text: `${item.label}-${color}` }),
      })
      const props = makeRootProps([chartDesc])
      const result = WorkerPreProcessor.process(props)

      const children = result.children as CanvasElement[]
      const items = (children[0] as any).props.options._preComputedLegendItems
      expect(items).toHaveLength(2)
      // First item has color '#AAA', second uses generated color
      expect(items[0]).toEqual({ __type: 'Text', text: 'Slice1-#AAA' })
      expect(items[1]).toEqual({ __type: 'Text', text: `Slice2-${CHART_COLORS[1]}` })
    })

    it('should pre-compute _preComputedLabelItems from renderLabelItem for bar chart', () => {
      const chartDesc = makeCartesianChartDesc('line', {
        renderLabelItem: ({ item, index }: any) => ({ __type: 'Text', text: `${item}:${index}` }),
      })
      const props = makeRootProps([chartDesc])
      const result = WorkerPreProcessor.process(props)

      const children = result.children as CanvasElement[]
      const items = (children[0] as any).props.options._preComputedLabelItems
      expect(items).toEqual([
        { __type: 'Text', text: 'A:0' },
        { __type: 'Text', text: 'B:1' },
        { __type: 'Text', text: 'C:2' },
      ])
      expect((children[0] as any).props.options.renderLabelItem).toBeUndefined()
    })

    it('should pre-compute _preComputedLabelItems from renderLabelItem for pie chart', () => {
      const chartDesc = makePieChartDesc('doughnut', {
        renderLabelItem: ({ item }: any) => ({ __type: 'Text', text: `${item.label}` }),
      })
      const props = makeRootProps([chartDesc])
      const result = WorkerPreProcessor.process(props)

      const children = result.children as CanvasElement[]
      const items = (children[0] as any).props.options._preComputedLabelItems
      expect(items).toEqual([
        { __type: 'Text', text: 'Slice1' },
        { __type: 'Text', text: 'Slice2' },
      ])
    })

    it('should pre-compute _preComputedValueItems from renderValueItem for bar chart', () => {
      const chartDesc: CanvasElement = {
        __type: 'Chart',
        props: {
          type: 'bar',
          data: {
            labels: ['A', 'B'],
            datasets: [
              { label: 'DS1', data: [10, 20] },
              { label: 'DS2', data: [30, 40] },
            ],
          },
          options: {
            renderValueItem: ({ item, index, datasetIndex }: any) => ({ __type: 'Text', text: `${datasetIndex}-${index}-${item}` }),
          },
        } as any,
      }
      const props = makeRootProps([chartDesc])
      const result = WorkerPreProcessor.process(props)

      const children = result.children as CanvasElement[]
      const items = (children[0] as any).props.options._preComputedValueItems
      expect(items).toEqual([
        [
          { __type: 'Text', text: '0-0-10' },
          { __type: 'Text', text: '0-1-20' },
        ],
        [
          { __type: 'Text', text: '1-0-30' },
          { __type: 'Text', text: '1-1-40' },
        ],
      ])
      expect((children[0] as any).props.options.renderValueItem).toBeUndefined()
    })

    it('should ignore xAxisLabelFormatter for non-cartesian chart types', () => {
      const chartDesc = makePieChartDesc('pie', {
        xAxisLabelFormatter: (v: string, i: number) => `${v}-${i}`,
      })
      const props = makeRootProps([chartDesc])
      const result = WorkerPreProcessor.process(props)

      const children = result.children as CanvasElement[]
      const chartProps = (children[0] as any).props
      // Should not have pre-computed labels since pie charts don't support axis formatters
      expect(chartProps.options._preComputedXAxisLabels).toBeUndefined()
      // The function should be stripped by stripNonSerializable
      expect(chartProps.options.xAxisLabelFormatter).toBeUndefined()
    })
  })

  describe('stripNonSerializable()', () => {
    it('should preserve primitives', () => {
      expect(WorkerPreProcessor.stripNonSerializable('hello')).toBe('hello')
      expect(WorkerPreProcessor.stripNonSerializable(42)).toBe(42)
      expect(WorkerPreProcessor.stripNonSerializable(true)).toBe(true)
    })

    it('should return null and undefined as-is', () => {
      expect(WorkerPreProcessor.stripNonSerializable(null)).toBeNull()
      expect(WorkerPreProcessor.stripNonSerializable(undefined)).toBeUndefined()
    })

    it('should strip functions', () => {
      expect(WorkerPreProcessor.stripNonSerializable(() => 'x')).toBeUndefined()
    })

    it('should strip symbols', () => {
      expect(WorkerPreProcessor.stripNonSerializable(Symbol('test'))).toBeUndefined()
    })

    it('should strip function values from objects', () => {
      const obj = { a: 1, b: 'hello', fn: () => 'x', sym: Symbol('s') }
      const result = WorkerPreProcessor.stripNonSerializable(obj)
      expect(result).toEqual({ a: 1, b: 'hello' })
      expect(result).not.toHaveProperty('fn')
      expect(result).not.toHaveProperty('sym')
    })

    it('should recursively strip from nested objects', () => {
      const obj = { a: { b: { fn: () => 'x', c: 3 } } }
      const result = WorkerPreProcessor.stripNonSerializable(obj)
      expect(result).toEqual({ a: { b: { c: 3 } } })
    })

    it('should handle arrays and strip functions within them', () => {
      const arr = [1, () => 'x', 'hello', { fn: () => 'y', a: 2 }]
      const result = WorkerPreProcessor.stripNonSerializable(arr)
      expect(result).toEqual([1, undefined, 'hello', { a: 2 }])
    })

    it('should preserve Buffer instances', () => {
      const buf = Buffer.from('hello')
      const result = WorkerPreProcessor.stripNonSerializable(buf)
      expect(Buffer.isBuffer(result)).toBe(true)
      expect(result).toBe(buf)
    })

    it('should preserve ArrayBuffer instances', () => {
      const ab = new ArrayBuffer(8)
      const result = WorkerPreProcessor.stripNonSerializable(ab)
      expect(result).toBe(ab)
      expect(result instanceof ArrayBuffer).toBe(true)
    })

    it('should preserve TypedArray instances', () => {
      const ta = new Uint8Array([1, 2, 3])
      const result = WorkerPreProcessor.stripNonSerializable(ta)
      expect(result).toBe(ta)
      expect(ArrayBuffer.isView(result)).toBe(true)
    })
  })

  describe('nodeToDescriptor()', () => {
    // Access private method via bracket notation for testing
    const nodeToDescriptor = (node: any) => (WorkerPreProcessor as any).nodeToDescriptor(node)

    it('should return CanvasElement as-is if it has __type', () => {
      const desc: CanvasElement = { __type: 'Box', props: {} as any }
      expect(nodeToDescriptor(desc)).toBe(desc)
    })

    it('should return null for class instances', () => {
      class MyClass {
        value = 1
      }
      expect(nodeToDescriptor(new MyClass())).toBeNull()
    })

    it('should return null for null input', () => {
      expect(nodeToDescriptor(null)).toBeNull()
    })

    it('should return undefined for undefined input', () => {
      expect(nodeToDescriptor(undefined)).toBeUndefined()
    })

    it('should return plain objects without __type as-is', () => {
      const plain = { foo: 'bar' }
      expect(nodeToDescriptor(plain)).toBe(plain)
    })
  })
})
