import type { GridProps } from '@/canvas/canvas.type.js'
import { BoxNode, RowNode } from '@/canvas/layout.canvas.util.js'
import { Style, FlexDirection } from '@/constant/common.const.js'

/**
 * Grid layout node that arranges children in a configurable number of columns or rows.
 * Uses Yoga's flexbox capabilities with wrapping and gap properties to simulate a grid.
 * @extends RowNode
 */
export class GridNode extends RowNode {
  private readonly columns: number
  private readonly columnGapValue: number | `${number}%`
  private readonly rowGapValue: number | `${number}%`
  private readonly isVertical: boolean // True if the main axis is vertical (flexDirection: column or column-reverse)

  /**
   * Creates a new grid layout node
   * @param props - Grid configuration properties
   */
  constructor(props: GridProps) {
    const columns = Math.max(1, props.columns || 1)
    const direction = props.direction || 'row' // Default to horizontal row
    const isVertical = direction === 'column' || direction === 'column-reverse'

    // Map direction string to Yoga FlexDirection
    let flexDirection: FlexDirection
    switch (direction) {
      case 'row':
        flexDirection = Style.FlexDirection.Row
        break
      case 'column':
        flexDirection = Style.FlexDirection.Column
        break
      case 'row-reverse':
        flexDirection = Style.FlexDirection.RowReverse
        break
      case 'column-reverse':
        flexDirection = Style.FlexDirection.ColumnReverse
        break
      default:
        console.warn(`[GridNode] Invalid direction "${direction}". Defaulting to "row".`)
        flexDirection = Style.FlexDirection.Row
    }

    // Determine the column and row gap values from props
    let columnGap: number | `${number}%` = 0
    let rowGap: number | `${number}%` = 0

    if (typeof props.gap === 'number' || (typeof props.gap === 'string' && props.gap.trim() !== '')) {
      // Single value applies to both row and column gaps
      columnGap = props.gap
      rowGap = props.gap
    } else if (props.gap && typeof props.gap === 'object') {
      // Object format: prioritize a specific direction (Column/Row), then All
      columnGap = props.gap.Column ?? props.gap.All ?? 0
      rowGap = props.gap.Row ?? props.gap.All ?? 0
    }

    super({
      name: 'Grid',
      flexWrap: Style.Wrap.Wrap, // Essential for grid behavior
      flexDirection,
      ...props,
      // Explicitly remove the 'direction' prop passed to super, as it's handled by flexDirection
      direction: undefined,
    })

    this.columns = columns
    this.columnGapValue = columnGap
    this.rowGapValue = rowGap
    this.isVertical = isVertical
  }

  /**
   * Appends a child node to this grid.
   * Overridden primarily for documentation/clarity, functionality is inherited.
   * @param child - Child node to append
   * @param index - Index at which to insert the child
   */
  protected override appendChild(child: BoxNode, index: number) {
    super.appendChild(child, index)
  }

  /**
   * Update layout calculations after the initial layout is computed.
   * This method calculates the appropriate flex-basis for children based on the
   * number of columns and gaps, respecting the container's padding,
   * and applies the gaps using Yoga's built-in properties.
   */
  protected override updateLayoutBasedOnComputedSize() {
    // Step 1: Early return if the grid is empty or invalid
    if (this.columns <= 0 || this.children.length === 0) {
      return
    }

    // Step 2: Get container dimensions and padding after the initial layout
    const width = this.node.getComputedWidth()
    const height = this.node.getComputedHeight()
    const paddingLeft = this.node.getComputedPadding(Style.Edge.Left)
    const paddingRight = this.node.getComputedPadding(Style.Edge.Right)
    const paddingTop = this.node.getComputedPadding(Style.Edge.Top)
    const paddingBottom = this.node.getComputedPadding(Style.Edge.Bottom)

    // Calculate content box dimensions
    const contentWidth = Math.max(0, width - paddingLeft - paddingRight)
    const contentHeight = Math.max(0, height - paddingTop - paddingBottom)

    // Step 3: Validate dimensions needed for calculations
    if (!this.isVertical && contentWidth <= 0 && width > 0) {
      console.warn(
        `[GridNode ${this.props.key} - Finalize] Grid content width (${contentWidth}) is zero or negative after accounting for padding (${paddingLeft}+${paddingRight}) on total width ${width}. Cannot calculate basis.`,
      )
      if (this.columns > 1) return
    }
    if (this.isVertical && contentHeight <= 0 && height > 0) {
      console.warn(
        `[GridNode ${this.props.key} - Finalize] Grid content height (${contentHeight}) is zero or negative after accounting for padding (${paddingTop}+${paddingBottom}) on total height ${height}. Cannot calculate basis.`,
      )
      if (this.columns > 1) return
    }

    // Step 4: Calculate Gap Values in Pixels
    let columnGapPixels = 0
    if (typeof this.columnGapValue === 'number') {
      columnGapPixels = this.columnGapValue
    } else if (typeof this.columnGapValue === 'string' && this.columnGapValue.trim().endsWith('%')) {
      try {
        const percent = parseFloat(this.columnGapValue)
        if (!isNaN(percent) && contentWidth > 0) {
          columnGapPixels = (percent / 100) * contentWidth
        } else if (isNaN(percent)) {
          console.warn(
            `[GridNode ${this.props.key}] Invalid percentage column gap format: "${this.columnGapValue}". Using 0px.`,
          )
        } else if (contentWidth <= 0) {
          console.warn(
            `[GridNode ${this.props.key}] Cannot calculate percentage column gap (${this.columnGapValue}) because content width is zero. Using 0px.`,
          )
        }
      } catch (e) {
        console.warn(
          `[GridNode ${this.props.key}] Error parsing percentage column gap: "${this.columnGapValue}". Using 0px.`,
          e,
        )
      }
    } else if (typeof this.columnGapValue === 'string' && this.columnGapValue.trim() !== '') {
      console.warn(
        `[GridNode ${this.props.key}] Unsupported string column gap format: "${this.columnGapValue}". Using 0px. Only numbers and percentages ('%') are supported.`,
      )
    }

    let rowGapPixels = 0
    if (typeof this.rowGapValue === 'number') {
      rowGapPixels = this.rowGapValue
    } else if (typeof this.rowGapValue === 'string' && this.rowGapValue.trim().endsWith('%')) {
      try {
        const percent = parseFloat(this.rowGapValue)
        if (!isNaN(percent) && contentHeight > 0) {
          rowGapPixels = (percent / 100) * contentHeight
        } else if (isNaN(percent)) {
          console.warn(
            `[GridNode ${this.props.key}] Invalid percentage row gap format: "${this.rowGapValue}". Using 0px.`,
          )
        } else if (contentHeight <= 0) {
          console.warn(
            `[GridNode ${this.props.key}] Cannot calculate percentage row gap (${this.rowGapValue}) because content height is zero. Using 0px.`,
          )
        }
      } catch (e) {
        console.warn(
          `[GridNode ${this.props.key}] Error parsing percentage row gap: "${this.rowGapValue}". Using 0px.`,
          e,
        )
      }
    } else if (typeof this.rowGapValue === 'string' && this.rowGapValue.trim() !== '') {
      console.warn(
        `[GridNode ${this.props.key}] Unsupported string row gap format: "${this.rowGapValue}". Using 0px. Only numbers and percentages ('%') are supported.`,
      )
    }

    // Ensure gaps are not negative
    columnGapPixels = Math.max(0, columnGapPixels)
    rowGapPixels = Math.max(0, rowGapPixels)

    // Step 5: Calculate flex-basis percentage for children
    const mainAxisGapPixels = this.isVertical ? rowGapPixels : columnGapPixels
    const mainAxisContentSize = this.isVertical ? contentHeight : contentWidth
    let childWidth = 0

    if (mainAxisContentSize > 0 && this.columns > 0) {
      // Total space taken up by gaps on the main axis
      const totalGapSpaceOnMainAxis = this.columns > 1 ? mainAxisGapPixels * (this.columns - 1) : 0

      // Calculate the space available *only* for the items themselves
      const availableSpaceOnMainAxis = Math.max(0, mainAxisContentSize - totalGapSpaceOnMainAxis)

      // Calculate the exact pixel of the total content size that each item should occupy
      const exactItemWidth = availableSpaceOnMainAxis / this.columns

      // Ensure it's not negative (shouldn't happen, but safety)
      childWidth = Math.max(0, exactItemWidth - 0.5) // Slightly reduce to avoid rounding issues
    } else if (this.columns === 1) {
      // If only one column, it takes up the full basis (gaps don't apply)
      childWidth = mainAxisContentSize
    }

    // Clamp basis percentage between 0 and 100 (mostly redundant after floor/max(0) but safe)
    childWidth = Math.max(0, Math.min(mainAxisContentSize, childWidth))

    // Step 6: Apply layout properties to children
    let childrenNeedRecalculation = false
    for (const child of this.children) {
      let childChanged = false
      const currentLayoutWidth = child.node.getWidth()
      const currentWidthValue = currentLayoutWidth.value
      const currentWidthUnit = currentLayoutWidth.unit

      let widthNeedsUpdate = false
      if (currentWidthUnit === Style.Unit.Point) {
        // If current width is in points, check if the value is significantly different
        if (Math.abs(currentWidthValue - childWidth) > 0.01) {
          widthNeedsUpdate = true
        }
      } else {
        // If current width is not in points (e.g., Auto, Percent, Undefined), it needs to be set to points
        widthNeedsUpdate = true
      }

      if (widthNeedsUpdate) {
        child.node.setWidth(childWidth)
        childChanged = true
      }

      // Ensure grow/shrink are set correctly for grid items
      if (child.node.getFlexGrow() !== 0) {
        child.node.setFlexGrow(0)
        childChanged = true
      }
      if (child.node.getFlexShrink() !== 1) {
        child.node.setFlexShrink(1) // Allow shrinking
        childChanged = true
      }

      // Remove margins that might interfere with gap property
      if (child.node.getMargin(Style.Edge.Bottom).unit !== Style.Unit.Undefined) {
        child.node.setMargin(Style.Edge.Bottom, undefined)
        childChanged = true
      }
      if (child.node.getMargin(Style.Edge.Right).unit !== Style.Unit.Undefined) {
        child.node.setMargin(Style.Edge.Right, undefined)
        childChanged = true
      }
      if (child.node.getMargin(Style.Edge.Top).unit !== Style.Unit.Undefined) {
        child.node.setMargin(Style.Edge.Top, undefined)
        childChanged = true
      }
      if (child.node.getMargin(Style.Edge.Left).unit !== Style.Unit.Undefined) {
        child.node.setMargin(Style.Edge.Left, undefined)
        childChanged = true
      }

      if (childChanged && !child.node.isDirty()) {
        child.node.markDirty()
        childrenNeedRecalculation = true
      }
    }

    // Step 7: Apply gaps using Yoga's built-in gap properties
    const currentColumnGap = this.node.getGap(Style.Gutter.Column).value
    const currentRowGap = this.node.getGap(Style.Gutter.Row).value
    let gapsChanged = false

    // Use a small tolerance for comparing gap pixels
    if (Math.abs(currentColumnGap - columnGapPixels) > 0.001) {
      this.node.setGap(Style.Gutter.Column, columnGapPixels)
      gapsChanged = true
    }
    if (Math.abs(currentRowGap - rowGapPixels) > 0.001) {
      this.node.setGap(Style.Gutter.Row, rowGapPixels)
      gapsChanged = true
    }

    // Step 8: Mark the grid node itself as dirty if gaps changed or children changed
    if ((gapsChanged || childrenNeedRecalculation) && !this.node.isDirty()) {
      this.node.markDirty()
    }
  }
}

/**
 * Factory function to create a new GridNode instance.
 * @param props - Grid configuration properties.
 * @returns A new GridNode instance.
 */
export const Grid = (props: GridProps) => new GridNode(props)
