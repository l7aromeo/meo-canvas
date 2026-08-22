import Yoga, * as All from 'yoga-layout'

/**
 * Style constants extending Yoga layout engine with additional border styles
 */
export enum Border {
  /** One unbroken line. */
  Solid,
  /** A run of dashes. */
  Dashed,
  /** A run of dots — shorter and more of them than {@link Border.Dashed}. */
  Dotted,
}

/**
 * Which of a glyph's fill and stroke is painted on top.
 *
 * CSS `paint-order`, and the reason it exists: a stroke is centred on the outline, so half of it
 * falls inside the glyph. Painted over the fill — the default — a thick stroke eats into the
 * letterform and thins it. Painted under, the fill stays whole and the stroke only widens the
 * glyph outward.
 */
export enum PaintOrder {
  /** Fill first, then the stroke over it. CSS's default, and what a browser draws unasked. */
  Fill = 'fill',
  /** Stroke first, then the fill over it, leaving the letterform whole. */
  Stroke = 'stroke',
}

/**
 * How a node's pixels are combined with what is already painted behind them.
 *
 * CSS `mix-blend-mode`. The values are the CSS keywords, so they can be handed to a canvas
 * context unchanged.
 */
export enum BlendMode {
  /** Paint over what is behind, ignoring it. The default. */
  Normal = 'normal',
  /** Multiply the two colours: the result is never lighter than either. Darkens. */
  Multiply = 'multiply',
  /** The inverse of {@link BlendMode.Multiply}: never darker than either. Lightens. */
  Screen = 'screen',
  /** {@link BlendMode.Multiply} on dark backdrops, {@link BlendMode.Screen} on light ones. */
  Overlay = 'overlay',
  /** Keeps whichever colour is darker, channel by channel. */
  Darken = 'darken',
  /** Keeps whichever colour is lighter, channel by channel. */
  Lighten = 'lighten',
  /** Brightens the backdrop to reflect the source. */
  ColorDodge = 'color-dodge',
  /** Darkens the backdrop to reflect the source. */
  ColorBurn = 'color-burn',
  /** {@link BlendMode.Overlay} with the roles reversed — the source decides. */
  HardLight = 'hard-light',
  /** A gentler {@link BlendMode.HardLight}, like shining a diffused light on the backdrop. */
  SoftLight = 'soft-light',
  /** The absolute difference between the two colours. */
  Difference = 'difference',
  /** {@link BlendMode.Difference} with less contrast. */
  Exclusion = 'exclusion',
  /** The source's hue, with the backdrop's saturation and brightness. */
  Hue = 'hue',
  /** The source's saturation, with the backdrop's hue and brightness. */
  Saturation = 'saturation',
  /** The source's hue and saturation, with the backdrop's brightness. */
  Color = 'color',
  /** The source's brightness, with the backdrop's hue and saturation. */
  Luminosity = 'luminosity',
}

/**
 * How a background image tiles to fill the box it is painted into.
 *
 * CSS `background-repeat`, which tiles unless told otherwise.
 */
export enum BackgroundRepeat {
  /** Tile on both axes, the CSS default. The last tile is cut off wherever the box ends. */
  Repeat = 'repeat',
  /** Tile across only, one row. */
  RepeatX = 'repeat-x',
  /** Tile down only, one column. */
  RepeatY = 'repeat-y',
  /** Draw once. */
  NoRepeat = 'no-repeat',
  /** Tile whole copies only, spreading the leftover space evenly between them. */
  Space = 'space',
  /** Tile whole copies only, stretching each one so they fill the box exactly. */
  Round = 'round',
}

/** How a background image is sized against the box, where it is not given a length. */
export enum BackgroundSize {
  /** Scale until the box is covered, cropping the overflow. Keeps the aspect ratio. */
  Cover = 'cover',
  /** Scale until the whole image fits, leaving space on one axis. Keeps the aspect ratio. */
  Contain = 'contain',
}

/** The shape a gradient's colour stops are laid along. */
export enum GradientType {
  /** Stops run along a line. */
  Linear = 'linear',
  /** Stops run outward from a centre point. */
  Radial = 'radial',
  /** Stops run around a centre point, sweeping clockwise from twelve o'clock. */
  Conic = 'conic',
}

/** How an image fills the box it is drawn into. CSS `object-fit`. */
export enum ObjectFit {
  /** Stretch to the box, ignoring the aspect ratio. The default. */
  Fill = 'fill',
  /** Scale until the whole image fits, keeping the aspect ratio. */
  Contain = 'contain',
  /** Scale until the box is covered, cropping the overflow, keeping the aspect ratio. */
  Cover = 'cover',
  /** Draw at its natural size. */
  None = 'none',
  /** {@link ObjectFit.None} or {@link ObjectFit.Contain}, whichever is smaller. */
  ScaleDown = 'scale-down',
}

/** Where a line of text sits across the width it is given. */
export enum TextAlign {
  /** The start edge for the writing direction. */
  Start = 'start',
  /** The end edge for the writing direction. */
  End = 'end',
  /** The left edge, whatever the direction. */
  Left = 'left',
  /** Centred. */
  Center = 'center',
  /** The right edge, whatever the direction. */
  Right = 'right',
  /** Spaced out to both edges, except on the last line. */
  Justify = 'justify',
}

/** Where a line of text sits within its line box. */
export enum VerticalAlign {
  /** Against the top of the box. */
  Top = 'top',
  /** Centred in the box. */
  Middle = 'middle',
  /** Against the bottom of the box. */
  Bottom = 'bottom',
}

/** A line drawn through, over or under text. CSS `text-decoration-line`. */
export enum TextDecoration {
  /** No line. */
  None = 'none',
  /** A line under the text, at the font's own underline position. */
  Underline = 'underline',
  /** A line above the text. */
  Overline = 'overline',
  /** A line through the middle of the text. */
  LineThrough = 'line-through',
}

/**
 * Consolidated Style object combining Yoga layout constants and custom border styles
 *
 * Everything Yoga defines, plus the constants for the parts of drawing Yoga has no notion of. The
 * custom enums are string-valued, and the values are the CSS keywords themselves — so a caller can
 * pass either `Style.BlendMode.Multiply` or `'multiply'`, and a value can be handed to a canvas
 * context without a lookup table in between. {@link Border} predates that and is numeric.
 */

/**
 * Position types, Yoga's plus the one it has no notion of.
 *
 * `Fixed` resolves against the page rather than against the nearest positioned ancestor. There is
 * no scrolling viewport to hold still against, so what it buys over `Absolute` is reaching past
 * every positioned ancestor in one step — and being captured by a transform or a filter, as CSS
 * has it.
 *
 * `Sticky` stays in the flow and treats its insets as constraints rather than offsets: the node
 * moves only where its flow position would put it nearer an edge than the inset allows. Nothing
 * scrolls here, so what remains of it is that clamp, which Chrome applies whether or not anything
 * scrolls.
 *
 * Both are numbered past Yoga's three so neither can be mistaken for one.
 */
const PositionType = {
  ...All.PositionType,
  Fixed: 3,
  Sticky: 4,
} as const

/** The value of {@link Style.PositionType.Fixed}, which Yoga's own enum does not carry. */
export type FixedPositionType = (typeof PositionType)['Fixed']

/** The value of {@link Style.PositionType.Sticky}, which Yoga's own enum does not carry. */
export type StickyPositionType = (typeof PositionType)['Sticky']

export const Style: Omit<typeof All, 'PositionType'> & {
  /** Yoga's position types plus `Fixed`, which it does not define. */
  PositionType: typeof PositionType
  /** Border styles, which Yoga has no notion of — it lays out a border's width, not its look. */
  Border: typeof Border
  /** Whether a glyph's stroke is painted over its fill. */
  PaintOrder: typeof PaintOrder
  /** How a node's pixels combine with what is behind them. */
  BlendMode: typeof BlendMode
  /** How a background image tiles. */
  BackgroundRepeat: typeof BackgroundRepeat
  /** How a background image is sized against its box. */
  BackgroundSize: typeof BackgroundSize
  /** The shape a gradient's stops are laid along. */
  GradientType: typeof GradientType
  /** How an image fills its box. */
  ObjectFit: typeof ObjectFit
  /** Where a line of text sits across its width. */
  TextAlign: typeof TextAlign
  /** Where a line of text sits within its line box. */
  VerticalAlign: typeof VerticalAlign
  /** A line drawn through, over or under text. */
  TextDecoration: typeof TextDecoration
} = {
  ...All,
  PositionType,
  Border,
  PaintOrder,
  BlendMode,
  BackgroundRepeat,
  BackgroundSize,
  GradientType,
  ObjectFit,
  TextAlign,
  VerticalAlign,
  TextDecoration,
}

export * from 'yoga-layout'
export default Yoga
