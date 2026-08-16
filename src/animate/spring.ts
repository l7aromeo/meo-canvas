/** A damped spring, described the way animation libraries describe one. */
export interface SpringConfig {
  /** Where the motion starts. @default 0 */
  from?: number
  /** Where it comes to rest. @default 1 */
  to?: number
  /** How hard it pulls toward the target. @default 170 */
  stiffness?: number
  /** How strongly motion is resisted. Higher settles sooner, and past critical stops any overshoot. @default 26 */
  damping?: number
  /** Inertia. Heavier is slower to start and slower to stop. @default 1 */
  mass?: number
  /** Speed at t = 0, in units per second. Positive points at the target. @default 0 */
  velocity?: number
}

/** react-spring's defaults, which most people's intuition for "a spring" is calibrated to. */
const DEFAULT_STIFFNESS = 170
const DEFAULT_DAMPING = 26
const DEFAULT_MASS = 1

/** How close to the target counts as arrived, as a fraction of the total distance. */
const DEFAULT_REST_DELTA = 0.005

/**
 * Width of the band around ζ = 1 treated as critical damping.
 *
 * The underdamped and overdamped solutions both divide by a term that vanishes as ζ approaches 1,
 * so near the boundary they lose precision long before they produce a literal division by zero.
 * Inside this band the critically damped form is used, which is exact there and continuous with
 * both neighbours.
 */
const CRITICAL_BAND = 1e-4

interface ResolvedSpring {
  from: number
  to: number
  omega0: number
  zeta: number
  velocity: number
}

function resolve(config: SpringConfig): ResolvedSpring {
  const { from = 0, to = 1, stiffness = DEFAULT_STIFFNESS, damping = DEFAULT_DAMPING, mass = DEFAULT_MASS, velocity = 0 } = config

  if (!(stiffness > 0)) throw new Error(`[canvas] spring stiffness must be greater than 0 (got ${stiffness})`)
  if (!(damping >= 0)) throw new Error(`[canvas] spring damping cannot be negative (got ${damping})`)
  if (!(mass > 0)) throw new Error(`[canvas] spring mass must be greater than 0 (got ${mass})`)

  // Undamped angular frequency, and the damping ratio that decides which regime the spring is in.
  const omega0 = Math.sqrt(stiffness / mass)
  const zeta = damping / (2 * Math.sqrt(stiffness * mass))

  return { from, to, omega0, zeta, velocity }
}

/**
 * Position of a damped spring at time `t`, in seconds.
 *
 * Solved in closed form rather than integrated step by step, which is what makes it usable from a
 * page builder: any page can be evaluated on its own, in any order, and asking twice gives the same
 * answer. A stepwise simulation would need every earlier frame first.
 *
 * The three regimes are genuinely different solutions of the same equation, not a single formula
 * with edge cases — underdamped motion oscillates, critically damped is the fastest approach that
 * does not, and overdamped crawls in without ever crossing.
 * @example
 * ```ts
 * spring(0.2, { from: 0, to: 100, stiffness: 190, damping: 12 }) // position at 200ms
 * ```
 */
export function spring(t: number, config: SpringConfig = {}): number {
  const { from, to, omega0, zeta, velocity } = resolve(config)

  // Before the motion starts there is nothing to report but the starting position.
  if (t <= 0) return from

  const distance = to - from
  // Displacement is measured from the target, so it decays to zero and the maths stays symmetric.
  const x0 = -distance
  const v0 = velocity

  let displacement: number

  if (Math.abs(zeta - 1) < CRITICAL_BAND) {
    // Critically damped: (x0 + (v0 + omega0 * x0) t) e^(-omega0 t)
    const decay = Math.exp(-omega0 * t)
    displacement = (x0 + (v0 + omega0 * x0) * t) * decay
  } else if (zeta < 1) {
    // Underdamped: oscillates inside an exponential envelope.
    const omegaD = omega0 * Math.sqrt(1 - zeta * zeta)
    const decay = Math.exp(-zeta * omega0 * t)
    const cosine = Math.cos(omegaD * t)
    const sine = Math.sin(omegaD * t)
    displacement = decay * (x0 * cosine + ((v0 + zeta * omega0 * x0) / omegaD) * sine)
  } else {
    // Overdamped: two real exponentials, no oscillation.
    const rate = omega0 * Math.sqrt(zeta * zeta - 1)
    const slow = -zeta * omega0 + rate
    const fast = -zeta * omega0 - rate
    const c2 = (v0 - slow * x0) / (fast - slow)
    const c1 = x0 - c2
    displacement = c1 * Math.exp(slow * t) + c2 * Math.exp(fast * t)
  }

  return to + displacement
}

/**
 * Rejects a spring whose own `from`/`to` would be discarded.
 *
 * A track and a sequence step each define their own range, and drive the spring over 0..1 so the
 * physics stays independent of the units. A `from` or `to` on the spring itself therefore cannot be
 * honoured — and dropping it silently would animate to a value the caller never asked for while
 * looking like it had been obeyed.
 */
export function assertSpringHasNoRange(config: SpringConfig, owner: string): void {
  if (config.from !== undefined || config.to !== undefined) {
    throw new Error(`[canvas] ${owner} defines its own \`from\`/\`to\`, so the spring cannot carry them as well — remove them from the spring config`)
  }
}

/** How long a spring takes to arrive, and how close counts as arrived. */
export interface SpringDurationOptions {
  /** Fraction of the total distance still to travel when it counts as at rest. @default 0.005 */
  restDelta?: number
}

/** Ceiling on the search, so a barely damped spring cannot run the loop forever. */
const MAX_SETTLE_SECONDS = 100
const SETTLE_STEP_SECONDS = 1 / 240
/** Oscillations a spring must spend inside the threshold before it counts as finished. */
const SETTLE_WINDOW_CYCLES = 1
/** Samples of quiet required when there is no oscillation to wait out. */
const SETTLE_WINDOW_SAMPLES = 2

/**
 * Seconds until the spring has settled, so a render can be sized by the motion rather than guessed.
 *
 * A spring approaches its target asymptotically and has no natural end, which is awkward when the
 * page count has to be a number: `duration: springDuration(config)` turns the physics into that
 * number instead of leaving it to trial and error.
 *
 * Found by walking the closed-form solution rather than solving the envelope analytically, because
 * the envelope is only an upper bound — an underdamped spring can sit inside it while still
 * swinging through the target, and the honest answer is when it stops moving, not when the bound
 * gets small.
 */
export function springDuration(config: SpringConfig = {}, options: SpringDurationOptions = {}): number {
  const { restDelta = DEFAULT_REST_DELTA } = options
  const { from = 0, to = 1 } = config

  const distance = Math.abs(to - from) || 1
  const threshold = restDelta * distance

  // How long the spring has to stay inside the threshold before the scan can stop.
  //
  // A single sample near the target proves nothing: an underdamped spring passes straight through
  // it twice per cycle. A full oscillation does prove it, because the envelope decays monotonically
  // — a spring that stayed inside the threshold for one whole cycle can never leave it again.
  // Without oscillation there is no cycle to wait out, so a couple of samples suffice.
  const { omega0, zeta } = resolve(config)
  const restWindow =
    zeta < 1 - CRITICAL_BAND ? ((2 * Math.PI) / (omega0 * Math.sqrt(1 - zeta * zeta))) * SETTLE_WINDOW_CYCLES : SETTLE_STEP_SECONDS * SETTLE_WINDOW_SAMPLES

  let settled = 0
  for (let t = 0; t <= MAX_SETTLE_SECONDS; t += SETTLE_STEP_SECONDS) {
    if (Math.abs(spring(t, config) - to) > threshold) {
      // Still moving at t, so it cannot have been at rest before it.
      settled = t
    } else if (t - settled >= restWindow) {
      // At rest for a whole cycle, and the envelope only shrinks from here.
      break
    }
  }

  return settled + SETTLE_STEP_SECONDS
}
