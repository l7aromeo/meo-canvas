/**
 * The style object a node carries.
 *
 * The same CSS names as the Rust surface, with the values each language writes
 * naturally: `'row'` where Rust has `FlexDirection::Row`, `16` where Rust has
 * `px(16.0)`. A number is logical pixels; a `` `${number}%` `` string is a
 * percentage, which is how CSS spells both.
 *
 * These properties sit directly in a factory's props rather than under a `style`
 * key, as v1 spells them — `Row({ gap: 16 })`, not `Row({ style: { gap: 16 } })`.
 * The type exists so one list of properties serves every factory.
 *
 * Every property is optional and nothing is defaulted here. The defaults live in
 * Rust, and a style is **read, never copied** — no spread, no per-node merge —
 * because both would cost per node on a path that has to stay cheap.
 *
 * @packageDocumentation
 */

import type { ImageSource } from './node.js'
import type {
  Align,
  GridAutoFlow,
  BoxSizing,
  Direction,
  Display,
  FlexDirection,
  FlexWrap,
  Justify,
  BackgroundRepeat,
  ObjectFit,
  PositionType,
  TextAlign,
  TrackSize,
} from './index.js'

/** A length in logical pixels, or a percentage of the reference extent. */
export type Length = number | `${number}%`

/** A length that may also be `'auto'`. */
export type Dimension = Length | 'auto'

/**
 * Space added between characters or words.
 *
 * v1's spelling exactly: a bare number and `'…px'` are logical pixels, `'…em'`
 * is a multiple of the em size, and `'normal'` is the font's own. Not
 * {@link Length}, because a percentage means nothing here and an em does — the
 * scene's `Spacing` has the same three forms for the same reason.
 */
export type Spacing = number | `${number}px` | `${number}em` | 'normal'

/**
 * How tall a line box is, in CSS's three stated spellings.
 *
 * `normal` is the fourth kind and is not here: it is the absence of a value,
 * written by leaving `lineHeight` out. An explicit `1` is a line box exactly
 * one em tall and is a different thing.
 */
export type LineHeight = number | `${number}px` | `${number}%`

/** One value on every edge, or each edge named. */
export type Sides<T> =
  | T
  | {
      /** The top edge. An absent edge is unset, which is not the same as zero. */
      readonly top?: T
      /** The right edge, on the same terms. */
      readonly right?: T
      /** The bottom edge, on the same terms. */
      readonly bottom?: T
      /** The left edge, on the same terms. */
      readonly left?: T
    }

/** One radius on every corner, or each corner named. */
export type Corners =
  | number
  | {
      /** Radius at the top-left corner, in pixels. Absent is square. */
      readonly topLeft?: number
      /** Radius at the top-right corner. */
      readonly topRight?: number
      /** Radius at the bottom-right corner. */
      readonly bottomRight?: number
      /** Radius at the bottom-left corner. */
      readonly bottomLeft?: number
    }

/**
 * A colour, as CSS spells one.
 *
 * Whatever the renderer's parser takes, which is what a browser takes:
 * `'#101014'`, `'#f0c'`, `'#80808080'`, `'red'`, `'rgb(40 80 220 / 60%)'`,
 * `'rgba(255,255,255,0.15)'`, `'hsl(210 90% 40%)'`, `'hwb(...)'`, `'lab(...)'`
 * and `'oklch(...)'`.
 *
 * The named forms are here for the completions they give an editor, and
 * `(string & {})` is what keeps every other syntax accepted alongside them —
 * without it the union would refuse strings the renderer takes. **No
 * TypeScript type can spell CSS colour syntax**, so the type cannot be the
 * check: an unreadable colour is refused by the renderer, naming the property
 * and quoting what it received — `borderColor is "potato", which is not a
 * colour any CSS syntax spells` — rather than described by a shape the
 * compiler wanted.
 */
export type Color =
  | `#${string}`
  | 'transparent'
  | 'currentColor'
  | 'black'
  | 'white'
  | 'red'
  | 'green'
  | 'blue'
  | 'yellow'
  | 'orange'
  | 'purple'
  | 'grey'
  | 'gray'
  // `string & {}` is the one idiom that keeps a union's completions and accepts the rest.
  | (string & {})

/** Weight from 1 to 1000, or the two keywords CSS names. */
export type FontWeight = number | 'normal' | 'bold'

/**
 * Upright or slanted glyphs.
 *
 * No `'oblique'`. v1 offers `'normal' | 'italic'` and the scene's `FontStyle`
 * has the same two variants, so a third would be a keyword this package accepts
 * and cannot carry.
 */
export type FontStyle = 'normal' | 'italic'

/** A line through, over or under the text. */
export type TextDecoration = 'none' | 'underline' | 'overline' | 'line-through'

/**
 * Where a line sits within its box.
 *
 * No `'baseline'`, for the reason {@link FontStyle} has no `'oblique'`: v1's
 * `VerticalAlign` is these three and so is the scene's.
 */
export type VerticalAlign = 'top' | 'middle' | 'bottom'

/**
 * One OpenType feature, spelled as CSS's `font-variant` spells it.
 *
 * Thirty-five keywords, which is what the shorthand accepts and what the
 * renderer carries. A list rather than a single value, because CSS's
 * `font-variant` is space-separated and a caller routinely wants two at once —
 * `small-caps tabular-nums` is one setting, not a choice between them.
 *
 * **A feature does nothing unless the face carries it.** Seventeen tags swept
 * against the repository's own Oswald move exactly one of them, `frac`: there
 * are no small-caps glyphs in that face and nothing synthesises them, so
 * `'small-caps'` draws the same picture as `'normal'` and is not a defect. A
 * test that reaches for "a representative feature" and picks the wrong one
 * reports a working property as dead.
 */
export type FontVariant =
  | 'normal'
  | 'historical-forms'
  | 'small-caps'
  | 'all-small-caps'
  | 'petite-caps'
  | 'all-petite-caps'
  | 'unicase'
  | 'titling-caps'
  | 'lining-nums'
  | 'oldstyle-nums'
  | 'proportional-nums'
  | 'tabular-nums'
  | 'diagonal-fractions'
  | 'stacked-fractions'
  | 'ordinal'
  | 'slashed-zero'
  | 'common-ligatures'
  | 'no-common-ligatures'
  | 'discretionary-ligatures'
  | 'no-discretionary-ligatures'
  | 'historical-ligatures'
  | 'no-historical-ligatures'
  | 'contextual'
  | 'no-contextual'
  | 'jis78'
  | 'jis83'
  | 'jis90'
  | 'jis04'
  | 'simplified'
  | 'traditional'
  | 'full-width'
  | 'proportional-width'
  | 'ruby'
  | 'super'
  | 'sub'

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

/**
 * How big each tile of a background image is drawn.
 *
 * CSS's `background-size` and nothing more: a length or `'auto'` per axis, or
 * `'cover'` and `'contain'` which scale to the box. There is no `'fill'`,
 * `'none'` or `'scale-down'` — those belong to `object-fit`, which is a
 * different property on a different kind of node.
 *
 * A bare value sizes the width and leaves the height to the picture's own
 * proportions, which is v1's reading of `size: 12`.
 */
export type BackgroundSize =
  | 'cover'
  | 'contain'
  | Dimension
  | {
      /** Tile width. Absent leaves the image's own, so one axis can be set alone. */
      readonly width?: Dimension
      /** Tile height, on the same terms. */
      readonly height?: Dimension
    }

/** Where the first tile of a background image sits, from the box's top-left. */
export interface BackgroundPosition {
  /** Distance from the left edge. */
  readonly x?: Length
  /** Distance from the top edge. */
  readonly y?: Length
}

/** A picture painted over the background colour, and how it is placed. */
export interface BackgroundImage {
  /** A path, a URL, or the bytes themselves. A bare string is a local path. */
  readonly src: string | ImageSource
  /** How it tiles. Defaults to tiling both ways, as CSS does. */
  readonly repeat?: BackgroundRepeat
  /** How big each tile is drawn. Defaults to the picture's own size. */
  readonly size?: BackgroundSize
  /** Where the first tile sits. Defaults to the box's top-left corner. */
  readonly position?: BackgroundPosition
}

/**
 * Where a linear gradient runs.
 *
 * Either an edge-to-edge direction, an angle in degrees clockwise from twelve
 * o'clock, or explicit endpoints. The tuple is `[x0, y0, x1, y1]` in the node's
 * own coordinates from its top-left corner, which is what a keyword resolves to
 * once the node's size is known — v1's wording and v1's shape.
 *
 * The bare angle is CSS's `linear-gradient(45deg, …)`, which v1 has no spelling
 * for. The names are CSS's, so where v1 offers less than CSS the reference
 * wins.
 */
export type GradientDirection =
  | 'to-top'
  | 'to-right'
  | 'to-bottom'
  | 'to-left'
  | 'to-top-right'
  | 'to-top-left'
  | 'to-bottom-right'
  | 'to-bottom-left'
  | number
  | readonly [Length, Length, Length, Length]

/** One colour at one position along a gradient. */
export interface GradientStop {
  /** Where it sits, `0` at the start and `1` at the end. */
  readonly offset: number
  /** The colour there. */
  readonly color: Color
}

/** The point a radial or conic gradient turns about. Defaults to the middle. */
export interface GradientCenter {
  /** Distance from the left edge. */
  readonly x?: Length
  /** Distance from the top edge. */
  readonly y?: Length
}

/**
 * The colours a gradient runs through.
 *
 * Two spellings, and exactly one of them: `colors` is v1's, a list spread
 * evenly from the first to the last, and `stops` places each colour itself. The
 * even spread is arithmetic rather than a second wire shape — the scene holds
 * offsets either way.
 */
export type GradientRamp =
  | {
      /** Colours spread evenly from one end to the other. */
      readonly colors: readonly Color[]
      /** Absent in this arm: give `colors` or `stops`, never both. */
      readonly stops?: undefined
    }
  | {
      /** Colours at chosen offsets, when even spacing is not what is wanted. */
      readonly stops: readonly GradientStop[]
      /** Absent in this arm: give `colors` or `stops`, never both. */
      readonly colors?: undefined
    }

/**
 * A gradient, as a fill or as the alpha of a {@link Mask}.
 *
 * One kind per geometry, and each carries only what it reads: a radial gradient
 * has no direction and a linear one has no centre, so neither can be given a
 * value nothing looks at.
 *
 * ```ts
 * import type { Gradient } from 'meo-canvas'
 *
 * const fade: Gradient = { type: 'linear', direction: 'to-bottom', colors: ['#101014', 'transparent'] }
 * const dial: Gradient = { type: 'conic', from: 90, stops: [{ offset: 0, color: '#f0c' }] }
 * ```
 */
export type Gradient =
  | ({
      /** Selects the linear geometry: a ramp along a straight line. */
      readonly type: 'linear'
      /** Which way the ramp runs. Absent is top to bottom. */
      readonly direction?: GradientDirection
    } & GradientRamp)
  | ({
      /** Selects the radial geometry: a ramp outward from a point. */
      readonly type: 'radial'
      /** The centre it spreads from. Absent is the middle of the box. */
      readonly at?: GradientCenter
    } & GradientRamp)
  | ({
      /** Selects the conic geometry: a ramp swept around a point. */
      readonly type: 'conic'
      /** The point it sweeps around. Absent is the middle of the box. */
      readonly at?: GradientCenter
      /** Where the sweep begins, in degrees clockwise from twelve o'clock. */
      readonly from?: number
    } & GradientRamp)

/**
 * Moving, turning and scaling a node after layout.
 *
 * v1's field names, which are CSS's split apart: `translateX` rather than a
 * `translate(…)` string, because a caller composing one from data should not
 * have to build a string for a renderer to parse back.
 */
export interface Transform {
  /** Horizontal movement. */
  readonly translateX?: Length
  /** Vertical movement. */
  readonly translateY?: Length
  /** Rotation in degrees, clockwise. */
  readonly rotate?: number
  /**
   * Both scale factors at once.
   *
   * A convenience v1 has: `scaleX` or `scaleY` beside it wins on that axis.
   */
  readonly scale?: number
  /** Horizontal scale factor. */
  readonly scaleX?: number
  /** Vertical scale factor. */
  readonly scaleY?: number
  /** The point it turns and scales about, from the left edge. Defaults to the middle. */
  readonly originX?: Length
  /** The point it turns and scales about, from the top edge. Defaults to the middle. */
  readonly originY?: Length
}

/** A shadow cast by the border box. */
export interface BoxShadow {
  /** Drawn inside the box rather than behind it. */
  readonly inset?: boolean
  /** Horizontal offset. */
  readonly offsetX?: number
  /** Vertical offset. */
  readonly offsetY?: number
  /** How far the edge is softened. */
  readonly blur?: number
  /** How much the shadow grows before it is blurred. */
  readonly spread?: number
  /** Its colour. */
  readonly color?: Color
}

/** A shadow cast by the glyphs. */
export interface TextShadow {
  /** Horizontal offset. */
  readonly offsetX?: number
  /** Vertical offset. */
  readonly offsetY?: number
  /** How far the edge is softened. */
  readonly blur?: number
  /** Its colour. */
  readonly color?: Color
}

/** An outline along the glyph edges, centred on them. */
export interface TextStroke {
  /** Thickness in pixels. Half falls inside the letter and half outside. */
  readonly width?: number
  /** Its colour. */
  readonly color?: Color
}

/** A shape a mask can name without writing a path. */
export type MaskShape = 'circle' | 'ellipse'

/** Which side of a winding counts as inside. */
export type FillRule = 'nonzero' | 'evenodd'

/**
 * What of a node is drawn, and how much of it.
 *
 * Two kinds, and they cost differently. A **shape or path** clips: hard edges,
 * nothing allocated, cheap enough to put on every node in a list. A
 * **gradient** composites, so the node is drawn into an offscreen canvas the
 * size of its box and multiplied by the gradient's alpha: soft edges, at the
 * cost of that canvas.
 *
 * A bare string is path data, which is v1's shorthand for `{ path }`.
 */
export type Mask =
  | string
  | {
      /** A circle or ellipse inscribed in the node's box. Clips, so the edge is hard. */
      readonly shape: MaskShape
    }
  | {
      /** SVG path data, in the node's own coordinates. Clips. */
      readonly path: string
      /** Which side of a winding counts as inside. Absent is `nonzero`. */
      readonly fillRule?: FillRule
    }
  | {
      /**
       * A gradient read as alpha rather than as colour, so the node fades
       * rather than being cut. This is the costly arm: it needs a layer to
       * composite through, where the clipping arms do not.
       */
      readonly gradient: Gradient
    }

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
  /**
   * Space between children. One value for both axes, or each named.
   *
   * `row` is the gap *between rows*, so it separates children stacked
   * vertically -- the axis names the gap it opens, not the direction it runs.
   */
  readonly gap?:
    | Length
    | {
        /** Space between rows, separating children stacked vertically. */
        readonly row?: Length
        /** Space between columns, separating children placed side by side. */
        readonly column?: Length
      }
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
  /**
   * A shorthand for that many equal columns.
   *
   * v1's `columns`, and pure sugar: `columns: 3` is
   * `gridTemplateColumns: [fr(1), fr(1), fr(1)]` and reaches the renderer as
   * exactly those tracks. Nothing new crosses the wire, which is the test of
   * whether a shorthand is a shorthand — if it needed a slot of its own, the
   * long form could not express it and that would be a different finding.
   *
   * Naming both this and {@link Style.gridTemplateColumns} is refused rather
   * than resolved by precedence: a caller who wrote both meant one of them,
   * and nothing here can tell which.
   *
   * Being sugar, it has no separate identity once it reaches the renderer, so
   * a failure reading these tracks names `gridTemplateColumns` however they
   * were written.
   */
  readonly columns?: number
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
  /**
   * Both axes at once, as CSS's `grid-area` orders them.
   *
   * `[rowStart, columnStart, rowEnd, columnEnd]`, lines counting from one and
   * the two ends **exclusive** — `[1, 1, 3, 2]` is the item covering rows 1
   * and 2 of column 1, which is CSS's reading of `grid-area: 1 / 1 / 3 / 2`.
   * Sugar over {@link Style.gridRow} and {@link Style.gridColumn}, so it adds
   * nothing to the wire.
   *
   * Naming this beside either of those is refused, for the reason
   * {@link Style.columns} is.
   *
   * Adding nothing to the wire means it has no identity there either, so a
   * failure reading a placement names `gridColumn` or `gridRow` — the axis it
   * was reading — rather than the shorthand that supplied it.
   */
  readonly gridArea?: readonly [number, number, number, number]

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
  /**
   * OpenType features applied to the run.
   *
   * Reaches the **measurer** as well as the painter: `diagonal-fractions`
   * moves a nineteen-character sample from 220.61 to 211.04, so a feature that
   * only reached the drawing would lay text out at one width and paint it at
   * another.
   */
  readonly fontVariant?: readonly FontVariant[]
  /** How lines sit within the text box. Inherits. */
  readonly textAlign?: TextAlign
  /** A line through, over or under. Inherits. */
  readonly textDecoration?: TextDecoration
  /** Where a line sits within its box. Inherits. */
  readonly verticalAlign?: VerticalAlign
  /** Which of a glyph's fill and stroke is on top. Inherits. */
  readonly paintOrder?: PaintOrder
  /**
   * How tall a line box is. Inherits.
   *
   * A number is a multiple of the font size and is recomputed by whoever
   * inherits it; `'24px'` is an absolute height and descends unchanged;
   * `'150%'` is a share of **this** element's size, resolved here and
   * inherited as the length it comes to. Leaving it out is CSS's `normal`,
   * which is not the same as `1`.
   */
  readonly lineHeight?: LineHeight
  /** Extra space between lines, in pixels. Inherits. */
  readonly lineGap?: number
  /** Space added between characters. Inherits. */
  readonly letterSpacing?: Spacing
  /** Space added between words. Inherits. */
  readonly wordSpacing?: Spacing

  // -- Effects --------------------------------------------------------
  /**
   * A gradient painted over the background colour.
   *
   * One kind per geometry, each carrying only what it reads.
   */
  readonly gradient?: Gradient
  /** A picture painted over the gradient. */
  readonly backgroundImage?: BackgroundImage
  /**
   * Moves, turns and scales the node after it is laid out.
   *
   * Applied about {@link Transform.originX} and {@link Transform.originY},
   * which default to the middle of the box. Layout does not see it: a
   * transformed node still occupies the space it was given, as CSS's
   * `transform` does.
   */
  readonly transform?: Transform
  /**
   * Shadows cast by the box, nearest first.
   *
   * One or many, as v1 takes them. Later shadows are drawn behind earlier ones,
   * which is CSS's order.
   */
  readonly boxShadow?: BoxShadow | readonly BoxShadow[]
  /**
   * Shadows cast by the glyphs, nearest first.
   *
   * Distinct from {@link Style.boxShadow}: this follows the letterforms and
   * that follows the border box.
   */
  readonly textShadow?: TextShadow | readonly TextShadow[]
  /** An outline drawn along the glyph edges. Inherits. */
  readonly textStroke?: TextStroke
  /**
   * What of the node is drawn.
   *
   * Covers everything the node renders — background, border, content and
   * children alike — as CSS's `mask` does, rather than only its contents.
   * Applied within the node's own box, so content pushed outside by
   * {@link Style.transform} is not masked back in.
   */
  readonly mask?: Mask
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
  /**
   * Where the image sits in its box when it does not fill it.
   *
   * CSS's `object-position`, as a horizontal and a vertical offset. Absent
   * centres it, which is CSS's own initial value and what the Rust surface
   * writes. Read only by an image.
   */
  readonly objectPosition?: readonly [Length, Length]
  /** Which frame of an animated source to draw. Read only by an image. */
  readonly frame?: number
}
