import { BoxNode } from '@/canvas/layout.canvas.util.js'
import type { BaseProps, CartesianChartData, ChartProps, ChartType, PieChartDataPoint } from '@/canvas/canvas.type.js'
import type { CanvasRenderingContext2D } from 'skia-canvas'
import { Style } from '@/constant/common.const.js'

export class ChartNode<T extends ChartType> extends BoxNode {
  private chartData: CartesianChartData | PieChartDataPoint[]
  private chartType: ChartProps<T>['type']
  private chartOptions: ChartProps<T>['options']

  constructor(props: ChartProps<T> & BaseProps) {
    // Set default intrinsic size if not provided
    const defaultWidth = props.width ?? 400
    const defaultHeight = props.height ?? 300

    super({
      ...props,
      width: defaultWidth,
      height: defaultHeight,
      name: 'Chart',
    })

    this.chartData = props.data
    this.chartType = props.type
    this.chartOptions = {
      showGrid: true,
      showLabels: true,
      showLegend: true,
      gridColor: '#e0e0e0',
      axisColor: '#333',
      labelFontSize: 12,
      legendPosition: 'bottom',
      ...props.options,
    }
  }

  protected _renderContent(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    // First render background/borders from parent
    super._renderContent(ctx, x, y, width, height)

    // Then render chart-specific content
    const paddingLeft = this.node.getComputedPadding(Style.Edge.Left)
    const paddingRight = this.node.getComputedPadding(Style.Edge.Right)
    const paddingTop = this.node.getComputedPadding(Style.Edge.Top)
    const paddingBottom = this.node.getComputedPadding(Style.Edge.Bottom)
    const contentX = x + paddingLeft
    const contentY = y + paddingTop
    const contentWidth = width - paddingLeft - paddingRight
    const contentHeight = height - paddingTop - paddingBottom

    switch (this.chartType) {
      case 'bar':
        this.renderBarChart(ctx, contentX, contentY, contentWidth, contentHeight)
        break
      case 'line':
        this.renderLineChart(ctx, contentX, contentY, contentWidth, contentHeight)
        break
      case 'pie':
        this.renderPieChart(ctx, contentX, contentY, contentWidth, contentHeight)
        break
      case 'doughnut':
        this.renderDoughnutChart(ctx, contentX, contentY, contentWidth, contentHeight)
        break
    }
  }

  private getLegendLayout(ctx: CanvasRenderingContext2D, totalWidth: number, totalHeight: number) {
    if (!this.chartOptions?.showLegend) {
      return { x: 0, y: 0, width: 0, height: 0, chartWidth: totalWidth, chartHeight: totalHeight, chartX: 0, chartY: 0 }
    }

    const legendItems = 'datasets' in this.chartData ? this.chartData.datasets : (this.chartData as PieChartDataPoint[])
    if (legendItems.length === 0) {
      return { x: 0, y: 0, width: 0, height: 0, chartWidth: totalWidth, chartHeight: totalHeight, chartX: 0, chartY: 0 }
    }

    const fontSize = this.chartOptions?.labelFontSize || 12
    ctx.font = `${fontSize}px ${this.props.fontFamily || 'sans-serif'}`
    const metrics = ctx.measureText('Mg')
    const textHeight = metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent
    const itemHeight = Math.ceil(textHeight + 8)
    const position = this.chartOptions.legendPosition
    const boxSize = Math.min(15, itemHeight - 2)

    const legendItemLabels =
      'datasets' in this.chartData ? this.chartData.datasets.map(d => d.label) : (this.chartData as PieChartDataPoint[]).map(p => `${p.label} (${p.value})`)

    let calculatedLegendHeight = 0
    let calculatedLegendWidth = 0

    if (position === 'top' || position === 'bottom') {
      let currentX = 0
      let numRows = 1
      const itemPadding = 20
      legendItemLabels.forEach(label => {
        const labelWidth = ctx.measureText(label).width
        const itemWidth = boxSize + 5 + labelWidth + itemPadding

        if (currentX > 0 && currentX + itemWidth > totalWidth) {
          numRows++
          currentX = 0
        }
        currentX += itemWidth
      })
      calculatedLegendHeight = numRows * itemHeight + 10
      calculatedLegendWidth = totalWidth
    } else {
      // 'left' or 'right'
      const maxLabelWidth = Math.max(...legendItemLabels.map(label => ctx.measureText(label).width))
      calculatedLegendWidth = maxLabelWidth + boxSize + 25 // padding + box + padding + text
      calculatedLegendHeight = totalHeight
    }

    let effectiveChartWidth = totalWidth
    let effectiveChartHeight = totalHeight
    let legendAreaX = 0
    let legendAreaY = 0
    let chartAreaX = 0
    let chartAreaY = 0
    let legendAreaWidth = 0
    let legendAreaHeight = 0

    if (position === 'top' || position === 'bottom') {
      effectiveChartHeight -= calculatedLegendHeight
      legendAreaHeight = calculatedLegendHeight
      legendAreaWidth = totalWidth
      legendAreaX = 0
      chartAreaX = 0

      if (position === 'top') {
        chartAreaY = calculatedLegendHeight
        legendAreaY = 0
      } else {
        // bottom
        legendAreaY = effectiveChartHeight
        chartAreaY = 0
      }
    } else {
      // 'left' or 'right'
      effectiveChartWidth -= calculatedLegendWidth
      legendAreaWidth = calculatedLegendWidth
      legendAreaHeight = totalHeight
      legendAreaY = 0
      chartAreaY = 0

      if (position === 'left') {
        chartAreaX = calculatedLegendWidth
        legendAreaX = 0
      } else {
        // right
        legendAreaX = effectiveChartWidth
        chartAreaX = 0
      }
    }

    return {
      x: legendAreaX,
      y: legendAreaY,
      width: legendAreaWidth,
      height: legendAreaHeight,
      chartWidth: effectiveChartWidth,
      chartHeight: effectiveChartHeight,
      chartX: chartAreaX,
      chartY: chartAreaY,
    }
  }

  private renderBarChart(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    if (!('datasets' in this.chartData) || this.chartData.datasets.length === 0) return

    const legendLayout = this.getLegendLayout(ctx, width, height)
    const chartX = x + legendLayout.chartX
    const chartY = y + legendLayout.chartY
    const chartWidth = legendLayout.chartWidth
    const chartHeight = legendLayout.chartHeight

    const { labels, datasets } = this.chartData
    const maxValue = Math.max(...datasets.flatMap(d => d.data))

    let labelHeight = 0
    if (this.chartOptions?.showLabels) {
      const fontSize = this.chartOptions.labelFontSize || 12
      ctx.font = `${fontSize}px ${this.props.fontFamily || 'sans-serif'}`
      const metrics = ctx.measureText('Mg')
      labelHeight = metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent + 10 // with padding
    }
    const finalChartHeight = chartHeight - labelHeight

    const groupWidth = chartWidth / labels.length
    const barSpacing = groupWidth * 0.2
    const barWidth = (groupWidth - barSpacing) / datasets.length

    // Render grid
    if (this.chartOptions?.showGrid) {
      ctx.strokeStyle = this.chartOptions.gridColor!
      ctx.lineWidth = 1
      for (let i = 0; i <= 5; i++) {
        const gridY = chartY + (finalChartHeight / 5) * i
        ctx.beginPath()
        ctx.moveTo(chartX, gridY)
        ctx.lineTo(chartX + chartWidth, gridY)
        ctx.stroke()
      }
    }

    // Render bars
    labels.forEach((label, index) => {
      const groupX = chartX + index * groupWidth + barSpacing / 2

      datasets.forEach((dataset, datasetIndex) => {
        const barHeight = (dataset.data[index] / maxValue) * finalChartHeight
        const barX = groupX + datasetIndex * barWidth
        const barY = chartY + finalChartHeight - barHeight

        ctx.fillStyle = dataset.color || this.generateColor(datasetIndex)
        ctx.fillRect(barX, barY, barWidth, barHeight)
      })

      // Render labels
      if (this.chartOptions?.showLabels) {
        ctx.fillStyle = this.chartOptions.axisColor!
        ctx.font = `${this.chartOptions.labelFontSize}px ${this.props.fontFamily || 'sans-serif'}`
        ctx.textAlign = 'center'
        ctx.textBaseline = 'middle'
        ctx.fillText(label, groupX + (groupWidth - barSpacing) / 2, chartY + finalChartHeight + labelHeight / 2)
      }
    })

    // Render legend
    if (this.chartOptions?.showLegend) {
      this.renderLegend(ctx, x + legendLayout.x, y + legendLayout.y, legendLayout.width, legendLayout.height)
    }
  }

  private renderLineChart(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    if (!('datasets' in this.chartData) || this.chartData.datasets.length === 0) return

    const legendLayout = this.getLegendLayout(ctx, width, height)
    const chartX = x + legendLayout.chartX
    const chartY = y + legendLayout.chartY
    const chartWidth = legendLayout.chartWidth
    const chartHeight = legendLayout.chartHeight

    const { labels, datasets } = this.chartData
    const maxValue = Math.max(...datasets.flatMap(d => d.data))

    let labelHeight = 0
    if (this.chartOptions?.showLabels) {
      const fontSize = this.chartOptions.labelFontSize || 12
      ctx.font = `${fontSize}px ${this.props.fontFamily || 'sans-serif'}`
      const metrics = ctx.measureText('Mg')
      labelHeight = metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent + 10 // with padding
    }
    const finalChartHeight = chartHeight - labelHeight
    const pointSpacing = chartWidth / (labels.length > 1 ? labels.length - 1 : 1)

    // Render grid
    if (this.chartOptions?.showGrid) {
      ctx.strokeStyle = this.chartOptions.gridColor!
      ctx.lineWidth = 1
      for (let i = 0; i <= 5; i++) {
        const gridY = chartY + (finalChartHeight / 5) * i
        ctx.beginPath()
        ctx.moveTo(chartX, gridY)
        ctx.lineTo(chartX + chartWidth, gridY)
        ctx.stroke()
      }
    }

    // Render lines and points
    datasets.forEach((dataset, datasetIndex) => {
      ctx.strokeStyle = dataset.color || this.generateColor(datasetIndex)
      ctx.lineWidth = 2
      ctx.beginPath()

      dataset.data.forEach((value, index) => {
        const pointX = chartX + index * pointSpacing
        const pointY = chartY + finalChartHeight - (value / maxValue) * finalChartHeight

        if (index === 0) {
          ctx.moveTo(pointX, pointY)
        } else {
          ctx.lineTo(pointX, pointY)
        }
      })
      ctx.stroke()

      // Render points
      dataset.data.forEach((value, index) => {
        const pointX = chartX + index * pointSpacing
        const pointY = chartY + finalChartHeight - (value / maxValue) * finalChartHeight
        ctx.fillStyle = dataset.color || this.generateColor(datasetIndex)
        ctx.beginPath()
        ctx.arc(pointX, pointY, 4, 0, Math.PI * 2)
        ctx.fill()
      })
    })

    // Render labels
    if (this.chartOptions?.showLabels) {
      labels.forEach((label, index) => {
        const pointX = chartX + index * pointSpacing
        ctx.fillStyle = this.chartOptions.axisColor!
        ctx.font = `${this.chartOptions.labelFontSize}px ${this.props.fontFamily || 'sans-serif'}`
        ctx.textAlign = 'center'
        ctx.textBaseline = 'middle'
        ctx.fillText(label, pointX, chartY + finalChartHeight + labelHeight / 2)
      })
    }

    if (this.chartOptions?.showLegend) {
      this.renderLegend(ctx, x + legendLayout.x, y + legendLayout.y, legendLayout.width, legendLayout.height)
    }
  }

  private renderPieChart(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    if (!Array.isArray(this.chartData) || this.chartData.length === 0) return

    const legendLayout = this.getLegendLayout(ctx, width, height)
    const chartX = x + legendLayout.chartX
    const chartY = y + legendLayout.chartY
    const chartWidth = legendLayout.chartWidth
    const chartHeight = legendLayout.chartHeight

    const data = this.chartData as PieChartDataPoint[]
    const centerX = chartX + chartWidth / 2
    const centerY = chartY + chartHeight / 2
    const radius = Math.min(chartWidth, chartHeight) / 2 - 10

    const total = data.reduce((sum, point) => sum + point.value, 0)
    let currentAngle = -Math.PI / 2 // Start at top

    data.forEach((point, index) => {
      const sliceAngle = (point.value / total) * Math.PI * 2

      ctx.fillStyle = point.color || this.generateColor(index)
      ctx.beginPath()
      ctx.moveTo(centerX, centerY)
      ctx.arc(centerX, centerY, radius, currentAngle, currentAngle + sliceAngle)
      ctx.closePath()
      ctx.fill()

      // Draw slice border
      ctx.strokeStyle = '#fff'
      ctx.lineWidth = 2
      ctx.stroke()

      currentAngle += sliceAngle
    })

    if (this.chartOptions?.showLegend) {
      this.renderLegend(ctx, x + legendLayout.x, y + legendLayout.y, legendLayout.width, legendLayout.height)
    }
  }

  private renderDoughnutChart(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    if (!Array.isArray(this.chartData) || this.chartData.length === 0) return

    const legendLayout = this.getLegendLayout(ctx, width, height)
    const chartX = x + legendLayout.chartX
    const chartY = y + legendLayout.chartY
    const chartWidth = legendLayout.chartWidth
    const chartHeight = legendLayout.chartHeight

    const data = this.chartData as PieChartDataPoint[]
    const centerX = chartX + chartWidth / 2
    const centerY = chartY + chartHeight / 2
    const outerRadius = Math.min(chartWidth, chartHeight) / 2 - 10
    const innerRadius = outerRadius * 0.6

    const total = data.reduce((sum, point) => sum + point.value, 0)
    let currentAngle = -Math.PI / 2

    data.forEach((point, index) => {
      const sliceAngle = (point.value / total) * Math.PI * 2

      ctx.fillStyle = point.color || this.generateColor(index)
      ctx.beginPath()
      ctx.arc(centerX, centerY, outerRadius, currentAngle, currentAngle + sliceAngle)
      ctx.arc(centerX, centerY, innerRadius, currentAngle + sliceAngle, currentAngle, true)
      ctx.closePath()
      ctx.fill()

      ctx.strokeStyle = '#fff'
      ctx.lineWidth = 2
      ctx.stroke()

      currentAngle += sliceAngle
    })

    if (this.chartOptions?.showLegend) {
      this.renderLegend(ctx, x + legendLayout.x, y + legendLayout.y, legendLayout.width, legendLayout.height)
    }
  }

  private renderLegend(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    const fontSize = this.chartOptions?.labelFontSize || 12
    ctx.font = `${fontSize}px ${this.props.fontFamily || 'sans-serif'}`

    const metrics = ctx.measureText('Mg')
    const textHeight = metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent
    const itemHeight = Math.ceil(textHeight + 8)
    const boxSize = Math.min(15, itemHeight - 2)

    const legendItems =
      'datasets' in this.chartData
        ? this.chartData.datasets.map(d => ({ label: d.label, value: d.data.reduce((a, b) => a + b, 0) }))
        : (this.chartData as PieChartDataPoint[])

    const position = this.chartOptions.legendPosition
    if (position === 'top' || position === 'bottom') {
      const itemPadding = 20 // horizontal padding between items
      const rows: { items: { label: string; color: string; width: number }[]; width: number }[] = []
      let currentRow: { items: { label: string; color: string; width: number }[]; width: number } = { items: [], width: 0 }

      legendItems.forEach((point, index) => {
        const color = ('datasets' in this.chartData ? this.chartData.datasets[index].color : (point as any).color) || this.generateColor(index)
        const label = 'datasets' in this.chartData ? point.label : `${point.label} (${point.value})`
        const labelWidth = ctx.measureText(label).width
        const itemWidth = boxSize + 5 + labelWidth

        if (currentRow.items.length > 0 && currentRow.width + itemPadding + itemWidth > width) {
          rows.push(currentRow)
          currentRow = { items: [], width: 0 }
        }

        currentRow.items.push({ label, color, width: itemWidth })
        currentRow.width += itemWidth + (currentRow.items.length > 1 ? itemPadding : 0)
      })
      rows.push(currentRow)

      let currentY = y + 5
      rows.forEach(row => {
        let currentX = x + (width - row.width) / 2
        row.items.forEach(item => {
          const boxY = currentY + (itemHeight - boxSize) / 2
          ctx.fillStyle = item.color
          ctx.fillRect(currentX, boxY, boxSize, boxSize)

          ctx.fillStyle = this.chartOptions?.axisColor || '#333'
          ctx.textAlign = 'left'
          ctx.textBaseline = 'middle'
          ctx.fillText(item.label, currentX + boxSize + 5, currentY + itemHeight / 2)

          currentX += item.width + itemPadding
        })
        currentY += itemHeight
      })
    } else {
      // 'left' or 'right'
      const totalHeight = legendItems.length * itemHeight
      const startY = y + (height - totalHeight) / 2

      legendItems.forEach((point, index) => {
        const itemX = x + 10
        const itemY = startY + index * itemHeight

        const boxY = itemY + (itemHeight - boxSize) / 2
        ctx.fillStyle = ('datasets' in this.chartData ? this.chartData.datasets[index].color : (point as any).color) || this.generateColor(index)
        ctx.fillRect(itemX, boxY, boxSize, boxSize)

        ctx.fillStyle = this.chartOptions?.axisColor || '#333'
        ctx.textAlign = 'left'
        ctx.textBaseline = 'middle'
        const label = 'datasets' in this.chartData ? point.label : `${point.label} (${point.value})`
        ctx.fillText(label, itemX + boxSize + 5, itemY + itemHeight / 2)
      })
    }
  }

  private generateColor(index: number): string {
    const colors = ['#FF6384', '#36A2EB', '#FFCE56', '#4BC0C0', '#9966FF', '#FF9F40', '#C9CBCF']
    return colors[index % colors.length]
  }
}

export const Chart = <T extends ChartType>(props: ChartProps<T> & BaseProps): ChartNode<T> => new ChartNode(props)
