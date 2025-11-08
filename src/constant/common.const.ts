import Yoga, * as All from 'yoga-layout'

/**
 * Style constants extending Yoga layout engine with additional border styles
 */
export enum Border {
  Solid,
  Dashed,
  Dotted,
}

/**
 * Consolidated Style object combining Yoga layout constants and custom border styles
 */
export const Style = {
  ...All,
  Border,
}

export * from 'yoga-layout'
export default Yoga
