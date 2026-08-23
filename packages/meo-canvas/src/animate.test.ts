import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import {
  assertSpringHasNoRange,
  cubicBezier,
  ease,
  EASING_NAMES,
  formatColor,
  interpolate,
  lerp,
  mapRange,
  mix,
  mixColor,
  parallel,
  sequence,
  spring,
  springDuration,
  steps,
  track,
  type EasingName,
} from './animate.js'

const HERE = dirname(fileURLToPath(import.meta.url))
const VECTORS = resolve(HERE, '../../../crates/meo-canvas-core/tests/assets/animate')

/** One table, comment lines and blanks dropped. */
function rows(name: string): string[][] {
  const text = readFileSync(resolve(VECTORS, name), 'utf8')
  return text
    .split('\n')
    .filter(line => line.length > 0 && !line.startsWith('#'))
    .map(line => line.split('\t'))
}

describe('the easing catalogue against v1', () => {
  const table = rows('easing.tsv')

  // **No epsilon.** The vectors are printed at JavaScript's default precision,
  // which is the shortest string that round-trips an `f64`, and this runs on
  // the engine that produced them. If a tolerance is ever needed here, that is
  // a finding about one of the two implementations rather than a reason to add
  // one — bring the row and the differing bit.
  it('reproduces every row exactly', () => {
    const wrong: string[] = []
    for (const [name, t, expected] of table) {
      const got = ease(name as EasingName, Number(t))
      if (got !== Number(expected)) {
        wrong.push(`${name} at t=${t}: v1 ${expected}, ours ${got}`)
      }
    }
    expect(wrong).toEqual([])
  })

  // The pinned-list rule applied to a vector table: **a curve nobody
  // implemented must not pass by not being asked about**, and a curve the table
  // has stopped covering must not sit here untested. Both directions.
  it('covers the same curves the table does, in both directions', () => {
    const named = [...new Set(table.map(([name]) => name))].sort()
    expect(named).toEqual([...EASING_NAMES].sort())
  })

  it('reads a table with rows in it', () => {
    expect(table.length).toBe(403)
  })
})

describe('cubic-bezier against v1', () => {
  const table = rows('bezier.tsv')

  it('reproduces every row exactly', () => {
    const wrong: string[] = []
    for (const [x1, y1, x2, y2, t, expected] of table) {
      const got = cubicBezier(Number(x1), Number(y1), Number(x2), Number(y2))(Number(t))
      if (got !== Number(expected)) {
        wrong.push(`cubic-bezier(${x1},${y1},${x2},${y2}) at t=${t}: v1 ${expected}, ours ${got}`)
      }
    }
    expect(wrong).toEqual([])
  })

  it('reads a table with rows in it', () => {
    expect(table.length).toBe(78)
  })
})

describe('steps against v1', () => {
  const table = rows('steps.tsv')

  // The boundaries are sampled a billionth either side, because floor and
  // round agree everywhere else — so a `round` implementation passes every
  // other row in this table.
  it('reproduces every row exactly', () => {
    const wrong: string[] = []
    for (const [count, t, expected] of table) {
      const got = steps(Number(count))(Number(t))
      if (got !== Number(expected)) {
        wrong.push(`steps(${count}) at t=${t}: v1 ${expected}, ours ${got}`)
      }
    }
    expect(wrong).toEqual([])
  })

  it('refuses a count that is not a whole number at least one', () => {
    expect(() => steps(0)).toThrow(/at least 1 step/)
    expect(() => steps(1.5)).toThrow(/at least 1 step/)
  })

  it('reads a table with rows in it', () => {
    expect(table.length).toBe(54)
  })
})

describe('the spring against v1', () => {
  const table = rows('spring.tsv')

  // **No tolerance here either, and that is a real claim rather than an
  // oversight.** The Rust half needs one unit of last-place slack on `exp`,
  // because a transcendental is not required to be correctly rounded and its
  // libm differs from V8's. This side runs on the engine that produced the
  // vectors, so the same slack would be hiding something rather than allowing
  // for it.
  it('reproduces every row exactly', () => {
    const wrong: string[] = []
    for (const [from, to, stiffness, damping, mass, velocity, t, expected] of table) {
      const got = spring(Number(t), {
        from: Number(from),
        to: Number(to),
        stiffness: Number(stiffness),
        damping: Number(damping),
        mass: Number(mass),
        velocity: Number(velocity),
      })
      if (got !== Number(expected)) {
        wrong.push(`spring k=${stiffness} c=${damping} m=${mass} at t=${t}: v1 ${expected}, ours ${got}`)
      }
    }
    expect(wrong).toEqual([])
  })

  // The regime counter Agent One's own run used to retract a coverage claim:
  // it reported that no natural configuration lands in the critical band, and
  // `stiffness: 100, damping: 20` is a ratio of exactly 1.
  it('covers all three damping regimes', () => {
    const regimes = new Set<string>()
    for (const [, , stiffness, damping, mass] of table) {
      const zeta = Number(damping) / (2 * Math.sqrt(Number(stiffness) * Number(mass)))
      regimes.add(Math.abs(zeta - 1) < 1e-4 ? 'critical' : zeta < 1 ? 'under' : 'over')
    }
    expect([...regimes].sort()).toEqual(['critical', 'over', 'under'])
  })

  it('refuses a configuration that is not a spring', () => {
    expect(() => spring(0.5, { stiffness: 0 })).toThrow(/stiffness/)
    expect(() => spring(0.5, { damping: -1 })).toThrow(/damping/)
    expect(() => spring(0.5, { mass: 0 })).toThrow(/mass/)
  })

  it('reads a table with rows in it', () => {
    expect(table.length).toBe(77)
  })
})

describe('the pieces with no vector table yet', () => {
  // **These values were taken by RUNNING v1, not by reading it** — the same
  // method that produced the checked-in tables, at the same tag `v9.0.2` /
  // `890eed2`. They are pinned here rather than in a `.tsv` because nobody has
  // generated one for these functions; if one arrives, this block should be
  // replaced by a walker rather than kept alongside it.
  it('springDuration matches v1', () => {
    expect(springDuration()).toBe(0.5666666666666659)
    expect(springDuration({ stiffness: 100, damping: 20 })).toBe(0.7458333333333319)
    expect(springDuration({ stiffness: 300, damping: 5, mass: 2 })).toBe(4.170833333333417)
    expect(springDuration({}, { restDelta: 0.05 })).toBe(0.36249999999999993)
  })

  // The three configurations above are one per damping regime, deliberately:
  // the default is underdamped, `k=100 c=20` is exactly critical, and the
  // rest-window branch differs between them.
  it('springDuration covers both rest-window branches', () => {
    const critical = springDuration({ stiffness: 100, damping: 20 })
    const under = springDuration()
    expect(critical).not.toBe(under)
  })

  it('lerp does not clamp, because the overshoot curves need it', () => {
    expect(lerp(0, 100, 0.25)).toBe(25)
    expect(lerp(0, 100, 1.25)).toBe(125)
    expect(lerp(0, 100, -0.25)).toBe(-25)
  })

  it('mapRange matches v1, including the empty input range', () => {
    expect(mapRange(50, [0, 100], [0, 1])).toBe(0.5)
    expect(mapRange(150, [0, 100], [0, 1], { clamp: true })).toBe(1)
    // An empty input range has no position to report; v1 answers with the
    // start of the output range rather than the NaN the division would give.
    expect(mapRange(5, [0, 0], [7, 9])).toBe(7)
  })

  it('refuses a spring that carries a range its owner already defines', () => {
    expect(() => assertSpringHasNoRange({ from: 0 }, 'a track')).toThrow(/cannot carry them/)
    expect(() => assertSpringHasNoRange({ to: 1 }, 'a track')).toThrow(/cannot carry them/)
    expect(() => assertSpringHasNoRange({ stiffness: 10 }, 'a track')).not.toThrow()
  })
})

describe('track, sequence and parallel against v1', () => {
  // Taken by running v1 at `v9.0.2` / `890eed2`, as above. `sequence at 3.2`
  // is inside the trailing hold, and `parallel.duration` is the longer member
  // — two boundary rules that a mid-motion sample would not reach.
  const page = (time: number) => ({ time })

  it('a track samples where v1 samples', () => {
    const eased = track({ from: 0, to: 100, duration: 1, ease: 'outCubic' })
    expect(eased.at(page(0.25))).toBe(57.8125)
    expect(eased.duration).toBe(1)
    expect(eased.totalDuration(3)).toBe(1)
  })

  it('a spring track takes its duration from the physics', () => {
    const sprung = track({ from: 0, to: 10, spring: { stiffness: 100, damping: 20 } })
    expect(sprung.at(page(0.3))).toBe(8.008517265285441)
    expect(sprung.duration).toBe(0.7458333333333319)
  })

  it('a stagger offsets by index and lengthens the total', () => {
    const staggered = track({ from: 0, to: 5, duration: 1, delay: 0.5, stagger: 0.25 })
    expect(staggered.at(page(1.0), 1)).toBe(1.25)
    expect(staggered.totalDuration(3)).toBe(2)
  })

  it('a sequence holds at a step and excludes the trailing hold from its length', () => {
    const steps = sequence({
      from: 0,
      steps: [
        { to: 10, duration: 1 },
        { to: 30, duration: 2, hold: 0.5 },
        { to: 0, duration: 1 },
      ],
    })
    expect(steps.at(page(0.5))).toBe(5)
    expect(steps.at(page(1.5))).toBe(15)
    // Inside the hold after step two: the value rests at `to` rather than
    // starting the next leg early.
    expect(steps.at(page(3.2))).toBe(30)
    expect(steps.duration).toBe(4.5)
  })

  it('a parallel group samples every member and is as long as the longest', () => {
    const group = parallel({
      x: track({ from: 0, to: 100, duration: 1, ease: 'outCubic' }),
      y: sequence({ from: 0, steps: [{ to: 4, duration: 2 }] }),
    })
    expect(group.at(page(0.25))).toEqual({ x: 57.8125, y: 0.5 })
    expect(group.duration).toBe(2)
  })

  it('refuses the configurations v1 refuses', () => {
    expect(() => track({ from: 0, to: 1, duration: 1, ease: 'linear', spring: {} })).toThrow(/not both/)
    expect(() => track({ from: 0, to: 1 })).toThrow(/needs a `duration`/)
    expect(() => track({ from: 0, to: 1, duration: 1, delay: -1 })).toThrow(/delay cannot be negative/)
    expect(() => track({ from: 0, to: 1, spring: { from: 0 } })).toThrow(/cannot carry them/)
    expect(() => sequence({ from: 0, steps: [] })).toThrow(/at least one step/)
    expect(() => parallel({})).toThrow(/at least one member/)
  })

  it('mix and interpolate handle numbers and arrays', () => {
    expect(mix(0, 10, 0.5)).toBe(5)
    expect(mix([0, 10], [10, 20], 0.5)).toEqual([5, 15])
    expect(() => mix([0], [0, 1], 0.5)).toThrow(/same length/)
    expect(interpolate(0.25, [0, 0.5, 1], [0, 100, 0])).toBe(50)
    // Holds outside the declared range rather than extrapolating.
    expect(interpolate(-1, [0, 1], [3, 9])).toBe(3)
    expect(interpolate(2, [0, 1], [3, 9])).toBe(9)
    expect(() => interpolate(0, [0], [1])).toThrow(/at least two stops/)
    expect(() => interpolate(0, [1, 0], [1, 2])).toThrow(/ascending order/)
  })
})

describe('colour', () => {
  // `formatColor` answers are v1's, taken by running it at `v9.0.2` /
  // `890eed2`. The out-of-gamut rows matter most: hex cannot hold a channel
  // above 255 or below 0, and clamping would substitute a duller colour
  // without saying so.
  it('formatColor matches v1', () => {
    expect(formatColor({ r: 128, g: 128, b: 128, a: 1 })).toBe('#808080')
    expect(formatColor({ r: 255, g: 0, b: 0, a: 0.5 })).toBe('rgba(255, 0, 0, 0.5)')
    expect(formatColor({ r: 300, g: -20, b: 128, a: 1 })).toBe('color(srgb 1.176471 -0.078431 0.501961)')
    expect(formatColor({ r: 300, g: -20, b: 128, a: 0.25 })).toBe('color(srgb 1.176471 -0.078431 0.501961 / 0.25)')
    expect(formatColor({ r: 127.5, g: 0.4, b: 254.6, a: 1 })).toBe('#8000ff')
  })

  it('mixColor reaches v1 through formatColor', () => {
    const black = { r: 0, g: 0, b: 0, a: 1 }
    const white = { r: 255, g: 255, b: 255, a: 1 }
    // v1's `mixColor('#000000', '#ffffff', 0.5)` is '#808080'. Ours returns
    // the Rgba and `formatColor` writes the same string.
    expect(formatColor(mixColor(black, white, 0.5))).toBe('#808080')
  })

  // **The deliberate divergence.** v1 clamps `t` here; we do not, because CSS
  // interpolates colour through an overshooting curve and clamps where the
  // colour becomes paint. v1 would give '#ffffff' for both of these.
  it('does not clamp t, where v1 does', () => {
    const black = { r: 0, g: 0, b: 0, a: 1 }
    const white = { r: 255, g: 255, b: 255, a: 1 }
    expect(mixColor(black, white, 1.25)).toEqual({ r: 318.75, g: 318.75, b: 318.75, a: 1 })
    expect(mixColor(black, white, -0.25)).toEqual({ r: -63.75, g: -63.75, b: -63.75, a: 1 })
    // And the overshoot survives to the string rather than being flattened.
    expect(formatColor(mixColor(black, white, 1.25))).toBe('color(srgb 1.25 1.25 1.25)')
  })

  it('mix and a track carry a colour the same way they carry a number', () => {
    const red = { r: 255, g: 0, b: 0, a: 1 }
    const blue = { r: 0, g: 0, b: 255, a: 1 }
    expect(mix(red, blue, 0.5)).toEqual({ r: 127.5, g: 0, b: 127.5, a: 1 })
    const fade = track({ from: red, to: blue, duration: 1 })
    expect(fade.at({ time: 0.5 })).toEqual({ r: 127.5, g: 0, b: 127.5, a: 1 })
  })
})
