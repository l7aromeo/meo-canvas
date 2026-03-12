import { Root } from '@/canvas/root.canvas.util.js'
import { Chart } from '@/canvas/chart.canvas.util.js'
import { Column, Row } from '@/canvas/layout.canvas.util.js'
import { Text } from '@/canvas/text.canvas.util.js'
import * as fs from 'fs'

async function run() {
  const commonProps = {
    width: '50%',
    height: '100%',
    padding: 10,
    options: {
      showValues: true,
      showLegend: true,
      labelFontSize: 12,
      legendPosition: 'bottom' as const,
    },
  }

  const titleStyle = {
    fontSize: 16,
    color: '#333',
    fontWeight: 'bold',
    marginBottom: 10,
    textAlign: 'center',
  } as const

  // 1. Bar Chart
  const barChartSection = Column({
    width: '50%',
    height: '100%',
    padding: 10,
    children: [
      Text('Bar Chart (with Values & Y-Axis)', titleStyle),
      Chart({
        type: 'bar',
        width: '100%',
        height: '100%', // Take remaining height after title
        flexGrow: 1,
        options: {
          ...commonProps.options,
          showYAxis: true,
          grid: { show: true },
        },
        data: {
          labels: ['Q1', 'Q2', 'Q3', 'Q4'],
          datasets: [
            {
              label: 'Revenue',
              data: [150, 230, 180, 320],
              color: '#36A2EB',
            },
          ],
        },
      }),
    ],
  })

  // 2. Line Chart
  const lineChartSection = Column({
    width: '50%',
    height: '100%',
    padding: 10,
    children: [
      Text('Line Chart (Multi-dataset)', titleStyle),
      Chart({
        type: 'line',
        width: '100%',
        height: '100%',
        flexGrow: 1,
        options: {
          ...commonProps.options,
          showYAxis: true,
          grid: { show: true, style: 'dashed' },
        },
        data: {
          labels: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri'],
          datasets: [
            {
              label: 'Traffic',
              data: [12, 19, 3, 5, 2],
              color: '#FF6384',
            },
            {
              label: 'Sales',
              data: [8, 11, 13, 9, 7],
              color: '#4BC0C0',
            },
          ],
        },
      }),
    ],
  })

  // 2b. Line Chart with Decimal Values (Response Times)
  const decimalLineChartSection = Column({
    width: '50%',
    height: '100%',
    padding: 10,
    children: [
      Text('Line Chart (Decimal Values)', titleStyle),
      Chart({
        type: 'line',
        width: '100%',
        height: '100%',
        flexGrow: 1,
        options: {
          ...commonProps.options,
          showYAxis: true,
          grid: { show: true, style: 'dashed' },
        },
        data: {
          labels: ['00:00', '04:00', '08:00', '12:00', '16:00', '20:00'],
          datasets: [
            {
              label: 'Response Time (ms)',
              data: [21.84, 43.69, 65.32, 87.15, 109.22, 52.46],
              color: '#FF9800',
            },
          ],
        },
      }),
    ],
  })

  // 3. Pie Chart
  const pieChartSection = Column({
    width: '50%',
    height: '100%',
    padding: 10,
    children: [
      Text('Pie Chart', titleStyle),
      Chart({
        type: 'pie',
        width: '100%',
        height: '100%',
        flexGrow: 1,
        ...commonProps.options, // Pie chart specific props spread if needed, but here just options
        data: [
          { label: 'Red', value: 300, color: '#FF6384' },
          { label: 'Blue', value: 50, color: '#36A2EB' },
          { label: 'Yellow', value: 100, color: '#FFCE56' },
        ],
      }),
    ],
  })

  // 4. Doughnut Chart
  const doughnutChartSection = Column({
    width: '50%',
    height: '100%',
    padding: 10,
    children: [
      Text('Doughnut Chart', titleStyle),
      Chart({
        type: 'doughnut',
        width: '100%',
        height: '100%',
        flexGrow: 1,
        options: {
          ...commonProps.options,
          innerRadius: 0.5,
        },
        data: [
          { label: 'React', value: 40, color: '#61DAFB' },
          { label: 'Vue', value: 30, color: '#42B883' },
          { label: 'Angular', value: 20, color: '#DD0031' },
          { label: 'Svelte', value: 10, color: '#FF3E00' },
        ],
      }),
    ],
  })

  const canvas = await Root({
    width: 1600,
    height: 800,
    backgroundColor: '#f0f0f0',
    padding: 10,
    children: [
      // Top Row - 2 charts
      Row({
        width: '100%',
        height: '50%',
        children: [barChartSection, lineChartSection],
      }),
      // Bottom Row - 3 charts
      Row({
        width: '100%',
        height: '50%',
        children: [pieChartSection, doughnutChartSection, decimalLineChartSection],
      }),
    ],
  })

  const outputPath = 'samples/chart_samples.png'
  fs.mkdirSync('samples', { recursive: true })

  // Root returns a canvas instance from skia-canvas-node (or wrapper)
  // Check Root signature: returns Promise<Canvas>
  await canvas.toFile(outputPath)
  console.log(`Charts saved to ${outputPath}`)
}

run().catch(console.error)
