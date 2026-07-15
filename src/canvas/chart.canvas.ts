import { BoxNode, RowNode } from '@/canvas/layout.canvas.js'
import type { BaseProps, CartesianChartData, ChartDataset, ChartProps, ChartType, PieChartDataPoint, CanvasElement } from '@/canvas/canvas.type.js'
import type { CanvasRenderingContext2D } from 'skia-canvas'
import { Style } from '@/constant/common.const.js'
import { TextNode } from '@/canvas/text.canvas.js'

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
      showLabels: true,
      showLegend: true,
      labelFontSize: 12,
      legendPosition: 'bottom',
      ...props.options,
    } as ChartProps<T>['options']

    this.validateProps()
  }

  private validateProps() {
    if (this.chartType === 'bar' || this.chartType === 'line') {
      const data = this.chartData as CartesianChartData
      if (!data.labels || !data.datasets) {
        console.warn(`[ChartNode] Warning: Cartesian chart (${this.chartType}) is missing 'labels' or 'datasets' in its data prop.`)
      }
      data.datasets?.forEach((dataset, i) => {
        if (dataset.data.length !== data.labels.length) {
          console.warn(
            `[ChartNode] Warning: In dataset ${i} ("${dataset.label}"), the number of data points (${dataset.data.length}) does not match the number of labels (${data.labels.length}).`,
          )
        }
      })
    } else if (this.chartType === 'pie' || this.chartType === 'doughnut') {
      const data = this.chartData as PieChartDataPoint[]
      if (!Array.isArray(data)) {
        console.warn(`[ChartNode] Warning: ${this.chartType} chart expects an array of PieChartDataPoint, but received a different type.`)
      }
    }
  }

  protected async _renderContent(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    // First render background/borders from parent
    await super._renderContent(ctx, x, y, width, height)

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
        await this.renderBarChart(ctx, contentX, contentY, contentWidth, contentHeight)
        break
      case 'line':
        await this.renderLineChart(ctx, contentX, contentY, contentWidth, contentHeight)
        break
      case 'pie':
        await this.renderPieChart(ctx, contentX, contentY, contentWidth, contentHeight)
        break
      case 'doughnut':
        await this.renderDoughnutChart(ctx, contentX, contentY, contentWidth, contentHeight)
        break
    }
  }

  private getSmartYAxisFormatter(maxValue: number): (v: number) => string {
    const absMax = Math.abs(maxValue)

    // Thresholds with corresponding decimal places, divisors, and suffixes
    const thresholds = [
      { min: 1000000, decimals: 1, divisor: 1000000, suffix: 'M' },
      { min: 1000, decimals: 0, divisor: 1, suffix: '' },
      { min: 100, decimals: 1, divisor: 1, suffix: '' },
      { min: 1, decimals: 2, divisor: 1, suffix: '' },
      { min: 0, decimals: 4, divisor: 1, suffix: '' },
    ]

    let config = thresholds[thresholds.length - 1]
    for (const threshold of thresholds) {
      if (absMax >= threshold.min) {
        config = threshold
        break
      }
    }

    return (v: number) => {
      const scaled = v / config.divisor
      const factor = Math.pow(10, config.decimals)
      const rounded = Math.round(scaled * factor) / factor
      return rounded.toString() + config.suffix
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

    let calculatedLegendHeight: number
    let calculatedLegendWidth: number

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
    let legendAreaX: number
    let legendAreaY: number
    let chartAreaX: number
    let chartAreaY: number
    let legendAreaWidth: number
    let legendAreaHeight: number

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

  private async renderBarChart(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    if (this.chartType !== 'bar') return
    const chartData = this.chartData as CartesianChartData
    const chartOptions = this.chartOptions as ChartProps<'bar'>['options']

    const { labels, datasets } = chartData
    const maxValue = Math.max(...datasets.flatMap(d => d.data))

    const legendLayout = this.getLegendLayout(ctx, width, height)
    let chartX = x + legendLayout.chartX
    const chartY = y + legendLayout.chartY
    let chartWidth = legendLayout.chartWidth
    const chartHeight = legendLayout.chartHeight

    if (chartOptions?.showYAxis) {
      const fontSize = chartOptions.yAxisFontSize || 12
      ctx.font = `${fontSize}px ${this.props.fontFamily || 'sans-serif'}`
      const maxLabel = chartOptions.yAxisLabelFormatter ? await chartOptions.yAxisLabelFormatter(maxValue) : this.getSmartYAxisFormatter(maxValue)(maxValue)
      const yAxisWidth = ctx.measureText(maxLabel).width + 10
      chartX += yAxisWidth
      chartWidth -= yAxisWidth
    }

    let labelHeight = 0
    if (chartOptions?.showLabels) {
      const fontSize = chartOptions.labelFontSize || 12
      ctx.font = `${fontSize}px ${this.props.fontFamily || 'sans-serif'}`
      const metrics = ctx.measureText('Mg')
      labelHeight = metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent + 10 // with padding
    }
    const finalChartHeight = chartHeight - labelHeight

    const groupWidth = chartWidth / labels.length
    const barSpacing = groupWidth * 0.2
    const barWidth = (groupWidth - barSpacing) / datasets.length

    // Render grid
    if (chartOptions?.grid?.show) {
      ctx.strokeStyle = chartOptions.grid.color || '#e0e0e0'
      ctx.lineWidth = 1
      if (chartOptions.grid.style === 'dashed') {
        ctx.setLineDash([5, 5])
      } else if (chartOptions.grid.style === 'dotted') {
        ctx.setLineDash([2, 2])
      }

      for (let i = 0; i <= 5; i++) {
        const gridY = chartY + (finalChartHeight / 5) * i
        ctx.beginPath()
        ctx.moveTo(chartX, gridY)
        ctx.lineTo(chartX + chartWidth, gridY)
        ctx.stroke()

        if (chartOptions?.showYAxis) {
          const value = maxValue - (maxValue / 5) * i
          const label = chartOptions.yAxisLabelFormatter ? await chartOptions.yAxisLabelFormatter(value) : this.getSmartYAxisFormatter(maxValue)(value)

          TextNode.renderSimpleText(ctx, label, chartX - 5, gridY, {
            color: chartOptions.yAxisColor || chartOptions.axisColor || '#000',
            fontSize: chartOptions.yAxisFontSize || 12,
            fontFamily: this.props.fontFamily,
            textAlign: 'right',
            textBaseline: 'middle',
          })
        }
      }
      ctx.setLineDash([])
    }

    // Render bars
    for (let index = 0; index < labels.length; index++) {
      const label = labels[index]
      const groupX = chartX + index * groupWidth + barSpacing / 2

      for (let datasetIndex = 0; datasetIndex < datasets.length; datasetIndex++) {
        const dataset = datasets[datasetIndex]
        const barHeight = (dataset.data[index] / maxValue) * finalChartHeight
        const barX = groupX + datasetIndex * barWidth
        const barY = chartY + finalChartHeight - barHeight

        ctx.fillStyle = dataset.color || this.generateColor(datasetIndex)
        ctx.fillRect(barX, barY, barWidth, barHeight)

        // Render values
        if (chartOptions?.showValues) {
          const value = dataset.data[index]
          const valueX = barX + barWidth / 2
          const valueY = barY - 5 // 5px padding above bar

          if (chartOptions.renderValueItem) {
            const valueNode = await chartOptions.renderValueItem({ item: value, index, datasetIndex })
            if (valueNode) {
              valueNode.processInitialChildren()
              valueNode.node.calculateLayout(undefined, undefined, Style.Direction.LTR)
              const layout = valueNode.node.getComputedLayout()
              await valueNode.render(ctx, valueX - layout.width / 2, valueY - layout.height)
            }
          } else {
            TextNode.renderSimpleText(ctx, value.toString(), valueX, valueY, {
              color: chartOptions.valueColor || '#000',
              fontSize: chartOptions.valueFontSize || 12,
              fontFamily: this.props.fontFamily,
              textAlign: 'center',
              textBaseline: 'bottom',
            })
          }
        }
      }

      // Render labels
      if (chartOptions?.showLabels) {
        const displayLabel = chartOptions.xAxisLabelFormatter ? await chartOptions.xAxisLabelFormatter(label, index) : label

        if (chartOptions.renderLabelItem) {
          const labelNode = await chartOptions.renderLabelItem({ item: label, index })
          if (labelNode) {
            labelNode.processInitialChildren()
            labelNode.node.calculateLayout(undefined, undefined, Style.Direction.LTR)
            const layout = labelNode.node.getComputedLayout()
            await labelNode.render(
              ctx,
              groupX + (groupWidth - barSpacing) / 2 - layout.width / 2,
              chartY + finalChartHeight + labelHeight / 2 - layout.height / 2,
            )
          }
        } else {
          TextNode.renderSimpleText(ctx, displayLabel, groupX + (groupWidth - barSpacing) / 2, chartY + finalChartHeight + labelHeight / 2, {
            color: chartOptions.labelColor || chartOptions.axisColor,
            fontSize: chartOptions.labelFontSize,
            fontFamily: this.props.fontFamily,
            textAlign: 'center',
            textBaseline: 'middle',
          })
        }
      }
    }

    // Render legend
    if (chartOptions?.showLegend) {
      await this.renderLegend(ctx, x + legendLayout.x, y + legendLayout.y, legendLayout.width, legendLayout.height)
    }
  }

  private async renderLineChart(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    if (this.chartType !== 'line') return
    const chartData = this.chartData as CartesianChartData
    const chartOptions = this.chartOptions as ChartProps<'line'>['options']

    const { labels, datasets } = chartData
    const maxValue = Math.max(...datasets.flatMap(d => d.data))

    const legendLayout = this.getLegendLayout(ctx, width, height)
    let chartX = x + legendLayout.chartX
    const chartY = y + legendLayout.chartY
    let chartWidth = legendLayout.chartWidth
    const chartHeight = legendLayout.chartHeight

    if (chartOptions?.showYAxis) {
      const fontSize = chartOptions.yAxisFontSize || 12
      ctx.font = `${fontSize}px ${this.props.fontFamily || 'sans-serif'}`
      const maxLabel = chartOptions.yAxisLabelFormatter ? await chartOptions.yAxisLabelFormatter(maxValue) : this.getSmartYAxisFormatter(maxValue)(maxValue)
      const yAxisWidth = ctx.measureText(maxLabel).width + 10
      chartX += yAxisWidth
      chartWidth -= yAxisWidth
    }

    let labelHeight = 0
    if (chartOptions?.showLabels) {
      const fontSize = chartOptions.labelFontSize || 12
      ctx.font = `${fontSize}px ${this.props.fontFamily || 'sans-serif'}`
      const metrics = ctx.measureText('Mg')
      labelHeight = metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent + 10 // with padding
    }
    const finalChartHeight = chartHeight - labelHeight
    const pointSpacing = chartWidth / (labels.length > 1 ? labels.length - 1 : 1)

    // Render grid
    if (chartOptions?.grid?.show) {
      ctx.strokeStyle = chartOptions.grid.color || '#e0e0e0'
      ctx.lineWidth = 1
      if (chartOptions.grid.style === 'dashed') {
        ctx.setLineDash([5, 5])
      } else if (chartOptions.grid.style === 'dotted') {
        ctx.setLineDash([2, 2])
      }

      for (let i = 0; i <= 5; i++) {
        const gridY = chartY + (finalChartHeight / 5) * i
        ctx.beginPath()
        ctx.moveTo(chartX, gridY)
        ctx.lineTo(chartX + chartWidth, gridY)
        ctx.stroke()

        if (chartOptions?.showYAxis) {
          const value = maxValue - (maxValue / 5) * i
          const label = chartOptions.yAxisLabelFormatter ? await chartOptions.yAxisLabelFormatter(value) : this.getSmartYAxisFormatter(maxValue)(value)

          TextNode.renderSimpleText(ctx, label, chartX - 5, gridY, {
            color: chartOptions.yAxisColor || chartOptions.axisColor || '#000',
            fontSize: chartOptions.yAxisFontSize || 12,
            fontFamily: this.props.fontFamily,
            textAlign: 'right',
            textBaseline: 'middle',
          })
        }
      }
      ctx.setLineDash([])
    }

    // Render lines and points
    for (let datasetIndex = 0; datasetIndex < datasets.length; datasetIndex++) {
      const dataset = datasets[datasetIndex]
      ctx.strokeStyle = dataset.color || this.generateColor(datasetIndex)
      ctx.lineWidth = 2
      ctx.beginPath()

      for (let index = 0; index < dataset.data.length; index++) {
        const value = dataset.data[index]
        const pointX = chartX + index * pointSpacing
        const pointY = chartY + finalChartHeight - (value / maxValue) * finalChartHeight

        if (index === 0) {
          ctx.moveTo(pointX, pointY)
        } else {
          ctx.lineTo(pointX, pointY)
        }
      }
      ctx.stroke()

      // Render points
      for (let index = 0; index < dataset.data.length; index++) {
        const pointX = chartX + index * pointSpacing
        const pointY = chartY + finalChartHeight - (dataset.data[index] / maxValue) * finalChartHeight
        ctx.fillStyle = dataset.color || this.generateColor(datasetIndex)
        ctx.beginPath()
        ctx.arc(pointX, pointY, 4, 0, Math.PI * 2)
        ctx.fill()
      }
    }

    // Render labels
    if (chartOptions?.showLabels) {
      for (let index = 0; index < labels.length; index++) {
        const label = labels[index]
        const pointX = chartX + index * pointSpacing
        const displayLabel = chartOptions.xAxisLabelFormatter ? await chartOptions.xAxisLabelFormatter(label, index) : label

        if (chartOptions.renderLabelItem) {
          const labelNode = await chartOptions.renderLabelItem({ item: label, index })
          if (labelNode) {
            labelNode.processInitialChildren()
            labelNode.node.calculateLayout(undefined, undefined, Style.Direction.LTR)
            const layout = labelNode.node.getComputedLayout()
            await labelNode.render(ctx, pointX - layout.width / 2, chartY + finalChartHeight + labelHeight / 2 - layout.height / 2)
          }
        } else {
          TextNode.renderSimpleText(ctx, displayLabel, pointX, chartY + finalChartHeight + labelHeight / 2, {
            color: chartOptions.labelColor || chartOptions.axisColor,
            fontSize: chartOptions.labelFontSize,
            fontFamily: this.props.fontFamily,
            textAlign: 'center',
            textBaseline: 'middle',
          })
        }
      }
    }

    if (chartOptions?.showLegend) {
      await this.renderLegend(ctx, x + legendLayout.x, y + legendLayout.y, legendLayout.width, legendLayout.height)
    }
  }

  private async renderPieChart(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    if (this.chartType !== 'pie') return
    const data = this.chartData as PieChartDataPoint[]
    const chartOptions = this.chartOptions as ChartProps<'pie'>['options']

    const legendLayout = this.getLegendLayout(ctx, width, height)
    const chartX = x + legendLayout.chartX
    const chartY = y + legendLayout.chartY
    const chartWidth = legendLayout.chartWidth
    const chartHeight = legendLayout.chartHeight

    const centerX = chartX + chartWidth / 2
    const centerY = chartY + chartHeight / 2
    const radius = Math.min(chartWidth, chartHeight) / 2 - 10

    const total = data.reduce((sum, point) => sum + point.value, 0)
    let currentAngle = -Math.PI / 2 // Start at top

    for (let index = 0; index < data.length; index++) {
      const point = data[index]
      const sliceAngle = (point.value / total) * Math.PI * 2
      const startAngle = currentAngle
      const endAngle = currentAngle + sliceAngle

      ctx.fillStyle = point.color || this.generateColor(index)
      ctx.beginPath()
      ctx.moveTo(centerX, centerY)
      ctx.arc(centerX, centerY, radius, startAngle, endAngle)
      ctx.closePath()
      ctx.fill()

      // Draw slice border
      ctx.strokeStyle = '#fff'
      ctx.lineWidth = 2
      ctx.stroke()

      // Render labels
      if (chartOptions?.showLabels) {
        const labelAngle = startAngle + sliceAngle / 2
        const labelRadius = radius * 0.7
        const labelX = centerX + Math.cos(labelAngle) * labelRadius
        const labelY = centerY + Math.sin(labelAngle) * labelRadius

        if (chartOptions.renderLabelItem) {
          const labelNode = await chartOptions.renderLabelItem({ item: point, index })
          if (labelNode) {
            labelNode.processInitialChildren()
            labelNode.node.calculateLayout(undefined, undefined, Style.Direction.LTR)
            const layout = labelNode.node.getComputedLayout()
            await labelNode.render(ctx, labelX - layout.width / 2, labelY - layout.height / 2)
          }
        } else {
          TextNode.renderSimpleText(ctx, point.label, labelX, labelY, {
            color: chartOptions.labelColor,
            fontSize: chartOptions.labelFontSize,
            fontFamily: this.props.fontFamily,
            textAlign: 'center',
            textBaseline: 'middle',
          })
        }
      }

      currentAngle = endAngle
    }

    if (chartOptions?.showLegend) {
      await this.renderLegend(ctx, x + legendLayout.x, y + legendLayout.y, legendLayout.width, legendLayout.height)
    }
  }

  private async renderDoughnutChart(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    if (this.chartType !== 'doughnut') return
    const data = this.chartData as PieChartDataPoint[]
    const chartOptions = this.chartOptions as ChartProps<'doughnut'>['options']

    const legendLayout = this.getLegendLayout(ctx, width, height)
    const chartX = x + legendLayout.chartX
    const chartY = y + legendLayout.chartY
    const chartWidth = legendLayout.chartWidth
    const chartHeight = legendLayout.chartHeight

    const centerX = chartX + chartWidth / 2
    const centerY = chartY + chartHeight / 2
    const outerRadius = Math.min(chartWidth, chartHeight) / 2 - 10
    const innerRadius = outerRadius * (chartOptions?.innerRadius ?? 0.6)

    const total = data.reduce((sum, point) => sum + point.value, 0)
    let currentAngle = -Math.PI / 2

    for (let index = 0; index < data.length; index++) {
      const point = data[index]
      const sliceAngle = (point.value / total) * Math.PI * 2
      const startAngle = currentAngle
      const endAngle = currentAngle + sliceAngle

      ctx.fillStyle = point.color || this.generateColor(index)
      ctx.beginPath()
      ctx.arc(centerX, centerY, outerRadius, startAngle, endAngle)
      ctx.arc(centerX, centerY, innerRadius, endAngle, startAngle, true)
      ctx.closePath()
      ctx.fill()

      ctx.strokeStyle = '#fff'
      ctx.lineWidth = 2
      ctx.stroke()

      // Render labels
      if (chartOptions?.showLabels) {
        const labelAngle = startAngle + sliceAngle / 2
        const labelRadius = innerRadius + (outerRadius - innerRadius) / 2
        const labelX = centerX + Math.cos(labelAngle) * labelRadius
        const labelY = centerY + Math.sin(labelAngle) * labelRadius

        if (chartOptions.renderLabelItem) {
          const labelNode = await chartOptions.renderLabelItem({ item: point, index })
          if (labelNode) {
            labelNode.processInitialChildren()
            labelNode.node.calculateLayout(undefined, undefined, Style.Direction.LTR)
            const layout = labelNode.node.getComputedLayout()
            await labelNode.render(ctx, labelX - layout.width / 2, labelY - layout.height / 2)
          }
        } else {
          TextNode.renderSimpleText(ctx, point.label, labelX, labelY, {
            color: chartOptions.labelColor,
            fontSize: chartOptions.labelFontSize,
            fontFamily: this.props.fontFamily,
            textAlign: 'center',
            textBaseline: 'middle',
          })
        }
      }

      currentAngle = endAngle
    }

    if (chartOptions?.showLegend) {
      await this.renderLegend(ctx, x + legendLayout.x, y + legendLayout.y, legendLayout.width, legendLayout.height)
    }
  }

  private async renderLegend(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    const chartOptions = this.chartOptions

    const renderLegendItem = chartOptions?.renderLegendItem as
      ((props: { item: unknown; index: number; color: string }) => BoxNode | null | undefined) | undefined

    if (renderLegendItem) {
      let legendNodes: (BoxNode | null | undefined)[]
      if (this.chartType === 'bar' || this.chartType === 'line') {
        const items = (this.chartData as CartesianChartData).datasets
        const render = renderLegendItem as (props: { item: ChartDataset; index: number; color: string }) => BoxNode | null | undefined
        legendNodes = await Promise.all(
          items.map(async (item, index) => {
            const color = item.color || this.generateColor(index)
            return render({ item, index, color })
          }),
        )
      } else {
        const items = this.chartData as PieChartDataPoint[]
        const render = renderLegendItem as (props: { item: PieChartDataPoint; index: number; color: string }) => BoxNode | null | undefined
        legendNodes = await Promise.all(
          items.map(async (item, index) => {
            const color = item.color || this.generateColor(index)
            return render({ item, index, color })
          }),
        )
      }

      const finalNodes = legendNodes.filter((node): node is BoxNode => !!node)

      if (finalNodes.length > 0) {
        const legendContainer = new RowNode({
          children: finalNodes,
          width,
          height,
          justifyContent: Style.Justify.Center,
          alignItems: Style.Align.Center,
          flexWrap: Style.Wrap.Wrap,
          gap: 10,
        })
        legendContainer.processInitialChildren()
        legendContainer.node.calculateLayout(width, height, Style.Direction.LTR)
        await legendContainer.render(ctx, x, y)
      }
      return
    }

    // Fallback to default rendering if renderLegendItem is not provided
    if (!this.chartData) return
    const legendItems =
      'datasets' in this.chartData
        ? this.chartData.datasets.map(d => ({ label: d.label, value: d.data.reduce((a, b) => a + b, 0), color: d.color }))
        : (this.chartData as PieChartDataPoint[])
    const fontSize = this.chartOptions?.labelFontSize || 12
    ctx.font = `${fontSize}px ${this.props.fontFamily || 'sans-serif'}`

    const metrics = ctx.measureText('Mg')
    const textHeight = metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent
    const itemHeight = Math.ceil(textHeight + 8)
    const boxSize = Math.min(15, itemHeight - 2)

    const position = this.chartOptions?.legendPosition
    if (position === 'top' || position === 'bottom') {
      const itemPadding = 20 // horizontal padding between items
      const rows: { items: { label: string; color: string; width: number }[]; width: number }[] = []
      let currentRow: { items: { label: string; color: string; width: number }[]; width: number } = { items: [], width: 0 }

      legendItems.forEach((point, index) => {
        const color = point.color || this.generateColor(index)
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

          TextNode.renderSimpleText(ctx, item.label, currentX + boxSize + 5, currentY + itemHeight / 2, {
            color: this.chartOptions?.labelColor,
            fontSize,
            fontFamily: this.props.fontFamily,
            textAlign: 'left',
            textBaseline: 'middle',
          })

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
        ctx.fillStyle = point.color || this.generateColor(index)
        ctx.fillRect(itemX, boxY, boxSize, boxSize)

        const label = 'datasets' in this.chartData ? point.label : `${point.label} (${point.value})`
        TextNode.renderSimpleText(ctx, label, itemX + boxSize + 5, itemY + itemHeight / 2, {
          color: this.chartOptions?.labelColor,
          fontSize,
          fontFamily: this.props.fontFamily,
          textAlign: 'left',
          textBaseline: 'middle',
        })
      })
    }
  }

  private generateColor(index: number): string {
    const colors = ['#FF6384', '#36A2EB', '#FFCE56', '#4BC0C0', '#9966FF', '#FF9F40', '#C9CBCF']
    return colors[index % colors.length]
  }
}

type ChartElement = Extract<CanvasElement, { __type: 'Chart' }>

export const Chart = <T extends ChartType>(props: ChartProps<T> & BaseProps): ChartElement => ({
  __type: 'Chart' as const,
  props: props as unknown as ChartElement['props'],
})
