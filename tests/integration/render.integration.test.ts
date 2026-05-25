import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { Chart } from '@/canvas/chart.canvas.js'
import { Grid } from '@/canvas/grid.canvas.js'
import { integrationFontFamily, integrationRootBase } from './helpers/integration-font.js'
import { expectPngMatch } from './helpers/png-match.js'

describe('integration renders', () => {
  it('renders a simple box with text', async () => {
    const canvas = await Root({
      ...integrationRootBase,
      width: 200,
      height: 100,
      workerMode: false,
      children: [
        Box({
          width: '100%',
          height: '100%',
          backgroundColor: '#3366cc',
          children: [Text('Hello', { fontSize: 24, color: '#ffffff', fontFamily: integrationFontFamily })],
        }),
      ],
    })

    const png = await canvas.toBuffer('png')
    await expectPngMatch('simple-box-text', png)
  })

  it('renders a minimal bar chart', async () => {
    const canvas = await Root({
      ...integrationRootBase,
      width: 320,
      height: 240,
      workerMode: false,
      children: [
        Chart({
          type: 'bar',
          width: '100%',
          height: '100%',
          fontFamily: integrationFontFamily,
          data: {
            labels: ['A', 'B', 'C'],
            datasets: [{ label: 'Values', data: [10, 20, 15], color: '#36A2EB' }],
          },
          options: {
            showValues: true,
            showYAxis: true,
            grid: { show: true },
          },
        }),
      ],
    })

    const png = await canvas.toBuffer('png')
    await expectPngMatch('bar-chart-minimal', png)
  })

  it('renders a basic 3-column grid', async () => {
    const canvas = await Root({
      ...integrationRootBase,
      width: 320,
      height: 120,
      workerMode: false,
      children: [
        Grid({
          templateColumns: [100, 100, 100],
          gap: 10,
          children: [
            Box({
              backgroundColor: '#FF5252',
              height: 50,
              children: [Text('1', { color: '#fff', fontFamily: integrationFontFamily })],
            }),
            Box({
              backgroundColor: '#448AFF',
              height: 50,
              children: [Text('2', { color: '#fff', fontFamily: integrationFontFamily })],
            }),
            Box({
              backgroundColor: '#69F0AE',
              height: 50,
              children: [Text('3', { color: '#fff', fontFamily: integrationFontFamily })],
            }),
          ],
        }),
      ],
    })

    const png = await canvas.toBuffer('png')
    await expectPngMatch('grid-basic-3col', png)
  })
})
