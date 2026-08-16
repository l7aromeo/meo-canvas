/** Maps normalised time to normalised progress. Both are 0–1, though some curves overshoot. */
export type EasingFn = (t: number) => number

/** An easing named from the catalogue, or supplied directly. */
export type Easing = EasingName | EasingFn

const clamp01 = (t: number): number => (t < 0 ? 0 : t > 1 ? 1 : t)

/** Turns an ease-in curve into its out and in-out counterparts, so each family is written once. */
const family = (easeIn: EasingFn) => ({
  in: (t: number) => easeIn(t),
  out: (t: number) => 1 - easeIn(1 - t),
  inOut: (t: number) => (t < 0.5 ? easeIn(t * 2) / 2 : 1 - easeIn((1 - t) * 2) / 2),
})

/** Overshoot for the back family, the constant CSS and every animation library settled on. */
const BACK_OVERSHOOT = 1.70158
/** Period of the elastic oscillation, as a fraction of the duration. */
const ELASTIC_PERIOD = 0.3
const BOUNCE_N = 7.5625
const BOUNCE_D = 2.75

const quad = family(t => t * t)
const cubic = family(t => t * t * t)
const quart = family(t => t * t * t * t)
const quint = family(t => t * t * t * t * t)
const sine = family(t => 1 - Math.cos((t * Math.PI) / 2))
const expo = family(t => (t === 0 ? 0 : Math.pow(2, 10 * t - 10)))
const circ = family(t => 1 - Math.sqrt(1 - t * t))
const back = family(t => (BACK_OVERSHOOT + 1) * t * t * t - BACK_OVERSHOOT * t * t)
const elastic = family(t => {
  if (t === 0 || t === 1) return t
  return -Math.pow(2, 10 * t - 10) * Math.sin(((t * 10 - 10.75) * (2 * Math.PI)) / ELASTIC_PERIOD)
})

/** Bounce is defined by its out form — the piecewise decay everyone recognises — and mirrored. */
const outBounce: EasingFn = t => {
  if (t < 1 / BOUNCE_D) return BOUNCE_N * t * t
  if (t < 2 / BOUNCE_D) return BOUNCE_N * (t -= 1.5 / BOUNCE_D) * t + 0.75
  if (t < 2.5 / BOUNCE_D) return BOUNCE_N * (t -= 2.25 / BOUNCE_D) * t + 0.9375
  return BOUNCE_N * (t -= 2.625 / BOUNCE_D) * t + 0.984375
}
const bounce = family(t => 1 - outBounce(1 - t))

/**
 * The standard easing catalogue.
 *
 * Every entry clamps its input, so a track that runs past its own duration holds at its end value
 * rather than continuing to accelerate off the curve.
 * @example
 * ```ts
 * easings.outCubic(0.5)  // 0.875
 * track({ from: 0, to: 1, duration: 1, ease: 'outCubic' })
 * ```
 */
export const easings = {
  linear: (t: number) => clamp01(t),

  inQuad: (t: number) => quad.in(clamp01(t)),
  outQuad: (t: number) => quad.out(clamp01(t)),
  inOutQuad: (t: number) => quad.inOut(clamp01(t)),

  inCubic: (t: number) => cubic.in(clamp01(t)),
  outCubic: (t: number) => cubic.out(clamp01(t)),
  inOutCubic: (t: number) => cubic.inOut(clamp01(t)),

  inQuart: (t: number) => quart.in(clamp01(t)),
  outQuart: (t: number) => quart.out(clamp01(t)),
  inOutQuart: (t: number) => quart.inOut(clamp01(t)),

  inQuint: (t: number) => quint.in(clamp01(t)),
  outQuint: (t: number) => quint.out(clamp01(t)),
  inOutQuint: (t: number) => quint.inOut(clamp01(t)),

  inSine: (t: number) => sine.in(clamp01(t)),
  outSine: (t: number) => sine.out(clamp01(t)),
  inOutSine: (t: number) => sine.inOut(clamp01(t)),

  inExpo: (t: number) => expo.in(clamp01(t)),
  outExpo: (t: number) => expo.out(clamp01(t)),
  inOutExpo: (t: number) => expo.inOut(clamp01(t)),

  inCirc: (t: number) => circ.in(clamp01(t)),
  outCirc: (t: number) => circ.out(clamp01(t)),
  inOutCirc: (t: number) => circ.inOut(clamp01(t)),

  inBack: (t: number) => back.in(clamp01(t)),
  outBack: (t: number) => back.out(clamp01(t)),
  inOutBack: (t: number) => back.inOut(clamp01(t)),

  inElastic: (t: number) => elastic.in(clamp01(t)),
  outElastic: (t: number) => elastic.out(clamp01(t)),
  inOutElastic: (t: number) => elastic.inOut(clamp01(t)),

  inBounce: (t: number) => bounce.in(clamp01(t)),
  outBounce: (t: number) => bounce.out(clamp01(t)),
  inOutBounce: (t: number) => bounce.inOut(clamp01(t)),
} satisfies Record<string, EasingFn>

export type EasingName = keyof typeof easings

/** How close the solver has to get before it accepts a value of t. */
const BEZIER_EPSILON = 1e-6
const BEZIER_MAX_ITERATIONS = 12

/**
 * Builds the CSS `cubic-bezier(x1, y1, x2, y2)` curve.
 *
 * The curve is parametric, so drawing it at a given time means first finding the parameter whose x
 * equals that time. Newton's method converges in a few steps for well-behaved curves; a bisection
 * fallback covers the steep ones, where the derivative approaches zero and Newton stalls.
 * @example
 * ```ts
 * const easeInOut = cubicBezier(0.42, 0, 0.58, 1) // the CSS ease-in-out curve
 * ```
 */
export function cubicBezier(x1: number, y1: number, x2: number, y2: number): EasingFn {
  const curve = (a: number, b: number, t: number) => {
    const c = 3 * a
    const bTerm = 3 * (b - a) - c
    const aTerm = 1 - c - bTerm
    return ((aTerm * t + bTerm) * t + c) * t
  }
  const slope = (a: number, b: number, t: number) => {
    const c = 3 * a
    const bTerm = 3 * (b - a) - c
    const aTerm = 1 - c - bTerm
    return (3 * aTerm * t + 2 * bTerm) * t + c
  }

  return (time: number): number => {
    const target = clamp01(time)
    if (target === 0 || target === 1) return target

    let t = target
    for (let i = 0; i < BEZIER_MAX_ITERATIONS; i++) {
      const x = curve(x1, x2, t) - target
      if (Math.abs(x) < BEZIER_EPSILON) return curve(y1, y2, t)
      const d = slope(x1, x2, t)
      if (Math.abs(d) < BEZIER_EPSILON) break
      t -= x / d
    }

    // Newton stalled on a near-flat section; bisect, which cannot.
    let low = 0
    let high = 1
    t = target
    while (high - low > BEZIER_EPSILON) {
      if (curve(x1, x2, t) < target) low = t
      else high = t
      t = (low + high) / 2
    }
    return curve(y1, y2, t)
  }
}

/**
 * Quantises progress into `count` equal jumps, as CSS `steps(count, end)` does.
 *
 * Floors rather than rounds, which is what makes it hold each step for its full width: a value a
 * hair below the next boundary belongs to the step it is still inside. The final instant is pinned
 * to 1 so the animation lands on its end value rather than a step short of it.
 */
export function steps(count: number): EasingFn {
  if (!Number.isInteger(count) || count < 1) {
    throw new Error(`[canvas] steps() needs at least 1 step (got ${count})`)
  }
  return (t: number) => {
    const clamped = clamp01(t)
    return clamped >= 1 ? 1 : Math.floor(clamped * count) / count
  }
}

/** Resolves a name or function into an easing, defaulting to linear. */
export function resolveEasing(easing: Easing | undefined): EasingFn {
  if (easing === undefined) return easings.linear
  if (typeof easing === 'function') return easing

  const found = easings[easing]
  if (!found) {
    throw new Error(`[canvas] "${easing}" is not a known easing — see \`easings\` for the catalogue`)
  }
  return found
}
