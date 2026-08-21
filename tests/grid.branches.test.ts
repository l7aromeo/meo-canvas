import { GridNode, GridItemNode } from '@/canvas/grid.canvas.js'
import Yoga, { Style } from '@/constant/common.const.js'
import type { GridProps, GridItemProps } from '@/canvas/canvas.type.js'

/** Lays a grid out inside a fixed-size parent and runs the post-layout pass. */
function layoutGrid(gridProps: GridProps, items: GridItemProps[], parentWidth = 600, parentHeight = 400) {
  const grid = new GridNode(gridProps)
  items.forEach((props, index) => {
    ;(grid as any).appendChild(new GridItemNode({ height: 40, ...props }), index)
  })
  const parent = Yoga.Node.create()
  parent.setWidth(parentWidth)
  parent.setHeight(parentHeight)
  parent.insertChild(grid.node, 0)
  parent.calculateLayout(parentWidth, parentHeight, Style.Direction.LTR)
  grid.finalizeLayout()
  return grid
}

describe('GridNode — track parsing', () => {
  it.each([
    ['number', 120, { type: 'px', value: 120 }],
    ['auto', 'auto', { type: 'auto', value: 0 }],
    ['fr', '2fr', { type: 'fr', value: 2 }],
    ['px string', '80px', { type: 'px', value: 80 }],
    ['bare numeric string', '64', { type: 'px', value: 64 }],
    ['unrecognised string', 'wide', { type: 'auto', value: 0 }],
  ])('parses a %s track', (_label, track, expected) => {
    const grid = new GridNode({})
    expect((grid as any).parseTrack(track, 400)).toEqual(expected)
  })

  it('parses a percentage track against the available space', () => {
    const grid = new GridNode({})
    expect((grid as any).parseTrack('50%', 400)).toEqual({ type: '%', value: 200 })
  })

  it('treats a non-string, non-number track as auto', () => {
    const grid = new GridNode({})
    expect((grid as any).parseTrack(undefined, 400)).toEqual({ type: 'auto', value: 0 })
  })
})

describe('GridNode — gap parsing', () => {
  it('reads a single number as both gaps', () => {
    const grid = new GridNode({})
    expect((grid as any).getGapPixels(12, 600, 400)).toEqual({ rowGap: 12, colGap: 12 })
  })

  it('reads a percentage string against the width for both gaps', () => {
    const grid = new GridNode({})
    expect((grid as any).getGapPixels('10%', 600, 400)).toEqual({ rowGap: 60, colGap: 60 })
  })

  it('reads Row and Column separately', () => {
    const grid = new GridNode({})
    expect((grid as any).getGapPixels({ Row: 8, Column: 16 }, 600, 400)).toEqual({ rowGap: 8, colGap: 16 })
  })

  it('falls back to All for a missing axis', () => {
    const grid = new GridNode({})
    expect((grid as any).getGapPixels({ All: 6 }, 600, 400)).toEqual({ rowGap: 6, colGap: 6 })
  })

  it('treats an object with neither axis nor All as no gap', () => {
    const grid = new GridNode({})
    expect((grid as any).getGapPixels({}, 600, 400)).toEqual({ rowGap: 0, colGap: 0 })
  })

  it('treats an absent gap as no gap', () => {
    const grid = new GridNode({})
    expect((grid as any).getGapPixels(undefined, 600, 400)).toEqual({ rowGap: 0, colGap: 0 })
  })
})

describe('GridNode — item placement', () => {
  it.each([
    ['a bare column start', { gridColumn: '2' }],
    ['a column span', { gridColumn: 'span 2' }],
    ['a start and an end', { gridColumn: '1 / 3' }],
    ['a start and a span', { gridColumn: '2 / span 2' }],
    ['a span and an end', { gridColumn: 'span 2 / 4' }],
    ['a bare row start', { gridRow: '2' }],
    ['a row span', { gridRow: 'span 2' }],
    ['a row start and end', { gridRow: '1 / 3' }],
    ['a row start and span', { gridRow: '2 / span 2' }],
    ['both axes fixed', { gridColumn: '2 / 4', gridRow: '1 / 3' }],
    ['both axes spanning', { gridColumn: 'span 2', gridRow: 'span 2' }],
    ['a malformed span with no number', { gridColumn: 'span' }],
    ['a malformed end with no number', { gridColumn: '1 / ' }],
  ])('places an item given %s', (_label, itemProps) => {
    const grid = layoutGrid({ columns: 4, gap: 8 }, [itemProps, { gridColumn: undefined }, {}])
    expect(grid.children.length).toBe(3)
    for (const child of grid.children) {
      expect(child.node.getComputedWidth()).toBeGreaterThanOrEqual(0)
    }
  })

  it('flows items that carry no placement at all', () => {
    const grid = layoutGrid({ columns: 3, gap: 4 }, [{}, {}, {}, {}, {}])
    expect(grid.children.length).toBe(5)
  })

  it('wraps the cursor onto a new row when a span will not fit', () => {
    const grid = layoutGrid({ columns: 2 }, [{ gridColumn: 'span 2' }, { gridColumn: 'span 2' }])
    expect(grid.children.length).toBe(2)
  })

  it('places items around one pinned to a fixed cell', () => {
    const grid = layoutGrid({ columns: 3 }, [{ gridColumn: '2', gridRow: '1' }, {}, {}, {}])
    expect(grid.children.length).toBe(4)
  })
})

describe('GridNode — track templates', () => {
  it.each([
    ['fr tracks', { templateColumns: ['1fr', '2fr'] }],
    ['mixed px and fr', { templateColumns: [100, '1fr'] }],
    ['percentage tracks', { templateColumns: ['25%', '75%'] }],
    ['auto tracks', { templateColumns: ['auto', 'auto'] }],
    ['explicit rows', { templateColumns: ['1fr'], templateRows: [60, '1fr'] }],
    ['more items than tracks', { templateColumns: ['1fr', '1fr'] }],
  ])('lays out with %s', (_label, props) => {
    const grid = layoutGrid({ ...props, gap: 10 } as GridProps, [{}, {}, {}, {}])
    expect(grid.children.length).toBe(4)
  })

  it('lays out with no columns and no template at all', () => {
    const grid = layoutGrid({}, [{}, {}])
    expect(grid.children.length).toBe(2)
  })

  it('survives a zero-width parent', () => {
    const grid = layoutGrid({ columns: 2, gap: 4 }, [{}, {}], 0, 0)
    expect(grid.children.length).toBe(2)
  })
})

describe('GridNode — placement combinations', () => {
  it.each([
    ['a span paired with an end', { gridColumn: 'span 2 / 5' }],
    ['a start paired with a span on rows', { gridRow: '2 / span 3' }],
    ['a span paired with an end on rows', { gridRow: 'span 2 / 5' }],
    ['a span on both halves', { gridColumn: 'span 2 / span 3' }],
    ['a span on both halves of a row', { gridRow: 'span 2 / span 3' }],
    ['an end before its start', { gridColumn: '4 / 2' }],
    ['a row end before its start', { gridRow: '4 / 2' }],
    ['a column fixed with a row span', { gridColumn: '2', gridRow: 'span 2' }],
    ['a row fixed with a column span', { gridRow: '2', gridColumn: 'span 2' }],
    ['a span wider than the grid', { gridColumn: 'span 9' }],
    ['a start past the last track', { gridColumn: '9' }],
  ])('places an item given %s', (_label, itemProps) => {
    const grid = layoutGrid({ columns: 4, gap: 6 }, [itemProps, {}, {}])
    expect(grid.children.length).toBe(3)
  })

  it('fills around several pinned items', () => {
    const grid = layoutGrid({ columns: 3 }, [{ gridColumn: '1', gridRow: '1' }, { gridColumn: '3', gridRow: '1' }, {}, {}, { gridColumn: 'span 2' }])
    expect(grid.children.length).toBe(5)
  })

  it('lays out explicit rows alongside implicit ones', () => {
    const grid = layoutGrid({ templateColumns: ['1fr', '1fr'], templateRows: [40] }, [{}, {}, {}, {}])
    expect(grid.children.length).toBe(4)
  })

  it('lays out with a row gap and a column gap that differ', () => {
    const grid = layoutGrid({ columns: 2, gap: { Row: 4, Column: 20 } }, [{}, {}, {}, {}])
    expect(grid.children.length).toBe(4)
  })

  it('lays out a grid with padding around it', () => {
    const grid = layoutGrid({ columns: 2, gap: 8, padding: 16 } as any, [{}, {}, {}])
    expect(grid.children.length).toBe(3)
  })

  it('lays out a single column', () => {
    const grid = layoutGrid({ columns: 1 }, [{}, {}, {}])
    expect(grid.children.length).toBe(3)
  })

  it('lays out an empty grid', () => {
    const grid = layoutGrid({ columns: 3 }, [])
    expect(grid.children.length).toBe(0)
  })

  it('lays out items taller than one row', () => {
    const grid = layoutGrid({ columns: 2, gap: 4 }, [{ height: 120 }, { height: 20 }, { height: 60 }])
    expect(grid.children.length).toBe(3)
  })
})
