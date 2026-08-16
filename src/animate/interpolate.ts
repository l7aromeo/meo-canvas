import { isColor, mixColor } from '@/animate/color.js'
import { type Easing, resolveEasing } from '@/animate/easing.js'

/** A value this module knows how to blend: a number, a colour string, or an array of either. */
export type Animatable = number | string | readonly (number | string)[]

/**
 * Blends two numbers.
 *
 * Deliberately unclamped: `outBack` and `outElastic` return values beyond 0–1, and clamping here
 * would flatten exactly the overshoot those curves exist to produce.
 * @example
 * ```ts
 * lerp(0, 100, 0.25) // 25
 * ```
 */
export function lerp(from: number, to: number, t: number): number {
  return from + (to - from) * t
}

/**
 * Rescales a value from one range to another.
 * @example
 * ```ts
 * mapRange(50, [0, 100], [0, 1])                  // 0.5
 * mapRange(150, [0, 100], [0, 1], { clamp: true }) // 1
 * ```
 */
export function mapRange(
  value: number,
  [inMin, inMax]: readonly [number, number],
  [outMin, outMax]: readonly [number, number],
  options: { clamp?: boolean } = {},
): number {
  const span = inMax - inMin
  // An empty input range has no position to report; the start of the output range is the honest
  // answer, and avoids handing back NaN that would surface later as an invalid layout value.
  if (span === 0) return outMin

  const t = (value - inMin) / span
  const mapped = lerp(outMin, outMax, t)
  if (!options.clamp) return mapped

  const low = Math.min(outMin, outMax)
  const high = Math.max(outMin, outMax)
  return Math.min(high, Math.max(low, mapped))
}

/**
 * Blends two values of the same kind, picking the right treatment for what it is given.
 *
 * Strings are treated as colours because that is the only string this library animates; anything
 * else would have no meaningful midpoint.
 * @example
 * ```ts
 * mix(0, 10, 0.5)                    // 5
 * mix('#000000', '#ffffff', 0.5)     // '#808080'
 * mix([0, 10], [10, 20], 0.5)        // [5, 15]
 * ```
 */
export function mix<T extends Animatable>(from: T, to: T, t: number): T {
  if (typeof from === 'number' && typeof to === 'number') {
    return lerp(from, to, t) as T
  }

  if (typeof from === 'string' && typeof to === 'string') {
    return mixColor(from, to, t) as T
  }

  if (Array.isArray(from) && Array.isArray(to)) {
    if (from.length !== to.length) {
      throw new Error(`[canvas] mix() needs both arrays to be the same length (got ${from.length} and ${to.length})`)
    }
    return from.map((value, i) => mix(value as Animatable, to[i] as Animatable, t)) as unknown as T
  }

  throw new Error(`[canvas] mix() needs both ends to be the same type (got ${typeof from} and ${typeof to})`)
}

/**
 * Interpolates across a keyframe track: `stops` are the positions, `values` what to be at each.
 *
 * Values hold outside the declared range rather than extrapolating, which is what a keyframe track
 * means — the first and last frames are states, not the start of a slope.
 * @example
 * ```ts
 * interpolate(0.25, [0, 0.5, 1], [0, 100, 0])                    // 50
 * interpolate(0.5, [0, 1], ['#000000', '#ffffff'])               // '#808080'
 * ```
 */
export function interpolate<T extends Animatable>(t: number, stops: readonly number[], values: readonly T[], options: { ease?: Easing } = {}): T {
  if (stops.length !== values.length) {
    throw new Error(`[canvas] interpolate() needs the same number of stops and values (got ${stops.length} and ${values.length})`)
  }
  if (stops.length < 2) {
    throw new Error('[canvas] interpolate() needs at least two stops')
  }
  for (let i = 1; i < stops.length; i++) {
    if (stops[i] <= stops[i - 1]) {
      throw new Error(`[canvas] interpolate() needs stops in ascending order (${stops[i - 1]} then ${stops[i]})`)
    }
  }

  if (t <= stops[0]) return values[0]
  if (t >= stops[stops.length - 1]) return values[values.length - 1]

  const upper = stops.findIndex(stop => stop > t)
  const lower = upper - 1
  const span = stops[upper] - stops[lower]
  const local = (t - stops[lower]) / span

  return mix(values[lower], values[upper], resolveEasing(options.ease)(local))
}

/** Re-exported so callers can ask whether a string is a colour before handing it to `mix`. */
export { isColor }
