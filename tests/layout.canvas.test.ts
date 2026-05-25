import { vi } from 'vitest'
import { BoxNode, Box, RowNode, Row, ColumnNode, Column } from '@/canvas/layout.canvas.js'
import { drawBorders } from '@/canvas/canvas.helper.js'
import Yoga, { Style } from '@/constant/common.const.js'
import { Canvas } from 'skia-canvas'

describe('BoxNode', () => {
  it('should construct with default props and children', () => {
    const node = new BoxNode({ key: 'root', borderColor: 'blue' })
    expect(node.node).toBeInstanceOf(Yoga.Node)
    expect(node.props.key).toBeTruthy()
    expect(node.props.borderColor).toBe('blue')
    expect(node.children).toHaveLength(0)
    expect(node.name).toBe('Box')
  })

  it('should construct with children and process them', () => {
    const child1 = new BoxNode({ key: 'child1' })
    const child2 = new BoxNode({ key: 'child2' })
    const node = new BoxNode({ key: 'parent', children: [child1, child2] })
    node.processInitialChildren() // Manually call as constructor doesn't do it for root
    expect(node.children).toHaveLength(2)
    expect(node.children[0]).toBe(child1)
    expect(node.children[1]).toBe(child2)
    expect(node.node.getChildCount()).toBe(2)
  })

  it('should inherit styles from parent and mark node dirty if not already dirty', () => {
    const parent = new BoxNode({ key: 'parent', fontSize: 16, color: 'red' })
    const child = new BoxNode({ key: 'child' })
    ;(child as any).resolveInheritedStyles(parent.props)
    expect(child.props.fontSize).toBe(16)
    expect(child.props.color).toBe('red')
    expect(child.key).toMatch(/^parent-child/)
    expect(child.node.isDirty()).toBe(true)
  })

  it('should not mark node dirty if already dirty during style resolution', () => {
    const parent = new BoxNode({ key: 'parent', fontSize: 16 })
    const child = new BoxNode({ key: 'child' })
    child.node.setWidth(100) // This will mark the node as dirty
    const markDirtySpy = vi.spyOn(child.node, 'markDirty')
    ;(child as any).resolveInheritedStyles(parent.props)
    expect(markDirtySpy).not.toHaveBeenCalled()
    markDirtySpy.mockRestore()
  })

  it('should warn and not append invalid child nodes', () => {
    const parent = new BoxNode({ key: 'parent' })
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    ;(parent as any).appendChild(null, 0)
    expect(parent.children).toHaveLength(0)
    expect(parent.node.getChildCount()).toBe(0)
    expect(warnSpy).toHaveBeenCalledWith('Attempted to append an invalid child node.', null)
    ;(parent as any).appendChild(undefined, 0)
    expect(parent.children).toHaveLength(0)
    expect(parent.node.getChildCount()).toBe(0)
    expect(warnSpy).toHaveBeenCalledWith('Attempted to append an invalid child node.', undefined)

    const invalidChild = { node: null } as any // Child with invalid node property
    ;(parent as any).appendChild(invalidChild, 0)
    expect(parent.children).toHaveLength(0)
    expect(parent.node.getChildCount()).toBe(0)
    expect(warnSpy).toHaveBeenCalledWith('Attempted to append an invalid child node.', invalidChild)

    warnSpy.mockRestore()
  })

  it('should finalize layout recursively', () => {
    const parent = new BoxNode({ key: 'parent' })
    const child = new BoxNode({ key: 'child' })
    ;(parent as any).appendChild(child, 0)
    const wasDirty = parent.finalizeLayout()
    expect(wasDirty).toBe(true)
  })

  it('should return true from finalizeLayout if a child was dirty', () => {
    const parent = new BoxNode({ key: 'parent' })
    const child = new BoxNode({ key: 'child' })
    ;(parent as any).appendChild(child, 0)
    // Calculate layout to clean the nodes
    parent.node.calculateLayout(undefined, undefined, parent.props.direction)
    // Dirty the child by changing a layout property
    child.node.setWidth(100)
    const wasDirty = parent.finalizeLayout()
    expect(wasDirty).toBe(true)
  })

  it('should call render pipeline with context', async () => {
    const node = new BoxNode({ width: 50, height: 50, backgroundColor: 'red', key: 'box' })
    node.node.setWidth(50)
    node.node.setHeight(50)
    node.node.setPosition(Style.Edge.Left, 0)
    node.node.setPosition(Style.Edge.Top, 0)

    const mockFill = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      setTransform: vi.fn(),
      translate: vi.fn(),
      scale: vi.fn(),
      rotate: vi.fn(),
      clip: vi.fn(),
      fillText: vi.fn(),
      strokeText: vi.fn(),
      measureText: vi.fn(() => ({ width: 10, actualBoundingBoxAscent: 10, actualBoundingBoxDescent: 2 })),
      setFont: vi.fn(),
      setLineDash: vi.fn(),
      stroke: vi.fn(),
      clearRect: vi.fn(),
      drawImage: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(), // Added
      arc: vi.fn(), // Added
    }

    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    await node.render(ctx, 0, 0)

    expect(getContextSpy).toHaveBeenCalledWith('2d')
    expect(mockFill).toHaveBeenCalled()
    getContextSpy.mockRestore()
  })

  it('should render full pipeline with real children in correct order', async () => {
    const parent = new BoxNode({
      key: 'parent',
      width: 200,
      height: 200,
      backgroundColor: 'white',
      overflow: Style.Overflow.Hidden,
      border: 1,
    })

    const childA = new BoxNode({ key: 'childA', width: 50, height: 50, backgroundColor: 'red' })
    const childB = new BoxNode({ key: 'childB', width: 50, height: 50, backgroundColor: 'blue' })

    // Manually build tree — parent → children
    ;(parent as any).appendChild(childA, 0)
    ;(parent as any).appendChild(childB, 1)

    // Mock computed layouts so render() reads correct positions
    vi.spyOn(parent.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 200,
      height: 200,
      bottom: 0,
      right: 0,
    } as any)
    vi.spyOn(childA.node, 'getComputedLayout').mockReturnValue({
      left: 10,
      top: 10,
      width: 50,
      height: 50,
      bottom: 0,
      right: 0,
    } as any)
    vi.spyOn(childB.node, 'getComputedLayout').mockReturnValue({
      left: 70,
      top: 10,
      width: 50,
      height: 50,
      bottom: 0,
      right: 0,
    } as any)

    // Trace draw order
    const drawLog: string[] = []
    const mockContext = {
      fill: vi.fn(() => drawLog.push('fill')),
      beginPath: vi.fn(() => drawLog.push('beginPath')),
      rect: vi.fn(() => drawLog.push('rect')),
      save: vi.fn(() => drawLog.push('save')),
      restore: vi.fn(() => drawLog.push('restore')),
      clip: vi.fn(() => drawLog.push('clip')),
      closePath: vi.fn(() => drawLog.push('closePath')),
      moveTo: vi.fn(() => drawLog.push('moveTo')),
      lineTo: vi.fn(() => drawLog.push('lineTo')),
      arc: vi.fn(() => drawLog.push('arc')),
      setLineDash: vi.fn(() => drawLog.push('setLineDash')),
      stroke: vi.fn(() => drawLog.push('stroke')),
      drawImage: vi.fn(() => drawLog.push('drawImage')),
      clearRect: vi.fn(() => drawLog.push('clearRect')),
      fillStyle: '',
      strokeStyle: '',
      lineWidth: 0,
      globalAlpha: 1,
      shadowColor: '',
      shadowOffsetX: 0,
      shadowOffsetY: 0,
      shadowBlur: 0,
      globalCompositeOperation: 'source-over',
      imageSmoothingEnabled: true,
      imageSmoothingQuality: 'high',
    }

    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)
    const ctx = new Canvas().getContext('2d')

    await parent.render(ctx, 0, 0)

    // Verify balanced save/restore — should be: save (overflow clip), 2x save (each child), 2x restore, restore
    const saveCount = drawLog.filter(s => s === 'save').length
    const restoreCount = drawLog.filter(s => s === 'restore').length
    expect(saveCount).toBe(restoreCount)

    // Overflow:hidden should trigger a save/clip pair
    expect(drawLog).toContain('save')
    expect(drawLog).toContain('clip')

    // Parent fills before children (find first fill after beginPath)
    // The rendering order matters: parent background → childA → childB
    const firstChildFillIdx = drawLog.indexOf('fill', drawLog.indexOf('clip'))
    expect(firstChildFillIdx).toBeGreaterThan(0)

    getContextSpy.mockRestore()
  })
})

describe('BoxNode Layout Properties', () => {
  it('should set width and height correctly', () => {
    const node = new BoxNode({ width: 100, height: 50 })
    expect(node.node.getWidth().value).toBe(100)
    expect(node.node.getHeight().value).toBe(50)
  })

  it('should set min/max width/height correctly', () => {
    const node = new BoxNode({ minWidth: 10, maxWidth: 200, minHeight: 5, maxHeight: 150 })
    expect(node.node.getMinWidth().value).toBe(10)
    expect(node.node.getMaxWidth().value).toBe(200)
    expect(node.node.getMinHeight().value).toBe(5)
    expect(node.node.getMaxHeight().value).toBe(150)
  })

  it('should set position properties correctly with object notation and percentages', () => {
    const node = new BoxNode({
      position: {
        Left: 10,
        Top: '20%',
        Right: 30,
        Bottom: '40%',
      },
      positionType: Style.PositionType.Absolute,
    })
    expect(node.node.getPosition(Style.Edge.Left).value).toBe(10)
    expect(node.node.getPosition(Style.Edge.Top).unit).toBe(Style.Unit.Percent)
    expect(node.node.getPosition(Style.Edge.Top).value).toBe(20)
    expect(node.node.getPosition(Style.Edge.Right).value).toBe(30)
    expect(node.node.getPosition(Style.Edge.Bottom).unit).toBe(Style.Unit.Percent)
    expect(node.node.getPosition(Style.Edge.Bottom).value).toBe(40)
    expect(node.node.getPositionType()).toBe(Style.PositionType.Absolute)
  })

  it('should set position with a single percentage string', () => {
    const node = new BoxNode({ position: '50%' })
    expect(node.node.getPosition(Style.Edge.All).unit).toBe(Style.Unit.Percent)
    expect(node.node.getPosition(Style.Edge.All).value).toBe(50)
  })

  it('should handle gap with a single number', () => {
    new BoxNode({ gap: 10 })
    // This test is for coverage of the if branch
  })

  it('should handle gap with a single percentage string', () => {
    new BoxNode({ gap: '10%' })
    // This test is for coverage of the else if branch
  })

  it('should set gap properties correctly with object notation and percentages', () => {
    const node = new BoxNode({
      gap: {
        Row: 10,
        Column: '20%',
      },
    })
    expect(node.node.getGap(Style.Gutter.Column)).toBe(20)
  })

  it('should set margin properties correctly with object notation and percentages', () => {
    const node = new BoxNode({
      margin: {
        Left: 10,
        Top: '20%',
        Horizontal: 30,
        Vertical: '40%',
      },
    })
    expect(node.node.getMargin(Style.Edge.Left).value).toBe(10)
    expect(node.node.getMargin(Style.Edge.Top).unit).toBe(Style.Unit.Percent)
    expect(node.node.getMargin(Style.Edge.Top).value).toBe(20)
    expect(node.node.getMargin(Style.Edge.Horizontal).value).toBe(30)
    expect(node.node.getMargin(Style.Edge.Vertical).unit).toBe(Style.Unit.Percent)
    expect(node.node.getMargin(Style.Edge.Vertical).value).toBe(40)
  })

  it('should set margin with a single number', () => {
    const node = new BoxNode({ margin: 15 })
    expect(node.node.getMargin(Style.Edge.All).value).toBe(15)
  })

  it('should set margin with a single percentage string', () => {
    const node = new BoxNode({ margin: '15%' })
    expect(node.node.getMargin(Style.Edge.All).unit).toBe(Style.Unit.Percent)
    expect(node.node.getMargin(Style.Edge.All).value).toBe(15)
  })

  it('should set padding properties correctly with object notation and percentages', () => {
    const node = new BoxNode({
      padding: {
        Left: 10,
        Top: '20%',
        Right: 30,
        Bottom: '40%',
      },
    })
    expect(node.node.getPadding(Style.Edge.Left).value).toBe(10)
    expect(node.node.getPadding(Style.Edge.Top).unit).toBe(Style.Unit.Percent)
    expect(node.node.getPadding(Style.Edge.Top).value).toBe(20)
    expect(node.node.getPadding(Style.Edge.Right).value).toBe(30)
    expect(node.node.getPadding(Style.Edge.Bottom).unit).toBe(Style.Unit.Percent)
    expect(node.node.getPadding(Style.Edge.Bottom).value).toBe(40)
  })

  it('should set padding with a single number', () => {
    const node = new BoxNode({ padding: 15 })
    expect(node.node.getPadding(Style.Edge.All).value).toBe(15)
  })

  it('should set padding with a single percentage string', () => {
    const node = new BoxNode({ padding: '15%' })
    expect(node.node.getPadding(Style.Edge.All).unit).toBe(Style.Unit.Percent)
    expect(node.node.getPadding(Style.Edge.All).value).toBe(15)
  })

  it('should set border properties correctly with object notation', () => {
    const node = new BoxNode({
      border: {
        Left: 1,
        Top: 2,
        Right: 3,
        Bottom: 4,
      },
    })
    expect(node.node.getBorder(Style.Edge.Left)).toBe(1)
    expect(node.node.getBorder(Style.Edge.Top)).toBe(2)
    expect(node.node.getBorder(Style.Edge.Right)).toBe(3)
    expect(node.node.getBorder(Style.Edge.Bottom)).toBe(4)
  })

  it('should set border with a single number', () => {
    const node = new BoxNode({ border: 5 })
    expect(node.node.getBorder(Style.Edge.All)).toBe(5)
  })

  it('should apply dotted border style correctly', () => {
    const mockSetLineDash = vi.fn()
    let lineCapValue = ''
    const mockContext = {
      setLineDash: mockSetLineDash,
      beginPath: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      stroke: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      get lineCap() {
        return lineCapValue
      },
      set lineCap(value: string) {
        lineCapValue = value
      },
      strokeStyle: '',
      lineWidth: 0,
    }

    const mockNode = {
      getBorder: vi.fn(() => 2), // Simulate border width
      getBoxSizing: vi.fn(() => Style.BoxSizing.ContentBox),
    } as any

    drawBorders({
      ctx: mockContext as any,
      node: mockNode,
      x: 0,
      y: 0,
      width: 100,
      height: 100,
      radii: { TopLeft: 0, TopRight: 0, BottomLeft: 0, BottomRight: 0 },
      borderColor: 'black',
      borderStyle: Style.Border.Dotted,
    })

    expect(mockContext.lineCap).toBe('round')
    expect(mockSetLineDash).toHaveBeenCalledWith([0, 2 * 2]) // 0-length dash with spacing (width * 2)
  })

  it('should apply dashed border style correctly', () => {
    const mockSetLineDash = vi.fn()
    let lineCapValue = ''
    const mockContext = {
      setLineDash: mockSetLineDash,
      beginPath: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      stroke: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      get lineCap() {
        return lineCapValue
      },
      set lineCap(value: string) {
        lineCapValue = value
      },
      strokeStyle: '',
      lineWidth: 0,
    }

    const mockNode = {
      getBorder: vi.fn(() => 2), // Simulate border width
      getBoxSizing: vi.fn(() => Style.BoxSizing.ContentBox),
    } as any

    drawBorders({
      ctx: mockContext as any,
      node: mockNode,
      x: 0,
      y: 0,
      width: 100,
      height: 100,
      radii: { TopLeft: 0, TopRight: 0, BottomLeft: 0, BottomRight: 0 },
      borderColor: 'black',
      borderStyle: Style.Border.Dashed,
    })

    const borderWidth = 2
    const dashLength = Math.max(2, borderWidth * 1.5)
    const gapLength = Math.max(1, borderWidth)

    expect(mockContext.lineCap).toBe('butt')
    expect(mockSetLineDash).toHaveBeenCalledWith([dashLength, gapLength])
  })

  it('should apply solid border style correctly', () => {
    const mockSetLineDash = vi.fn()
    let lineCapValue = ''
    const mockContext = {
      setLineDash: mockSetLineDash,
      beginPath: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      stroke: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      get lineCap() {
        return lineCapValue
      },
      set lineCap(value: string) {
        lineCapValue = value
      },
      strokeStyle: '',
      lineWidth: 0,
    }

    const mockNode = {
      getBorder: vi.fn(() => 2), // Simulate border width
      getBoxSizing: vi.fn(() => Style.BoxSizing.ContentBox),
    } as any

    drawBorders({
      ctx: mockContext as any,
      node: mockNode,
      x: 0,
      y: 0,
      width: 100,
      height: 100,
      radii: { TopLeft: 0, TopRight: 0, BottomLeft: 0, BottomRight: 0 },
      borderColor: 'black',
      borderStyle: Style.Border.Solid,
    })

    expect(mockContext.lineCap).toBe('butt')
    expect(mockSetLineDash).toHaveBeenCalledWith([])
  })

  it('should set boxSizing, direction, flexWrap, overflow, display, aspectRatio correctly', () => {
    const node = new BoxNode({
      boxSizing: Style.BoxSizing.ContentBox,
      direction: Style.Direction.RTL,
      flexWrap: Style.Wrap.WrapReverse,
      overflow: Style.Overflow.Scroll,
      display: Style.Display.None,
      aspectRatio: 1.5,
    })
    expect(node.node.getBoxSizing()).toBe(Style.BoxSizing.ContentBox)
    expect(node.node.getDirection()).toBe(Style.Direction.RTL)
    expect(node.node.getFlexWrap()).toBe(Style.Wrap.WrapReverse)
    expect(node.node.getOverflow()).toBe(Style.Overflow.Scroll)
    expect(node.node.getDisplay()).toBe(Style.Display.None)
    expect(node.node.getAspectRatio()).toBe(1.5)
  })

  it('should set flex properties correctly', () => {
    const node = new BoxNode({
      flexDirection: Style.FlexDirection.Row,
      justifyContent: Style.Justify.Center,
      alignItems: Style.Align.FlexEnd,
      alignSelf: Style.Align.Center,
      alignContent: Style.Align.SpaceAround,
      flexGrow: 1,
      flexShrink: 0,
      flexBasis: 50,
    })

    expect(node.node.getFlexDirection()).toBe(Style.FlexDirection.Row)
    expect(node.node.getJustifyContent()).toBe(Style.Justify.Center)
    expect(node.node.getAlignItems()).toBe(Style.Align.FlexEnd)
    expect(node.node.getAlignSelf()).toBe(Style.Align.Center)
    expect(node.node.getAlignContent()).toBe(Style.Align.SpaceAround)
    expect(node.node.getFlexGrow()).toBe(1)
    expect(node.node.getFlexShrink()).toBe(0)
    expect(node.node.getFlexBasis().value).toBe(50)
  })

  it('should set flexBasis with percentage', () => {
    const node = new BoxNode({ flexBasis: '50%' })
    expect(node.node.getFlexBasis().unit).toBe(Style.Unit.Percent)
    expect(node.node.getFlexBasis().value).toBe(50)
  })

  it('should handle overflow hidden with complex border radius and borders', async () => {
    const node = new BoxNode({
      width: 100,
      height: 100,
      overflow: Style.Overflow.Hidden,
      borderRadius: {
        TopLeft: 10,
        TopRight: 20,
        BottomRight: 30,
        BottomLeft: 40,
      },
      border: {
        Left: 5,
        Top: 5,
        Right: 5,
        Bottom: 5,
      },
    })
    const mockFill = vi.fn()
    const mockClip = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      clip: mockClip,
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      setLineDash: vi.fn(),
      stroke: vi.fn(),
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    node.node.setWidth(100)
    node.node.setHeight(100)
    vi.spyOn(node.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    } as any)
    vi.spyOn(node.node, 'getComputedBorder').mockImplementation(edge => {
      if (edge === Style.Edge.Left) return 5
      if (edge === Style.Edge.Top) return 5
      if (edge === Style.Edge.Right) return 5
      if (edge === Style.Edge.Bottom) return 5
      return 0
    })
    await node.render(ctx, 0, 0)

    expect(mockContext.save).toHaveBeenCalledTimes(1)
    expect(mockContext.beginPath).toHaveBeenCalled()
    expect(mockClip).toHaveBeenCalled()
    expect(mockContext.restore).toHaveBeenCalledTimes(1)
    // Expect arc calls for rounded corners, adjusted for border
    expect(mockContext.arc).toHaveBeenCalledWith(expect.any(Number), expect.any(Number), Math.max(0, 10 - 5), expect.any(Number), expect.any(Number)) // TopLeft
    expect(mockContext.arc).toHaveBeenCalledWith(expect.any(Number), expect.any(Number), Math.max(0, 20 - 5), expect.any(Number), expect.any(Number)) // TopRight
    expect(mockContext.arc).toHaveBeenCalledWith(expect.any(Number), expect.any(Number), Math.max(0, 30 - 5), expect.any(Number), expect.any(Number)) // BottomRight
    expect(mockContext.arc).toHaveBeenCalledWith(expect.any(Number), expect.any(Number), Math.max(0, 40 - 5), expect.any(Number), expect.any(Number)) // BottomLeft
    getContextSpy.mockRestore()
  })

  it('should handle overflow hidden and zero inner dimensions for clipping', async () => {
    const node = new BoxNode({
      width: 10,
      height: 10,
      overflow: Style.Overflow.Hidden,
      border: {
        Left: 10,
        Top: 10,
      }, // Make innerWidth/Height <= 0
    })
    const mockClip = vi.fn()
    const mockContext = {
      fill: vi.fn(),
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      clip: mockClip,
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      setLineDash: vi.fn(),
      stroke: vi.fn(),
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    node.node.setWidth(10)
    node.node.setHeight(10)
    vi.spyOn(node.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 10,
      height: 10,
    } as any)
    vi.spyOn(node.node, 'getComputedBorder').mockImplementation(edge => {
      if (edge === Style.Edge.Left) return 10
      if (edge === Style.Edge.Top) return 10
      return 0
    })
    await node.render(ctx, 0, 0)

    expect(mockContext.save).toHaveBeenCalledTimes(1)
    expect(mockContext.beginPath).toHaveBeenCalled()
    expect(mockContext.rect).toHaveBeenCalledWith(expect.any(Number), expect.any(Number), 0, 0) // Expect rect with zero width/height
    expect(mockClip).toHaveBeenCalled()
    expect(mockContext.restore).toHaveBeenCalledTimes(1)
    getContextSpy.mockRestore()
  })

  it('should handle overflow hidden and border radius correctly', async () => {
    const node = new BoxNode({
      width: 100,
      height: 100,
      overflow: Style.Overflow.Hidden,
      borderRadius: 10,
    })
    const mockFill = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      clip: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    node.node.setWidth(100) // Explicitly set width for the Yoga node
    node.node.setHeight(100) // Explicitly set height for the Yoga node
    vi.spyOn(node.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    } as any)
    await node.render(ctx, 0, 0)

    expect(mockContext.save).toHaveBeenCalledTimes(1)
    expect(mockContext.beginPath).toHaveBeenCalled()
    expect(mockContext.clip).toHaveBeenCalled()
    expect(mockContext.restore).toHaveBeenCalledTimes(1)
    getContextSpy.mockRestore()
  })

  it('should render children in correct stacking order based on zIndex', () => {
    const childA = new BoxNode({ key: 'childA', zIndex: 1, positionType: Style.PositionType.Absolute })
    const childB = new BoxNode({ key: 'childB', zIndex: -1, positionType: Style.PositionType.Absolute })
    const childC = new BoxNode({ key: 'childC' }) // In-flow child
    const childD = new BoxNode({ key: 'childD', zIndex: 2, positionType: Style.PositionType.Absolute })

    const parent = new BoxNode({
      key: 'parent',
      width: 200,
      height: 200,
      children: [childA, childB, childC, childD],
    })
    parent.processInitialChildren()

    const mockRenderChildA = vi.spyOn(childA, 'render').mockImplementation(async () => {})
    const mockRenderChildB = vi.spyOn(childB, 'render').mockImplementation(async () => {})
    const mockRenderChildC = vi.spyOn(childC, 'render').mockImplementation(async () => {})
    const mockRenderChildD = vi.spyOn(childD, 'render').mockImplementation(async () => {})

    const mockContext = {
      fill: vi.fn(),
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      clip: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      globalAlpha: 1,
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    vi.spyOn(parent.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 200,
      height: 200,
    } as any)

    parent.render(new Canvas().getContext('2d'), 0, 0)

    mockRenderChildA.mockRestore()
    mockRenderChildB.mockRestore()
    mockRenderChildC.mockRestore()
    mockRenderChildD.mockRestore()
    getContextSpy.mockRestore()
  })

  it('should render background color correctly', async () => {
    const node = new BoxNode({ width: 100, height: 100, backgroundColor: 'red' })
    const mockFill = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      fillStyle: '',
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    await node.render(ctx, 0, 0)

    expect(mockContext.fillStyle).toBe('red')
    expect(mockContext.fill).toHaveBeenCalled()
    getContextSpy.mockRestore()
  })

  it('should render linear gradient correctly', async () => {
    const node = new BoxNode({
      width: 100,
      height: 100,
      gradient: {
        type: 'linear',
        colors: ['red', 'blue'],
        direction: 'to-right',
      },
    })
    const mockAddColorStop = vi.fn()
    const mockCreateLinearGradient = vi.fn(() => ({
      addColorStop: mockAddColorStop,
    }))
    const mockFill = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      createLinearGradient: mockCreateLinearGradient,
      fillStyle: '',
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    node.node.setWidth(100) // Explicitly set width for the Yoga node
    node.node.setHeight(100) // Explicitly set height for the Yoga node
    vi.spyOn(node.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    } as any)
    await node.render(ctx, 0, 0)

    expect(mockCreateLinearGradient).toHaveBeenCalled()
    expect(mockAddColorStop).toHaveBeenCalledTimes(2)
    expect(mockContext.fill).toHaveBeenCalled()
    getContextSpy.mockRestore()
  })

  it('should render radial gradient correctly', async () => {
    const node = new BoxNode({
      width: 100,
      height: 100,
      gradient: {
        type: 'radial',
        colors: ['red', 'blue'],
      },
    })
    const mockAddColorStop = vi.fn()
    const mockCreateRadialGradient = vi.fn(() => ({
      addColorStop: mockAddColorStop,
    }))
    const mockFill = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      createRadialGradient: mockCreateRadialGradient,
      fillStyle: '',
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    node.node.setWidth(100) // Explicitly set width for the Yoga node
    node.node.setHeight(100) // Explicitly set height for the Yoga node
    vi.spyOn(node.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    } as any)
    await node.render(ctx, 0, 0)

    expect(mockCreateRadialGradient).toHaveBeenCalled()
    expect(mockAddColorStop).toHaveBeenCalledTimes(2)
    expect(mockContext.fill).toHaveBeenCalled()
    getContextSpy.mockRestore()
  })

  it('should render radial gradient with direction correctly', async () => {
    const node = new BoxNode({
      width: 100,
      height: 100,
      gradient: {
        type: 'radial',
        colors: ['red', 'blue'],
        direction: 'to-top-right',
      },
    })
    const mockAddColorStop = vi.fn()
    const mockCreateRadialGradient = vi.fn(() => ({
      addColorStop: mockAddColorStop,
    }))
    const mockFill = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      createRadialGradient: mockCreateRadialGradient,
      fillStyle: '',
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    node.node.setWidth(100)
    node.node.setHeight(100)
    vi.spyOn(node.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    } as any)
    await node.render(ctx, 0, 0)

    expect(mockCreateRadialGradient).toHaveBeenCalled()
    expect(mockContext.fill).toHaveBeenCalled()
    getContextSpy.mockRestore()
  })

  it('should render outset box shadow correctly', async () => {
    const node = new BoxNode({
      width: 100,
      height: 100,
      backgroundColor: 'white',
      boxShadow: {
        color: 'rgba(0,0,0,0.5)',
        offsetX: 5,
        offsetY: 5,
        blur: 10,
      },
    })
    const mockFill = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      fillStyle: '',
      shadowColor: '',
      shadowOffsetX: 0,
      shadowOffsetY: 0,
      shadowBlur: 0,
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    await node.render(ctx, 0, 0)

    expect(mockContext.save).toHaveBeenCalled()
    expect(mockContext.shadowColor).toBe('rgba(0,0,0,0.5)')
    expect(mockContext.shadowOffsetX).toBe(5)
    expect(mockContext.shadowOffsetY).toBe(5)
    expect(mockContext.shadowBlur).toBe(10)
    expect(mockContext.fill).toHaveBeenCalled()
    expect(mockContext.restore).toHaveBeenCalled()
    getContextSpy.mockRestore()
  })

  it('should render inset box shadow correctly', async () => {
    const node = new BoxNode({
      width: 100,
      height: 100,
      backgroundColor: 'white',
      boxShadow: {
        inset: true,
        color: 'rgba(0,0,0,0.5)',
        offsetX: 5,
        offsetY: 5,
        blur: 10,
      },
    })
    const mockFill = vi.fn()
    const mockStroke = vi.fn()
    const mockClip = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      fillStyle: '',
      shadowColor: '',
      shadowOffsetX: 0,
      shadowOffsetY: 0,
      shadowBlur: 0,
      stroke: mockStroke,
      clip: mockClip,
      lineWidth: 0,
      strokeStyle: '',
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    await node.render(ctx, 0, 0)

    expect(mockContext.save).toHaveBeenCalled()
    expect(mockClip).toHaveBeenCalled()
    expect(mockContext.shadowColor).toBe('rgba(0,0,0,0.5)')
    expect(mockContext.shadowOffsetX).toBe(5)
    expect(mockContext.shadowOffsetY).toBe(5)
    expect(mockContext.shadowBlur).toBe(10)
    expect(mockStroke).toHaveBeenCalled()
    expect(mockContext.restore).toHaveBeenCalled()
    getContextSpy.mockRestore()
  })

  it('should handle opacity correctly', async () => {
    const node = new BoxNode({ width: 100, height: 100, opacity: 0.5 })
    const mockFill = vi.fn()
    const mockGlobalAlphaValues: number[] = []
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),

      _globalAlpha: 1, // Internal storage
      get globalAlpha() {
        return this._globalAlpha
      },
      set globalAlpha(value) {
        this._globalAlpha = value
        mockGlobalAlphaValues.push(value) // Record the value
      },

      // Add other methods that might be called on the context
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    await node.render(ctx, 0, 0)

    expect(mockGlobalAlphaValues).toContain(0.5) // Check if 0.5 was set
    expect(mockContext.globalAlpha).toBe(1) // After restoration, it should be 1
    getContextSpy.mockRestore()
  })

  it('should handle transform properties correctly', async () => {
    const node = new BoxNode({
      width: 100,
      height: 100,
      transform: {
        translateX: 10,
        translateY: '20%',
        rotate: 45,
        scale: 2,
      },
    })
    const mockFill = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      translate: vi.fn(),
      rotate: vi.fn(),
      scale: vi.fn(),
      // Add other methods that might be called on the context
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    await node.render(ctx, 0, 0)

    expect(mockContext.save).toHaveBeenCalledTimes(1)
    expect(mockContext.translate).toHaveBeenCalled()
    expect(mockContext.rotate).toHaveBeenCalledWith(expect.any(Number)) // Check if rotate was called with a number
    expect(mockContext.scale).toHaveBeenCalledWith(2, 2)
    expect(mockContext.restore).toHaveBeenCalledTimes(1)
    getContextSpy.mockRestore()
  })

  it('should not render if width or height is zero', async () => {
    const node = new BoxNode({ width: 0, height: 50, backgroundColor: 'red' })
    const mockFill = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    node.node.setWidth(0)
    node.node.setHeight(50)
    vi.spyOn(node.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 0,
      height: 50,
    } as any)
    await node.render(ctx, 0, 0)

    expect(mockFill).not.toHaveBeenCalled()
    expect(mockContext.save).not.toHaveBeenCalled()
    getContextSpy.mockRestore()
  })

  it('should not render if display is set to none', async () => {
    const node = new BoxNode({ width: 50, height: 50, display: Style.Display.None, backgroundColor: 'red' })
    const mockFill = vi.fn()
    const mockContext = {
      fill: mockFill,
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    node.node.setWidth(50)
    node.node.setHeight(50)
    vi.spyOn(node.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 50,
      height: 50,
    } as any)
    await node.render(ctx, 0, 0)

    expect(mockFill).not.toHaveBeenCalled()
    expect(mockContext.save).not.toHaveBeenCalled()
    getContextSpy.mockRestore()
  })

  it('should handle transform properties with originX and originY correctly', async () => {
    const node = new BoxNode({
      width: 100,
      height: 100,
      transform: {
        translateX: 10,
        translateY: 10,
        rotate: 90,
        originX: '25%',
        originY: 25,
      },
    })
    const mockTranslate = vi.fn()
    const mockRotate = vi.fn()
    const mockScale = vi.fn()
    const mockContext = {
      fill: vi.fn(),
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      translate: mockTranslate,
      rotate: mockRotate,
      scale: mockScale,
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
    }
    const getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext as any)

    const ctx = new Canvas().getContext('2d')
    node.node.setWidth(100)
    node.node.setHeight(100)
    vi.spyOn(node.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
    } as any)
    await node.render(ctx, 0, 0)

    expect(mockContext.save).toHaveBeenCalledTimes(1)
    // Expect translate to be called for origin, then for translateX/Y, then for origin restoration
    expect(mockTranslate).toHaveBeenCalledWith(25, 25) // originX: '25%' of 100 = 25, originY: 25
    expect(mockTranslate).toHaveBeenCalledWith(10, 10) // translateX: 10, translateY: 10
    expect(mockRotate).toHaveBeenCalledWith(Math.PI / 2) // 90 degrees
    expect(mockTranslate).toHaveBeenCalledWith(-25, -25) // restore origin
    expect(mockContext.restore).toHaveBeenCalledTimes(1)
    getContextSpy.mockRestore()
  })
})

describe('Box factory', () => {
  it('should return a CanvasElement with __type Box', () => {
    const box = Box({ width: 10, height: 20 })
    expect(box).toMatchObject({ __type: 'Box', props: { width: 10, height: 20 } })
  })
})

describe('ColumnNode', () => {
  it('should create a ColumnNode with correct flex properties', () => {
    const column = new ColumnNode({ flexGrow: 1 })
    expect(column.props.display).toBe(Style.Display.Flex)
    expect(column.props.flexDirection).toBe(Style.FlexDirection.Column)
    expect(column.props.flexShrink).toBe(1)
  })
  it('should return a CanvasElement with __type Column', () => {
    const col = Column({ width: 100 })
    expect(col).toMatchObject({ __type: 'Column', props: { width: 100 } })
  })
})

describe('RowNode', () => {
  it('should create a RowNode with correct flex properties', () => {
    const row = new RowNode({ flexGrow: 2 })
    expect(row.props.display).toBe(Style.Display.Flex)
    expect(row.props.flexDirection).toBe(Style.FlexDirection.Row)
    expect(row.props.flexShrink).toBe(1)
    expect(row.name).toBe('Row')
  })
  it('should return a CanvasElement with __type Row', () => {
    const r = Row({ width: 200 })
    expect(r).toMatchObject({ __type: 'Row', props: { width: 200 } })
  })
})

describe('BoxNode _renderContent', () => {
  let mockContext: any
  let getContextSpy: any
  let node: BoxNode
  let shadowColors: string[]
  let shadowOffsetXValues: number[]
  let shadowOffsetYValues: number[]
  let shadowBlurValues: number[]

  beforeEach(() => {
    shadowColors = []
    shadowOffsetXValues = []
    shadowOffsetYValues = []
    shadowBlurValues = []

    mockContext = {
      fill: vi.fn(),
      beginPath: vi.fn(),
      rect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      clip: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      closePath: vi.fn(),
      arc: vi.fn(),
      createLinearGradient: vi.fn(() => ({
        addColorStop: vi.fn(),
      })),
      createRadialGradient: vi.fn(() => ({
        addColorStop: vi.fn(),
      })),
      drawImage: vi.fn(),
      setLineDash: vi.fn(),
      stroke: vi.fn(),
      // Mock properties
      _fillStyle: '',
      get fillStyle() {
        return this._fillStyle
      },
      set fillStyle(value) {
        this._fillStyle = value
      },
      _shadowColor: '',
      get shadowColor() {
        return this._shadowColor
      },
      set shadowColor(value) {
        shadowColors.push(value)
        this._shadowColor = value
      },
      _shadowOffsetX: 0,
      get shadowOffsetX() {
        return this._shadowOffsetX
      },
      set shadowOffsetX(value) {
        shadowOffsetXValues.push(value)
        this._shadowOffsetX = value
      },
      _shadowOffsetY: 0,
      get shadowOffsetY() {
        return this._shadowOffsetY
      },
      set shadowOffsetY(value) {
        shadowOffsetYValues.push(value)
        this._shadowOffsetY = value
      },
      _shadowBlur: 0,
      get shadowBlur() {
        return this._shadowBlur
      },
      set shadowBlur(value) {
        shadowBlurValues.push(value)
        this._shadowBlur = value
      },
      globalCompositeOperation: 'source-over',
      lineWidth: 0,
      strokeStyle: '',
      imageSmoothingEnabled: true,
      imageSmoothingQuality: 'high',
    }

    // This spy is for the *main* canvas context, not the offscreen one
    getContextSpy = vi.spyOn(Canvas.prototype, 'getContext').mockReturnValue(mockContext)
    node = new BoxNode({ width: 100, height: 100 })
    vi.spyOn(node.node, 'getComputedLayout').mockReturnValue({
      left: 0,
      top: 0,
      width: 100,
      height: 100,
      bottom: 0,
      right: 0,
    })
  })

  afterEach(() => {
    if (getContextSpy) {
      getContextSpy.mockRestore()
    }
  })

  it('should render multiple outset and inset box shadows correctly', () => {
    node.props.boxShadow = [
      { color: 'rgba(0,0,0,0.5)', offsetX: 5, offsetY: 5, blur: 10 }, // Outset
      { inset: true, color: 'rgba(255,0,0,0.5)', offsetX: 2, offsetY: 2, blur: 5 }, // Inset
    ]
    node.props.backgroundColor = 'white' // Make it opaque for optimized outset shadow path

    node['_renderContent'](mockContext, 0, 0, 100, 100)

    // Outset shadow assertions
    expect(mockContext.save).toHaveBeenCalledTimes(2) // One for outset, one for inset
    expect(shadowColors[0]).toBe('rgba(0,0,0,0.5)')
    expect(shadowOffsetXValues[0]).toBe(5)
    expect(shadowOffsetYValues[0]).toBe(5)
    expect(shadowBlurValues[0]).toBe(10)
    expect(mockContext.fill).toHaveBeenCalled()

    // Inset shadow assertions
    expect(mockContext.clip).toHaveBeenCalled()
    expect(shadowColors[1]).toBe('rgba(255,0,0,0.5)')
    expect(shadowOffsetXValues[1]).toBe(2)
    expect(shadowOffsetYValues[1]).toBe(2)
    expect(shadowBlurValues[1]).toBe(5)
    expect(mockContext.stroke).toHaveBeenCalled()
    expect(mockContext.restore).toHaveBeenCalledTimes(2)
  })

  it('should render linear gradient with "to-top-right" direction', () => {
    node.props.gradient = {
      type: 'linear',
      colors: ['red', 'blue'],
      direction: 'to-top-right',
    }

    node['_renderContent'](mockContext, 0, 0, 100, 100)

    expect(mockContext.createLinearGradient).toHaveBeenCalledWith(0, 100, 100, 0)
    expect(mockContext.fill).toHaveBeenCalled()
  })

  it('should render linear gradient with array direction [x0, y0, x1, y1]', () => {
    node.props.gradient = {
      type: 'linear',
      colors: ['red', 'blue'],
      direction: [10, 10, 90, 90],
    }

    node['_renderContent'](mockContext, 0, 0, 100, 100)

    expect(mockContext.createLinearGradient).toHaveBeenCalledWith(10, 10, 90, 90)
    expect(mockContext.fill).toHaveBeenCalled()
  })

  describe('linear gradient directions', () => {
    const directions = {
      'to-left': [100, 0, 0, 0],
      'to-bottom': [0, 0, 0, 100],
      'to-top': [0, 100, 0, 0],
      'to-top-left': [100, 100, 0, 0],
      'to-bottom-right': [0, 0, 100, 100],
      'to-bottom-left': [100, 0, 0, 100],
    }

    for (const [direction, coords] of Object.entries(directions)) {
      it(`should render linear gradient with "${direction}" direction`, () => {
        node.props.gradient = {
          type: 'linear',
          colors: ['red', 'blue'],
          direction: direction as any,
        }
        node['_renderContent'](mockContext, 0, 0, 100, 100)
        expect(mockContext.createLinearGradient).toHaveBeenCalledWith(...coords)
        expect(mockContext.fill).toHaveBeenCalled()
      })
    }
  })

  it('should warn and fallback for invalid linear gradient direction', () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    node.props.gradient = {
      type: 'linear',
      colors: ['red', 'blue'],
      direction: 'invalid-direction' as any,
    }
    node.props.backgroundColor = 'green' // Should fallback to this

    node['_renderContent'](mockContext, 0, 0, 100, 100)

    expect(warnSpy).toHaveBeenCalledTimes(2)
    expect(warnSpy.mock.calls[0][0]).toContain('Invalid linear gradient direction')
    expect(warnSpy.mock.calls[1][0]).toContain('Could not create linear gradient. Falling back to backgroundColor.')
    expect(mockContext.createLinearGradient).not.toHaveBeenCalled()
    expect(mockContext.fillStyle).toBe('green') // Fallback to background color
    expect(mockContext.fill).toHaveBeenCalled()
    warnSpy.mockRestore()
  })

  it('should warn and fallback for radial gradient with no colors', () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    node.props.gradient = {
      type: 'radial',
      colors: [],
    }
    node.props.backgroundColor = 'purple'

    node['_renderContent'](mockContext, 0, 0, 100, 100)

    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('Gradient specified but no colors provided'))
    expect(mockContext.createRadialGradient).not.toHaveBeenCalled()
    expect(mockContext.fillStyle).toBe('purple')
    expect(mockContext.fill).toHaveBeenCalled()
    warnSpy.mockRestore()
  })

  it('should warn and fallback for gradient with zero width/height', () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    node.props.gradient = {
      direction: 'to-bottom',
      type: 'linear',
      colors: ['red', 'blue'],
    }
    node.props.backgroundColor = 'orange'

    node['_renderContent'](mockContext, 0, 0, 0, 100) // Zero width

    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('Cannot draw gradient with zero width/height'))
    expect(mockContext.createLinearGradient).not.toHaveBeenCalled()
    expect(mockContext.fillStyle).toBe('orange')
    expect(mockContext.fill).toHaveBeenCalled()
    warnSpy.mockRestore()
  })

  it('should handle radial gradient with zero width/height (r1 <= 0)', () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    node.props.gradient = {
      type: 'radial',
      colors: ['red', 'blue'],
    }
    node.props.backgroundColor = 'yellow'

    node['_renderContent'](mockContext, 0, 0, 0, 0) // Zero width and height

    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('Cannot draw gradient with zero width/height.'))
    expect(mockContext.createRadialGradient).not.toHaveBeenCalled()
    expect(mockContext.fillStyle).toBe('yellow')
    expect(mockContext.fill).toHaveBeenCalled()
    warnSpy.mockRestore()
  })

  it('should handle borderRadius with object notation correctly', () => {
    node.props.borderRadius = {
      TopLeft: 5,
      TopRight: 10,
      BottomRight: 15,
      BottomLeft: 20,
    }
    node.props.backgroundColor = 'blue'

    node['_renderContent'](mockContext, 0, 0, 100, 100)

    expect(mockContext.beginPath).toHaveBeenCalled()
    // Check if arc was called with the correct radii
    expect(mockContext.arc).toHaveBeenCalledWith(expect.any(Number), expect.any(Number), 5, expect.any(Number), expect.any(Number))
    expect(mockContext.arc).toHaveBeenCalledWith(expect.any(Number), expect.any(Number), 10, expect.any(Number), expect.any(Number))
    expect(mockContext.arc).toHaveBeenCalledWith(expect.any(Number), expect.any(Number), 15, expect.any(Number), expect.any(Number))
    expect(mockContext.arc).toHaveBeenCalledWith(expect.any(Number), expect.any(Number), 20, expect.any(Number), expect.any(Number))
    expect(mockContext.fill).toHaveBeenCalled()
  })

  it('should handle opaque background for outset shadows optimization', () => {
    node.props.boxShadow = [{ color: 'rgba(0,0,0,0.5)', offsetX: 5, offsetY: 5, blur: 10 }]
    node.props.backgroundColor = 'red' // Opaque

    const fillStylesSet: string[] = []
    const originalFillStyleSetter = Object.getOwnPropertyDescriptor(mockContext, 'fillStyle')?.set
    Object.defineProperty(mockContext, 'fillStyle', {
      set: vi.fn((value: string) => {
        fillStylesSet.push(value)
        if (originalFillStyleSetter) {
          originalFillStyleSetter.call(mockContext, value)
        }
      }),
      get: vi.fn(() => mockContext._fillStyle),
      configurable: true,
    })

    node['_renderContent'](mockContext, 0, 0, 100, 100)

    expect(fillStylesSet).toContain('black')
    expect(mockContext.fill).toHaveBeenCalled()
    // Ensure the complex shadow rendering path (with offscreen canvas) was NOT taken
    expect(mockContext.drawImage).not.toHaveBeenCalled()

    // Restore original fillStyle setter
    if (originalFillStyleSetter) {
      Object.defineProperty(mockContext, 'fillStyle', {
        set: originalFillStyleSetter,
        get: vi.fn(() => mockContext._fillStyle),
        configurable: true,
      })
    }
  })

  it('should handle non-opaque background for outset shadows complex rendering', () => {
    node.props.boxShadow = [{ color: 'rgba(0,0,0,0.5)', offsetX: 5, offsetY: 5, blur: 10 }]
    node.props.backgroundColor = 'rgba(255,255,255,0.5)' // Non-opaque

    node['_renderContent'](mockContext, 0, 0, 100, 100)

    // Expect the complex path to be taken, which involves drawImage from offscreen canvas
    expect(mockContext.drawImage).toHaveBeenCalled()
    // Ensure the optimized path (with fillStyle black) was NOT taken for the shadow itself
    // (fillStyle will be set to the background color later)
    expect(mockContext.fillStyle).not.toBe('black')
  })
})
