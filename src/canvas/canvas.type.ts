import { BoxNode } from '@/canvas/layout.canvas.js'
import type { Canvas, ColorSpace, ColorType, ExportFormat, ExportOptions, SaveOptions } from 'meo-skia-canvas'
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
  | {
      /** A plain container. */
      __type: 'Box'
      /** Everything but the children, which are their own field. */
      props: Omit<BoxProps, 'children'>
      /** Nested descriptors, drawn inside this one. */
      children?: CanvasElement[]
    }
  | {
      /** A container that stacks its children vertically. */
      __type: 'Column'
      /** Everything but the children, which are their own field. */
      props: Omit<BoxProps, 'children'>
      /** Nested descriptors, drawn inside this one. */
      children?: CanvasElement[]
    }
  | {
      /** A container that lays its children out in a line. */
      __type: 'Row'
      /** Everything but the children, which are their own field. */
      props: Omit<BoxProps, 'children'>
      /** Nested descriptors, drawn inside this one. */
      children?: CanvasElement[]
    }
  | {
      /** A container that places its children on a grid. */
      __type: 'Grid'
      /** Everything but the children, which are their own field. */
      props: Omit<GridProps, 'children'>
      /** Nested descriptors, drawn inside this one. */
      children?: CanvasElement[]
    }
  | {
      /** One cell of a grid, which may span several tracks. */
      __type: 'GridItem'
      /** Everything but the children, which are their own field. */
      props: Omit<GridItemProps, 'children'>
      /** Nested descriptors, drawn inside this one. */
      children?: CanvasElement[]
    }
  | {
      /** A raster image, fitted into its box by `objectFit`. */
      __type: 'Image'
      /** The load callbacks are dropped: a function cannot cross into a worker. */
      props: Omit<ImageProps, 'onLoad' | 'onError'>
    }
  | {
      /** An arbitrary shape from SVG path data. */
      __type: 'Path'
      /** The path and how it is painted. */
      props: PathProps
    }
  | {
      /** A run of text, which may carry inline markup. */
      __type: 'Text'
      /** The string to draw. Numbers are accepted so a count needs no conversion. */
      text: string | number
      /** Styling for the run. */
      props?: TextProps
    }
  | {
      /** A chart, drawn from data rather than from child nodes. */
      __type: 'Chart'
      /** The chart minus its options, which are widened so they survive the worker boundary. */
      props: Omit<ChartProps<ChartType>, 'options'> & {
        /** Chart options, typed loosely here and narrowed again when the node is built. */
        options?: Record<string, unknown>
      }
    }

/**
 * A font family and the files that provide it.
 * @example
 * ```ts
 * { family: 'Roboto', paths: ['./fonts/Roboto-Regular.ttf', './fonts/Roboto-Bold.ttf'] }
 * ```
 */
export interface FontRegistrationInfo {
  /** The name `fontFamily` will refer to. Any name may be chosen; it need not match the file. */
  family: string
  /** Absolute paths to the font files that make up the family — one per weight or style. */
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
   * Grows the shadow beyond the node's box before it is blurred, as the fourth length of the CSS
   * `box-shadow` shorthand does. A negative value shrinks it.
   *
   * An inset shadow spreads the other way, reaching further in from every edge.
   * @unit Pixels.
   * @default 0
   */
  spread?: number

  /**
   * The color of the shadow.
   * Accepts standard CSS color strings.
   */
  color?: string
}

/**
 * Where a gradient runs, either as a named edge-to-edge direction or as explicit endpoints.
 *
 * The tuple is `[x0, y0, x1, y1]` in the node's own coordinates, measured from its top-left corner,
 * which is what a keyword resolves to once the node's size is known.
 */
export type GradientDirection =
  [number, number, number, number] | 'to-top' | 'to-right' | 'to-bottom' | 'to-left' | 'to-top-right' | 'to-top-left' | 'to-bottom-right' | 'to-bottom-left'

/**
 * A gradient, as a background fill or as the alpha of a {@link Mask}.
 *
 * Colours are spread evenly from the first to the last; a single colour sits at the midpoint. A
 * radial gradient runs from the node's centre to the corner, so it covers the whole box, and a
 * conic gradient sweeps clockwise from twelve o'clock, as CSS does.
 */
export type Gradient =
  | {
      /** A straight run between two points. */
      type: 'linear' | Style.GradientType.Linear
      /** Stops in order, spread evenly from the first to the last. */
      colors: readonly string[]
      /** Which way the run goes — a named edge-to-edge direction, or explicit endpoints. */
      direction: GradientDirection
    }
  | {
      /** A run outward from the node's centre, reaching the corners. */
      type: 'radial' | Style.GradientType.Radial
      /** Stops in order, from the centre outwards. */
      colors: readonly string[]
      /** Unused for a radial gradient, which always runs centre to corner. */
      direction?: GradientDirection
    }
  | {
      /** A sweep around a centre, the stops running clockwise from twelve o'clock. */
      type: 'conic' | Style.GradientType.Conic
      /** Stops in order, spread evenly around the sweep. The first and last meet at the seam. */
      colors: readonly string[]

      /**
       * Where the sweep starts, in degrees clockwise from twelve o'clock. CSS `from <angle>`.
       * @unit Degrees.
       * @default 0 (twelve o'clock)
       */
      from?: number

      /**
       * The point the sweep turns about, as a fraction of the box or a percentage of it. CSS
       * `at <position>`.
       * @default the centre of the box
       */
      at?: {
        /** Distance from the left edge — `0.25` and `'25%'` mean the same thing. */
        x?: number | `${number}%`
        /** Distance from the top edge. */
        y?: number | `${number}%`
      }
    }

/** Shapes a {@link Mask} can name without writing a path, each inscribed in the node's box. */
export type MaskShape = 'circle' | 'ellipse'

/**
 * What of a node is drawn, and how much of it.
 *
 * A mask covers everything the node renders — background, border, content and children alike — the
 * way CSS `mask` does, rather than only its contents. It comes in two kinds, and they cost
 * different amounts:
 *
 * - A **shape or path** clips. Hard edges, nothing else allocated, and cheap enough to put on every
 *   node in a list.
 * - A **gradient** composites, so the node is drawn into an offscreen canvas the size of its box
 *   and then multiplied by the gradient's alpha. Soft edges, at the cost of that canvas.
 *
 * Applied within the node's own box: content pushed outside by `transform` is not masked back in.
 * @example
 * ```ts
 * Box({ mask: { shape: 'circle' }, children: [avatar] })
 * Box({ mask: 'M 0 0 L 100 0 L 50 100 Z' })
 * Box({ mask: { gradient: { type: 'linear', direction: 'to-bottom', colors: ['#000', 'transparent'] } } })
 * ```
 */
export type Mask =
  /** SVG path data, in the node's own coordinates. Shorthand for `{ path }`. */
  | string
  | {
      /** A shape inscribed in the node's box. */
      shape: MaskShape
    }
  | {
      /** SVG path data, in the node's own coordinates. */
      path: string
      /** How the path's interior is decided where it crosses itself. */
      fillRule?: 'nonzero' | 'evenodd'
    }
  /** Only the alpha of each colour matters: opaque keeps a pixel, transparent removes it. */
  | {
      /** The gradient whose alpha the node is multiplied by. */
      gradient: Gradient
    }

/**
 * A radius for each corner, in pixels. A corner left out is not rounded.
 *
 * Radii larger than the box allows are scaled down together, so opposite corners meet rather than
 * overlapping — the same rule CSS applies to `border-radius`.
 */
export interface CornerRadii {
  /** Radius of the top-left corner. */
  TopLeft?: number
  /** Radius of the top-right corner. */
  TopRight?: number
  /** Radius of the bottom-left corner. */
  BottomLeft?: number
  /** Radius of the bottom-right corner. */
  BottomRight?: number
}

/**
 * A colour for each edge. An edge left out falls back to `borderColor`'s single-string form, or to
 * black when there is none.
 *
 * Where two edges of different colours meet at a corner, the corner is split between them — the
 * same join CSS makes, so a card with one accent edge does not smear that colour round the bend.
 */
export interface EdgeColors {
  /** Colour of the top edge. */
  Top?: string
  /** Colour of the right edge. */
  Right?: string
  /** Colour of the bottom edge. */
  Bottom?: string
  /** Colour of the left edge. */
  Left?: string
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
   * `ABSOLUTE`: Taken out of the flow and positioned against its **immediate parent**.
   *
   * That last part is where this differs from CSS, and the difference is Yoga's rather than this
   * library's. CSS resolves an absolute node against the nearest *positioned* ancestor, skipping
   * every static box in between; Yoga always uses the parent, whether or not it is positioned. A
   * layout ported from the browser that relies on skipping an intermediate box will land somewhere
   * else — give the node's own parent the offsets instead.
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
   * Colour of the node's border — one colour for every edge, or a colour per edge.
   *
   * Accepts standard CSS colour strings (`'red'`, `'#FF0000'`, `'rgba(255,0,0,0.5)'`). Given an
   * object, an edge left out falls back to black. Where two edges of different colours meet at a
   * rounded corner the arc is split between them, as CSS joins them.
   * @default 'black' (set in BoxNode constructor)
   * @example
   * ```ts
   * Box({ border: 2, borderColor: '#cbd5e1' })
   * Box({ border: { Left: 4 }, borderColor: { Left: '#2563eb' } })
   * ```
   */
  borderColor?: string | `#${string}` | EdgeColors

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
  borderRadius?: CornerRadii | number

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
   * `SCROLL`: Yoga lays the node out as a scroll container, but nothing is clipped — a scroll
   * container is a box a reader moves, and nothing here is interactive. It draws as `VISIBLE`.
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
  gradient?: Gradient

  /**
   * Dither this node's drawing, trading a little noise for the banding an eight-bit surface shows
   * across a long, subtle gradient.
   *
   * Costs nothing where nothing bands: a flat fill, text and a blurred shadow encode to identical
   * bytes either way, because a dither only perturbs a pixel whose colour falls between two the
   * surface can hold. On a gradient reckon on about a third again the PNG bytes, and near nothing
   * for WebP or JPEG, whose quantizers absorb the noise.
   *
   * Inherited by descendants, so setting it on `Root` covers the page and a node overrides it for
   * its own subtree without touching its siblings. Pointless under a float `colorType`, which has
   * the precision to draw the ramp outright.
   * @default false
   * @example
   * ```ts
   * Root({ width: 800, dither: true, children: [
   *   Box({ height: 400, gradient: { type: 'linear', direction: 'to-bottom', colors: ['#0b1220', '#1e2b4a'] } }),
   *   Box({ dither: false, children: [Text({ children: 'left alone' })] }),
   * ] })
   * ```
   */
  dither?: boolean

  /**
   * Limits what of this node is drawn — see {@link Mask}.
   *
   * Covers everything the node renders, its background, border, content and children alike. A shape
   * or path clips; a gradient fades. Inherited by every component, so `Text`, `Image`, `Chart` and
   * `Grid` take it too.
   * @default undefined (nothing masked)
   * @example
   * ```ts
   * Image({ src: avatar, width: 96, height: 96, mask: { shape: 'circle' } })
   * Box({ mask: { gradient: { type: 'linear', direction: 'to-bottom', colors: ['#000', 'transparent'] } } })
   * ```
   */
  mask?: Mask

  /**
   * Graphical effects applied to the node and everything inside it, in CSS `filter` notation.
   *
   * The whole subtree is drawn once and the chain applied to the result, which is what CSS does —
   * so two overlapping children are filtered together rather than each on its own. Functions run
   * left to right, and anything that does not parse is ignored rather than throwing.
   *
   * `saturate` on an {@link ImageProps} is the same machinery: where both are given, the shorthand
   * runs first, then this chain.
   *
   * A blur reaches past the node's box, as it does in CSS, and is not clipped to it.
   * @default undefined (no filter)
   * @example
   * ```ts
   * Box({ filter: 'grayscale(1)' })
   * Box({ filter: 'brightness(1.2) contrast(0.9)' })
   * Box({ filter: 'blur(4px) hue-rotate(90deg)' })
   * ```
   */
  filter?: string

  /**
   * Graphical effects applied to whatever is painted behind the node, in CSS `filter` notation.
   *
   * The filtered backdrop is clipped to the node's own box, corners included, and the node's
   * background paints over the result — so a translucent background over a blurred backdrop is the
   * frosted glass CSS produces. A node with no background of its own shows the backdrop filtered
   * and nothing else.
   *
   * Only what has already been drawn is a backdrop: a sibling declared after this node paints over
   * it and is not included, as in CSS.
   * @default undefined (the backdrop is untouched)
   * @example
   * ```ts
   * Box({
   *   backdropFilter: 'blur(12px) saturate(1.4)',
   *   backgroundColor: 'rgba(255,255,255,0.15)',
   *   borderRadius: 24,
   * })
   * ```
   */
  backdropFilter?: string

  /**
   * How the node and everything inside it is combined with what is already painted behind it.
   *
   * CSS `mix-blend-mode`. The subtree is drawn once and the blend applied to the result, so two
   * overlapping children blend with the backdrop together rather than each in turn.
   *
   * Only what has already been painted counts as a backdrop, which is the same rule
   * `backdropFilter` follows.
   * @default Style.BlendMode.Normal (painted straight over the backdrop)
   * @example
   * ```ts
   * Box({ backgroundColor: '#0af', mixBlendMode: Style.BlendMode.Multiply })
   * Text('watermark', { mixBlendMode: Style.BlendMode.Overlay })
   * ```
   */
  mixBlendMode?: Style.BlendMode | 'normal' | 'multiply' | 'screen' | 'overlay' | (string & {})

  /**
   * A picture painted across the node's box, behind its content and over its background colour.
   *
   * CSS `background-image` and the properties that place it. The source is fetched and decoded
   * before layout, the same way an {@link ImageProps.src} is, and shares the same cache — a picture
   * used as one node's background and another's image is loaded once.
   *
   * Unlike an `Image`, this never affects layout: the box is whatever the box was.
   * @default undefined (no picture)
   * @example
   * ```ts
   * Box({ backgroundImage: { src: 'texture.png' } })
   * Box({ backgroundImage: { src: 'hero.jpg', size: Style.BackgroundSize.Cover, repeat: Style.BackgroundRepeat.NoRepeat } })
   * Box({ backgroundImage: { src: 'dot.svg', size: 12, position: { x: '50%', y: 0 } } })
   * ```
   */
  backgroundImage?: {
    /** A URL, a file path, or the bytes themselves. */
    src: string | Buffer

    /**
     * How the picture tiles to fill the box.
     * @default Style.BackgroundRepeat.Repeat (tiled both ways, as CSS does)
     */
    repeat?: Style.BackgroundRepeat | 'repeat' | 'repeat-x' | 'repeat-y' | 'no-repeat' | 'space' | 'round'

    /**
     * How big each tile is drawn. A number is a width in pixels with the height following the
     * picture's own proportions; a pair sizes both edges; `Cover` and `Contain` scale to the box.
     * @default the picture's natural size
     */
    size?:
      | Style.BackgroundSize
      | 'cover'
      | 'contain'
      | number
      | {
          /** Width of one tile, or a share of the box's width. */
          width?: number | `${number}%`
          /** Height of one tile, or a share of the box's height. */
          height?: number | `${number}%`
        }

    /**
     * Where the first tile sits, from the box's top-left. Percentages place the picture the way CSS
     * does — `'100%'` puts its far edge against the box's far edge rather than pushing it outside.
     * @default the top-left corner
     */
    position?: {
      /** Distance from the left edge, or the share of the slack CSS lines the picture up by. */
      x?: number | `${number}%`
      /** Distance from the top edge, read the same way. */
      y?: number | `${number}%`
    }
    /** Recolours an SVG's fills before it is rasterised, as {@link ImageProps.color} does. */
    color?: string
    /** Options for a remote fetch. Ignored for a local path or a buffer. */
    httpOptions?: ImageProps['httpOptions']
  }

  /**
   * Sets the opacity of the node and its children when drawing.
   * A value between 0 (fully transparent) and 1 (fully opaque).
   *
   * The node's whole drawing — background, border, content and children — is composited once and
   * then faded, as CSS does. Two overlapping children inside a half-transparent parent are exactly
   * as dark as one of them, rather than compounding into a darker patch where they meet.
   *
   * Nesting multiplies: a child at 0.5 inside a parent at 0.5 draws at 0.25 against the page.
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
   * Stack order among absolutely positioned siblings. A larger value paints over a smaller one, and
   * equal values paint in the order they were declared.
   *
   * Only absolutely positioned nodes take part. An in-flow child is painted in flow order and is
   * never lifted by a `zIndex`.
   *
   * Leaving it unset is CSS's `z-index: auto`, which shares a layer with `0` — so an absolutely
   * positioned child still paints above in-flow siblings, whether it is declared before or after
   * them. A negative value puts it below them instead, which is how a decoration is placed behind
   * the content of its own parent.
   * @example
   * ```ts
   * Box({
   *   positionType: Style.PositionType.Relative,
   *   children: [
   *     // Behind the content, though it is declared first.
   *     Box({ positionType: Style.PositionType.Absolute, zIndex: -1, backgroundColor: '#eef' }),
   *     Text('over the decoration'),
   *   ],
   * })
   * ```
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
  textAlign?: Style.TextAlign | 'start' | 'end' | 'left' | 'center' | 'right' | 'justify'

  /**
   * Lines drawn on the text, in the notation CSS `text-decoration` uses.
   *
   * A line keyword on its own is the common case; a style, a colour and a thickness may follow in
   * any order, and two line keywords may be combined. Anything that does not parse draws nothing
   * rather than throwing. Inherited, so a heading and its nested spans are decorated together.
   * @default undefined (no lines)
   * @example
   * ```ts
   * Text('Sold out', { textDecoration: 'line-through' })
   * Text('Heading', { textDecoration: 'underline 3px #2563eb' })
   * Text('Misspelt', { textDecoration: 'underline wavy #dc2626' })
   * Text('Both', { textDecoration: 'underline line-through' })
   * ```
   */
  textDecoration?: Style.TextDecoration | 'none' | 'underline' | 'overline' | 'line-through' | (string & {})

  /**
   * An outline drawn on the glyphs, as CSS `-webkit-text-stroke`.
   *
   * The stroke is centred on the glyph's outline, so half of it falls inside the letter. Painted
   * over the fill — which is what CSS does unless told otherwise — a thick stroke eats inward and
   * thins the letterform; {@link TextProps.paintOrder} moves it under the fill, where it only
   * widens the glyph outward.
   * @default undefined (no outline)
   * @example
   * ```ts
   * Text('Outlined', { color: '#ffd400', textStroke: { width: 4, color: '#102a43' } })
   * Text('Whole letters', {
   *   color: '#ffd400',
   *   textStroke: { width: 4, color: '#102a43' },
   *   paintOrder: Style.PaintOrder.Stroke,
   * })
   * ```
   */
  textStroke?: {
    /**
     * Thickness of the outline, centred on the glyph — half falls inside the letter, half outside.
     * @unit Pixels.
     * @default 0 (no outline)
     */
    width?: number

    /**
     * Colour of the outline.
     * @default the text's own colour
     */
    color?: string
  }

  /**
   * Whether a glyph's stroke is painted over its fill or under it. CSS `paint-order`.
   * @default Style.PaintOrder.Fill (the stroke over the fill, as CSS does unasked)
   */
  paintOrder?: Style.PaintOrder | 'fill' | 'stroke'

  /**
   * Vertical text alignment within the node's bounds.
   * Note: Simple implementation aligns based on the first line.
   * @default 'top'
   */
  verticalAlign?: Style.VerticalAlign | 'top' | 'middle' | 'bottom'

  /**
   * Height of each line box.
   *
   * Left unset, a line is the face's own height — its ascent plus its descent — which is what
   * `line-height: normal` means in CSS and is a little over 1.3em for most text faces. Set smaller
   * than that and the lines overlap rather than the box quietly growing, again as CSS does.
   * @unit Pixels.
   * @default undefined (the face's own height)
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
   * Which columns the item occupies, in the notation CSS `grid-column` uses: a line-to-line range
   * like `'1 / 3'`, or `'span 2'` to take that many from wherever the item lands.
   * @default undefined (the next free cell)
   */
  gridColumn?: string

  /**
   * Which rows the item occupies, read the same way as {@link gridColumn}.
   * @default undefined (the next free cell)
   */
  gridRow?: string

  /**
   * Row and column together, as CSS `grid-area` writes them: `'1 / 2 / 3 / 4'` is
   * row-start / column-start / row-end / column-end.
   * @default undefined
   */
  gridArea?: string
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
   *
   * Spans the sequence inclusively, `index / (count - 1)`, which is what a one-shot animation
   * wants: it should finish at its end value on the frame the viewer stops on. It is the wrong
   * curve for anything that repeats — see {@link PageInfo.cycle}.
   */
  progress: number

  /**
   * Position around a loop, `0` on the first page and approaching `1` on the last without
   * reaching it — `index / count`.
   *
   * The one to feed anything periodic: a rotation, an orbit, a sine, a gradient sweep. `1` and `0`
   * are the same point on a circle, so driving those from {@link PageInfo.progress} makes the last
   * page a copy of the first, and the animation visibly stutters for one frame on every repeat.
   * Because a full turn lands exactly where the next loop begins, this closes seamlessly instead:
   *
   * ```ts
   * // stutters: the final page repeats page 0
   * Math.sin(progress * 2 * Math.PI)
   * // seamless: the final page is one step short of the start
   * Math.sin(cycle * 2 * Math.PI)
   * ```
   *
   * A single-page render reports `0`.
   */
  cycle: number

  /**
   * Seconds elapsed at this page, derived as `index / fps`.
   * Physics and spring integration want this rather than {@link PageInfo.progress}.
   *
   * Spans `[0, duration)` for the same reason {@link PageInfo.cycle} does — the page after the last
   * is the next loop's first — so time-driven periodic motion is already seamless.
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
   * Width of the canvas in pixels. Required — everything else can be derived from the content, but
   * text cannot wrap without knowing how much room it has.
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
   * Rasterize on the GPU when one is available. `false` forces the CPU backend.
   *
   * Asking is not getting: a build without GPU support, a driver that declines, and a float
   * `colorType` all fall back to the CPU. The rendered canvas reports what it settled on through
   * `gpu` and `engine`.
   *
   * Set it `false` for output that must be identical between machines — GPU and CPU rasterizers
   * resolve anti-aliased edges a level or two apart, which a pixel comparison sees.
   * @default true
   */
  gpu?: boolean

  /**
   * Pixel format the canvas composites in.
   *
   * Governs the precision everything is drawn at, and the depth the encoded formats that carry one
   * write. `RGBAF32` keeps colour outside sRGB rather than clipping it as it is drawn, and is what
   * a sixteen-bit PNG or a wide-gamut export needs — at the cost of the CPU backend, since no GPU
   * composites float.
   * @default 'rgba'
   */
  colorType?: ColorType

  /**
   * Space the canvas composites in.
   *
   * Fixed for the whole render rather than chosen per export: colours are interpreted in it, and
   * one outside its gamut is clipped as it is drawn. Exports convert out of it when asked.
   * @default 'srgb'
   */
  colorSpace?: ColorSpace

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
export interface RootPropsWithWorkerBase extends RootProps {
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
export interface RootPropsWithoutWorkerBase extends RootProps {
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
  /** The tree for one page, already resolved — a builder has been run by the time this is built. */
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
  /** The tree to draw. */
  children?: Children | Children[]
  /** Not available on a still render — see the note above. */
  pages?: never
  /** Not available on a still render — see the note above. */
  duration?: never
  /** Not available on a still render — see the note above. */
  fps?: never
}

/**
 * Multi-page content: a builder, run once per page.
 *
 * Exactly one of `pages` or `duration` is required — expressed as two members of a union rather
 * than two optional properties, so omitting both and supplying both are each rejected.
 */
export type PagedContent = {
  /** Run once per page, and returns that page's tree. */
  children: PageBuilder
} & (
  | {
      /** How many pages to render. Mutually exclusive with `duration`. */
      pages: number
      /** Not available alongside `pages` — the count is already fixed. */
      duration?: never
      /** Rate the page times are derived at. Describes the render, not the encode. */
      fps?: number
    }
  | {
      /** How long the sequence runs, in seconds. The page count becomes `ceil(duration * fps)`. */
      duration: number
      /** Not available alongside `duration` — the count is derived from it. */
      pages?: never
      /** Rate the page count and page times are derived at. */
      fps?: number
    }
)

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
  /** Encodes the canvas and resolves with the bytes. An animated format takes every page. */
  toBuffer(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): Promise<Buffer>
  /** Encodes the canvas and resolves with the bytes. */
  toBuffer(format: StillFormat, options?: StillExportOptions): Promise<Buffer>

  /** `toBuffer`, blocking instead of resolving. */
  toBufferSync(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): Buffer
  /** `toBuffer`, blocking instead of resolving. */
  toBufferSync(format: StillFormat, options?: StillExportOptions): Buffer

  /** `toBuffer`, resolved as a `data:` URL. */
  toURL(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): Promise<string>
  /** `toBuffer`, resolved as a `data:` URL. */
  toURL(format: StillFormat, options?: StillExportOptions): Promise<string>

  /** `toBuffer`, as a `data:` URL, blocking instead of resolving. */
  toURLSync(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): string
  /** `toBuffer`, as a `data:` URL, blocking instead of resolving. */
  toURLSync(format: StillFormat, options?: StillExportOptions): string

  /** `toURLSync` under its `HTMLCanvasElement` name. */
  toDataURLSync(format: AnimatedFormat, options?: ExportOptions & AnimationExportOptions): string
  /** `toURLSync` under its `HTMLCanvasElement` name. */
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
  /** The run of characters this segment draws. */
  text: string
  /** Colour for this run, from a `<color>` tag. Falls back to the node's `color`. */
  color?: string
  /** Weight for this run, from a `<weight>` tag. Falls back to the node's `fontWeight`. */
  weight?: BoxProps['fontWeight']
  /** Whether a `<b>` tag encloses this run. */
  b?: boolean
  /** Whether an `<i>` tag encloses this run. */
  i?: boolean

  /**
   * Size for this run, from a `<size>` tag. Falls back to the node's `fontSize`.
   * @unit Pixels.
   */
  size?: number

  /**
   * The run's measured width, cached while the line is being laid out so wrapping does not measure
   * the same run twice.
   */
  width?: number
}

/**
 * Defines the content and styling properties for a TextNode.
 */
export interface TextProps extends Omit<BoxProps, 'children' | 'gap' | 'flexDirection' | 'justifyContent' | 'alignContent' | 'alignItems'> {
  /** Maximum number of lines to display. Text exceeding this limit will be truncated. */
  maxLines?: number

  /**
   * Marks the last visible line when `maxLines` cuts the text short.
   *
   * `true` uses `…`, the character CSS uses. A string replaces it — a longer one simply leaves the
   * text less room.
   *
   * The last line is filled to the character rather than to the last whole word that fitted, which
   * is what a browser does: `Flower of Paradise Lost` in 140px ends `Flower of Par…`, not
   * `Flower of…`. Text is drawn up from the lines `maxLines` discards to do it, but never across a
   * newline in the text — that break was asked for, where a wrap is only where the width ran out.
   * @default false
   * @example
   * ```ts
   * Text('Flower of Paradise Lost', { width: 140, maxLines: 1, ellipsis: true })
   * Text('Read the whole thing', { maxLines: 2, ellipsis: ' — more' })
   * ```
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
 * How a {@link PathProps} path is painted: a CSS colour, or a gradient over the node's box.
 *
 * The gradient is the same shape the `gradient` prop takes, measured against the node rather than
 * against the path — so two paths in the same box share a ramp instead of each restarting it.
 */
export type PathPaint = string | Gradient

/**
 * An arbitrary shape, drawn from SVG path data and laid out like any other node.
 *
 * The escape hatch for what the components cannot describe — an arrow, a tick, a connector, a
 * badge with a notch. It stays declarative rather than exposing a drawing context, so it works in
 * worker mode, where a context cannot go.
 *
 * Coordinates are the node's own: `0,0` is its top-left corner, as with {@link Mask}. Give it a
 * `width` and `height` so flexbox has something to place — the path itself does not size the node,
 * since a path can extend anywhere and layout has to be decided before it is drawn.
 * @example
 * ```ts
 * Path({ d: 'M 0 0 L 100 0 L 50 80 Z', fill: '#38bdf8', width: 100, height: 80 })
 * Path({ d: 'M 0 20 H 80', stroke: '#f43f5e', lineWidth: 4, lineCap: 'round', width: 80, height: 40 })
 * ```
 */
export interface PathProps extends Omit<BoxProps, 'children'> {
  /** SVG path data, in the node's own coordinates. */
  d: string

  /** Paint for the interior. Nothing is filled without it. */
  fill?: PathPaint

  /** Paint for the outline. Nothing is stroked without it. */
  stroke?: PathPaint

  /**
   * Width of the stroke.
   * @default 1
   */
  lineWidth?: number

  /**
   * Which side of a crossing counts as inside — `evenodd` makes nested subpaths cut holes.
   * @default 'nonzero'
   */
  fillRule?: 'nonzero' | 'evenodd'

  /**
   * Shape of a stroke's ends.
   * @default 'butt'
   */
  lineCap?: 'butt' | 'round' | 'square'

  /**
   * Shape of a stroke's corners.
   * @default 'miter'
   */
  lineJoin?: 'bevel' | 'round' | 'miter'

  /** Dash and gap lengths for the stroke, in the pattern `[dash, gap, …]`. */
  lineDash?: number[]

  /** Where the dash pattern starts, which is what animates a marching-ants outline. */
  lineDashOffset?: number
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
   * Frame to draw from an animated source, instead of playing it.
   *
   * An animated `gif`, `apng`, `webp` or `avif` plays by itself in a paged render, advancing at the
   * source's own rate. Naming a frame pins it to that one — a poster, a thumbnail, or a sequence
   * driven by hand. Negative counts from the end, and a frame the source does not have is refused.
   * @example
   * ```ts
   * Image({ src: 'spinner.gif', frame: 0 })   // first frame, however long the animation is
   * Image({ src: 'spinner.gif', frame: -1 })  // last frame
   * ```
   * @default undefined (plays in a paged render, first frame in a still one)
   */
  frame?: number

  /**
   * Whether an animated source restarts once it reaches its last frame.
   *
   * `false` holds the last frame instead, which is what a one-shot animation wants when the render
   * outlasts it. Ignored when {@link ImageProps.frame} pins a frame.
   * @default true
   */
  loop?: boolean

  /**
   * Specifies how the image should be resized to fit its container.
   * - `fill`: Stretches the image to fill the container, ignoring an aspect ratio. (Default)
   * - `contain`: Scales the image to fit within the container while preserving an aspect ratio.
   * - `cover`: Scales the image to maintain an aspect ratio while filling the container. The image will be clipped if necessary.
   * - `none`: The image is not resized. It will be centered unless dimensions exceed the container, then clipped.
   * - `scale-down`: Compares `contain` and `none`, picking the smaller concrete object size.
   * @default 'fill'
   */
  objectFit?: Style.ObjectFit | 'fill' | 'contain' | 'cover' | 'none' | 'scale-down'

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
  /** Shown in the legend and, unless `showLabels` is off, beside the slice. */
  label: string
  /** The slice's size, as a share of every value in the set rather than as a percentage. */
  value: number
  /** Left unset, a colour is chosen from the built-in sequence by position. */
  color?: string
}

/**
 * Represents a single dataset for a cartesian chart (like bar or line).
 * - `label`: The name of the dataset (e.g., "Sales 2023").
 * - `data`: An array of numeric values for this dataset.
 * - `color`: Optional CSS color string for the entire dataset.
 */
export interface ChartDataset {
  /** Names the series in the legend. */
  label: string
  /** One value per label on the category axis; a shorter array leaves the remaining slots empty. */
  data: number[]
  /** Left unset, a colour is chosen from the built-in sequence by position. */
  color?: string
}

/**
 * Defines the data structure for cartesian charts (bar, line).
 * - `labels`: An array of strings for the x-axis categories.
 * - `datasets`: An array of `ChartDataset` objects, each representing a series.
 */
export interface CartesianChartData {
  /** The category axis, one entry per position. Every dataset is read against these. */
  labels: string[]
  /** One series per entry, drawn together against the same axes. */
  datasets: ChartDataset[]
}

/** One entry in a chart legend, handed to a custom `renderLegendItem`. */
export type LegendItem<T extends ChartType> = T extends 'bar' | 'line' ? ChartDataset : PieChartDataPoint

/** One axis or slice label, handed to a custom `renderLabelItem`. */
export type LabelItem<T extends ChartType> = T extends 'bar' | 'line' ? string : PieChartDataPoint

/** Grid lines behind a cartesian chart. */
export interface GridOptions {
  /**
   * Draw the grid lines behind the plot.
   * @default false
   */
  show?: boolean

  /**
   * Colour of the grid lines. Accepts standard CSS colour strings.
   */
  color?: string

  /**
   * How each line is drawn.
   * @default 'solid'
   */
  style?: 'solid' | 'dashed' | 'dotted'
}

/**
 * What a chart's item callbacks may hand back.
 *
 * `Box`, `Row` and the rest return descriptors, and `BoxNode` is exported as a type only — so a
 * descriptor is the only one of these a consumer outside the package can actually build. The chart
 * appends the result to a live tree, and builds a descriptor into a node on the way.
 */
export type ChartItem = BoxNode | CanvasElement | null | undefined

/** Options every chart type understands. */
export interface BaseChartOptions<T extends ChartType> {
  /**
   * Draw the label beside each value — the category name on a bar or line chart, the slice's own
   * label on a pie or doughnut.
   * @default true
   */
  showLabels?: boolean

  /**
   * Draw the legend.
   * @default true
   */
  showLegend?: boolean

  /**
   * Size of the label text.
   * @unit Pixels.
   * @default 12
   */
  labelFontSize?: number

  /**
   * Colour of the label text. Accepts standard CSS colour strings.
   */
  labelColor?: string

  /**
   * Which side of the plot the legend sits on.
   * @default 'bottom'
   */
  legendPosition?: 'top' | 'bottom' | 'left' | 'right'

  /**
   * Draws one legend entry in place of the built-in one. Return nothing to leave that entry out.
   * @example ({ item, color }) => Row({ children: [Box({ width: 12, height: 12, backgroundColor: color }), Text(item.label)] })
   */
  renderLegendItem?: (props: { item: LegendItem<T>; index: number; color: string }) => ChartItem

  /**
   * Draws one axis or slice label in place of the built-in one. Return nothing to leave it out.
   */
  renderLabelItem?: (props: { item: LabelItem<T>; index: number }) => ChartItem
}

/** Options only a chart with axes understands — `bar` and `line`. */
export interface CartesianChartSpecificOptions {
  /**
   * The grid lines behind the plot — see {@link GridOptions}.
   * @default undefined (no grid)
   */
  grid?: GridOptions

  /**
   * Colour of the axis lines. Accepts standard CSS colour strings.
   */
  axisColor?: string

  /**
   * Print each value above its bar or point.
   * @default false
   */
  showValues?: boolean

  /**
   * Size of the printed values.
   * @unit Pixels.
   * @default 12
   */
  valueFontSize?: number

  /**
   * Colour of the printed values. Accepts standard CSS colour strings.
   */
  valueColor?: string

  /**
   * Draws the value printed above one bar or point in place of the built-in one. `datasetIndex`
   * says which series it belongs to. Return nothing to leave it out.
   */
  renderValueItem?: (props: { item: number; index: number; datasetIndex: number }) => ChartItem

  /**
   * Draw the value axis down the left of the plot.
   * @default false
   */
  showYAxis?: boolean

  /**
   * Size of the value axis labels.
   * @unit Pixels.
   * @default 12
   */
  yAxisFontSize?: number

  /**
   * Colour of the value axis labels. Accepts standard CSS colour strings.
   */
  yAxisColor?: string

  /**
   * Rewrites each value-axis label before it is drawn — for a currency prefix, a unit, or a
   * thousands separator.
   * @example (value) => `$${value.toLocaleString()}`
   */
  yAxisLabelFormatter?: (value: number) => string

  /**
   * Rewrites each category-axis label before it is drawn. The index is its position along the axis,
   * which is what lets every other one be dropped on a crowded chart.
   * @example (label, index) => (index % 2 ? '' : label)
   */
  xAxisLabelFormatter?: (value: string, index: number) => string
}

/** Options only a chart of slices understands — `pie` and `doughnut`. */
export interface PieChartSpecificOptions {
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
