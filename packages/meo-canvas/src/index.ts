/**
 * The type vocabulary a scene is described in.
 *
 * Every enumerated value is a string-literal union rather than a numeric enum or a
 * bare `string`. A union is what an editor completes as you type and what the
 * compiler rejects a misspelling against; `string` accepts anything and finds the
 * mistake at render time, and a numeric enum costs an import at every call site to
 * name a value that is already a word.
 *
 * The values are the CSS spellings, because that is the vocabulary the layout rules
 * come from and the one a caller already knows. Each union carries exactly the
 * variants its Rust counterpart in `meo-canvas-scene` carries, spelling for
 * spelling: the scene crate is the contract, and a value this file accepts that the
 * decoder does not know is a render-time error dressed up as a compile-time
 * success.
 *
 * @packageDocumentation
 */

/** Axis children are placed along, and the direction they run in. */
export type FlexDirection = 'row' | 'row-reverse' | 'column' | 'column-reverse'

/** How a node's children are arranged, or `none` to lay out and draw nothing. */
export type Display = 'flex' | 'grid' | 'block' | 'none'

/** Distribution of free space along the main axis. */
export type Justify = 'flex-start' | 'flex-end' | 'center' | 'space-between' | 'space-around' | 'space-evenly'

/**
 * Placement along the cross axis.
 *
 * One union serves `alignItems`, `alignSelf` and `alignContent`, carrying the union
 * of what the three accept rather than their intersection: the `space-*` values
 * belong to `alignContent` alone, and `baseline` to the other two. There is no
 * `auto`: a property left unset is what defers to the parent, so absence says it
 * and a value cannot.
 */
export type Align = 'flex-start' | 'flex-end' | 'center' | 'stretch' | 'baseline' | 'space-between' | 'space-around' | 'space-evenly'

/** Whether a node is placed by the flow or by its own offsets. */
export type PositionType = 'relative' | 'absolute'

/**
 * What happens to content larger than its box.
 *
 * `scroll` clips like `hidden` and additionally reserves room for a scrollbar.
 * Nothing scrolls in a still image; the reserved gutter is what changes the layout,
 * which is why the two are distinct here.
 */
export type Overflow = 'visible' | 'hidden' | 'scroll'

/** Whether children overflow onto further lines, and which way those lines stack. */
export type FlexWrap = 'nowrap' | 'wrap' | 'wrap-reverse'

/** Whether `width` and `height` include padding and border. */
export type BoxSizing = 'border-box' | 'content-box'

/** How an image fills the box it was given. */
export type ObjectFit = 'fill' | 'contain' | 'cover' | 'none' | 'scale-down'

/**
 * Horizontal alignment of text within its box.
 *
 * `start` and `end` follow the reading direction; `left` and `right` do not, and are
 * the pair to reach for when a label must sit on one particular side whatever the
 * text direction is.
 */
export type TextAlign = 'start' | 'end' | 'left' | 'center' | 'right' | 'justify'

/** Inline direction, which decides which edge is the start. */
export type Direction = 'ltr' | 'rtl'

/** The order the grid's auto-placement algorithm fills tracks in. */
export type GridAutoFlow = 'row' | 'column' | 'row-dense' | 'column-dense'

/**
 * One track of a grid template.
 *
 * A bare number is pixels, which is what every other length in this vocabulary
 * means by a bare number. The suffixed forms are the three units a track can carry:
 * `px` for a fixed size, `%` of the container, and `fr` for a share of what is left
 * once the fixed tracks are placed.
 */
export type TrackSize = number | 'auto' | `${number}px` | `${number}%` | `${number}fr`

export type {
  BlendMode,
  BorderStyle,
  Color,
  Corners,
  Dimension,
  FontStyle,
  FontWeight,
  GridPlacement,
  Length,
  Overflow as OverflowValue,
  PaintOrder,
  Sides,
  Style,
  TextDecoration,
  VerticalAlign,
} from './style.js'

export { Box, Column, Grid, Image, NODE_KEYS, Path, RichText, Row, Text } from './node.js'
export type { ContainerProps, ImageProps, ImageSource, NodeKind, PathProps, SceneNode, TextProps, TextSegment } from './node.js'
