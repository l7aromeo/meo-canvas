import { vi } from 'vitest'
import { mockNodeCreate, clearCreatedNodes } from '@/__mocks__/yoga-layout.js'

export const yogaNode = {
  current: mockNodeCreate(),
}

// Mock the actual BoxNode class
export const BoxNode = vi.fn(function (this: any, props: any) {
  this.initialProps = props
  this.node = yogaNode.current
  this.children = []
  this.props = { ...props }
  this.name = props.name || 'Box'
  this.key = props.key || `${this.name}-0`

  // Mock methods that are called on the instance
  this.processInitialChildren = vi.fn()
  this.resolveInheritedStyles = vi.fn()
  this.applyDefaults = vi.fn()
  this.appendChild = vi.fn()
  this.finalizeLayout = vi.fn(() => false)
  this.resolveContainingBlocks = vi.fn(() => false)
  this.updateLayoutBasedOnComputedSize = vi.fn()
  this.setLayout = vi.fn()
  this._renderContent = vi.fn()
})
// `render` is defined on each mock class's own prototype rather than inherited from `BoxNode`'s.
// vitest 5 refuses to spy on a property a subject does not own, so a suite reaching for
// `vi.spyOn(ColumnNode.prototype, 'render')` -- which is how the root's rendering is swallowed --
// failed with "The property render is not defined on the object" against an inherited one.
BoxNode.prototype.render = vi.fn()

// Mock the actual ColumnNode class, inheriting from mocked BoxNode
export const ColumnNode = vi.fn(function (this: any, props: any) {
  // Call BoxNode constructor to set up properties
  Object.assign(this, new BoxNode({ name: 'Column', ...props }))
})
Object.setPrototypeOf(ColumnNode.prototype, BoxNode.prototype)
ColumnNode.prototype.render = vi.fn()

// Mock the actual RowNode class, inheriting from mocked BoxNode
export const RowNode = vi.fn(function (this: any, props: any) {
  Object.assign(this, new BoxNode({ name: 'Row', ...props }))
})
Object.setPrototypeOf(RowNode.prototype, BoxNode.prototype)
RowNode.prototype.render = vi.fn()

// Mock the factory functions
export const Box = vi.fn((props: any) => new BoxNode(props))
export const Column = vi.fn((props: any) => new ColumnNode(props))
export const Row = vi.fn((props: any) => new RowNode(props))

export const normalizeDescriptorChildren = vi.fn((children: any): any[] | undefined => {
  if (children === undefined || children === null || children === false) return undefined
  const arr = (Array.isArray(children) ? children : [children]).filter(Boolean)
  return arr.length > 0 ? arr : undefined
})

export const __mocks__ = {
  yogaNode: yogaNode.current,
  BoxNode,
  ColumnNode,
  RowNode,
  Box,
  Column,
  Row,
  normalizeDescriptorChildren,
  reset: () => {
    // Reset all vi.fn() mocks
    clearCreatedNodes()
    yogaNode.current = mockNodeCreate()

    BoxNode.mockClear()
    BoxNode.prototype.render.mockClear() // Clear the prototype render mock
    ColumnNode.mockClear()
    ColumnNode.prototype.render.mockClear()
    RowNode.mockClear()
    RowNode.prototype.render.mockClear()
    Box.mockClear()
    Column.mockClear()
    Row.mockClear()
    normalizeDescriptorChildren.mockClear()
  },
}
