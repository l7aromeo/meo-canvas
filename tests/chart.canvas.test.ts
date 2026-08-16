import { vi, type MockInstance } from 'vitest'
import { Chart, ChartNode } from '@/canvas/chart.canvas.js'
import { BoxNode } from '@/canvas/layout.canvas.js'
import { extractFunctions, restoreFunctions } from '@/worker/comlink.pool.js'
import type { CartesianChartData, PieChartDataPoint } from '@/canvas/canvas.type.js'
import { Style } from '@/constant/common.const.js'
import { invalidateTextMeasurements } from '@/canvas/text.metrics.js'
import type { CanvasRenderingContext2D } from 'meo-skia-canvas'

/**
 * Measurements are cached across the process, and every mock context here reports the same font
 * state — so the second test to measure `"Q1"` would be answered from what the first one measured
 * and never reach its own spy. Retiring them between tests keeps each one measuring for itself,
 * which is also what a fresh process does.
 */
beforeEach(() => invalidateTextMeasurements())

const createMockContext = () => {
  const ctx = {
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 0,
    font: '',
    globalAlpha: 1,
    lineCap: 'butt',
    lineJoin: 'miter',
    save: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    closePath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    arc: vi.fn(),
    rect: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    clip: vi.fn(),
    fillRect: vi.fn(),
    strokeRect: vi.fn(),
    setLineDash: vi.fn(),
    measureText: vi.fn((text: string) => ({
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
    fillText: vi.fn(),
    strokeText: vi.fn(),
    drawImage: vi.fn(),
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
    createLinearGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    createRadialGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
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
    expect((descriptor.props as any).width).toBe(500)
    expect((descriptor.props as any).height).toBe(400)
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

  it('should set default options (showLabels, showLegend, labelFontSize, legendPosition)', async () => {
    const node = new ChartNode({ type: 'bar', data: barData })
    // Access chartOptions via rendering behavior: labels and legend should render by default
    const ctx = createMockContext()
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    await node.render(ctx, 0, 0)

    // Default showLabels: true => measureText should be called for labels
    expect(ctx.measureText).toHaveBeenCalled()
    // Default showLegend: true => fillRect should be called for legend boxes
    expect(ctx.fillRect).toHaveBeenCalled()
  })

  it('should warn on dataset length mismatch', () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const mismatchedData: CartesianChartData = {
      labels: ['Jan', 'Feb'],
      datasets: [{ label: 'Sales', data: [10, 20, 30], color: '#FF6384' }],
    }
    new ChartNode({ type: 'bar', data: mismatchedData })
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('does not match the number of labels'))
    warnSpy.mockRestore()
  })

  it('should warn on invalid pie data', () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    // Force non-array data for pie chart
    new ChartNode({ type: 'pie', data: 'not-an-array' as any })
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('expects an array'))
    warnSpy.mockRestore()
  })
})

// ---------- 3. Bar chart rendering ----------

describe('ChartNode rendering - bar chart', () => {
  it('should call fillRect for bars', async () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'bar', data: barData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    await node.render(ctx, 0, 0)

    // Each label x each dataset = 3 labels * 2 datasets = 6 bar fillRects
    // Plus legend colored boxes (2 datasets)
    // fillRect is called for bars and legend items
    expect(ctx.fillRect).toHaveBeenCalled()
    const fillRectCalls = (ctx.fillRect as unknown as MockInstance).mock.calls
    // At least 6 calls for bar rects (3 labels * 2 datasets)
    expect(fillRectCalls.length).toBeGreaterThanOrEqual(6)
  })

  it('should render labels when showLabels is true', async () => {
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
    await node.render(ctx, 0, 0)

    // measureText is called for labels and legend layout
    expect(ctx.measureText).toHaveBeenCalled()
  })

  it('should not render labels when showLabels is false', async () => {
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
    await node.render(ctx, 0, 0)

    // fillRect still called for bars (6 calls), but no legend boxes
    const fillRectCalls = (ctx.fillRect as unknown as MockInstance).mock.calls
    expect(fillRectCalls.length).toBe(6) // only bars, no legend boxes
  })
})

// ---------- 4. Line chart rendering ----------

describe('ChartNode rendering - line chart', () => {
  it('should call arc for data points and stroke for lines', async () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'line', data: lineData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    await node.render(ctx, 0, 0)

    // arc is called for each data point (4 points in 1 dataset)
    expect(ctx.arc).toHaveBeenCalled()
    const arcCalls = (ctx.arc as unknown as MockInstance).mock.calls
    expect(arcCalls.length).toBeGreaterThanOrEqual(4)

    // stroke is called for the line path + slice borders (at least once for the line)
    expect(ctx.stroke).toHaveBeenCalled()
  })

  it('should call moveTo for the first point and lineTo for subsequent points', async () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'line', data: lineData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    await node.render(ctx, 0, 0)

    expect(ctx.moveTo).toHaveBeenCalled()
    expect(ctx.lineTo).toHaveBeenCalled()
    // 3 lineTo calls for 4 data points (first point uses moveTo)
    const lineToCalls = (ctx.lineTo as unknown as MockInstance).mock.calls
    expect(lineToCalls.length).toBeGreaterThanOrEqual(3)
  })
})

// ---------- 5. Pie chart rendering ----------

describe('ChartNode rendering - pie chart', () => {
  it('should call arc for each slice and fill for each', async () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'pie', data: pieData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    await node.render(ctx, 0, 0)

    // arc called once per slice (3 slices)
    expect(ctx.arc).toHaveBeenCalled()
    const arcCalls = (ctx.arc as unknown as MockInstance).mock.calls
    expect(arcCalls.length).toBeGreaterThanOrEqual(3)

    // fill called for each slice
    expect(ctx.fill).toHaveBeenCalled()
    const fillCalls = (ctx.fill as unknown as MockInstance).mock.calls
    expect(fillCalls.length).toBeGreaterThanOrEqual(3)
  })

  it('should call closePath for each pie slice', async () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'pie', data: pieData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    await node.render(ctx, 0, 0)

    expect(ctx.closePath).toHaveBeenCalled()
    const closePathCalls = (ctx.closePath as unknown as MockInstance).mock.calls
    expect(closePathCalls.length).toBeGreaterThanOrEqual(3)
  })
})

// ---------- 6. Doughnut chart rendering ----------

describe('ChartNode rendering - doughnut chart', () => {
  it('should call arc twice per slice (outer and inner radius)', async () => {
    const ctx = createMockContext()
    const node = new ChartNode({ type: 'doughnut', data: pieData, width: 400, height: 300 })
    node.processInitialChildren()
    node.node.calculateLayout(400, 300, Style.Direction.LTR)
    await node.render(ctx, 0, 0)

    // 2 arcs per slice (outer + inner) * 3 slices = 6 arc calls
    const arcCalls = (ctx.arc as unknown as MockInstance).mock.calls
    expect(arcCalls.length).toBeGreaterThanOrEqual(6)
  })

  it('should render with custom innerRadius', async () => {
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
    await node.render(ctx, 0, 0)

    // Verify arc calls exist — inner radius param differs from default (0.6)
    const arcCalls = (ctx.arc as unknown as MockInstance).mock.calls
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

// ---------- 7. Legend rendering ----------

describe('ChartNode legend rendering', () => {
  it('should render default legend with colored boxes and labels', async () => {
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
    await node.render(ctx, 0, 0)

    // Legend renders colored boxes via fillRect and labels via measureText
    // Bar rects (6) + legend boxes (2 datasets) = at least 8 fillRect calls
    const fillRectCalls = (ctx.fillRect as unknown as MockInstance).mock.calls
    expect(fillRectCalls.length).toBeGreaterThanOrEqual(8)
  })

  it('should not render legend when showLegend is false', async () => {
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
    await node.render(ctx, 0, 0)

    // Only bar rects (6), no legend
    const fillRectCalls = (ctx.fillRect as unknown as MockInstance).mock.calls
    expect(fillRectCalls.length).toBe(6)
  })

  it('should render with custom renderLegendItem function', async () => {
    const ctx = createMockContext()
    const renderLegendItem = vi.fn(({ color }: any) => {
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
    await node.render(ctx, 0, 0)

    // renderLegendItem should be called once per dataset
    expect(renderLegendItem).toHaveBeenCalledTimes(2)
    expect(renderLegendItem).toHaveBeenCalledWith(expect.objectContaining({ index: 0, color: '#FF6384' }))
    expect(renderLegendItem).toHaveBeenCalledWith(expect.objectContaining({ index: 1, color: '#36A2EB' }))
  })
})

// ---------- 9. Grid rendering ----------

describe('ChartNode grid rendering', () => {
  it('should draw grid lines when grid.show is true', async () => {
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
    await node.render(ctx, 0, 0)

    // Grid draws 6 horizontal lines (i = 0 to 5)
    expect(ctx.beginPath).toHaveBeenCalled()
    expect(ctx.moveTo).toHaveBeenCalled()
    expect(ctx.lineTo).toHaveBeenCalled()
    expect(ctx.stroke).toHaveBeenCalled()

    const moveToCalls = (ctx.moveTo as unknown as MockInstance).mock.calls
    const lineToCalls = (ctx.lineTo as unknown as MockInstance).mock.calls
    // 6 grid lines = 6 moveTo + 6 lineTo
    expect(moveToCalls.length).toBeGreaterThanOrEqual(6)
    expect(lineToCalls.length).toBeGreaterThanOrEqual(6)
  })

  it('should apply dashed grid style', async () => {
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
    await node.render(ctx, 0, 0)

    // setLineDash should be called with [5, 5] for dashed style, then reset with []
    expect(ctx.setLineDash).toHaveBeenCalledWith([5, 5])
    expect(ctx.setLineDash).toHaveBeenCalledWith([])
  })

  it('should apply dotted grid style', async () => {
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
    await node.render(ctx, 0, 0)

    expect(ctx.setLineDash).toHaveBeenCalledWith([2, 2])
    expect(ctx.setLineDash).toHaveBeenCalledWith([])
  })

  it('should not draw grid when grid.show is false or absent', async () => {
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
    await node.render(ctx, 0, 0)

    // No setLineDash calls since grid is not enabled
    expect(ctx.setLineDash).not.toHaveBeenCalled()
  })
})

describe('Chart function props serialization', () => {
  it('should extract formatter functions from chart descriptor and restore them', async () => {
    const formatter = (value: string, index: number) => (index % 2 === 0 ? value : '')
    const yFormatter = (value: number) => `${value}%`

    const descriptor = Chart({
      type: 'line',
      data: barData,
      options: {
        xAxisLabelFormatter: formatter,
        yAxisLabelFormatter: yFormatter,
        showLegend: true,
        legendPosition: 'bottom',
      },
    })

    // Wrap in a Root-like structure
    const props = { width: 800, children: [descriptor] }

    // Extract — should replace functions with sentinels
    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    const cleaned = extractFunctions(props, fnMap, { value: 0 })

    expect(fnMap.size).toBe(2)
    // Cleaned props should be structured-clone safe (no functions)
    const json = JSON.stringify(cleaned)
    expect(json).toContain('__comlinkFnId')
    expect(json).not.toContain('[Function')

    // Restore — sentinels become callable functions via callback proxy
    const mockCallFn = async (id: number, ...args: unknown[]) => {
      const fn = fnMap.get(id)
      return fn!(...args)
    }
    const restored = restoreFunctions(cleaned, mockCallFn)
    const restoredOptions = (restored.children[0] as any).props.options

    expect(await restoredOptions.xAxisLabelFormatter('Jan', 0)).toBe('Jan')
    expect(await restoredOptions.xAxisLabelFormatter('Feb', 1)).toBe('')
    expect(await restoredOptions.yAxisLabelFormatter(42)).toBe('42%')
  })

  it('should handle chart descriptors with no function props', () => {
    const descriptor = Chart({
      type: 'pie',
      data: pieData,
      options: { showLabels: false, showLegend: true },
    })

    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    const cleaned = extractFunctions({ width: 400, children: [descriptor] }, fnMap, { value: 0 })

    expect(fnMap.size).toBe(0)
    expect(cleaned).toEqual({ width: 400, children: [descriptor] })
  })

  it('should handle renderLegendItem function prop', async () => {
    const renderLegendItem = ({ color }: { item: unknown; color: string }) => new BoxNode({ width: 50, height: 20, backgroundColor: color })

    const descriptor = Chart({
      type: 'doughnut',
      data: pieData,
      options: { renderLegendItem: renderLegendItem as any },
    })

    const fnMap = new Map<number, (...args: unknown[]) => unknown>()
    const cleaned = extractFunctions({ width: 400, children: [descriptor] }, fnMap, { value: 0 })

    expect(fnMap.size).toBe(1)

    const mockCallFn = async (id: number, ...args: unknown[]) => fnMap.get(id)!(...args)
    const restored = restoreFunctions(cleaned, mockCallFn)
    const restoredFn = (restored.children[0] as any).props.options.renderLegendItem

    const result = await restoredFn({ item: { label: 'A', value: 10 }, index: 0, color: '#ff0000' })
    expect(result).toBeInstanceOf(BoxNode)
  })
})
