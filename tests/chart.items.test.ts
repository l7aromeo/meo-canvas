import { buildTree } from '@/canvas/root.canvas.js'
import { Chart } from '@/canvas/chart.canvas.js'
import { Box, BoxNode, Row } from '@/canvas/layout.canvas.js'
import type { ChartItem, ChartOptions } from '@/canvas/canvas.type.js'

/**
 * The legend item the README documents: a coloured dot beside a label.
 *
 * Typed on what all three callbacks have in common, plus an optional colour, so one item stands in
 * for the label and value callbacks too — neither is handed a colour of its own.
 */
const legendItem = ({ color }: { index: number; color?: string }) => Row({ children: [Box({ width: 12, height: 12, backgroundColor: color })] })

/** Reads back what `buildTree` left in place of the callback the caller wrote. */
function wrapped(options: Partial<ChartOptions<'doughnut'>>) {
  const node = buildTree(
    Chart({
      type: 'doughnut',
      data: [{ label: 'Red', value: 1, color: '#ff0000' }],
      options: options as ChartOptions<'doughnut'>,
    }),
  )
  return (node.props as { options?: Record<string, (args: never) => ChartItem> }).options
}

/** The same, for a chart type that has values to draw — `renderValueItem` is Cartesian-only. */
function wrappedBar(options: Partial<ChartOptions<'bar'>>) {
  const node = buildTree(
    Chart({
      type: 'bar',
      data: { labels: ['A'], datasets: [{ label: 'Values', data: [1] }] },
      options: options as ChartOptions<'bar'>,
    }),
  )
  return (node.props as { options?: Record<string, (args: never) => ChartItem> }).options
}

describe('chart item callbacks', () => {
  it('builds a descriptor into a node, which is the only form a caller can return', () => {
    // `Box`, `Row` and the rest hand back descriptors, and `BoxNode` is exported as a type only —
    // so without this the documented callback returns something the tree rejects and the item is
    // silently missing from the chart.
    const render = wrapped({ renderLegendItem: legendItem })!.renderLegendItem

    expect(render({ item: { label: 'Red', value: 1 }, index: 0, color: '#ff0000' } as never)).toBeInstanceOf(BoxNode)
  })

  it('leaves a node alone', () => {
    const node = new BoxNode({ width: 10, height: 10 })
    const render = wrapped({ renderLegendItem: () => node })!.renderLegendItem

    expect(render({ item: { label: 'Red', value: 1 }, index: 0, color: '#ff0000' } as never)).toBe(node)
  })

  it('passes an absent item through, which is how a caller skips one', () => {
    const render = wrapped({ renderLegendItem: () => null })!.renderLegendItem
    expect(render({ item: { label: 'Red', value: 1 }, index: 0, color: '#ff0000' } as never)).toBeNull()
  })

  it('wraps the label and value callbacks on the same terms', () => {
    expect(wrapped({ renderLabelItem: legendItem })!.renderLabelItem({ item: { label: 'Red', value: 1 }, index: 0 } as never)).toBeInstanceOf(BoxNode)
    expect(wrappedBar({ renderValueItem: legendItem })!.renderValueItem({ item: 1, index: 0, datasetIndex: 0 } as never)).toBeInstanceOf(BoxNode)
  })

  it('leaves options that name no callback exactly as they were', () => {
    const options = { showLegend: true, innerRadius: 0.7 }
    // Same object, not a copy: a chart that asked for nothing here should carry no wrapper.
    expect(wrapped(options)).toBe(options)
  })
})
