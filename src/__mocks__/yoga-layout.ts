import { vi } from 'vitest'
import { Style } from '@/constant/common.const.js' // Import Style for Unit enum

// Track all created nodes
export let createdNodes: any[] = []

// Create the mock Node.create function
export const mockNodeCreate = vi.fn(() => {
  const node = {
    setWidth: vi.fn(),
    setHeight: vi.fn(),
    setMinWidth: vi.fn(),
    setMinHeight: vi.fn(),
    setMaxWidth: vi.fn(),
    setMaxHeight: vi.fn(),
    calculateLayout: vi.fn(),
    getComputedHeight: vi.fn(() => 0),
    getComputedWidth: vi.fn(() => 0),
    getComputedLayout: vi.fn(() => ({ left: 0, top: 0, width: 100, height: 100 })),
    getComputedBorder: vi.fn(() => 0),
    getComputedPadding: vi.fn(() => 0),
    getWidth: vi.fn(() => ({ value: 0, unit: Style.Unit.Point })),
    getHeight: vi.fn(() => ({ value: 0, unit: Style.Unit.Point })),
    getFlexGrow: vi.fn(() => 0), // Add this
    getFlexWrap: vi.fn(() => Style.Wrap.Wrap), // Add this
    getFlexShrink: vi.fn(() => 0), // Add this
    getFlexDirection: vi.fn(() => Style.FlexDirection.Column), // Add this
    getMargin: vi.fn(() => ({ value: 0, unit: Style.Unit.Point })), // Add this
    getGap: vi.fn(() => ({ value: 0, unit: Style.Unit.Point })), // Add this
    insertChild: vi.fn(),
    removeChild: vi.fn(),
    getChildCount: vi.fn(() => 0),
    setFlexDirection: vi.fn(),
    setJustifyContent: vi.fn(),
    setAlignItems: vi.fn(),
    setAlignSelf: vi.fn(),
    setAlignContent: vi.fn(),
    setFlexGrow: vi.fn(),
    setFlexShrink: vi.fn(),
    setFlexBasis: vi.fn(),
    setFlexWrap: vi.fn(),
    setPositionType: vi.fn(),
    setPosition: vi.fn(),
    setPositionPercent: vi.fn(),
    setGap: vi.fn(),
    setGapPercent: vi.fn(),
    setMargin: vi.fn(),
    setMarginPercent: vi.fn(),
    setPadding: vi.fn(),
    setPaddingPercent: vi.fn(),
    setBorder: vi.fn(),
    setDisplay: vi.fn(),
    setOverflow: vi.fn(),
    setBoxSizing: vi.fn(),
    setDirection: vi.fn(),
    setAspectRatio: vi.fn(),
    isDirty: vi.fn(() => false),
    markDirty: vi.fn(),
    free: vi.fn(),
    // Releases a node and its descendants. Present so renders that free their layout tree do so
    // silently here too, rather than warning on a mock that happens to lack the method.
    freeRecursive: vi.fn(),
  }
  createdNodes.push(node)
  return node
})

export const Yoga = {
  Node: {
    create: mockNodeCreate,
  },
  Direction: {
    LTR: 0,
    RTL: 1,
  },
  Display: {
    Flex: 0,
    None: 1,
  },
  FlexDirection: {
    Column: 0,
    Row: 1,
  },
  PositionType: {
    Relative: 0,
    Absolute: 1,
  },
  Overflow: {
    Visible: 0,
    Hidden: 1,
  },
  BoxSizing: {
    ContentBox: 0,
    BorderBox: 1,
  },
  Edge: {
    Left: 0,
    Top: 1,
    Right: 2,
    Bottom: 3,
    All: 4,
  },
  Gutter: {
    Column: 0,
    Row: 1,
    All: 2,
  },
  Unit: {
    // Add Unit enum
    Undefined: 0,
    Point: 1,
    Percent: 2,
    Auto: 3,
  },
  DIRECTION_LTR: 0,
  DIRECTION_RTL: 1,
  FLEX_DIRECTION_COLUMN: 0,
  FLEX_DIRECTION_ROW: 1,
  JUSTIFY_FLEX_START: 0,
  ALIGN_FLEX_START: 0,
}

export function clearCreatedNodes() {
  createdNodes = []
}

export const __mocks__ = {
  Yoga,
  clearCreatedNodes,
  createdNodes,
}
