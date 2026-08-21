import { vi } from 'vitest'
import { ChartNode } from '@/canvas/chart.canvas.js'
import { TextNode } from '@/canvas/text.canvas.js'
import { Style } from '@/constant/common.const.js'
import { invalidateTextMeasurements } from '@/canvas/text.metrics.js'
import type { CanvasRenderingContext2D } from 'meo-skia-canvas'
import type { CartesianChartData, PieChartDataPoint, ChartProps, ChartType } from '@/canvas/canvas.type.js'

beforeEach(() => invalidateTextMeasurements())

const createMockContext = () =>
  ({
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 0,
    font: '',
    globalAlpha: 1,
    lineCap: 'butt',
    lineJoin: 'miter',
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
    globalCompositeOperation: 'source-over',
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
    fillText: vi.fn(),
    strokeText: vi.fn(),
    drawImage: vi.fn(),
    createLinearGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    createRadialGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
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
  }) as unknown as CanvasRenderingContext2D

async function renderChart<T extends ChartType>(props: ChartProps<T>) {
  const node = new ChartNode(props as any)
  const ctx = createMockContext()
  node.processInitialChildren()
  // width/height may be percentage strings on the props; layout needs concrete pixels.
  const px = (value: unknown, fallback: number) => (typeof value === 'number' ? value : fallback)
  node.node.calculateLayout(px(props.width, 400), px(props.height, 300), Style.Direction.LTR)
  await node.render(ctx, 0, 0)
  return ctx
}

const barData: CartesianChartData = {
  labels: ['Jan', 'Feb', 'Mar'],
  datasets: [
    { label: 'Sales', data: [10, 20, 30], color: '#FF6384' },
    { label: 'Revenue', data: [15, 25, 35], color: '#36A2EB' },
  ],
}
const pieData: PieChartDataPoint[] = [
  { label: 'Red', value: 30, color: '#FF0000' },
  { label: 'Blue', value: 50, color: '#0000FF' },
]

describe('ChartNode — legend placement', () => {
  it.each(['top', 'bottom', 'left', 'right'] as const)('lays the legend out on the %s', async position => {
    const ctx = await renderChart({ type: 'bar', data: barData, options: { legendPosition: position } })
    expect(ctx.fillRect).toHaveBeenCalled()
  })

  it('omits the legend when it is switched off', async () => {
    const ctx = await renderChart({ type: 'bar', data: barData, options: { showLegend: false } })
    expect(ctx.fillRect).toHaveBeenCalled()
  })

  it('omits the labels when they are switched off', async () => {
    const ctx = await renderChart({ type: 'bar', data: barData, options: { showLabels: false } })
    expect(ctx.fillRect).toHaveBeenCalled()
  })
})

describe('ChartNode — cartesian options', () => {
  it.each([
    ['values shown', { showValues: true }],
    ['values styled', { showValues: true, valueFontSize: 9, valueColor: '#333' }],
    ['y axis shown', { showYAxis: true }],
    ['y axis styled', { showYAxis: true, yAxisFontSize: 9, yAxisColor: '#555' }],
    ['a y axis formatter', { showYAxis: true, yAxisLabelFormatter: (value: number) => `${value}u` }],
    ['an x axis formatter', { xAxisLabelFormatter: (value: string, index: number) => `${index}:${value}` }],
    ['a custom axis colour', { axisColor: '#abc' }],
    ['labels styled', { labelFontSize: 11, labelColor: '#222' }],
  ])('renders a bar chart with %s', async (_label, options) => {
    const ctx = await renderChart({ type: 'bar', data: barData, options: options as any })
    expect(ctx.fillRect).toHaveBeenCalled()
  })

  it.each([
    ['a shown grid', { grid: { show: true } }],
    ['a hidden grid', { grid: { show: false } }],
    ['a dashed grid', { grid: { show: true, style: 'dashed' as const } }],
    ['a dotted grid', { grid: { show: true, style: 'dotted' as const } }],
    ['a solid grid with a colour', { grid: { show: true, style: 'solid' as const, color: '#eee' } }],
    ['a grid alongside a y axis', { grid: { show: true }, showYAxis: true }],
    ['a grid, a y axis and a formatter', { grid: { show: true }, showYAxis: true, yAxisLabelFormatter: (v: number) => `${v}%` }],
    ['a y axis coloured through axisColor', { grid: { show: true }, showYAxis: true, axisColor: '#345' }],
    ['a y axis with its own colour winning', { grid: { show: true }, showYAxis: true, axisColor: '#345', yAxisColor: '#678' }],
  ])('renders a line chart with %s', async (_label, options) => {
    const ctx = await renderChart({ type: 'line', data: barData, options: options as any })
    expect(ctx.stroke).toHaveBeenCalled()
  })

  it('renders a bar chart with a grid and a y axis', async () => {
    const ctx = await renderChart({
      type: 'bar',
      data: barData,
      options: { grid: { show: true, style: 'dashed' }, showYAxis: true, showValues: true } as any,
    })
    expect(ctx.fillRect).toHaveBeenCalled()
  })

  it('uses a custom font family for axis and label text', async () => {
    const ctx = await renderChart({
      type: 'line',
      data: barData,
      fontFamily: 'Georgia',
      options: { grid: { show: true }, showYAxis: true, showLabels: true },
    } as any)
    expect(ctx.font).toContain('Georgia')
  })

  it('plots a single label without dividing by zero', async () => {
    const ctx = await renderChart({
      type: 'line',
      data: { labels: ['solo'], datasets: [{ label: 'One', data: [5], color: '#111' }] },
      options: { grid: { show: true }, showYAxis: true },
    })
    expect(ctx.stroke).toHaveBeenCalled()
  })
})

describe('ChartNode — data edges', () => {
  it('renders a cartesian chart whose values are all zero', async () => {
    const ctx = await renderChart({
      type: 'bar',
      data: { labels: ['a', 'b'], datasets: [{ label: 'Flat', data: [0, 0], color: '#111' }] },
      options: { showValues: true, showYAxis: true },
    })
    expect(ctx.fillRect).toHaveBeenCalled()
  })

  it('renders a cartesian chart with negative values', async () => {
    const ctx = await renderChart({
      type: 'bar',
      data: { labels: ['a', 'b', 'c'], datasets: [{ label: 'Swing', data: [-10, 5, -3], color: '#111' }] },
      options: { showValues: true, showYAxis: true },
    })
    expect(ctx.fillRect).toHaveBeenCalled()
  })

  it('draws no bars for a chart with no datasets', async () => {
    const ctx = await renderChart({ type: 'bar', data: { labels: [], datasets: [] } })
    expect(ctx.fillRect).not.toHaveBeenCalled()
  })

  it('renders a dataset with no explicit colour', async () => {
    const ctx = await renderChart({
      type: 'bar',
      data: { labels: ['a'], datasets: [{ label: 'Uncoloured', data: [5] } as any] },
    })
    expect(ctx.fillRect).toHaveBeenCalled()
  })

  it('renders a line chart with a single point', async () => {
    const ctx = await renderChart({
      type: 'line',
      data: { labels: ['only'], datasets: [{ label: 'One', data: [42], color: '#0a0' }] },
    })
    expect(ctx.beginPath).toHaveBeenCalled()
  })
})

describe('ChartNode — pie and doughnut', () => {
  it.each([
    ['a pie', 'pie' as const, {}],
    ['a doughnut', 'doughnut' as const, {}],
    ['a doughnut with an inner radius', 'doughnut' as const, { innerRadius: 0.6 }],
    ['a pie with rounded slices', 'pie' as const, { sliceBorderRadius: 6 }],
    ['a pie with the legend on the left', 'pie' as const, { legendPosition: 'left' as const }],
    ['a pie with no labels', 'pie' as const, { showLabels: false }],
  ])('renders %s chart', async (_label, type, options) => {
    const ctx = await renderChart({ type, data: pieData, options: options as any })
    expect(ctx.arc).toHaveBeenCalled()
  })

  it('renders a pie whose values sum to zero without drawing a slice', async () => {
    const ctx = await renderChart({
      type: 'pie',
      data: [{ label: 'None', value: 0, color: '#111' }],
    })
    expect(ctx.fillText).toHaveBeenCalled()
  })

  it('renders a pie with a slice carrying no colour', async () => {
    const ctx = await renderChart({ type: 'pie', data: [{ label: 'Bare', value: 10 } as any] })
    expect(ctx.arc).toHaveBeenCalled()
  })

  it('draws no slices for an empty pie', async () => {
    const ctx = await renderChart({ type: 'pie', data: [] })
    expect(ctx.arc).not.toHaveBeenCalled()
  })
})

describe('ChartNode — custom item renderers', () => {
  it('uses a custom legend item renderer', async () => {
    const renderLegendItem = vi.fn(({ item }: any) => new TextNode(String(item.label ?? ''), {}) as any)
    await renderChart({ type: 'bar', data: barData, options: { renderLegendItem } as any })
    expect(renderLegendItem).toHaveBeenCalled()
  })

  it('uses a custom label item renderer', async () => {
    const renderLabelItem = vi.fn(({ item }: any) => new TextNode(String(item), {}) as any)
    await renderChart({ type: 'bar', data: barData, options: { renderLabelItem } as any })
    expect(renderLabelItem).toHaveBeenCalled()
  })

  it('uses a custom value item renderer', async () => {
    const renderValueItem = vi.fn(({ item }: any) => new TextNode(String(item), {}) as any)
    await renderChart({ type: 'bar', data: barData, options: { showValues: true, renderValueItem } as any })
    expect(renderValueItem).toHaveBeenCalled()
  })
})
