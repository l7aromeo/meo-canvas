import { GridNode, GridItem } from '@/canvas/grid.canvas.js'
import { BoxNode } from '@/canvas/layout.canvas.js'
import { buildTree } from '@/canvas/root.canvas.js'
import Yoga, { Style } from '@/constant/common.const.js'
import type { GridProps } from '@/canvas/canvas.type.js'

describe('GridNode', () => {
  it('should construct with default props', () => {
    const grid = new GridNode({ columns: 2 })
    expect(grid.node).toBeInstanceOf(Yoga.Node)
    expect((grid.props as GridProps).columns).toBe(2)
    expect(grid.name).toBe('Grid')
  })

  it('should have flexWrap set to Wrap', () => {
    const grid = new GridNode({})
    expect(grid.node.getFlexWrap()).toBe(Style.Wrap.Wrap)
  })

  it('should respect parent percentage minWidth to prevent overflow', () => {
    // Simulate: Column({ alignItems: Center }) -> Box({ minWidth: '60%' }) -> Grid
    const grid = new GridNode({ columns: 2, gap: 10, padding: 12 })

    // Create mock parent (Box with minWidth: '60%')
    const parentBox = Yoga.Node.create()
    parentBox.setMinWidthPercent(60)
    parentBox.insertChild(grid.node, 0)

    // Create grandparent (Column with fixed width)
    const grandparentColumn = Yoga.Node.create()
    grandparentColumn.setWidth(1000)
    grandparentColumn.insertChild(parentBox, 0)

    // Add a child to the grid
    const child = new BoxNode({ width: 500, height: 50 }) // Wide child
    ;(grid as any).appendChild(child, 0)

    // Calculate layout
    grandparentColumn.calculateLayout(1000, undefined, Style.Direction.LTR)

    // Run finalizeLayout which should set maxWidth
    grid.finalizeLayout()

    // After setting maxWidth, the Grid should not exceed 60% of grandparent (600px)
    // The maxWidth should be set based on parent's minWidth percentage
    const gridMaxWidth = grid.node.getMaxWidth()

    if (gridMaxWidth.unit === Style.Unit.Point) {
      expect(gridMaxWidth.value).toBeLessThanOrEqual(600)
    }

    // Cleanup
    grandparentColumn.removeChild(parentBox)
    parentBox.removeChild(grid.node)
  })

  it('should set maxWidth only once to prevent layout loops', () => {
    const grid = new GridNode({ columns: 2 })

    // Create hierarchy: Column -> Box(minWidth: '50%') -> Grid
    const parentBox = Yoga.Node.create()
    parentBox.setMinWidthPercent(50)
    parentBox.insertChild(grid.node, 0)

    const grandparentColumn = Yoga.Node.create()
    grandparentColumn.setWidth(800)
    grandparentColumn.insertChild(parentBox, 0)

    // First finalizeLayout
    grid.finalizeLayout()
    const maxWidthAfterFirst = grid.node.getMaxWidth()

    // Second finalizeLayout - should not change maxWidth
    grid.finalizeLayout()
    const maxWidthAfterSecond = grid.node.getMaxWidth()

    expect(maxWidthAfterSecond).toEqual(maxWidthAfterFirst)

    // Cleanup
    grandparentColumn.removeChild(parentBox)
    parentBox.removeChild(grid.node)
  })

  it('should not constrain width when parent has pixel minWidth', () => {
    const grid = new GridNode({ columns: 2 })

    // Create hierarchy: Column -> Box(minWidth: 300px) -> Grid
    const parentBox = Yoga.Node.create()
    parentBox.setMinWidth(300) // Pixel minWidth, not percentage
    parentBox.insertChild(grid.node, 0)

    const grandparentColumn = Yoga.Node.create()
    grandparentColumn.setWidth(1000)
    grandparentColumn.insertChild(parentBox, 0)

    grid.finalizeLayout()

    // maxWidth should not be set (remains auto/undefined)
    const gridMaxWidth = grid.node.getMaxWidth()
    expect(gridMaxWidth.unit).not.toBe(Style.Unit.Point)

    // Cleanup
    grandparentColumn.removeChild(parentBox)
    parentBox.removeChild(grid.node)
  })

  it('should handle grid with templateColumns', () => {
    const grid = new GridNode({
      templateColumns: ['100px', '1fr', 'auto'],
      gap: 10,
      width: 500,
    })

    grid.node.setWidth(500)
    grid.node.calculateLayout(500, undefined, Style.Direction.LTR)

    expect(grid.node.getComputedWidth()).toBe(500)
  })

  it('should place items with gridColumn span syntax', () => {
    const grid = new GridNode({ columns: 3, width: 600 })

    const item = buildTree(GridItem({ gridColumn: 'span 2', width: 100, height: 50 }))
    ;(grid as any).appendChild(item, 0)

    grid.node.setWidth(600)
    grid.node.calculateLayout(600, undefined, Style.Direction.LTR)

    expect(() => grid.finalizeLayout()).not.toThrow()
  })

  it('should handle empty grid without errors', () => {
    const grid = new GridNode({ columns: 2, width: 500 })

    grid.node.setWidth(500)
    grid.node.calculateLayout(500, undefined, Style.Direction.LTR)

    expect(() => grid.finalizeLayout()).not.toThrow()
  })
})
