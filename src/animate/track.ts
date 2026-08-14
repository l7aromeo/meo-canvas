import type { PageInfo } from '@/canvas/canvas.type.js'
import { type Easing, resolveEasing } from '@/animate/easing.js'
import { type Animatable, mix } from '@/animate/interpolate.js'
import { type SpringConfig, assertSpringHasNoRange, spring, springDuration } from '@/animate/spring.js'

/** A reusable animation: what to move between, when, and how. */
export interface TrackConfig<T extends Animatable> {
  /** Value before the track starts. */
  from: T
  /** Value once it has finished. */
  to: T
  /** Seconds the motion lasts. Required unless `spring` supplies one. */
  duration?: number
  /** Seconds to wait before starting. @default 0 */
  delay?: number
  /** Extra delay per item index, for staggering a row of elements. @default 0 */
  stagger?: number
  /** Easing curve, by name or function. Mutually exclusive with `spring`. @default linear */
  ease?: Easing
  /** Spring physics instead of an easing. Supplies its own duration. */
  spring?: SpringConfig
}

/** A configured track, sampled per page. */
export interface Track<T extends Animatable> {
  /** Value at this page, optionally for the item at `index` when the track staggers. */
  at(page: PageInfo, index?: number): T
  /** Seconds from the start of the render until the first item finishes. */
  readonly duration: number
  /** Seconds until `count` staggered items have all finished. */
  totalDuration(count: number): number
}

/**
 * Declares an animation once and samples it per page.
 *
 * Tracks work in seconds because that is what `duration` and `fps` already speak, and because a
 * delay expressed as a fraction of the whole sequence changes meaning whenever the sequence length
 * changes. `index` folds in the stagger, so a row of bars is one track rather than per-item
 * arithmetic at the call site.
 */
export function track<T extends Animatable>(config: TrackConfig<T>): Track<T> {
  const { from, to, delay = 0, stagger = 0, ease, spring: springConfig } = config

  if (springConfig && ease !== undefined) {
    throw new Error('[canvas] a track takes `spring` or `ease`, not both — a spring carries its own curve')
  }
  if (springConfig) assertSpringHasNoRange(springConfig, 'a track')
  if (delay < 0) throw new Error(`[canvas] track delay cannot be negative (got ${delay})`)
  if (stagger < 0) throw new Error(`[canvas] track stagger cannot be negative (got ${stagger})`)

  // A spring settles rather than ending, so its duration comes from the physics unless overridden.
  const duration = config.duration ?? (springConfig ? springDuration(springConfig) : undefined)

  if (duration === undefined) {
    throw new Error('[canvas] a track needs a `duration` in seconds, or a `spring` to derive one from')
  }
  if (duration < 0) throw new Error(`[canvas] track duration cannot be negative (got ${duration})`)

  const easing = resolveEasing(ease)

  const at = (page: PageInfo, index = 0): T => {
    const elapsed = page.time - delay - stagger * index

    // Finished is checked first so a zero-duration track reads as instantaneous rather than as
    // never having started — the two conditions are both true at once when `duration` is 0.
    if (elapsed >= duration) return to
    if (elapsed <= 0) return from

    if (springConfig) {
      // The spring is solved over its own 0..1 range and mapped onto the endpoints, so `from` and
      // `to` stay the track's business and the physics stays independent of the units involved.
      const unit = spring(elapsed, { ...springConfig, from: 0, to: 1 })
      return mix(from, to, unit)
    }

    return mix(from, to, easing(elapsed / duration))
  }

  return {
    at,
    duration: delay + duration,
    totalDuration: (count: number) => delay + duration + stagger * Math.max(0, count - 1),
  }
}
