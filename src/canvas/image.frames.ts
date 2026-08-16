/** How an animated source behaves once it reaches its last frame. */
export interface FrameAtTimeOptions {
  /** Restart from the beginning. `false` holds the last frame instead. @default true */
  loop?: boolean
}

/**
 * Delay substituted for a frame that declares none.
 *
 * Encoders do write zero, and a zero-length frame has no duration to advance through — a whole
 * source of them would divide by zero here and never move. Browsers substitute a default for the
 * same reason; 100ms is the one they settled on.
 */
const FALLBACK_DELAY_MS = 100

const MILLISECONDS = 1000

/**
 * Which frame of an animated source is showing at a given moment.
 *
 * Driven by the source's own per-frame delays rather than by a frame number, because those are two
 * different clocks: a GIF at 10fps drawn into a 24fps render advances every other page or so, and
 * anything that maps page index straight to frame index plays it at the wrong speed. Handing the
 * delays back to the caller to do this arithmetic would be handing them the same mistake.
 * @param delays Per-frame durations in milliseconds, as the decoded image reports them.
 * @param seconds Elapsed time, which for a paged render is the page's own `time`.
 */
export function frameAtTime(delays: readonly number[], seconds: number, options: FrameAtTimeOptions = {}): number {
  const { loop = true } = options

  if (delays.length <= 1) return 0
  if (seconds <= 0) return 0

  const timings = delays.map(delay => (delay > 0 ? delay : FALLBACK_DELAY_MS))
  const total = timings.reduce((sum, delay) => sum + delay, 0)

  let elapsed = seconds * MILLISECONDS

  if (elapsed >= total) {
    if (!loop) return timings.length - 1
    // Modulo rather than a running subtraction: a long render should not walk the whole timeline.
    elapsed %= total
  }

  let cursor = 0
  for (let index = 0; index < timings.length; index++) {
    cursor += timings[index]
    if (elapsed < cursor) return index
  }

  // Only reachable through floating-point drift at the very end of the last frame.
  return timings.length - 1
}
