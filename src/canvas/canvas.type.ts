import { BoxNode } from '@/canvas/layout.canvas.js'
import type { Canvas, ExportFormat, ExportOptions, SaveOptions } from 'meo-skia-canvas'
import type { TextNode } from '@/canvas/text.canvas.js'
import type { ImageNode } from '@/canvas/image.canvas.js'
import type { GridNode } from '@/canvas/grid.canvas.js'
import type { FontVariantSetting } from 'meo-skia-canvas'
import * as Style from '@/constant/common.const.js'

/** Fields every component accepts, whatever it draws. */
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

/**
 * Anything that can sit inside a container.
 *
 * `false` and `undefined` are allowed so `condition && Box({…})` reads naturally and renders
 * nothing when the condition fails. There is deliberately no function member: that is what lets
 * `Root` tell a page builder from ordinary children without ambiguity.
 */
export type Children = BoxNode | TextNode | ImageNode | GridNode | CanvasElement | false | undefined

/**
 * A component described as plain data, tagged by `__type`.
 *
 * Factories return these rather than live nodes, which is what lets a whole tree cross into a
 * worker thread by structured clone.
 */
export type CanvasElement =
  | { __type: 'Box'; props: Omit<BoxProps, 'children'>; children?: CanvasElement[] }
  | { __type: 'Column'; props: Omit<BoxProps, 'children'>; children?: CanvasElement[] }
  | { __type: 'Row'; props: Omit<BoxProps, 'children'>; children?: CanvasElement[] }
  | { __type: 'Grid'; props: Omit<GridProps, 'children'>; children?: CanvasElement[] }
  | { __type: 'GridItem'; props: Omit<GridItemProps, 'children'>; children?: CanvasElement[] }
  | { __type: 'Image'; props: Omit<ImageProps, 'onLoad' | 'onError'> }
  | { __type: 'Text'; text: string | number; props?: TextProps }
  | { __type: 'Chart'; props: Omit<ChartProps<ChartType>, 'options'> & { options?: Record<string, unknown> } }

/**
 * A font family and the files that provide it.
 * @example
 * ```ts
 * { family: 'Roboto', paths: ['./fonts/Roboto-Regular.ttf', './fonts/Roboto-Bold.ttf'] }
 * ```
 */
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
   * @see Style.FlexDirection (`COLUMN`, `ROW`, `COLUMN_REVERSE`, `ROW_REVERSE`)
   * @default Yoga default (`COLUMN`)
   * @see https://yogalayout.dev/docs/styling/flex-direction
   */
  flexDirection?: Style.FlexDirection

  /**
   * Defines how flex items are distributed along the main axis of the container.
   * @see Style.Justify (`FLEX_START`, `CENTER`, `FLEX_END`, `SPACE_BETWEEN`, `SPACE_AROUND`, `SPACE_EVENLY`)
   * @default Yoga default (`FLEX_START`)
   * @see https://yogalayout.dev/docs/styling/justify-content
   */
  justifyContent?: Style.Justify

  /**
   * Defines how flex items are aligned along the cross axis of the container.
   * @see Style.Align (`AUTO`, `FLEX_START`, `CENTER`, `FLEX_END`, `STRETCH`, `BASELINE`, `SPACE_BETWEEN`, `SPACE_AROUND`)
   * @default Yoga default (`STRETCH`)
   * @see https://yogalayout.dev/docs/styling/align-items-self
   */
  alignItems?: Style.Align

  /**
   * Allows overriding the parent's `alignItems` value for a specific flex item.
   * @see Style.Align (`AUTO`, `FLEX_START`, `CENTER`, `FLEX_END`, `STRETCH`, `BASELINE`, `SPACE_BETWEEN`, `SPACE_AROUND`)
   * @default Yoga default (`AUTO`) - Inherits from `alignItems`.
   * @see https://yogalayout.dev/docs/styling/align-items-self
   */
  alignSelf?: Style.Align

  /**
   * Defines how lines are distributed along the cross axis when `flexWrap` is `WRAP` or `WRAP_REVERSE`.
   * Has no effect when there is only one line of flex items.
   * @see Style.Align (`AUTO`, `FLEX_START`, `CENTER`, `FLEX_END`, `STRETCH`, `BASELINE`, `SPACE_BETWEEN`, `SPACE_AROUND`)
   * @default Yoga default (`FLEX_START`)
   * @see https://yogalayout.dev/docs/styling/align-content
   */
  alignContent?: Style.Align

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
   * @see Style.PositionType (`RELATIVE`, `ABSOLUTE`)
   * @default Yoga default (`RELATIVE`)
   * @see https://yogalayout.dev/docs/styling/position
   */
  positionType?: Style.PositionType

  /**
   * Specifies the offset distances for positioned elements (`positionType: 'ABSOLUTE'` or `RELATIVE`).
   * Can be a single number for all edges or an object specifying individual edges (`Top`, `Right`, `Bottom`, `Left`, `Start`, `End`).
   * `Start` and `End` are affected by `direction` (LTR/RTL).
   * @unit Pixels.
   * @default Yoga default (undefined for each edge)
   * @see https://yogalayout.dev/docs/styling/position
   */
  position?: Partial<Record<keyof typeof Style.Edge, number | `${number}%`>> | number | `${number}%`

  /**
   * Sets the margin space on the outside of the node's border.
   * Can be a single number for all edges or an object specifying individual edges.
   * @unit Pixels.
   * @default Yoga default (0 for each edge)
   * @see https://yogalayout.dev/docs/styling/margin-padding-border
   */
  margin?: Partial<Record<keyof typeof Style.Edge, number | `${number}%` | 'auto'>> | number | `${number}%` | 'auto'

  /**
   * Sets the padding space on the inside of the node's border, around the content.
   * Can be a single number for all edges or an object specifying individual edges.
   * @unit Pixels.
   * @default Yoga default (0 for each edge)
   * @see https://yogalayout.dev/docs/styling/margin-padding-border
   */
  padding?: Partial<Record<keyof typeof Style.Edge, number | `${number}%`>> | number | `${number}%`

  /**
   * Sets the width of the node's border.
   * Can be a single number for all edges or an object specifying individual edges.
   * @unit Pixels.
   * @default Yoga default (0 for each edge)
   * @see https://yogalayout.dev/docs/styling/margin-padding-border
   */
  border?: Partial<Record<keyof typeof Style.Edge, number>> | number

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
   * @see Style.Border.Dotted (2)
   * @default Style.Border.Solid (set in BoxNode constructor)
   */
  borderStyle?: typeof Style.Border.Solid | typeof Style.Border.Dashed | typeof Style.Border.Dotted

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
   * @see Style.Overflow (`VISIBLE`, `HIDDEN`, `SCROLL`)
   * @default Yoga default (`VISIBLE`)
   * @see https://yogalayout.dev/docs/styling/overflow
   */
  overflow?: Style.Overflow

  /**
   * Controls whether the node and its children are included in the layout calculation and rendering.
   * `FLEX`: The node participates in a flex layout.
   * `NONE`: The node and its subtree are ignored by layout and rendering.
   * @see Style.Display (`FLEX`, `NONE`)
   * @default Yoga default (`FLEX`)
   * @see https://yogalayout.dev/docs/styling/display
   */
  display?: Style.Display

  /**
   * Sets the primary text and layout direction (Left-to-Right or Right-to-Left).
   * Affects the meaning of `Start` and `End` edges for properties like `position`, `margin`, `padding`, `border`.
   * `INHERIT`: Uses the direction of the parent node.
   * @see Style.Direction (`INHERIT`, `LTR`, `RTL`)
   * @default `Style.DIRECTION_LTR` (set in `setLayout`)
   * @see https://yogalayout.dev/docs/styling/layout-direction
   */
  direction?: Style.Direction

  /**
   * Controls whether flex items are forced onto a single line or can wrap onto multiple lines.
   * @see Style.Wrap (`NO_WRAP`, `WRAP`, `WRAP_REVERSE`)
   * @default Yoga default (`NO_WRAP`)
   * @see https://yogalayout.dev/docs/styling/flex-wrap
   */
  flexWrap?: Style.Wrap

  /**
   * Defines the space between flex items along the main axis.
   * @unit Pixels.
   * @default Yoga default (0)
   * @see https://yogalayout.dev/docs/styling/gap
   */
  gap?: Partial<Record<keyof typeof Style.Gutter, number | `${number}%`>> | number | `${number}%`

  /**
   * Defines how the `width` and `height` properties are interpreted regarding padding and border.
   * `CONTENT_BOX`: Width/height apply only to the content area. Padding and border are added outside.
   * `BORDER_BOX`: Width/height include content, padding, and border.
   * @see Style.BoxSizing (`CONTENT_BOX`, `BORDER_BOX`)
   * @default `Style.BOX_SIZING_BORDER_BOX` (set in `setLayout`)
   */
  boxSizing?: Style.BoxSizing

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
 * Defines the properties for a GridItemNode.
 * Includes all BoxProperties plus Grid placement properties.
 */
export interface GridItemProps extends BoxProps {
  /**
   * Grid Placement Props
   */
  gridColumn?: string // e.g., "1 / 3" or "span 2"
  gridRow?: string // e.g., "1 / 2"
  gridArea?: string // shorthand
}

/**
 * Root component props for canvas rendering.
 * Extends BoxProps for layout and styling capabilities.
 */

/**
 * Describes one page to the function that builds it.
 *
 * A page is a frame for `gif`/`apng`, a sheet for `pdf`/`tiff`, and a size for `ico`.
 */
export interface PageInfo {
  /** Zero-based position in the sequence. */
  index: number

  /** Total pages in this render. */
  count: number

  /**
   * Position along the sequence, `0` on the first page and `1` on the last.
   * Interpolation and easing want this. A single-page render reports `0`.
   */
  progress: number

  /**
   * Seconds elapsed at this page, derived as `index / fps`.
   * Physics and spring integration want this rather than {@link PageInfo.progress}.
   */
  time: number
}

/**
 * Builds the content of one page. May be async.
 *
 * Only `Root` accepts this form — pages exist at the canvas level, so a nested element has no page
 * of its own to describe.
 */
export type PageBuilder = (page: PageInfo) => Children | Children[] | Promise<Children | Children[]>

/**
 * Props accepted by `Root`.
 *
 * `children` is widened here to include the page-builder form. The public `Root` overloads narrow
 * it again into the mutually exclusive still and paged shapes; this interface is the internal union
 * both collapse to, and the shape that crosses the worker boundary.
 */
export interface RootProps extends Omit<BoxProps, 'children'> {
  /**
   * Content to draw.
   *
   * Pass elements for a single-page render, or a function to render a sequence — one page per call.
   * The function form requires either {@link RootProps.pages} or {@link RootProps.duration}.
   */
  children?: Children | Children[] | PageBuilder

  /**
   * Number of pages to render. Mutually exclusive with {@link RootProps.duration}.
   * Only meaningful when `children` is a function.
   */
  pages?: number

  /**
   * Length of the sequence in seconds; the page count becomes `ceil(duration * fps)`.
   * Mutually exclusive with {@link RootProps.pages}, and only meaningful when `children` is a function.
   */
  duration?: number

  /**
   * Frame rate used to derive {@link RootProps.duration} and {@link PageInfo.time}. Defaults to 30.
   *
   * This describes the render, not the encode: pass `fps` to `toBuffer('gif', { fps })` as well if
   * the encoded animation should play at this rate.
   */
  fps?: number

  /**
   * Pages already resolved from the builder, one entry per page.
   *
   * Internal, and the only form a paged render takes across the worker boundary: the builder is a
   * function, functions cannot be structured-cloned, and running it on the worker side would cost a
   * round trip per page. `Root` resolves it on the calling thread and sends the result instead.
   * @internal
   */
  pagedChildren?: (Children | Children[])[]

  /**
   * Width of the canvas in pixels.
   * @required
   */
  width: number

  /**
   * Optional height of the canvas in pixels.
   * If not set, height is calculated from content.
   */
  height?: number

  /**
   * Scale factor for high-DPI rendering.
   * @default 1
   * @example 2 // For 2x Retina displays
   */
  scale?: number

  /**
   * Font files to register for use in the canvas.
   */
  fonts?: FontRegistrationInfo[]

  /**
   * Write fetched images to disk during this render for faster re-decode
   * when the same source appears multiple times. Disk entries are deleted
   * when the render completes — no cross-render sharing.
   * @default false
   */
  useDiskCache?: boolean

  /**
   * Maximum number of images to fetch concurrently during render.
   * @default 5
   */
  imageConcurrency?: number

  /**
   * Enable worker thread rendering for non-blocking operation.
   * Worker mode renders in a separate thread to avoid blocking the event loop.
   * @default true
   */
  workerMode?: boolean

  /**
   * Number of worker threads to use when workerMode is enabled.
   * Only applies when workerMode is true or undefined (default).
   * Has no effect when workerMode: false.
   * @default cpus().length - 1
   */
  workers?: number
}

/**
 * Root props when worker mode is enabled (default behavior).
 * Includes .release() method for memory cleanup.
 */
interface RootPropsWithWorkerBase extends RootProps {
  /**
   * Worker mode enabled or default (undefined defaults to true).
   */
  workerMode?: true

  /**
   * Number of worker threads (only available in worker mode).
   */
  workers?: number
}

/**
 * Root props when worker mode is disabled.
 * Returns plain Canvas without .release() method.
 * workers prop is not available in this mode.
 */
interface RootPropsWithoutWorkerBase extends RootProps {
  /**
   * Worker mode explicitly disabled.
   */
  workerMode: false

  /**
   * workers prop is not available when workerMode is false.
   * Setting this will cause a TypeScript error.
   */
  workers?: never
}

/**
 * Root props in worker mode, narrowed to one valid content shape.
 */
export type RootPropsWithWorker = RootPropsWithWorkerBase & RootContent

/**
 * Root props with worker mode disabled, narrowed to one valid content shape.
 */
export type RootPropsWithoutWorker = RootPropsWithoutWorkerBase & RootContent

/**
 * Props a {@link RootProps} render is reduced to before a `RootNode` is built.
 *
 * A node draws one page, so it never sees the builder form: `Root` resolves the sequence first and
 * constructs one node per page with that page's already-built children.
 */
export type RootNodeProps = Omit<RootProps, 'children' | 'pages' | 'duration' | 'fps' | 'pagedChildren'> & {
  children?: Children | Children[]
}

/**
 * Single-page content: elements, drawn once.
 *
 * The page props are `never` here so that naming one alongside static children is a compile error
 * rather than a silently ignored request. `resolvePageCount` rejects the same combination at
 * runtime, which is the half that catches untyped callers.
 */
export interface StillContent {
  children?: Children | Children[]
  pages?: never
  duration?: never
  fps?: never
}

/**
 * Multi-page content: a builder, run once per page.
 *
 * Exactly one of `pages` or `duration` is required — expressed as two members of a union rather
 * than two optional properties, so omitting both and supplying both are each rejected.
 */
export type PagedContent = { children: PageBuilder } & ({ pages: number; duration?: never; fps?: number } | { duration: number; pages?: never; fps?: number })

/** Content shapes `Root` accepts: one page of elements, or a sequence from a builder. */
export type RootContent = StillContent | PagedContent

/**
 * Formats that play a canvas's pages as an animation.
 *
 * Everything else encodes a single page — `png` and friends take one, `pdf` and `tiff` gather them
 * all as sheets — which is why the timing options below are rejected outside this pair.
 */
export type AnimatedFormat = Extract<ExportFormat, 'gif' | 'apng' | 'webp' | 'avif'>

/** Formats that encode without a timeline. */
export type StillFormat = Exclude<ExportFormat, AnimatedFormat>

/**
 * Encode options that only mean something for an animation.
 *
 * The renderer raises a `TypeError` when any of these reaches a format that cannot animate, rather
 * than dropping it silently. Splitting the export signatures by format turns that runtime failure
 * into a compile error.
 */
export interface AnimationExportOptions {
  /** Frames per second; one page is one frame. Defaults to 30. */
  fps?: number

  /** Per-frame durations in milliseconds, one per page. Overrides {@link AnimationExportOptions.fps}. */
  frameDelays?: number[]

  /** Times the animation repeats. `0` — the default — loops forever. */
  loop?: number
}

/**
 * The renderer's own `Canvas`, with its exports narrowed by format.
 *
 * A non-worker render hands back the real canvas, whose `toBuffer` accepts any format with any
 * options — so `toBuffer('png', { fps: 30 })` compiled there while the identical call through
 * `WorkerCanvas` did not. The same mistake was a compile error or a runtime `TypeError` depending
 * only on which mode the render happened to use.
 *
 * This is that narrowing, and nothing else: the object handed back is unchanged, every other member
 * is reachable, and a real `Canvas` is assignable to it.
 */
export type RenderedCanvas = Omit<Canvas, 'toBuffer' | 'toBufferSync' | 'toURL' | 'toURLSync' | 'toDataURLSync'> & {
  toBuffer(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): Promise<Buffer>
  toBuffer(format: StillFormat, options?: StillExportOptions): Promise<Buffer>

  toBufferSync(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): Buffer
  toBufferSync(format: StillFormat, options?: StillExportOptions): Buffer

  toURL(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): Promise<string>
  toURL(format: StillFormat, options?: StillExportOptions): Promise<string>

  toURLSync(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): string
  toURLSync(format: StillFormat, options?: StillExportOptions): string

  toDataURLSync(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): string
  toDataURLSync(format: StillFormat, options?: StillExportOptions): string
}

/** Export options accepted for a still format: everything except the animation timing. */
export type StillExportOptions = Omit<ExportOptions, keyof AnimationExportOptions>

/** Save options accepted for a still format. */
export type StillSaveOptions = Omit<SaveOptions, keyof AnimationExportOptions>

/**
 * Tracks can be specified as:
 * - `number` (pixels)
 * - `'auto'`
 * - `${number}fr` (fraction of available space)
 * - `${number}%` (percentage of container size)
 */
export type GridTrackSize = number | 'auto' | `${number}px` | `${number}fr` | `${number}%`

/**
 * Defines the properties for a GridNode.
 */
export interface GridProps extends BoxProps {
  /**
   * Number of columns in the grid layout.
   * @default 1
   */
  columns?: number

  /**
   * Defines the columns of the grid with a space-separated list of track sizes.
   * @example ['100px', '1fr', 'auto']
   */
  templateColumns?: GridTrackSize[]

  /**
   * Defines the rows of the grid with a space-separated list of track sizes.
   * @example ['100px', '1fr', 'auto']
   */
  templateRows?: GridTrackSize[]

  /**
   * Specifies the size of implicitly created rows.
   * @default 'auto'
   */
  autoRows?: GridTrackSize

  /**
   * Controls how the auto-placement algorithm works.
   * @default 'row'
   */
  autoFlow?: 'row' | 'column' | 'row-dense' | 'column-dense'
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
export interface TextProps extends Omit<BoxProps, 'children' | 'gap' | 'flexDirection' | 'justifyContent' | 'alignContent' | 'alignItems'> {
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
   * Request options forwarded to `fetch` when `src` is a remote (`http`/`https`) URL.
   * Accepts the standard Web `RequestInit` shape — `headers`, `method`, `body`,
   * `credentials`, `redirect`, `signal`, etc.
   *
   * Ignored when `src` is a local file path or a `Buffer` (no request is made).
   *
   * When set, the options are also folded into the image cache key, so the same
   * URL fetched with different headers/method/body is cached separately.
   * @example { headers: { Authorization: 'Bearer <token>' } }
   * @default undefined
   */
  httpOptions?: RequestInit

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
   * Alternative text description of the image (used for accessibility or if the image fails to load).
   * Currently not rendered visually, but good practice to include.
   */
  alt?: string

  /**
   * Callback function that executes when the image loads successfully.
   */
  onLoad?: () => void

  /**
   * Callback function that executes when the image fails to load.
   * @param error The error that occurred during loading.
   */
  onError?: (error: Error) => void
}

/** Chart shapes {@link Chart} can draw. */
export type ChartType = 'pie' | 'doughnut' | 'bar' | 'line'

/**
 * A single data point for pie or doughnut chart rendering.
 * - `label`: Human-readable label for the data point (shown on axes/legend).
 * - `value`: Numeric value used to determine the visual size/height of the point.
 * - `color`: Optional CSS color string to override the default dataset/series color for this point.
 */
export interface PieChartDataPoint {
  label: string
  value: number
  color?: string
}

/**
 * Represents a single dataset for a cartesian chart (like bar or line).
 * - `label`: The name of the dataset (e.g., "Sales 2023").
 * - `data`: An array of numeric values for this dataset.
 * - `color`: Optional CSS color string for the entire dataset.
 */
export interface ChartDataset {
  label: string
  data: number[]
  color?: string
}

/**
 * Defines the data structure for cartesian charts (bar, line).
 * - `labels`: An array of strings for the x-axis categories.
 * - `datasets`: An array of `ChartDataset` objects, each representing a series.
 */
export interface CartesianChartData {
  labels: string[]
  datasets: ChartDataset[]
}

/** One entry in a chart legend, handed to a custom `renderLegendItem`. */
export type LegendItem<T extends ChartType> = T extends 'bar' | 'line' ? ChartDataset : PieChartDataPoint

/** One axis or slice label, handed to a custom `renderLabelItem`. */
export type LabelItem<T extends ChartType> = T extends 'bar' | 'line' ? string : PieChartDataPoint

/** Grid lines behind a cartesian chart. */
export interface GridOptions {
  show?: boolean
  color?: string
  style?: 'solid' | 'dashed' | 'dotted'
}

// Base options common to all charts
interface BaseChartOptions<T extends ChartType> {
  showLabels?: boolean
  showLegend?: boolean
  labelFontSize?: number
  labelColor?: string
  legendPosition?: 'top' | 'bottom' | 'left' | 'right'
  renderLegendItem?: (props: { item: LegendItem<T>; index: number; color: string }) => BoxNode | null | undefined
  renderLabelItem?: (props: { item: LabelItem<T>; index: number }) => BoxNode | null | undefined
}

// Options specific to Cartesian charts
interface CartesianChartSpecificOptions {
  grid?: GridOptions
  axisColor?: string
  showValues?: boolean
  valueFontSize?: number
  valueColor?: string
  renderValueItem?: (props: { item: number; index: number; datasetIndex: number }) => BoxNode | null | undefined
  showYAxis?: boolean
  yAxisFontSize?: number
  yAxisColor?: string
  yAxisLabelFormatter?: (value: number) => string
  xAxisLabelFormatter?: (value: string, index: number) => string
}

// Options specific to Pie/Doughnut charts
interface PieChartSpecificOptions {
  /**
   * The radius of the inner circle in a doughnut chart, expressed as a
   * percentage of the outer radius. Should be between 0 and 1.
   * @default 0.6
   */
  innerRadius?: number

  /**
   * The border radius for the corners of each slice in a pie or doughnut chart.
   * @unit Pixels.
   * @default 0
   */
  sliceBorderRadius?: number
}

// The main conditional type for options
/** Rendering and style options for a chart, narrowed by its {@link ChartType}. */
export type ChartOptions<T extends ChartType> = T extends 'bar' | 'line'
  ? BaseChartOptions<T> & CartesianChartSpecificOptions
  : T extends 'pie' | 'doughnut'
    ? BaseChartOptions<T> & PieChartSpecificOptions
    : BaseChartOptions<T>

/**
 * Properties for rendering a chart inside a `BoxNode`.
 * Extends `BoxProps` so layout and visual styles can be applied.
 *
 * - `type`: Chart kind to render. Implementation may vary per type.
 * - `data`: Data for the chart. The structure depends on the chart type.
 * - `options`: Optional rendering and styling flags.
 */
export interface ChartProps<T extends ChartType> extends BoxProps {
  /**
   * Chart type to render.
   * - 'bar' | 'line' | 'pie' | 'doughnut'
   */
  type: T

  /**
   * Data for the chart.
   * - For 'bar' and 'line' charts, use `CartesianChartData`.
   * - For 'pie' and 'doughnut' charts, use an array of `PieChartDataPoint`.
   */
  data: T extends 'bar' | 'line' ? CartesianChartData : PieChartDataPoint[]

  /**
   * Optional rendering and style options, specific to the chart type.
   */
  options?: ChartOptions<T>
}
