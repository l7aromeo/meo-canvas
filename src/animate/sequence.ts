import type { PageInfo } from '@/canvas/canvas.type.js'
import { type Easing, resolveEasing } from '@/animate/easing.js'
import { type Animatable, mix } from '@/animate/interpolate.js'
import { type SpringConfig, assertSpringHasNoRange, spring, springDuration } from '@/animate/spring.js'
import type { Track } from '@/animate/track.js'

/** One leg of a sequence: where to move to, over how long, and how. */
export interface SequenceStep<T extends Animatable> {
  /** Value at the end of this step. The start is wherever the previous step finished. */
  to: T
  /** Seconds this leg lasts. Required unless `spring` supplies one. */
  duration?: number
  /** Easing for this leg. Mutually exclusive with `spring`. @default linear */
  ease?: Easing
  /** Spring physics for this leg instead of an easing. Supplies its own duration. */
  spring?: SpringConfig
  /** Seconds to rest at `to` before the next step begins. @default 0 */
  hold?: number
}

/** A multi-step animation on a single value. */
export interface SequenceConfig<T extends Animatable> {
  /** Value before the sequence starts, and the start of its first step. */
  from: T
  /** Steps, run one after another. */
  steps: SequenceStep<T>[]
  /** Seconds to wait before the first step. @default 0 */
  delay?: number
  /** Extra delay per item index, for offsetting a row of elements. @default 0 */
  stagger?: number
}

/** A step with its timing resolved and its position on the timeline fixed. */
interface PlannedStep<T extends Animatable> {
  from: T
  to: T
  start: number
  duration: number
  ease: ReturnType<typeof resolveEasing>
  spring?: SpringConfig
}

/**
 * Chains several animations on one value, each starting where the last finished.
 *
 * A {@link Track} moves between two values, which covers most of what a scene needs. A value that
 * has to move, wait, and move again — a badge that drops in, pauses, then slides away — would
 * otherwise mean working out each boundary by hand and keeping those numbers in step with every
 * later edit. Declaring the legs instead keeps the arithmetic in one place.
 *
 * Returns the same shape a track does, so the two are interchangeable at the call site.
 * @example
 * ```ts
 * const badge = sequence({
 *   from: -28,
 *   steps: [
 *     { to: 0, spring: { stiffness: 200, damping: 15 } }, // drop in
 *     { to: 0, duration: 0.5, hold: 0.35 },               // rest there
 *     { to: -28, duration: 0.3, ease: 'inCubic' },        // leave
 *   ],
 *   delay: 0.35,
 * })
 *
 * Box({ transform: { translateY: badge.at(page) } })
 * ```
 */
export function sequence<T extends Animatable>(config: SequenceConfig<T>): Track<T> {
  const { from, steps, delay = 0, stagger = 0 } = config

  if (steps.length === 0) {
    throw new Error('[canvas] a sequence needs at least one step')
  }
  if (delay < 0) throw new Error(`[canvas] sequence delay cannot be negative (got ${delay})`)
  if (stagger < 0) throw new Error(`[canvas] sequence stagger cannot be negative (got ${stagger})`)

  const planned: PlannedStep<T>[] = []
  let cursor = delay
  let previous = from

  for (const [index, step] of steps.entries()) {
    if (step.spring && step.ease !== undefined) {
      throw new Error(`[canvas] sequence step ${index} takes \`spring\` or \`ease\`, not both — a spring carries its own curve`)
    }

    if (step.spring) assertSpringHasNoRange(step.spring, `sequence step ${index}`)

    const duration = step.duration ?? (step.spring ? springDuration(step.spring) : undefined)
    if (duration === undefined) {
      throw new Error(`[canvas] sequence step ${index} needs a \`duration\` in seconds, or a \`spring\` to derive one from`)
    }
    if (duration < 0) throw new Error(`[canvas] sequence step ${index} duration cannot be negative (got ${duration})`)

    const hold = step.hold ?? 0
    if (hold < 0) throw new Error(`[canvas] sequence step ${index} hold cannot be negative (got ${hold})`)

    planned.push({
      from: previous,
      to: step.to,
      start: cursor,
      duration,
      ease: resolveEasing(step.ease),
      spring: step.spring,
    })

    // The hold sits after the motion, so the next step begins once the rest is over.
    cursor += duration + hold
    previous = step.to
  }

  const last = planned[planned.length - 1]
  // The trailing hold is not part of the sequence's length: nothing moves during it, so counting it
  // would pad every render that sizes itself from `duration`.
  const end = last.start + last.duration
  const final = last.to

  const at = (page: PageInfo, index = 0): T => {
    const elapsed = page.time - stagger * index

    if (elapsed <= delay) return from
    if (elapsed >= end) return final

    // Linear scan rather than a binary search: a sequence is a handful of steps, and the scan keeps
    // the boundary rules — hold before the next step, motion during a step — in one readable place.
    for (const step of planned) {
      if (elapsed < step.start) return step.from
      if (elapsed >= step.start + step.duration) continue

      const local = elapsed - step.start
      if (step.spring) {
        const unit = spring(local, { ...step.spring, from: 0, to: 1 })
        return mix(step.from, step.to, unit)
      }
      return mix(step.from, step.to, step.ease(step.duration === 0 ? 1 : local / step.duration))
    }

    return final
  }

  return {
    at,
    duration: end,
    totalDuration: (count: number) => end + stagger * Math.max(0, count - 1),
  }
}
