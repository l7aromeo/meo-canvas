import { jest } from '@jest/globals'
import { Chart, ChartNode } from '@/canvas/chart.canvas.js'
import { BoxNode } from '@/canvas/layout.canvas.js'
import type { CanvasElement, CartesianChartData, PieChartDataPoint } from '@/canvas/canvas.type.js'
import { Style } from '@/constant/common.const.js'
import type { CanvasRenderingContext2D } from 'skia-canvas'

const createMockContext = () => {
  const ctx = {
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 0,
    font: '',
    globalAlpha: 1,
    lineCap: 'butt',
    lineJoin: 'miter',
    save: jest.fn(),
    restore: jest.fn(),
    beginPath: jest.fn(),
    closePath: jest.fn(),
    moveTo: jest.fn(),
    lineTo: jest.fn(),
    arc: jest.fn(),
    rect: jest.fn(),
    fill: jest.fn(),
    stroke: jest.fn(),
    clip: jest.fn(),
    fillRect: jest.fn(),
    strokeRect: jest.fn(),
    setLineDash: jest.fn(),
    measureText: jest.fn((text: string) => ({
      width: text.length * 8,
      actualBoundingBoxAscent: 10,
      actualBoundingBoxDescent: 3,
      actualBoundingBoxLeft: 0,
      actualBoundingBoxRight: text.length * 8,
      alphabeticBaseline: 0,
      emHeightAscent: 10,
      emHeightDescent: 3,
      fontBoundingBoxAscent: 12,
      fontBoundingBoxDescent: 4,
      hangingBaseline: 8,
      ideographicBaseline: -3,
      lines: [],
    })),
    fillText: jest.fn(),
    strokeText: jest.fn(),
    drawImage: jest.fn(),
    filter: '',
    shadowOffsetX: 0,
    shadowOffsetY: 0,
    shadowBlur: 0,
    shadowColor: '',
    imageSmoothingEnabled: true,
    imageSmoothingQuality: 'high',
    textBaseline: 'alphabetic',
    textAlign: 'start',
    letterSpacing: '',
    wordSpacing: '',
    fontVariant: 'normal',
    createLinearGradient: jest.fn(() => ({ addColorStop: jest.fn() })),
    createRadialGradient: jest.fn(() => ({ addColorStop: jest.fn() })),
    globalCompositeOperation: 'source-over',
  }
  return ctx as unknown as CanvasRenderingContext2D
}

const barData: CartesianChartData = {
  labels: ['Jan', 'Feb', 'Mar'],
  datasets: [
    { label: 'Sales', data: [10, 20, 30], color: '#FF6384' },
    { label: 'Revenue', data: [15, 25, 35], color: '#36A2EB' },
  ],
}

const lineData: CartesianChartData = {
  labels: ['Mon', 'Tue', 'Wed', 'Thu'],
  datasets: [{ label: 'Visitors', data: [100, 200, 150, 300], color: '#4BC0C0' }],
}

const pieData: PieChartDataPoint[] = [
  { label: 'Red', value: 30, color: '#FF0000' },
  { label: 'Blue', value: 50, color: '#0000FF' },
  { label: 'Green', value: 20, color: '#00FF00' },
]

// ---------- 1. Chart factory function ----------

describe('Chart factory function', () => {
  it('should return a CanvasElement with __type "Chart"', () => {
    const descriptor = Chart({ type: 'bar', data: barData })
    expect(descriptor.__type).toBe('Chart')
  })

  it('should pass props through to the descriptor', () => {
    const descriptor = Chart({
      type: 'bar',
      data: barData,
      width: 500,
      height: 400,
      options: { showLabels: false },
    })
    expect(descriptor.__type).toBe('Chart')
    expect((descriptor.props as any).type).toBe('bar')
    expect((descriptor.props as any).data).toBe(barData)
    expect(descriptor.props.width).toBe(500)
    expect(descriptor.props.height).toBe(400)
  })

  it('should return correct descriptor for pie chart', () => {
    const descriptor = Chart({ type: 'pie', data: pieData })
    expect(descriptor.__type).toBe('Chart')
    expect((descriptor.props as any).type).toBe('pie')
    expect((descriptor.props as any).data).toBe(pieData)
  })
})

// ---------- 2. ChartNode construction ----------

describe('ChartNode construction', () => {
  it('should construct with bar data and default width=400, height=300', () => {
    const node = new ChartNode({ type: 'bar', data: barData })
    expect(node.node).toBeDefined()
    expect(node.name).toBe('Chart')
    // Calculate layout to verify defaults
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    const layout = node.node.getComputedLayout()
    expect(layout.width).toBe(400)
    expect(layout.height).toBe(300)
  })

  it('should construct with provided width/height overrides', () => {
    const node = new ChartNode({ type: 'bar', data: barData, width: 800, height: 600 })
    node.processInitialChildren()
    node.node.calculateLayout(800, 600, Style.Direction.LTR)
    const layout = node.node.getComputedLayout()
    expect(layout.width).toBe(800)
    expect(layout.height).toBe(600)
  })

  it('should set default options (showLabels, showLegend, labelFontSize, legendPosition)', () => {
    const node = new ChartNode({ type: 'bar', data: barData })
    // Access chartOptions via rendering behavior: labels and legend should render by default
    const ctx = createMockContext()
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // Default showLabels: true => measureText should be called for labels
    expect(ctx.measureText).toHaveBeenCalled()
    // Default showLegend: true => fillRect should be called for legend boxes
    expect(ctx.fillRect).toHaveBeenCalled()
  })

  it('should warn on dataset length mismatch', () => {
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {})
    const mismatchedData: CartesianChartData = {
      labels: ['Jan', 'Feb'],
      datasets: [{ label: 'Sales', data: [10, 20, 30], color: '#FF6384' }],
    }
    new ChartNode({ type: 'bar', data: mismatchedData })
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('does not match the number of labels'))
    warnSpy.mockRestore()
  })

  it('should warn on invalid pie data', () => {
    const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {})
    // Force non-array data for pie chart
    new ChartNode({ type: 'pie', data: 'not-an-array' as any })
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('expects an array'))
    warnSpy.mockRestore()
  })
})

// ---------- 3. Bar chart rendering ----------

describe('ChartNode rendering - bar chart', () => {
  it('should call fillRect for bars', () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'bar', data: barData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // Each label x each dataset = 3 labels * 2 datasets = 6 bar fillRects
    // Plus legend colored boxes (2 datasets)
    // fillRect is called for bars and legend items
    expect(ctx.fillRect).toHaveBeenCalled()
    const fillRectCalls = (ctx.fillRect as jest.Mock).mock.calls
    // At least 6 calls for bar rects (3 labels * 2 datasets)
    expect(fillRectCalls.length).toBeGreaterThanOrEqual(6)
  })

  it('should render labels when showLabels is true', () => {
    const ctx = createMockContext()
    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: { showLabels: true },
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // measureText is called for labels and legend layout
    expect(ctx.measureText).toHaveBeenCalled()
  })

  it('should not render labels when showLabels is false', () => {
    const ctx = createMockContext()
    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: { showLabels: false, showLegend: false },
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // fillRect still called for bars (6 calls), but no legend boxes
    const fillRectCalls = (ctx.fillRect as jest.Mock).mock.calls
    expect(fillRectCalls.length).toBe(6) // only bars, no legend boxes
  })
})

// ---------- 4. Line chart rendering ----------

describe('ChartNode rendering - line chart', () => {
  it('should call arc for data points and stroke for lines', () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'line', data: lineData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // arc is called for each data point (4 points in 1 dataset)
    expect(ctx.arc).toHaveBeenCalled()
    const arcCalls = (ctx.arc as jest.Mock).mock.calls
    expect(arcCalls.length).toBeGreaterThanOrEqual(4)

    // stroke is called for the line path + slice borders (at least once for the line)
    expect(ctx.stroke).toHaveBeenCalled()
  })

  it('should call moveTo for the first point and lineTo for subsequent points', () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'line', data: lineData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    expect(ctx.moveTo).toHaveBeenCalled()
    expect(ctx.lineTo).toHaveBeenCalled()
    // 3 lineTo calls for 4 data points (first point uses moveTo)
    const lineToCalls = (ctx.lineTo as jest.Mock).mock.calls
    expect(lineToCalls.length).toBeGreaterThanOrEqual(3)
  })
})

// ---------- 5. Pie chart rendering ----------

describe('ChartNode rendering - pie chart', () => {
  it('should call arc for each slice and fill for each', () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'pie', data: pieData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // arc called once per slice (3 slices)
    expect(ctx.arc).toHaveBeenCalled()
    const arcCalls = (ctx.arc as jest.Mock).mock.calls
    expect(arcCalls.length).toBeGreaterThanOrEqual(3)

    // fill called for each slice
    expect(ctx.fill).toHaveBeenCalled()
    const fillCalls = (ctx.fill as jest.Mock).mock.calls
    expect(fillCalls.length).toBeGreaterThanOrEqual(3)
  })

  it('should call closePath for each pie slice', () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'pie', data: pieData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    expect(ctx.closePath).toHaveBeenCalled()
    const closePathCalls = (ctx.closePath as jest.Mock).mock.calls
    expect(closePathCalls.length).toBeGreaterThanOrEqual(3)
  })
})

// ---------- 6. Doughnut chart rendering ----------

describe('ChartNode rendering - doughnut chart', () => {
  it('should call arc twice per slice (outer and inner radius)', () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'doughnut', data: pieData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // 2 arcs per slice (outer + inner) * 3 slices = 6 arc calls
    const arcCalls = (ctx.arc as jest.Mock).mock.calls
    expect(arcCalls.length).toBeGreaterThanOrEqual(6)
  })

  it('should render with custom innerRadius', () => {
    const ctx = createMockContext()
    const node = new ChartNode({
      type: 'doughnut',
      data: pieData,
      width: 400,
      height: 300,
      options: { innerRadius: 0.3 },
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // Verify arc calls exist — inner radius param differs from default (0.6)
    const arcCalls = (ctx.arc as jest.Mock).mock.calls
    expect(arcCalls.length).toBeGreaterThanOrEqual(6)

    // The inner radius arcs should use the custom 0.3 ratio
    // outer radius = min(chartWidth, chartHeight) / 2 - 10
    // inner radius = outer * 0.3
    // Check that at least one arc has the smaller (inner) radius
    const radii = arcCalls.map((call: any[]) => call[2]) // 3rd arg is radius
    const uniqueRadii = [...new Set(radii)]
    expect(uniqueRadii.length).toBeGreaterThanOrEqual(2) // at least outer and inner
  })
})

// ---------- 7. Pre-computed fields consumption ----------

describe('ChartNode pre-computed fields', () => {
  it('should use _preComputedXAxisLabels instead of formatter', () => {
    const ctx = createMockContext()
    const customLabels = ['January', 'February', 'March']
    const formatterSpy = jest.fn((v: string) => `formatted-${v}`)

    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: {
        showLabels: true,
        showLegend: false,
        xAxisLabelFormatter: formatterSpy,
        _preComputedXAxisLabels: customLabels,
      } as any,
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // The formatter should NOT be called when pre-computed labels exist
    expect(formatterSpy).not.toHaveBeenCalled()
  })

  it('should use _preComputedYAxisLabels instead of formatter', () => {
    const ctx = createMockContext()
    const customYLabels = ['0', '7', '14', '21', '28', '35']
    const yFormatterSpy = jest.fn((v: number) => `y-${v}`)

    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: {
        showLabels: true,
        showLegend: false,
        showYAxis: true,
        grid: { show: true },
        yAxisLabelFormatter: yFormatterSpy,
        _preComputedYAxisLabels: customYLabels,
      } as any,
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // The y-axis formatter should NOT be called when pre-computed labels exist
    expect(yFormatterSpy).not.toHaveBeenCalled()
  })

  it('should use _preComputedLegendItems with buildDescriptorTree', () => {
    const ctx = createMockContext()
    const legendDescriptors: CanvasElement[] = [
      {
        __type: 'Box',
        props: { width: 50, height: 20 },
        children: [{ __type: 'Text', text: 'Legend A', props: {} }],
      },
    ]

    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: {
        showLegend: true,
        _preComputedLegendItems: legendDescriptors,
      } as any,
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)

    // Should not throw; buildDescriptorTree processes the descriptors
    expect(() => node.render(ctx, 0, 0)).not.toThrow()
  })
})

// ---------- 8. Legend rendering ----------

describe('ChartNode legend rendering', () => {
  it('should render default legend with colored boxes and labels', () => {
    const ctx = createMockContext()
    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: { showLegend: true },
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // Legend renders colored boxes via fillRect and labels via measureText
    // Bar rects (6) + legend boxes (2 datasets) = at least 8 fillRect calls
    const fillRectCalls = (ctx.fillRect as jest.Mock).mock.calls
    expect(fillRectCalls.length).toBeGreaterThanOrEqual(8)
  })

  it('should not render legend when showLegend is false', () => {
    const ctx = createMockContext()
    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: { showLegend: false, showLabels: false },
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // Only bar rects (6), no legend
    const fillRectCalls = (ctx.fillRect as jest.Mock).mock.calls
    expect(fillRectCalls.length).toBe(6)
  })

  it('should render with custom renderLegendItem function', () => {
    const ctx = createMockContext()
    const renderLegendItem = jest.fn(({ color }: any) => {
      return new BoxNode({ width: 60, height: 20, backgroundColor: color })
    })

    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: {
        showLegend: true,
        renderLegendItem,
      },
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // renderLegendItem should be called once per dataset
    expect(renderLegendItem).toHaveBeenCalledTimes(2)
    expect(renderLegendItem).toHaveBeenCalledWith(expect.objectContaining({ index: 0, color: '#FF6384' }))
    expect(renderLegendItem).toHaveBeenCalledWith(expect.objectContaining({ index: 1, color: '#36A2EB' }))
  })
})

// ---------- 9. Grid rendering ----------

describe('ChartNode grid rendering', () => {
  it('should draw grid lines when grid.show is true', () => {
    const ctx = createMockContext()
    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: {
        grid: { show: true, color: '#ccc' },
        showLegend: false,
        showLabels: false,
      },
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // Grid draws 6 horizontal lines (i = 0 to 5)
    expect(ctx.beginPath).toHaveBeenCalled()
    expect(ctx.moveTo).toHaveBeenCalled()
    expect(ctx.lineTo).toHaveBeenCalled()
    expect(ctx.stroke).toHaveBeenCalled()

    const moveToCalls = (ctx.moveTo as jest.Mock).mock.calls
    const lineToCalls = (ctx.lineTo as jest.Mock).mock.calls
    // 6 grid lines = 6 moveTo + 6 lineTo
    expect(moveToCalls.length).toBeGreaterThanOrEqual(6)
    expect(lineToCalls.length).toBeGreaterThanOrEqual(6)
  })

  it('should apply dashed grid style', () => {
    const ctx = createMockContext()
    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: {
        grid: { show: true, style: 'dashed' },
        showLegend: false,
        showLabels: false,
      },
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // setLineDash should be called with [5, 5] for dashed style, then reset with []
    expect(ctx.setLineDash).toHaveBeenCalledWith([5, 5])
    expect(ctx.setLineDash).toHaveBeenCalledWith([])
  })

  it('should apply dotted grid style', () => {
    const ctx = createMockContext()
    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: {
        grid: { show: true, style: 'dotted' },
        showLegend: false,
        showLabels: false,
      },
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    expect(ctx.setLineDash).toHaveBeenCalledWith([2, 2])
    expect(ctx.setLineDash).toHaveBeenCalledWith([])
  })

  it('should not draw grid when grid.show is false or absent', () => {
    const ctx = createMockContext()
    const node = new ChartNode({
      type: 'bar',
      data: barData,
      width: 400,
      height: 300,
      options: { showLegend: false, showLabels: false },
    })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    node.render(ctx, 0, 0)

    // No setLineDash calls since grid is not enabled
    expect(ctx.setLineDash).not.toHaveBeenCalled()
  })
})
