import { jest } from '@jest/globals'

export const mockYogaNode = {
  getComputedHeight: jest.fn(() => 0),
  calculateLayout: jest.fn(),
  getComputedLayout: jest.fn(() => ({
    left: 0,
    top: 0,
    width: 0,
    height: 0,
  })),
  insertChild: jest.fn(),
  setAspectRatio: jest.fn(),
  setWidth: jest.fn(),
  isDirty: jest.fn(() => false),
  markDirty: jest.fn(),
  getComputedBorder: jest.fn(() => 0),
  getComputedPadding: jest.fn(() => 0),
}

// Mock the actual BoxNode class
export const BoxNode = jest.fn(function (this: any, props: any) {
  this.initialProps = props
  this.node = mockYogaNode
  this.children = []
  this.props = { ...props }
  this.name = props.name || 'Box'
  this.key = props.key || `${this.name}-0`

  // Mock methods that are called on the instance
  this.processInitialChildren = jest.fn()
  this.resolveInheritedStyles = jest.fn()
  this.applyDefaults = jest.fn()
  this.appendChild = jest.fn()
  this.finalizeLayout = jest.fn(() => false)
  this.updateLayoutBasedOnComputedSize = jest.fn()
  this.setLayout = jest.fn()
  this._renderContent = jest.fn()
})
// Define render on the prototype so it can be spied on
BoxNode.prototype.render = jest.fn()

// Mock the actual ColumnNode class, inheriting from mocked BoxNode
export const ColumnNode = jest.fn(function (this: any, props: any) {
  // Call BoxNode constructor to set up properties
  Object.assign(this, new BoxNode({ name: 'Column', ...props }))
})
// ColumnNode.prototype will inherit render from BoxNode.prototype due to setPrototypeOf
Object.setPrototypeOf(ColumnNode.prototype, BoxNode.prototype)

// Mock the actual RowNode class, inheriting from mocked BoxNode
export const RowNode = jest.fn(function (this: any, props: any) {
  Object.assign(this, new BoxNode({ name: 'Row', ...props }))
})
Object.setPrototypeOf(RowNode.prototype, BoxNode.prototype)

// Mock the factory functions
export const Box = jest.fn((props: any) => new BoxNode(props))
export const Column = jest.fn((props: any) => new ColumnNode(props))
export const Row = jest.fn((props: any) => new RowNode(props))

export const __mocks__ = {
  mockYogaNode,
  BoxNode,
  ColumnNode,
  RowNode,
  Box,
  Column,
  Row,
  reset: () => {
    // Reset all jest.fn() mocks
    for (const key in mockYogaNode) {
      if (jest.isMockFunction((mockYogaNode as any)[key])) {
        ;(mockYogaNode as any)[key].mockClear()
      }
    }
    mockYogaNode.getComputedHeight.mockReturnValue(0)
    mockYogaNode.isDirty.mockReturnValue(false)
    mockYogaNode.getComputedLayout.mockReturnValue({ left: 0, top: 0, width: 0, height: 0 })
    mockYogaNode.getComputedBorder.mockReturnValue(0)
    mockYogaNode.getComputedPadding.mockReturnValue(0)

    BoxNode.mockClear()
    BoxNode.prototype.render.mockClear() // Clear the prototype render mock
    ColumnNode.mockClear()
    RowNode.mockClear()
    Box.mockClear()
    Column.mockClear()
    Row.mockClear()
  },
}
