import type { PageInfo } from '@/canvas/canvas.type.js'
import type { Animatable } from '@/animate/interpolate.js'

/**
 * Anything sampled per page and able to report how long it lasts.
 *
 * A `track`, a `sequence`, and a group built here all satisfy it, which is what lets groups nest
 * and lets a member be swapped for a sequence without the call site changing.
 */
export interface Sampled<T> {
  /** The value at one page. `index` overrides the page's own position when sampling a group. */
  at(page: PageInfo, index?: number): T
  /** How long one pass takes, in seconds. */
  readonly duration: number
  /** How long `count` pages take in total, in seconds — a pass plus whatever stagger separates them. */
  totalDuration(count: number): number
}

/** The value each member of a group yields, keyed the way the group was declared. */
export type GroupValue<M extends Record<string, Sampled<unknown>>> = {
  [K in keyof M]: M[K] extends Sampled<infer V> ? V : never
}

/**
 * Runs several animations at once and samples them together.
 *
 * Two things a scene needs that a single track cannot give. One is a value per member from one
 * call, so a builder reads `const { scale, tint } = ring.at(page)` rather than sampling each by
 * hand. The other is the length of the whole group: a render has to be told how long to be, and
 * working that out by hand means a `Math.max` over every track that has to be corrected every time
 * one of them changes — silently producing a render that stops before its own animation does.
 *
 * Members keep their own delays, staggers and easings; this only gathers them.
 * @example
 * ```ts
 * const ring = parallel({
 *   tint: track({ from: '#38bdf8', to: '#f472b6', duration: 1.4, ease: 'inOutSine' }),
 *   scale: track({ from: 0.6, to: 1, spring: { stiffness: 190, damping: 12 } }),
 * })
 *
 * await Root({
 *   width: 200,
 *   height: 200,
 *   duration: ring.duration, // covers the longest member, whichever that is
 *   fps: 24,
 *   children: page => {
 *     const { tint, scale } = ring.at(page)
 *     return Box({ borderColor: tint, transform: { scale } })
 *   },
 * })
 * ```
 */
export function parallel<M extends Record<string, Sampled<Animatable | Record<string, unknown>>>>(members: M): Sampled<GroupValue<M>> {
  const entries = Object.entries(members)

  if (entries.length === 0) {
    throw new Error('[canvas] a parallel group needs at least one member')
  }

  return {
    at: (page: PageInfo, index = 0) => Object.fromEntries(entries.map(([name, member]) => [name, member.at(page, index)])) as GroupValue<M>,

    // The group is over when its last member is, so the longest one decides. Taken once at
    // construction: a member's duration cannot change afterwards.
    duration: Math.max(...entries.map(([, member]) => member.duration)),

    totalDuration: (count: number) => Math.max(...entries.map(([, member]) => member.totalDuration(count))),
  }
}
