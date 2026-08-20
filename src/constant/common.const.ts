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
 * Consolidated Style object combining Yoga layout constants and custom border styles
 */
export const Style: typeof All & {
  /** Border styles, which Yoga has no notion of — it lays out a border's width, not its look. */
  Border: typeof Border
} = {
  ...All,
  Border,
}

export * from 'yoga-layout'
export default Yoga
