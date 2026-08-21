import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Chart } from '@/canvas/chart.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { integrationFontFamily, integrationRootBase } from './helpers/integration-font.js'
import type { CartesianChartData, PieChartDataPoint } from '@/canvas/canvas.type.js'

/**
 * `ChartItem` permits a descriptor as well as a node, and a descriptor is the only one of the two a
 * consumer can produce: `ChartNode` is not exported at all and `BoxNode` is exported as a type. So
 * the descriptor arm is not a convenience — it is the whole of the public contract.
 *
 * What makes it work is `withBuiltChartItems`, which wraps these callbacks in `buildTree` before
 * `ChartNode` is constructed; `built` inside the chart is then a narrowing cast rather than a
 * conversion. Nothing enforced that arrangement, and it is easy to mistake for a bug from inside
 * the package, where `new ChartNode(props)` skips the wrapper and throws on the descriptor. These
 * assertions go through `Root`, which is the only way a consumer can reach a chart.
 */
const WIDTH = 320
const HEIGHT = 240

const cartesian: CartesianChartData = {
  labels: ['Jan', 'Feb'],
  datasets: [{ label: 'Sales', data: [10, 20], color: '#0066cc' }],
}

const pie: PieChartDataPoint[] = [
  { label: 'Red', value: 30, color: '#cc0000' },
  { label: 'Blue', value: 70, color: '#0000cc' },
]

const label = (text: string) => Text(text, { fontFamily: integrationFontFamily, fontSize: 10, color: '#000' })

describe('chart item callbacks returning descriptors', () => {
  it('builds a descriptor from every cartesian item callback', async () => {
    const seen = { legend: 0, label: 0, value: 0 }

    const canvas = await Root({
      ...integrationRootBase,
      width: WIDTH,
      height: HEIGHT,
      workerMode: false,
      gpu: false,
      backgroundColor: '#ffffff',
      children: [
        Chart({
          type: 'bar',
          width: '100%',
          height: '100%',
          data: cartesian,
          options: {
            showValues: true,
            // A Box wrapping a Text, and a bare Text: both descriptors, neither a node.
            renderLegendItem: ({ item }) => {
              seen.legend++
              return Box({ children: [label(String(item.label ?? ''))] })
            },
            renderLabelItem: ({ item }) => {
              seen.label++
              return label(String(item))
            },
            renderValueItem: ({ item }) => {
              seen.value++
              return label(String(item))
            },
          },
        }),
      ],
    })

    expect(seen.legend).toBeGreaterThan(0)
    expect(seen.label).toBeGreaterThan(0)
    expect(seen.value).toBeGreaterThan(0)
    expect(canvas.toBufferSync('png').length).toBeGreaterThan(0)
  })

  it('builds a descriptor from every pie item callback', async () => {
    const seen = { legend: 0, label: 0 }

    const canvas = await Root({
      ...integrationRootBase,
      width: WIDTH,
      height: HEIGHT,
      workerMode: false,
      gpu: false,
      backgroundColor: '#ffffff',
      children: [
        Chart({
          type: 'doughnut',
          width: '100%',
          height: '100%',
          data: pie,
          options: {
            renderLegendItem: ({ item }) => {
              seen.legend++
              return Box({ children: [label(String(item.label))] })
            },
            renderLabelItem: ({ item }) => {
              seen.label++
              return label(String(item.label))
            },
          },
        }),
      ],
    })

    expect(seen.legend).toBeGreaterThan(0)
    expect(seen.label).toBeGreaterThan(0)
    expect(canvas.toBufferSync('png').length).toBeGreaterThan(0)
  })

  it('leaves a callback that returns nothing alone', async () => {
    const canvas = await Root({
      ...integrationRootBase,
      width: WIDTH,
      height: HEIGHT,
      workerMode: false,
      gpu: false,
      backgroundColor: '#ffffff',
      children: [
        Chart({
          type: 'bar',
          width: '100%',
          height: '100%',
          data: cartesian,
          options: {
            showValues: true,
            renderLabelItem: () => null,
            renderValueItem: () => undefined,
          },
        }),
      ],
    })

    expect(canvas.toBufferSync('png').length).toBeGreaterThan(0)
  })
})
