/**
 * Reading a colour string through the renderer's own parser, `meo-canvas-core`'s
 * `parse_channels`, so a string that renders is a string that animates. Exported
 * because the colour half of `animate.ts` takes an {@link Rgba} nothing else yields.
 * Channels are unclamped: `r`, `g` and `b` on 0 to 255, and `a` on 0 to 1.
 */

import type { Rgba } from './animate.js'
import { resolveAddon } from './addon.js'

/** The colour half of the addon. */
interface ColorAddon {
  /** The channels of a colour string, or `null` if it is not one. */
  parseColor(css: string): Rgba | null
  /** Whether a string is a colour this renderer understands. */
  isColor(css: string): boolean
}

/**
 * The addon, loaded on first use rather than at import, as `Root` does, so a caller
 * building a scene or supplying a renderer need not have it present.
 */
let addon: ColorAddon | undefined

function loaded(): ColorAddon {
  addon ??= resolveAddon<ColorAddon>()
  return addon
}

/**
 * The channels of a colour string, or `null` where it is not a colour.
 *
 * `null` rather than a throw, because asking whether a string is a colour is
 * the ordinary case and an exception is a poor way to answer a question. Use
 * {@link isColor} where only the answer matters.
 *
 * @example
 * ```ts
 * import { formatColor, mixColor, parseColor } from 'meo-canvas'
 *
 * const from = parseColor('#f2aa4c')
 * const to = parseColor('rebeccapurple')
 * if (from && to) formatColor(mixColor(from, to, 0.5))
 * ```
 */
export function parseColor(css: string): Rgba | null {
  return loaded().parseColor(css)
}

/**
 * Whether a string is a colour this renderer understands.
 *
 * The same parser as {@link parseColor}, so the two cannot disagree about a
 * string the way a validator written beside a parser eventually does.
 */
export function isColor(css: string): boolean {
  return loaded().isColor(css)
}
