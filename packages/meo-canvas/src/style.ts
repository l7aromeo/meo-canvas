/**
 * The style object a node carries.
 *
 * The same `style` key as the Rust surface, the same CSS names, and the values
 * each language writes naturally: `'row'` where Rust has `FlexDirection::Row`,
 * `16` where Rust has `px(16.0)`. A number is logical pixels; a `` `${number}%` ``
 * string is a percentage, which is how CSS spells both.
 *
 * Every property is optional and nothing is defaulted here. The defaults live in
 * Rust, and a style is **read, never copied** — no spread, no per-node merge —
 * because both would cost per node on a path that has to stay cheap.
 *
 * @packageDocumentation
 */

import type {
  Align,
  GridAutoFlow,
  BoxSizing,
  Direction,
  Display,
  FlexDirection,
  FlexWrap,
  Justify,
  ObjectFit,
  PositionType,
  TextAlign,
  TrackSize,
} from './index.js'

/** A length in logical pixels, or a percentage of the reference extent. */
export type Length = number | `${number}%`

/** A length that may also be `'auto'`. */
export type Dimension = Length | 'auto'

/** One value on every edge, or each edge named. */
export type Sides<T> =
  | T
  | {
      readonly top?: T
      readonly right?: T
      readonly bottom?: T
      readonly left?: T
    }

/** One radius on every corner, or each corner named. */
export type Corners =
  | number
  | {
      readonly topLeft?: number
      readonly topRight?: number
      readonly bottomRight?: number
      readonly bottomLeft?: number
    }

/** A colour, as CSS spells one: `'#101014'`, `'#f0c'`, `'#80808080'`. */
export type Color = string

/** Weight from 1 to 1000, or the two keywords CSS names. */
export type FontWeight = number | 'normal' | 'bold'

/** Upright or slanted glyphs. */
export type FontStyle = 'normal' | 'italic' | 'oblique'

/** A line through, over or under the text. */
export type TextDecoration = 'none' | 'underline' | 'overline' | 'line-through'

/** Where a line sits within its box. */
export type VerticalAlign = 'top' | 'middle' | 'bottom' | 'baseline'

/** Which of a glyph's fill and stroke is painted on top. */
export type PaintOrder = 'fill' | 'stroke'

/** How a node composites onto what is beneath it. */
export type BlendMode =
  | 'normal'
  | 'multiply'
  | 'screen'
  | 'overlay'
  | 'darken'
  | 'lighten'
  | 'color-dodge'
  | 'color-burn'
  | 'hard-light'
  | 'soft-light'
  | 'difference'
  | 'exclusion'
  | 'hue'
  | 'saturation'
  | 'color'
  | 'luminosity'

/** Whether a border is solid, dashed or dotted. */
export type BorderStyle = 'solid' | 'dashed' | 'dotted'

/** What happens to content larger than its box. */
export type Overflow = 'visible' | 'hidden' | 'scroll'

/** Where a grid item sits on one axis: a line, or a line and a span. */
export interface GridPlacement {
  /** The line it starts at, counting from one. Absent is auto-placement. */
  readonly start?: number
  /** How many tracks it covers. Absent is one. */
  readonly span?: number
}

/**
 * Everything a node can be styled with.
 *
 * Flat, as CSS is flat and as the Rust `Style` is: a reader never has to know
 * which group `gap` lives in versus `background`. The renderer keeps the four
 * groups because the wire format needs them separated.
 */
export interface Style {
  // -- Layout ---------------------------------------------------------
  /** How this node's children are arranged. */
  readonly display?: Display
  /**
   * Whether the node is placed by the flow or by its own offsets.
   *
   * `positionType`, not `position`: v1 spells the offsets `position`, so the
   * two would collide. A caller porting a v1 tree writes both and neither means
   * the other.
   */
  readonly positionType?: PositionType
  /** Offsets from the container's edges. */
  readonly position?: Sides<Length>
  /** Requested width. */
  readonly width?: Dimension
  /** Requested height. */
  readonly height?: Dimension
  /** Lower bound on the width. */
  readonly minWidth?: Dimension
  /** Lower bound on the height. */
  readonly minHeight?: Dimension
  /** Upper bound on the width. */
  readonly maxWidth?: Dimension
  /** Upper bound on the height. */
  readonly maxHeight?: Dimension
  /** Width divided by height, honoured when one axis is automatic. */
  readonly aspectRatio?: number
  /** Space outside the border. */
  readonly margin?: Sides<Dimension>
  /** Space inside the border. */
  readonly padding?: Sides<Length>
  /** Border thickness, which occupies space whether or not it is painted. */
  readonly border?: Sides<number>
  /** The axis children run along. */
  readonly flexDirection?: FlexDirection
  /** Whether children overflow onto further lines. */
  readonly flexWrap?: FlexWrap
  /** Share of free space this node absorbs. */
  readonly flexGrow?: number
  /** Share of overflow this node gives up. */
  readonly flexShrink?: number
  /** Size along the main axis before growing or shrinking. */
  readonly flexBasis?: Dimension
  /** Main-axis distribution of children. */
  readonly justifyContent?: Justify
  /** Cross-axis placement of children. */
  readonly alignItems?: Align
  /** This node's own cross-axis placement. */
  readonly alignSelf?: Align
  /** Cross-axis distribution of wrapped lines. */
  readonly alignContent?: Align
  /**
   * Space between children.
   *
   * A single value applies to both axes; `{ row, column }` names them apart.
   * v1 takes the same pair of forms and has no separate `rowGap`.
   */
  readonly gap?: Length | { readonly row?: Length; readonly column?: Length }
  /** Clipping behaviour, on both axes. */
  readonly overflow?: Overflow
  /** Whether `width` and `height` include padding and border. */
  readonly boxSizing?: BoxSizing
  /** Inline direction, which decides which edge is the start. */
  readonly direction?: Direction
  /**
   * The grid's column tracks.
   *
   * The CSS spelling rather than v1's `templateColumns`, and the rule already
   * decides it: the names are CSS's, because someone porting a design should
   * not have to translate. v1's shorter name was unambiguous only because its
   * grid properties lived on a separate `GridProps` type; in one flat style it
   * would sit beside `padding` with nothing saying which box model it belongs
   * to. Where v1 itself diverges from the reference, the reference wins — the
   * same clause that settled the bare container's defaults.
   */
  readonly gridTemplateColumns?: readonly TrackSize[]
  /** The grid's row tracks. */
  readonly gridTemplateRows?: readonly TrackSize[]
  /** Size given to rows the template does not name. */
  readonly gridAutoRows?: TrackSize
  /** Size given to columns the template does not name. */
  readonly gridAutoColumns?: TrackSize
  /** The order auto-placement fills tracks in. */
  readonly gridAutoFlow?: GridAutoFlow
  /** Where this item sits on the column axis. */
  readonly gridColumn?: GridPlacement
  /** Where this item sits on the row axis. */
  readonly gridRow?: GridPlacement

  // -- Paint ----------------------------------------------------------
  /**
   * The box's fill.
   *
   * `backgroundColor`, as v1 spells it and as CSS names the property, and
   * distinct from {@link Style.color}, which is
   * the text colour. The two sit adjacent and mean different things; that is
   * CSS's trap and keeping its names is what lets a design be ported without
   * translation.
   */
  readonly backgroundColor?: Color
  /**
   * Border colour, on every edge or per edge.
   *
   * One property, as v1 has one. The scene splits it — a fallback colour beside
   * per-edge overrides — but that split exists for the wire format's
   * convenience rather than the caller's, so the encoder routes the scalar form
   * to one field and the edge form to the other and no v2-only name reaches
   * this surface.
   */
  readonly borderColor?: Sides<Color>
  /** Whether the border is solid, dashed or dotted. */
  readonly borderStyle?: BorderStyle
  /** Corner radii. */
  readonly borderRadius?: Corners
  /** Opacity of this node and its subtree, from `0` to `1`. */
  readonly opacity?: number
  /** How this node composites onto what is beneath it. */
  readonly mixBlendMode?: BlendMode
  /** Whether gradients are dithered. */
  readonly dither?: boolean
  /** Paint order among positioned siblings. */
  readonly zIndex?: number

  // -- Text -----------------------------------------------------------
  /** The family name text is drawn in. Inherits. */
  readonly fontFamily?: string
  /** Em size in logical pixels. Inherits. */
  readonly fontSize?: number
  /** Weight from 1 to 1000. Inherits. */
  readonly fontWeight?: FontWeight
  /** Upright or italic. Inherits. */
  readonly fontStyle?: FontStyle
  /** The colour glyphs are drawn in, CSS's `color`. Inherits. */
  readonly color?: Color
  /** Horizontal alignment within the box. Inherits. */
  readonly textAlign?: TextAlign
  /** A line through, over or under. Inherits. */
  readonly textDecoration?: TextDecoration
  /** Where a line sits within its box. Inherits. */
  readonly verticalAlign?: VerticalAlign
  /** Which of a glyph's fill and stroke is on top. Inherits. */
  readonly paintOrder?: PaintOrder
  /** Line box height as a multiple of the em size. Inherits. */
  readonly lineHeight?: number
  /** Extra space between lines, in pixels. Inherits. */
  readonly lineGap?: number
  /** Space added between characters. Inherits. */
  readonly letterSpacing?: Length
  /** Space added between words. Inherits. */
  readonly wordSpacing?: Length

  // -- Effects --------------------------------------------------------
  /** A CSS filter applied to this node's own drawing. */
  readonly filter?: string
  /** A CSS filter applied to what shows through this node. */
  readonly backdropFilter?: string

  // -- Image ----------------------------------------------------------
  /**
   * How an image fills its box.
   *
   * Read only by an image, as CSS's `object-fit` applies to replaced elements
   * and is inert on everything else.
   */
  readonly objectFit?: ObjectFit
  /** Which frame of an animated source to draw. Read only by an image. */
  readonly frame?: number
}
