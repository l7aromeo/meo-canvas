import { jest } from '@jest/globals'
import { Style } from '@/constant/common.const.js' // Import Style for Unit enum

// Track all created nodes
export let createdNodes: any[] = []

// Create the mock Node.create function
export const mockNodeCreate = jest.fn(() => {
  const node = {
    setWidth: jest.fn(),
    setHeight: jest.fn(),
    setMinWidth: jest.fn(),
    setMinHeight: jest.fn(),
    setMaxWidth: jest.fn(),
    setMaxHeight: jest.fn(),
    calculateLayout: jest.fn(),
    getComputedHeight: jest.fn(() => 0),
    getComputedWidth: jest.fn(() => 0),
    getComputedLayout: jest.fn(() => ({ left: 0, top: 0, width: 100, height: 100 })),
    getComputedBorder: jest.fn(() => 0),
    getComputedPadding: jest.fn(() => 0),
    getWidth: jest.fn(() => ({ value: 0, unit: Style.Unit.Point })),
    getHeight: jest.fn(() => ({ value: 0, unit: Style.Unit.Point })),
    getFlexGrow: jest.fn(() => 0), // Add this
    getFlexWrap: jest.fn(() => Style.Wrap.Wrap), // Add this
    getFlexShrink: jest.fn(() => 0), // Add this
    getFlexDirection: jest.fn(() => Style.FlexDirection.Column), // Add this
    getMargin: jest.fn(() => ({ value: 0, unit: Style.Unit.Point })), // Add this
    getGap: jest.fn(() => ({ value: 0, unit: Style.Unit.Point })), // Add this
    insertChild: jest.fn(),
    removeChild: jest.fn(),
    getChildCount: jest.fn(() => 0),
    setFlexDirection: jest.fn(),
    setJustifyContent: jest.fn(),
    setAlignItems: jest.fn(),
    setAlignSelf: jest.fn(),
    setAlignContent: jest.fn(),
    setFlexGrow: jest.fn(),
    setFlexShrink: jest.fn(),
    setFlexBasis: jest.fn(),
    setFlexWrap: jest.fn(),
    setPositionType: jest.fn(),
    setPosition: jest.fn(),
    setPositionPercent: jest.fn(),
    setGap: jest.fn(),
    setGapPercent: jest.fn(),
    setMargin: jest.fn(),
    setMarginPercent: jest.fn(),
    setPadding: jest.fn(),
    setPaddingPercent: jest.fn(),
    setBorder: jest.fn(),
    setDisplay: jest.fn(),
    setOverflow: jest.fn(),
    setBoxSizing: jest.fn(),
    setDirection: jest.fn(),
    setAspectRatio: jest.fn(),
    isDirty: jest.fn(() => false),
    markDirty: jest.fn(),
    free: jest.fn(),
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
