import * as StylePropTypes from 'yoga-layout'
import { BoxNode } from '@/canvas/layout.canvas.util.js'
import type { TextNode } from '@/canvas/text.canvas.util.js'
import type { ImageNode } from '@/canvas/image.canvas.util.js'
import type { GridNode } from '@/canvas/grid.canvas.util.js'
import type { FontVariantSetting } from 'skia-canvas'
import { Style } from '@/constant/common.const.js'

export interface BaseProps {
  /**
   * Optional display name for debugging purposes.
   */
  name?: string
  /**
   * Optional key identifier for component reconciliation.
   */
  key?: string
}

export type Children = BoxNode | TextNode | ImageNode | GridNode | false | undefined

export interface FontRegistrationInfo {
  family: string
  paths: string[]
}

/**
 * Defines the 2D transformation properties for a BoxNode.
 * Transformations are applied relative to the specified origin.
 */
export interface TransformProps {
  /**
   * Horizontal translation (movement along the X-axis).
   * Applied after positioning via layout.
   * @unit Pixels if it's number, percentage of the node's width if it's string (e.g., '10%').
   * @default undefined (no translation)
   */
  translateX?: number | `${number}%`
  /**
   * Vertical translation (movement along the Y-axis).
   * Applied after positioning via layout.
   * @unit Pixels if it's number, percentage of the node's height if it's string (e.g., '10%').
   * @default undefined (no translation)
   */
  translateY?: number | `${number}%`
  /**
   * Rotation around the transform origin.
   * @unit Angle in degrees. Positive values rotate clockwise.
   * @default undefined (no rotation)
   */
  rotate?: number // degrees
  /**
   * Uniform scaling factor (applied to both X and Y axes).
   * A value of 1 means no scaling, 2 means double size, 0.5 means half-size.
   * This value is overridden by `scaleX` and `scaleY` if they are also provided.
   * @default undefined (no scaling, effectively 1)
   */
  scale?: number
  /**
   * Horizontal scaling factor. Overrides the X component of `scale` if provided.
   * @default undefined (no scaling, effectively 1)
   */
  scaleX?: number
  /**
   * Vertical scaling factor. Overrides the Y component of `scale` if provided.
   * @default undefined (no scaling, effectively 1)
   */
  scaleY?: number
  /**
   * The horizontal origin point for transformations (rotate, scale).
   * @unit Pixels from the left edge if it's number, percentage of the node's width if it's string.
   * @default '50%' (center)
   */
  originX?: number | `${number}%`
  /**
   * The vertical origin point for transformations (rotate, scale).
   * @unit Pixels from the top edge if it's number, percentage of the node's height if it's string.
   * @default '50%' (center)
   */
  originY?: number | `${number}%`
}

/**
 * Defines the properties for a single box-shadow effect, similar to CSS box-shadow.
 */
export interface BoxShadowProps {
  /**
   * If true, the shadow is drawn inside the border (inset) instead of outside.
   * @default false (outset)
   */
  inset?: boolean
  /**
   * The horizontal offset of the shadow. Positive values move it right, negative values left.
   * @unit Pixels.
   * @default 0
   */
  offsetX?: number
  /**
   * The vertical offset of the shadow. Positive values move it down, negative values up.
   * @unit Pixels.
   * @default 0
   */
  offsetY?: number
  /**
   * The blur radius. Larger values create bigger, lighter blurs. Negative values are invalid.
   * @unit Pixels.
   * @default 0 (hard shadow)
   */
  blur?: number
  /**
   * The color of the shadow.
   * Accepts standard CSS color strings.
   */
  color?: string
}

/**
 * Defines the layout and style properties for a BoxNode, analogous to CSS properties.
 */
export interface BoxProps extends BaseProps {
  /**
   * Sets the width of the node.
   * @unit Pixels if it's number, percentage of the parent's width if it's string.
   * @default Yoga default (typically 'auto')
   * @see https://yogalayout.dev/docs/styling/width-height
   */
  width?: number | `${number}%`
  /**
   * Sets the height of the node.
   * @unit Pixels if it's number, percentage of the parent's height if it's string.
   * @default Yoga default (typically 'auto')
   * @see https://yogalayout.dev/docs/styling/width-height
   */
  height?: number | `${number}%`
  /**
   * Sets the minimum width of the node.
   * @unit Pixels if it's number, percentage of the parent's width if it's string.
   * @default Yoga default (0)
   * @see https://yogalayout.dev/docs/styling/min-max-width-height
   */
  minWidth?: number | `${number}%`
  /**
   * Sets the minimum height of the node.
   * @unit Pixels if it's number, percentage of the parent's height if it's string.
   * @default Yoga default (0)
   * @see https://yogalayout.dev/docs/styling/min-max-width-height
   */
  minHeight?: number | `${number}%`
  /**
   * Sets the maximum width of the node.
   * @unit Pixels if it's number, percentage of the parent's width if it's string.
   * @default Yoga default (undefined / infinity)
   * @see https://yogalayout.dev/docs/styling/min-max-width-height
   */
  maxWidth?: number | `${number}%`
  /**
   * Sets the maximum height of the node.
   * @unit Pixels if it's number, percentage of the parent's height if it's string.
   * @default Yoga default (undefined / infinity)
   * @see https://yogalayout.dev/docs/styling/min-max-width-height
   */
  maxHeight?: number | `${number}%`
  /**
   * Defines the direction of the main axis for flex items within this container.
   * @see StylePropTypes.FlexDirection (`COLUMN`, `ROW`, `COLUMN_REVERSE`, `ROW_REVERSE`)
   * @default Yoga default (`COLUMN`)
   * @see https://yogalayout.dev/docs/styling/flex-direction
   */
  flexDirection?: StylePropTypes.FlexDirection
  /**
   * Defines how flex items are distributed along the main axis of the container.
   * @see StylePropTypes.Justify (`FLEX_START`, `CENTER`, `FLEX_END`, `SPACE_BETWEEN`, `SPACE_AROUND`, `SPACE_EVENLY`)
   * @default Yoga default (`FLEX_START`)
   * @see https://yogalayout.dev/docs/styling/justify-content
   */
  justifyContent?: StylePropTypes.Justify
  /**
   * Defines how flex items are aligned along the cross axis of the container.
   * @see StylePropTypes.Align (`AUTO`, `FLEX_START`, `CENTER`, `FLEX_END`, `STRETCH`, `BASELINE`, `SPACE_BETWEEN`, `SPACE_AROUND`)
   * @default Yoga default (`STRETCH`)
   * @see https://yogalayout.dev/docs/styling/align-items-self
   */
  alignItems?: StylePropTypes.Align
  /**
   * Allows overriding the parent's `alignItems` value for a specific flex item.
   * @see StylePropTypes.Align (`AUTO`, `FLEX_START`, `CENTER`, `FLEX_END`, `STRETCH`, `BASELINE`, `SPACE_BETWEEN`, `SPACE_AROUND`)
   * @default Yoga default (`AUTO`) - Inherits from `alignItems`.
   * @see https://yogalayout.dev/docs/styling/align-items-self
   */
  alignSelf?: StylePropTypes.Align
  /**
   * Defines how lines are distributed along the cross axis when `flexWrap` is `WRAP` or `WRAP_REVERSE`.
   * Has no effect when there is only one line of flex items.
   * @see StylePropTypes.Align (`AUTO`, `FLEX_START`, `CENTER`, `FLEX_END`, `STRETCH`, `BASELINE`, `SPACE_BETWEEN`, `SPACE_AROUND`)
   * @default Yoga default (`FLEX_START`)
   * @see https://yogalayout.dev/docs/styling/align-content
   */
  alignContent?: StylePropTypes.Align
  /**
   * Defines the ability of a flex item to grow if necessary, relative to other items.
   * A non-negative number indicating the proportion of available space the item should take.
   * @default Yoga default (0) - Item does not grow.
   * @see https://yogalayout.dev/docs/styling/flex-basis-grow-shrink
   */
  flexGrow?: number
  /**
   * Defines the ability of a flex item to shrink if necessary, relative to other items.
   * A non-negative number indicating the proportion of overflow space the item should lose.
   * @default Yoga default (1 for non-root nodes, 0 for root) - Item can shrink.
   * @see https://yogalayout.dev/docs/styling/flex-basis-grow-shrink
   */
  flexShrink?: number
  /**
   * Defines the default size of a flex item along the main axis before the remaining space is distributed.
   * @unit Pixels.
   * @default Yoga default (`AUTO`)
   * @see https://yogalayout.dev/docs//styling/flex-basis-grow-shrink
   */
  flexBasis?: number | 'auto' | `${number}%`
  /**
   * Specifies the positioning method used for the node.
   * `RELATIVE`: Positioned according to the normal flow, then offset relative to that position.
   * `ABSOLUTE`: Positioned relative to its nearest positioned ancestor (or the root). Layout calculation ignores this node.
   * @see StylePropTypes.PositionType (`RELATIVE`, `ABSOLUTE`)
   * @default Yoga default (`RELATIVE`)
   * @see https://yogalayout.dev/docs/styling/position
   */
  positionType?: StylePropTypes.PositionType
  /**
   * Specifies the offset distances for positioned elements (`positionType: 'ABSOLUTE'` or `RELATIVE`).
   * Can be a single number for all edges or an object specifying individual edges (`Top`, `Right`, `Bottom`, `Left`, `Start`, `End`).
   * `Start` and `End` are affected by `direction` (LTR/RTL).
   * @unit Pixels.
   * @default Yoga default (undefined for each edge)
   * @see https://yogalayout.dev/docs/styling/position
   */
  position?: Partial<Record<keyof typeof StylePropTypes.Edge, number | `${number}%`>> | number | `${number}%`
  /**
   * Sets the margin space on the outside of the node's border.
   * Can be a single number for all edges or an object specifying individual edges.
   * @unit Pixels.
   * @default Yoga default (0 for each edge)
   * @see https://yogalayout.dev/docs/styling/margin-padding-border
   */
  margin?:
    | Partial<Record<keyof typeof StylePropTypes.Edge, number | `${number}%` | 'auto'>>
    | number
    | `${number}%`
    | 'auto'
  /**
   * Sets the padding space on the inside of the node's border, around the content.
   * Can be a single number for all edges or an object specifying individual edges.
   * @unit Pixels.
   * @default Yoga default (0 for each edge)
   * @see https://yogalayout.dev/docs/styling/margin-padding-border
   */
  padding?: Partial<Record<keyof typeof StylePropTypes.Edge, number | `${number}%`>> | number | `${number}%`
  /**
   * Sets the width of the node's border.
   * Can be a single number for all edges or an object specifying individual edges.
   * @unit Pixels.
   * @default Yoga default (0 for each edge)
   * @see https://yogalayout.dev/docs/styling/margin-padding-border
   */
  border?: Partial<Record<keyof typeof StylePropTypes.Edge, number>> | number
  /**
   * Sets the color of the node's border.
   * Accepts standard CSS color strings (e.g., 'red', '#FF0000', 'rgba(255,0,0,0.5)').
   * @default 'black' (set in BoxNode constructor)
   */
  borderColor?: string | `#${string}`
  /**
   * Sets the style of the node's border.
   * @see Style.Border.Solid (0)
   * @see Style.Border.Dashed (1)
   * @default Style.Border.Solid (set in BoxNode constructor)
   */
  borderStyle?: typeof Style.Border.Solid | typeof Style.Border.Dashed
  /**
   * Sets the radius of the node's corners, creating rounded effects.
   * Can be a single number for all corners or an object specifying individual corners (`TopLeft`, `TopRight`, `BottomLeft`, `BottomRight`).
   * @unit Pixels.
   * @default undefined (no rounding)
   */
  borderRadius?:
    | Partial<{
        TopLeft: number
        TopRight: number
        BottomLeft: number
        BottomRight: number
      }>
    | number
  /**
   * Locks the aspect ratio (width / height) of the node.
   * If set, Yoga might adjust the height based on the calculated width or vice versa.
   * @unit Ratio (e.g., 16 / 9).
   * @default Yoga default (undefined)
   * @see https://yogalayout.dev/docs/styling/aspect-ratio
   */
  aspectRatio?: number
  /**
   * Defines how content that overflows the node's bounds is handled.
   * `VISIBLE`: Content is not clipped and may render outside the node's box.
   * `HIDDEN`: Content is clipped and the rest is invisible.
   * `SCROLL`: Content is clipped, but Yoga calculates layout as if it were visible (used for scrollable containers, though an actual scrolling mechanism is external).
   * @see StylePropTypes.Overflow (`VISIBLE`, `HIDDEN`, `SCROLL`)
   * @default Yoga default (`VISIBLE`)
   * @see https://yogalayout.dev/docs/styling/overflow
   */
  overflow?: StylePropTypes.Overflow
  /**
   * Controls whether the node and its children are included in the layout calculation and rendering.
   * `FLEX`: The node participates in a flex layout.
   * `NONE`: The node and its subtree are ignored by layout and rendering.
   * @see StylePropTypes.Display (`FLEX`, `NONE`)
   * @default Yoga default (`FLEX`)
   * @see https://yogalayout.dev/docs/styling/display
   */
  display?: StylePropTypes.Display
  /**
   * Sets the primary text and layout direction (Left-to-Right or Right-to-Left).
   * Affects the meaning of `Start` and `End` edges for properties like `position`, `margin`, `padding`, `border`.
   * `INHERIT`: Uses the direction of the parent node.
   * @see StylePropTypes.Direction (`INHERIT`, `LTR`, `RTL`)
   * @default `Style.DIRECTION_LTR` (set in `setLayout`)
   * @see https://yogalayout.dev/docs/styling/layout-direction
   */
  direction?: StylePropTypes.Direction
  /**
   * Controls whether flex items are forced onto a single line or can wrap onto multiple lines.
   * @see StylePropTypes.Wrap (`NO_WRAP`, `WRAP`, `WRAP_REVERSE`)
   * @default Yoga default (`NO_WRAP`)
   * @see https://yogalayout.dev/docs/styling/flex-wrap
   */
  flexWrap?: StylePropTypes.Wrap

  /**
   * Defines the space between flex items along the main axis.
   * @unit Pixels.
   * @default Yoga default (0)
   * @see https://yogalayout.dev/docs/styling/gap
   */
  gap?: Partial<Record<keyof typeof StylePropTypes.Gutter, number | `${number}%`>> | number | `${number}%`

  /**
   * Defines how the `width` and `height` properties are interpreted regarding padding and border.
   * `CONTENT_BOX`: Width/height apply only to the content area. Padding and border are added outside.
   * `BORDER_BOX`: Width/height include content, padding, and border.
   * @see StylePropTypes.BoxSizing (`CONTENT_BOX`, `BORDER_BOX`)
   * @default `Style.BOX_SIZING_BORDER_BOX` (set in `setLayout`)
   */
  boxSizing?: StylePropTypes.BoxSizing
  /**
   * Sets the background color of the node. Drawn beneath the content and padding, extending to the border edge.
   * Accepts standard CSS color strings.
   * @default undefined (transparent)
   */
  backgroundColor?: string
  /**
   * Sets a linear gradient as the background. Overrides `backgroundColor` if provided.
   * `colors`: Array of CSS color strings for the gradient stops.
   * `direction`: Array of four numbers `[x0, y0, x1, y1]` defining the start and end points of the gradient line, relative to the node's top-left corner.
   * @default undefined
   */
  gradient?:
    | {
        type: 'linear'
        colors: string[]
        direction:
          | [number, number, number, number] // 0, y0, x1, y1 relative to node
          | 'to-top'
          | 'to-right'
          | 'to-bottom'
          | 'to-left'
          | 'to-top-right'
          | 'to-top-left'
          | 'to-bottom-right'
          | 'to-bottom-left'
      }
    | {
        type: 'radial'
        colors: string[]
        direction?:
          | [number, number, number, number]
          | 'to-top'
          | 'to-right'
          | 'to-bottom'
          | 'to-left'
          | 'to-top-right'
          | 'to-top-left'
          | 'to-bottom-right'
          | 'to-bottom-left'
      }

  /**
   * Sets the opacity of the node and its children when drawing.
   * A value between 0 (fully transparent) and 1 (fully opaque).
   * Opacity is applied to the entire rendered output of the node, including background, border, content, and children.
   * Opacity values stack multiplicatively (e.g., a parent with 0.5 opacity containing a child with 0.5 opacity results in the child rendering at 0.25 opacity relative to the background).
   * @default 1
   */
  opacity?: number

  /**
   * Defines the 2D transformations (translate, rotate, scale) applied to the node *after* layout.
   * @see TransformProps
   * @default undefined (no transformation)
   */
  transform?: TransformProps

  /**
   * Specifies the stack order of an element.
   * Only applies to nodes with `positionType: 'absolute'`.
   * Elements with a larger zIndex cover elements with a smaller one.
   * If elements share the same zIndex, their stacking order is based on their
   * original order in the children array.
   * Elements without a defined zIndex or not absolutely positioned are treated
   * as if they have zIndex: 0 for stacking relative to positioned siblings,
   * but are rendered in their normal flow order relative to other non-positioned elements.
   */
  zIndex?: number

  /**
   * Applies one or more box-shadow effects to the node.
   * Can be a single shadow object or an array of shadow objects.
   * Shadows are drawn in the order specified.
   * @see BoxShadowProps
   * @default undefined (no shadow)
   */
  boxShadow?: BoxShadowProps | BoxShadowProps[]

  // Font Props to pass to the child TextNode
  /**
   * Font size.
   * @unit Pixels.
   * @default 16
   */
  fontSize?: number

  /**
   * Font family (e.g., 'Arial', 'Helvetica', 'sans-serif').
   * Ensure the font is available in the rendering environment.
   * @default 'sans-serif'
   */
  fontFamily?: string

  /**
   * Font weight (e.g., 'normal', 'bold', 400, 700).
   * @default 'normal'
   */
  fontWeight?: 'normal' | 'bold' | '100' | '200' | '300' | '400' | '500' | '600' | '700' | '800' | '900' | number

  /**
   * Font style.
   * @default 'normal'
   */
  fontStyle?: 'normal' | 'italic'

  /**
   * Text color. Accepts standard CSS color strings.
   * @default 'black'
   */
  color?: string

  /**
   * Horizontal text alignment within the node's bounds.
   * @default 'left'
   */
  textAlign?: 'start' | 'end' | 'left' | 'center' | 'right' | 'justify' // Canvas textAlign values + 'justify'

  /**
   * Vertical text alignment within the node's bounds.
   * Note: Simple implementation aligns based on the first line.
   * @default 'top'
   */
  verticalAlign?: 'top' | 'middle' | 'bottom'

  /**
   * Line height.
   * @unit Pixels. If not set, estimated from font size.
   * @default undefined
   */
  lineHeight?: number

  /**
   * Specifies font variation settings. Provides fine control over font variation axis.
   * Accepts string in CSS font-variation-settings format.
   * @example "normal" | "historical-forms" | "small-caps" | "all-small-caps" | "petite-caps" | "all-petite-caps" | "unicase" | "titling-caps" | "lining-nums" | "oldstyle-nums" | "proportional-nums" | ...
   * @default undefined
   */
  fontVariant?: FontVariantSetting

  /**
   * Additional vertical spacing between lines of text.
   * @unit Pixels.
   * @default 0
   */
  lineGap?: number

  /**
   * Sets the spacing between letters (tracking).
   * Accepts CSS units like 'normal', '2px', '0.1em'.
   * @default 'normal' (relies on canvas default)
   * @see https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/letterSpacing
   */
  letterSpacing?: number | `${number}px` | `${number}em` | 'normal'

  /**
   * Sets the spacing between words.
   * Accepts CSS units like 'normal', '10px', '0.5em'.
   * This space is added to the intrinsic width of the space character.
   * @default 'normal' (relies on canvas default)
   * @see https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/wordSpacing
   */
  wordSpacing?: number | `${number}px` | `${number}em` | 'normal'
  // End of Font Props

  /**
   * Child nodes to be laid out within this node.
   * Can be a single BoxNode or an array of BoxNodes.
   * @default undefined
   */
  children?: Children | Children[]
}

/**
 * Defines the properties for a RootNode.
 */
export interface RootProps extends BoxProps {
  /** Width of the canvas in pixels */
  width: number
  /** Optional height of the canvas in pixels */
  height?: number
  /** Scale factor for rendering (e.g., 2 for 2x resolution) */
  scale?: number
  /** Font files to register for use */
  fonts?: FontRegistrationInfo[]
}

/**
 * Defines the properties for a GridNode.
 */
export interface GridProps extends Omit<BoxProps, 'direction'> {
  /**
   * Number of columns in the grid layout.
   * @default 1
   */
  columns?: number

  /**
   * Direction of the grid layout.
   * - 'row': Items are arranged horizontally (default)
   * - 'column': Items are arranged vertically
   * - 'row-reverse': Items are arranged horizontally in reverse order
   * - 'column-reverse': Items are arranged vertically in reverse order
   * @default 'row'
   */
  direction?: 'row' | 'column' | 'row-reverse' | 'column-reverse'
}

/**
 * Represents a text segment with styling information
 */
export interface TextSegment {
  text: string
  color?: string
  weight?: BoxProps['fontWeight']
  b?: boolean
  i?: boolean
  size?: number // Font size in pixels
  width?: number // Used for pre-calculation optimizations
}

/**
 * Defines the content and styling properties for a TextNode.
 */
export interface TextProps
  extends Omit<BoxProps, 'children' | 'gap' | 'flexDirection' | 'justifyContent' | 'alignContent' | 'alignItems'> {
  lineHeight?: number // Optional explicit line height

  /** Maximum number of lines to display. Text exceeding this limit will be truncated. */
  maxLines?: number
  /**
   * If true, adds '...' to the end of the last visible line when text is truncated due to `maxLines`.
   * If a string is provided, that string is used as the ellipsis character(s).
   * Defaults to false.
   */
  ellipsis?: boolean | string

  /**
   * Applies one or more text-shadow effects to the text.
   * Can be a single shadow object or an array of shadow objects.
   * Shadows are drawn in the order specified.
   * @see TextShadowProps
   * @default undefined (no shadow)
   */
  textShadow?: TextShadowProps | TextShadowProps[]
}

/**
 * Defines the properties for a drop-shadow effect, similar to CSS filter: drop-shadow().
 * This shadow respects the transparency of the content, unlike box-shadow.
 */
export interface DropShadowProps {
  /**
   * The horizontal offset of the shadow. Positive values move it right, negative values left.
   * @unit Pixels.
   * @default 0
   */
  offsetX?: number
  /**
   * The vertical offset of the shadow. Positive values move it down, negative values up.
   * @unit Pixels.
   * @default 0
   */
  offsetY?: number
  /**
   * The blur radius. Larger values create bigger, softer shadows. Must be non-negative.
   * @unit Pixels.
   * @default 0 (hard shadow)
   */
  blur?: number
  /**
   * The color of the shadow.
   * Accepts standard CSS color strings.
   * @default 'black'
   */
  color?: string
}

/**
 * Defines the properties for a single text-shadow effect, similar to CSS text-shadow.
 */
export interface TextShadowProps {
  /**
   * The horizontal offset of the shadow. Positive values move it right, negative values left.
   * @unit Pixels.
   * @default 0
   */
  offsetX?: number
  /**
   * The vertical offset of the shadow. Positive values move it down, negative values up.
   * @unit Pixels.
   * @default 0
   */
  offsetY?: number
  /**
   * The blur radius. Larger values create bigger, lighter blurs. Negative values are invalid.
   * @unit Pixels.
   * @default 0 (hard shadow)
   */
  blur?: number
  /**
   * The color of the shadow.
   * Accepts standard CSS color strings.
   */
  color?: string
}

/**
 * Defines the source and rendering properties for an ImageNode.
 * It extends BoxProps, so it can use all Box layout and style properties.
 */
export interface ImageProps extends Omit<BoxProps, 'children'> {
  /**
   * The source URL or file path of the image.
   */
  src: string | Buffer<ArrayBufferLike>

  /**
   * Specifies how the image should be resized to fit its container.
   * - `fill`: Stretches the image to fill the container, ignoring an aspect ratio. (Default)
   * - `contain`: Scales the image to fit within the container while preserving an aspect ratio.
   * - `cover`: Scales the image to maintain an aspect ratio while filling the container. The image will be clipped if necessary.
   * - `none`: The image is not resized. It will be centered unless dimensions exceed the container, then clipped.
   * - `scale-down`: Compares `contain` and `none`, picking the smaller concrete object size.
   * @default 'fill'
   */
  objectFit?: 'fill' | 'contain' | 'cover' | 'none' | 'scale-down'

  /**
   * Specifies the alignment of the image's content within its box using an object.
   * Provide values for `Left` or `Right` (for horizontal) and `Top` or `Bottom` (for vertical).
   * Values can be numbers (pixels) or percentage strings (`'50%'`).
   * - Horizontal: `Left` takes precedence over `Right`. Defaults to `'50%'` if neither is provided.
   * - Vertical: `Top` takes precedence over `Bottom`. Defaults to `'50%'` if neither is provided.
   * Affects rendering when `objectFit` is `contain`, `cover`, `none`, or `scale-down`.
   * @example { Left: '10%', Top: 20 } // 10% from left, 20px from top
   * @example { Right: '0%', Bottom: '0%' } // Align to bottom-right
   * @default { Left: '50%', Top: '50%' } // Center center
   */
  objectPosition?: Partial<Record<'Top' | 'Left' | 'Bottom' | 'Right', number | `${number}%`>>

  /**
   * Adjusts the saturation level of the image.
   * A value of 1 means the image is unchanged.
   * A value of 0 makes the image completely unsaturated (grayscale).
   * Values greater than 1 increase saturation.
   * @default 1
   */
  saturate?: number

  /**
   * Applies a drop-shadow effect based on the image's alpha channel,
   * similar to the CSS `filter: drop-shadow(...)`.
   * @see DropShadowProps
   * @default undefined (no drop shadow)
   */
  dropShadow?: DropShadowProps

  /**
   * Alternative text description of the image (used for accessibility or if the image fails).
   * Currently not rendered visually, but good practice to include.
   */
  alt?: string

  /**
   * Callback function that executes when the image loads successfully.
   */
  onLoad?: () => void

  /**
   * Callback function that executes when the image fails to load.
   * @param error - The error that occurred during loading.
   */
  onError?: (error: Error) => void
}
