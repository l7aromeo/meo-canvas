/**
 * Reading a colour string, through the renderer's own parser.
 *
 * # Why this is exported rather than internal
 *
 * Without it the colour half of {@link './animate.js'} has no way in.
 * `mixColor` takes an {@link Rgba}, `interpolate` and `track` blend one, and
 * `formatColor` turns one back into a string — so everything exported operates
 * on a value nothing exported could produce. A caller starts from a string,
 * because a string is what the renderer accepts everywhere else, and
 * `animate.ts`'s own documentation says as much: *a caller with a string parses
 * it once and animates the result*. This is that parse.
 *
 * # Why the addon and not a parser here
 *
 * **One parser, one answer.** The renderer reads colours with
 * `meo-canvas-core`'s `parse_channels`, and a second implementation in
 * JavaScript would agree with it until it did not — v1 has exactly that, a
 * regex subset beside a canvas probe, lossy at alpha zero by its own comment.
 * Going through the addon means a string that renders is a string that
 * animates, by construction rather than by two test suites agreeing.
 *
 * # The channel ranges, because they are not the obvious ones
 *
 * `r`, `g` and `b` come back on **0 to 255 and are not clamped**; `a` is 0 to 1.
 * Unclamped is deliberate: `color(srgb 1.25 1.25 1.25)` is a real colour outside
 * the gamut, mixing two colours needs room to overshoot between them, and
 * clamping at the parse would flatten that before any mix saw it. The clamp
 * belongs where a colour becomes paint. {@link formatColor} writes an
 * out-of-gamut value back as `color(srgb …)` for the same reason.
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
 * The addon, loaded once and kept.
 *
 * Resolved on first use rather than at import, matching `Root`: a caller who
 * only builds a scene, or who supplies their own renderer, should not need the
 * native module present to import this package.
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
