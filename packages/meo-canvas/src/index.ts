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

/**
 * Whether a node is placed by the flow, by its own offsets, or not at all.
 *
 * `'static'` is CSS's default and takes no offsets — a node with `position`
 * edges set under it ignores them. The scene distinguishes it from
 * `'relative'`, which is laid out the same way and does read them.
 */
export type PositionType = 'static' | 'relative' | 'absolute'

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

/**
 * The pixel layout a canvas composites in.
 *
 * **Upstream's spellings, exactly, and they are not this package's house
 * style.** Every other keyword union here is ours to name; this one is already
 * written down in callers' source — `colorType: 'RGBA8888'` is a string a v1
 * caller has. Renaming it would fail as an invalid *value* rather than as a
 * visible rename, with nothing pointing at what it became.
 *
 * The aliases are upstream's too: `'rgba'` and `'RGBA8888'` are one layout,
 * `'rgb'` and `'RGB888x'` another.
 *
 * `'RGBAF32'` keeps colour outside sRGB rather than clipping it as it is drawn,
 * which is what a sixteen-bit PNG or a wide-gamut export needs — at the cost of
 * the GPU, since no GPU composites float.
 */
export type ColorType =
  | 'Alpha8'
  | 'Gray8'
  | 'R8UNorm'
  | 'A16Float'
  | 'A16UNorm'
  | 'ARGB4444'
  | 'R8G8UNorm'
  | 'RGB565'
  | 'rgb'
  | 'RGB888x'
  | 'rgba'
  | 'RGBA8888'
  | 'bgra'
  | 'BGRA8888'
  | 'BGR101010x'
  | 'BGRA1010102'
  | 'R16G16Float'
  | 'R16G16UNorm'
  | 'RGB101010x'
  | 'RGBA1010102'
  | 'SRGBA8888'
  | 'N32'
  | 'R16G16B16A16UNorm'
  | 'RGBAF16'
  | 'RGBAF16Norm'
  | 'RGBAF32'

/**
 * The colour space a canvas composites in.
 *
 * Upstream's spellings, for the reason {@link ColorType} takes upstream's —
 * and upstream spells the two differently from each other, which is why this
 * one is kebab-case and that one is not. The short forms are aliases of the
 * long: `'p3'` is `'display-p3'`, `'hdr10'` is `'rec2020-pq'`.
 *
 * Fixed for the whole render rather than chosen per export: colours are
 * interpreted in it, and one outside its gamut is clipped as it is drawn.
 */
export type ColorSpace =
  | 'srgb'
  | 'srgb-linear'
  | 'linear'
  | 'display-p3'
  | 'p3'
  | 'display-p3-linear'
  | 'p3-linear'
  | 'rec2020'
  | 'bt2020'
  | 'rec2020-linear'
  | 'bt2020-linear'
  | 'rec2020-pq'
  | 'hdr10'
  | 'rec2020-hlg'
  | 'hlg'

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
  Spacing,
  Style,
  TextDecoration,
  VerticalAlign,
} from './style.js'

export { Box, Column, Grid, Image, NODE_KEYS, Path, RichText, Row, Text } from './node.js'
export type {
  Child,
  Children,
  ContainerProps,
  ImageProps,
  ImageSource,
  NodeKind,
  ParagraphOptions,
  PathProps,
  SceneNode,
  TextProps,
  TextSegment,
} from './node.js'

export { Root } from './root.js'
export type { FontRegistration, NativeRenderer, PageBuilder, PageInfo, PaintOptions, RootDependencies, RootProps } from './root.js'

export { Canvas } from './canvas.js'
export type { EncodeOptions, Format, NativeCanvas, WriteFile, WriteFileSync } from './canvas.js'
