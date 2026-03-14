import type { GridProps, GridTrackSize, GridItemProps } from '@/canvas/canvas.type.js'
import { BoxNode, RowNode } from '@/canvas/layout.canvas.util.js'
import { Style } from '@/constant/common.const.js'
import { parsePercentage } from '@/canvas/canvas.helper.js'

/**
 * GridItem Node. Theoretically just a BoxNode but typed differently in factory.
 * In runtime, it behaves almost like a BoxNode, but we can detect it if needed,
 * or simply rely on the props being present in the instance.
 */
export class GridItemNode extends BoxNode {
  constructor(props: GridItemProps) {
    super({
      ...props,
      name: 'GridItem',
    })
  }
}

/**
 * Factory for GridItem.
 */
export const GridItem = (props: GridItemProps) => new GridItemNode(props)

/**
 * Grid layout node that arranges children in a 2D grid.
 * Implements a simplified version of the CSS Grid Layout algorithm.
 */
export class GridNode extends RowNode {
  /**
   * Creates a new grid layout node
   * @param props Grid configuration properties
   */
  constructor(props: GridProps) {
    super({
      ...props,
      name: props.name || 'Grid',
      flexWrap: Style.Wrap.Wrap,
    })
  }

  /**
   * Helper to parse a track size definition.
   */
  private parseTrack(track: GridTrackSize, availableSpace: number): { type: 'px' | '%' | 'fr' | 'auto'; value: number } {
    if (typeof track === 'number') {
      return { type: 'px', value: track }
    }
    if (track === 'auto') {
      return { type: 'auto', value: 0 }
    }
    if (typeof track === 'string') {
      if (track.endsWith('fr')) {
        return { type: 'fr', value: parseFloat(track) }
      }
      if (track.endsWith('%')) {
        return { type: '%', value: parsePercentage(track, availableSpace) }
      }
      if (track.endsWith('px')) {
        return { type: 'px', value: parseFloat(track) }
      }
      // Try parsing as number (px) if just string "100"
      const num = parseFloat(track)
      if (!isNaN(num)) return { type: 'px', value: num }
    }
    return { type: 'auto', value: 0 }
  }

  /**
   * Parses the gap property into pixels.
   */
  private getGapPixels(gap: GridProps['gap'], width: number, height: number) {
    let rowGap = 0
    let colGap = 0

    if (typeof gap === 'number') {
      rowGap = colGap = gap
    } else if (typeof gap === 'string') {
      const val = parsePercentage(gap, width) // Use width as base for simplicity if %
      rowGap = colGap = val
    } else if (gap && typeof gap === 'object') {
      const colVal = gap.Column ?? gap.All ?? 0
      const rowVal = gap.Row ?? gap.All ?? 0
      colGap = parsePercentage(colVal as string | number, width)
      rowGap = parsePercentage(rowVal as string | number, height)
    }

    return { rowGap, colGap }
  }

  /**
   * Update layout calculations after the initial layout is computed.
   */
  protected override updateLayoutBasedOnComputedSize() {
    // 1. Get Container Dimensions
    let width = this.node.getComputedWidth()
    const parent = this.node.getParent()

    if (parent) {
      const parentWidth = parent.getWidth()
      const parentMaxWidth = parent.getMaxWidth()

      // Case: Parent has % width but no maxWidth - we're likely expanding it
      if (parentWidth.unit === Style.Unit.Percent && (parentMaxWidth.unit === Style.Unit.Undefined || parentMaxWidth.unit === Style.Unit.Auto)) {
        const grandparent = parent.getParent()
        if (grandparent) {
          const intended = (parentWidth.value / 100) * grandparent.getComputedWidth()
          // Only constrain if we expanded beyond intended (don't shrink if already smaller)
          if (width > intended) {
            width = intended
          }
        }
      }
    }

    const paddingLeft = this.node.getComputedPadding(Style.Edge.Left)
    const paddingRight = this.node.getComputedPadding(Style.Edge.Right)
    const paddingTop = this.node.getComputedPadding(Style.Edge.Top)
    const paddingBottom = this.node.getComputedPadding(Style.Edge.Bottom)

    const contentWidth = Math.max(0, width - paddingLeft - paddingRight)
    const computedHeight = this.node.getComputedHeight()
    const contentHeight = Math.max(0, computedHeight - paddingTop - paddingBottom)

    const { templateColumns, templateRows, autoRows = 'auto', gap, columns } = this.props as GridProps

    // 2. Resolve Gaps
    const { rowGap, colGap } = this.getGapPixels(gap, contentWidth, contentHeight)

    // 3. Resolve Columns (Tracks)
    let explicitColTracks: GridTrackSize[] = templateColumns || []
    if (explicitColTracks.length === 0 && columns) {
      explicitColTracks = Array(columns).fill('1fr')
    }
    if (explicitColTracks.length === 0) explicitColTracks = ['1fr']

    const resolvedColTracks = this.resolveTracks(explicitColTracks, contentWidth, colGap)

    // Pre-calculate Col Offsets needed for placement/width
    const colOffsetsValues = [0]
    for (let i = 0; i < resolvedColTracks.length; i++) {
      colOffsetsValues.push(colOffsetsValues[i] + resolvedColTracks[i] + colGap)
    }

    // 4. Place Items & Resolve Explicit Row Tracks
    const explicitRowTracks = templateRows || []
    const resolvedExplicitRowTracks = this.resolveTracks(explicitRowTracks, contentHeight, rowGap)

    const cells: boolean[][] = [] // true if occupied
    const items: { node: BoxNode; rowStart: number; rowEnd: number; colStart: number; colEnd: number }[] = []

    const isOccupied = (r: number, c: number) => {
      if (!cells[r]) return false
      return cells[r][c] === true
    }
    const setOccupied = (r: number, c: number) => {
      if (!cells[r]) cells[r] = []
      cells[r][c] = true
    }

    let cursorRow = 0
    let cursorCol = 0

    for (const child of this.children) {
      const childProps = child.props as GridItemProps
      const { gridColumn, gridRow } = childProps

      let colStart: number | undefined
      let colEnd: number | undefined
      let colSpan = 1
      let rowStart: number | undefined
      let rowEnd: number | undefined
      let rowSpan = 1

      // ... Grid Placement Logic ...
      if (gridColumn) {
        const parts = gridColumn.split('/').map(s => s.trim())
        if (parts[0]) {
          if (parts[0].startsWith('span')) {
            colSpan = parseInt(parts[0].replace('span', '')) || 1
          } else {
            colStart = parseInt(parts[0]) - 1
          }
        }
        if (parts[1]) {
          if (parts[1].startsWith('span')) {
            const span = parseInt(parts[1].replace('span', '')) || 1
            if (colStart !== undefined) {
              colEnd = colStart + span
              colSpan = span
            } else {
              // If start is undefined but end is span? Unusual. Treat as span.
              colSpan = span
            }
          } else {
            colEnd = parseInt(parts[1]) - 1
            if (colStart !== undefined) {
              colSpan = colEnd - colStart
            }
          }
        }
      }

      if (gridRow) {
        const parts = gridRow.split('/').map(s => s.trim())
        if (parts[0]) {
          if (parts[0].startsWith('span')) {
            rowSpan = parseInt(parts[0].replace('span', '')) || 1
          } else {
            rowStart = parseInt(parts[0]) - 1
          }
        }
        if (parts[1]) {
          if (parts[1].startsWith('span')) {
            const span = parseInt(parts[1].replace('span', '')) || 1
            if (rowStart !== undefined) {
              rowEnd = rowStart + span
              rowSpan = span
            } else {
              rowSpan = span
            }
          } else {
            rowEnd = parseInt(parts[1]) - 1
            if (rowStart !== undefined) {
              rowSpan = rowEnd - rowStart
            }
          }
        }
      }

      if (colStart !== undefined && rowStart !== undefined) {
        // Fixed position: Check overlap in simpler V1? Or just place?
        // Just place.
      } else {
        // Auto placement
        let placed = false
        while (!placed) {
          if (!cells[cursorRow]) cells[cursorRow] = []

          if (colStart !== undefined) cursorCol = colStart

          let fits = true
          for (let r = 0; r < rowSpan; r++) {
            for (let c = 0; c < colSpan; c++) {
              if (isOccupied(cursorRow + r, cursorCol + c)) {
                fits = false
                break
              }
            }
            if (!fits) break
          }

          if (fits) {
            rowStart = cursorRow
            colStart = cursorCol
            placed = true
          } else {
            cursorCol++
            if (cursorCol + colSpan > resolvedColTracks.length) {
              cursorCol = 0
              cursorRow++
            }
          }
        }
        cursorCol += colSpan
        if (cursorCol >= resolvedColTracks.length) {
          cursorCol = 0
          cursorRow++
        }
      }

      rowEnd = (rowStart ?? 0) + rowSpan
      colEnd = (colStart ?? 0) + colSpan

      for (let r = rowStart!; r < rowEnd!; r++) {
        for (let c = colStart!; c < colEnd!; c++) {
          setOccupied(r, c)
        }
      }

      // CRITICAL FIX: Pre-set width on item to ensure height calculation is accurate later
      const itemColStart = colStart!
      const itemColEnd = colEnd!

      // Extend local offsets if needed for spanned columns beyond track count (rare but safe)
      while (colOffsetsValues.length <= itemColEnd) {
        colOffsetsValues.push(colOffsetsValues[colOffsetsValues.length - 1] + 0 + colGap)
      }

      const cs = Math.min(itemColStart, colOffsetsValues.length - 1)
      const ce = Math.min(itemColEnd, colOffsetsValues.length - 1)
      const targetWidth = Math.max(0, colOffsetsValues[ce] - colOffsetsValues[cs] - colGap)

      child.node.setWidth(targetWidth)
      child.node.calculateLayout(targetWidth, Number.NaN, Style.Direction.LTR)

      // Recursively finalize nested children (e.g. inner Grids) so their
      // computed heights are accurate before we measure row sizes.
      child.finalizeLayout()
      if (child.node.isDirty()) {
        child.node.calculateLayout(targetWidth, Number.NaN, Style.Direction.LTR)
      }

      items.push({ node: child, rowStart: rowStart!, rowEnd: rowEnd!, colStart: itemColStart, colEnd: itemColEnd })
    }

    // 6. Finalize Rows (Implicit)
    const totalRowsNeeded = Math.max(resolvedExplicitRowTracks.length, ...items.map(i => i.rowEnd))
    const resolvedRowTracks = [...resolvedExplicitRowTracks]

    // Fill implicit rows
    for (let r = resolvedExplicitRowTracks.length; r < totalRowsNeeded; r++) {
      let rowSize = 0

      // Better 'auto' handling:
      if (autoRows === 'auto') {
        const rowItems = items.filter(i => i.rowStart === r && i.rowEnd - i.rowStart === 1)
        for (const item of rowItems) {
          rowSize = Math.max(rowSize, item.node.node.getComputedHeight())
        }
      } else {
        const parsed = this.parseTrack(autoRows, contentHeight)
        rowSize = parsed.value
      }
      resolvedRowTracks.push(rowSize)
    }

    // 6. Calculate Offsets (Rows) & Final Layout Application
    const colOffsets = colOffsetsValues // Re-use
    const rowOffsets = [0]
    for (let i = 0; i < resolvedRowTracks.length; i++) {
      let size = resolvedRowTracks[i]
      // Re-check auto-sized explicit rows (value 0)
      if (size === 0) {
        const rowItems = items.filter(it => it.rowStart === i && it.rowEnd - it.rowStart === 1)
        for (const item of rowItems) {
          size = Math.max(size, item.node.node.getComputedHeight())
        }
        resolvedRowTracks[i] = size
      }
      rowOffsets.push(rowOffsets[i] + size + rowGap)
    }

    // 7. Apply Positions
    for (const item of items) {
      const x = colOffsets[item.colStart] + paddingLeft

      while (colOffsets.length <= item.colEnd) {
        colOffsets.push(colOffsets[colOffsets.length - 1] + 0 + colGap)
      }

      const widthStart = colOffsets[item.colStart]
      const widthEnd = colOffsets[item.colEnd]
      const totalWidth = Math.max(0, widthEnd - widthStart - colGap)

      const y = rowOffsets[item.rowStart] + paddingTop

      const heightStart = rowOffsets[item.rowStart]
      const heightEnd = rowOffsets[item.rowEnd]
      const totalHeight = Math.max(0, heightEnd - heightStart - rowGap)

      const childNode = item.node.node

      if (childNode.getPositionType() !== Style.PositionType.Absolute) {
        childNode.setPositionType(Style.PositionType.Absolute)
      }

      if (childNode.getPosition(Style.Edge.Left).value !== x) {
        childNode.setPosition(Style.Edge.Left, x)
      }
      if (childNode.getPosition(Style.Edge.Top).value !== y) {
        childNode.setPosition(Style.Edge.Top, y)
      }

      if (childNode.getWidth().unit !== Style.Unit.Point || Math.abs(childNode.getWidth().value - totalWidth) > 0.1) {
        childNode.setWidth(totalWidth)
      }
      if (childNode.getHeight().unit !== Style.Unit.Point || Math.abs(childNode.getHeight().value - totalHeight) > 0.1) {
        childNode.setHeight(totalHeight)
      }
    }

    // 9. Update Grid Height
    const totalGridHeight = Math.max(0, rowOffsets[rowOffsets.length - 1] - rowGap)
    const currentHeightStyle = this.node.getHeight()
    if (currentHeightStyle.unit === Style.Unit.Auto || currentHeightStyle.unit === Style.Unit.Undefined) {
      const targetTotalHeight = totalGridHeight + paddingTop + paddingBottom
      this.node.setHeight(targetTotalHeight)
    }
  }

  /**
   * Resolves track sizes to pixels.
   */
  private resolveTracks(tracks: GridTrackSize[], availableSpace: number, gap: number): number[] {
    const resolved: number[] = []
    let usedSpace = 0
    let totalFr = 0
    const frIndices: number[] = []

    tracks.forEach((t, i) => {
      const parsed = this.parseTrack(t, availableSpace)
      if (parsed.type === 'px' || parsed.type === '%') {
        resolved[i] = parsed.value
        usedSpace += parsed.value
      } else if (parsed.type === 'fr') {
        totalFr += parsed.value
        resolved[i] = 0
        frIndices.push(i)
      } else {
        resolved[i] = 0
      }
    })

    const totalGaps = Math.max(0, tracks.length - 1) * gap
    usedSpace += totalGaps

    const remainingSpace = Math.max(0, availableSpace - usedSpace)
    if (totalFr > 0) {
      frIndices.forEach(i => {
        const parsed = this.parseTrack(tracks[i], availableSpace)
        const share = (parsed.value / totalFr) * remainingSpace
        resolved[i] = share
      })
    }

    return resolved
  }
}

/**
 * Factory function to create a new GridNode instance.
 */
export const Grid = (props: GridProps) => new GridNode(props)
