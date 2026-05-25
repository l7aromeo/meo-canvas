import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { Chart } from '@/canvas/chart.canvas.js'
import { Grid } from '@/canvas/grid.canvas.js'

const FIXTURES_DIR = join(dirname(fileURLToPath(import.meta.url)), '../fixtures/renders')
const UPDATE_FIXTURES = process.env.UPDATE_FIXTURES === '1'

async function expectPngMatch(name: string, buffer: Buffer) {
  const fixturePath = join(FIXTURES_DIR, `${name}.png`)
  if (UPDATE_FIXTURES || !existsSync(fixturePath)) {
    mkdirSync(FIXTURES_DIR, { recursive: true })
    writeFileSync(fixturePath, buffer)
  }
  const expected = readFileSync(fixturePath)
  expect(buffer.equals(expected)).toBe(true)
}

describe('integration renders', () => {
  it('renders a simple box with text', async () => {
    const canvas = await Root({
      width: 200,
      height: 100,
      workerMode: false,
      children: [
        Box({
          width: '100%',
          height: '100%',
          backgroundColor: '#3366cc',
          children: [Text('Hello', { fontSize: 24, color: '#ffffff' })],
        }),
      ],
    })

    const png = await canvas.toBuffer('png')
    await expectPngMatch('simple-box-text', png)
  })

  it('renders a minimal bar chart', async () => {
    const canvas = await Root({
      width: 320,
      height: 240,
      workerMode: false,
      children: [
        Chart({
          type: 'bar',
          width: '100%',
          height: '100%',
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
      width: 320,
      height: 120,
      workerMode: false,
      children: [
        Grid({
          templateColumns: [100, 100, 100],
          gap: 10,
          children: [
            Box({ backgroundColor: '#FF5252', height: 50, children: [Text('1', { color: '#fff' })] }),
            Box({ backgroundColor: '#448AFF', height: 50, children: [Text('2', { color: '#fff' })] }),
            Box({ backgroundColor: '#69F0AE', height: 50, children: [Text('3', { color: '#fff' })] }),
          ],
        }),
      ],
    })

    const png = await canvas.toBuffer('png')
    await expectPngMatch('grid-basic-3col', png)
  })
})
