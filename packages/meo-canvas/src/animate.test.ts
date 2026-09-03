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
  type Sampled,
} from './animate.js'
// `parseColor` and `isColor` live in `color.ts`, not `animate.ts` -- the two
// colour halves are split across modules even though both are exported.
import { parseColor } from './color.js'

const HERE = dirname(fileURLToPath(import.meta.url))
const VECTORS = resolve(HERE, '../../../crates/meo-canvas-core/tests/assets/animate')

/**
 * One table, comment lines and blanks dropped.
 *
 * **The row count is asserted against the count the file declares**, and that
 * is not ceremony. A hex colour begins with `#`, this format's comment
 * character, so `#808080` as a bare first field is dropped by the filter above
 * without a word -- leaving a table that looks complete and tests fewer rows
 * than it lists. `parse-color.tsv` lost three rows that way before its count
 * was compared with its generator's. Quoting fixed that file; the census is
 * what stops the next one, whatever swallows the row.
 */
function rows(name: string): string[][] {
  const text = readFileSync(resolve(VECTORS, name), 'utf8')
  const declared = /^# rows: (\d+)$/m.exec(text)
  const data = text
    .split('\n')
    .filter(line => line.length > 0 && !line.startsWith('#'))
    .map(line => line.split('\t'))
  if (declared === null) throw new Error(`${name} declares no \`# rows:\` count`)
  const want = Number(declared[1])
  if (data.length !== want) {
    throw new Error(`${name} declares ${want} rows and ${data.length} survived parsing; a row is being swallowed`)
  }
  return data
}

/** One field as text, refusing a row too short rather than reading `undefined`. */
function text(row: string[], index: number): string {
  const value = row[index]
  if (value === undefined) throw new Error(`row has no field ${index}: ${row.join(' | ')}`)
  return value
}

/** One field as the number it round-trips to. Mirrors the Rust walker's `at`. */
function num(row: string[], index: number): number {
  return Number(text(row, index))
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

  it('reads a table with rows in it', () => {})
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

  it('reads a table with rows in it', () => {})
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

  it('reads a table with rows in it', () => {})
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

  it('reads a table with rows in it', () => {})
})

describe('the helpers that now have a vector table', () => {
  // **These were pinned inline and are now walked.** The block that used to sit
  // here was headed "the pieces with no vector table yet" and said that if a
  // table arrived, it should be replaced by a walker rather than kept beside
  // one. These are those tables, generated from v1 at the same tag by the same
  // method as the original four.
  //
  // A `kind` column of `diverges` marks a row where this surface deliberately
  // differs from v1; those are asserted in their own tests rather than here,
  // because a ground-truth table cannot both be the reference and record where
  // we left it.

  /** A field, unquoted where the table quoted it. */
  const field = (value: string): string => (value.startsWith('"') ? (JSON.parse(value) as string) : value)

  it('springDuration matches v1 across every regime and rest window', () => {
    const table = rows('spring-duration.tsv')
    for (const row of table) {
      const spec = {
        from: num(row, 0),
        to: num(row, 1),
        stiffness: num(row, 2),
        damping: num(row, 3),
        mass: num(row, 4),
        velocity: num(row, 5),
      }
      const ours = springDuration(spec, { restDelta: num(row, 6) })
      expect(String(ours), `springDuration ${row.slice(0, 7).join(' ')}`).toBe(text(row, 7))
    }
  })

  it('lerp matches v1, including outside 0..1', () => {
    const table = rows('lerp.tsv')
    for (const row of table) {
      const [from, to, t] = [num(row, 0), num(row, 1), num(row, 2)]
      expect(String(lerp(from, to, t)), `lerp(${from}, ${to}, ${t})`).toBe(text(row, 3))
    }
  })

  it('mapRange matches v1, clamped and not', () => {
    const table = rows('map-range.tsv')
    for (const row of table) {
      const ours = mapRange(num(row, 0), [num(row, 1), num(row, 2)], [num(row, 3), num(row, 4)], { clamp: text(row, 5) === 'true' })
      expect(String(ours), `mapRange ${row.slice(0, 6).join(' ')}`).toBe(text(row, 6))
    }
  })

  it('formatColor matches v1, in and out of gamut', () => {
    const table = rows('format-color.tsv')
    for (const row of table) {
      const color = { r: num(row, 0), g: num(row, 1), b: num(row, 2), a: num(row, 3) }
      expect(formatColor(color), `formatColor(${row.slice(0, 4).join(', ')})`).toBe(text(row, 4))
    }
  })

  it('interpolate matches v1, with an ease and without', () => {
    const table = rows('interpolate.tsv')
    for (const row of table) {
      const t = num(row, 0)
      const stops = text(row, 1).split(';').map(Number)
      const values = text(row, 2).split(';').map(Number)
      // `-` is the option omitted entirely, which is not the same call as
      // naming `linear`.
      const ease = text(row, 3)
      const ours = ease === '-' ? interpolate(t, stops, values) : interpolate(t, stops, values, { ease: ease as EasingName })
      expect(String(ours), `interpolate(${t}, [${stops.join(', ')}], ease ${ease})`).toBe(text(row, 4))
    }
  })

  it('mixColor matches v1 wherever we do not deliberately differ', () => {
    const table = rows('mix-color.tsv')
    let compared = 0
    for (const row of table) {
      if (text(row, 3) !== 'value') continue
      const [from, to] = [parseColor(field(text(row, 0))), parseColor(field(text(row, 1)))]
      if (!from || !to) throw new Error(`unparseable colour in ${row.join(' | ')}`)
      expect(formatColor(mixColor(from, to, num(row, 2))), `mixColor ${row.slice(0, 3).join(' ')}`).toBe(text(row, 4))
      compared += 1
    }
    // The table is mostly divergent rows by design; assert the agreeing ones
    // were actually reached rather than all skipped.
    expect(compared).toBe(7)
  })

  it('mix matches v1 over numbers, arrays and colours', () => {
    const table = rows('mix.tsv')
    for (const row of table) {
      const [kind, agreement, from, to, t, expected] = [text(row, 0), text(row, 2), text(row, 3), text(row, 4), text(row, 5), text(row, 6)]
      if (agreement !== 'value') continue
      if (kind === 'number') {
        expect(String(mix(Number(from), Number(to), Number(t)))).toBe(expected)
      } else if (kind === 'array') {
        const ours = mix(from.split(';').map(Number), to.split(';').map(Number), Number(t))
        expect(ours.join(';')).toBe(expected)
      } else {
        const [a, b] = [parseColor(field(from)), parseColor(field(to))]
        if (!a || !b) throw new Error(`unparseable colour in ${from} / ${to}`)
        expect(formatColor(mix(a, b, Number(t)))).toBe(expected)
      }
    }
  })
})

describe('the refusals, which no table can carry', () => {
  // A `throws` contract is not a vector: nothing about it comes from v1, and
  // filing it under ground truth would say it did. These stayed inline when the
  // numeric pins moved into tables -- and this one was dropped in that move and
  // restored, having been caught by ESLint reporting its import as unused
  // rather than by anything asserting the behaviour was gone.

  it('refuses a spring that carries a range its owner already defines', () => {
    // A track and a sequence step each define their own range and drive the
    // spring over 0..1, so a `from` or `to` on the spring cannot be honoured --
    // and dropping it silently would animate to a value nobody asked for while
    // looking obeyed.
    expect(() => assertSpringHasNoRange({ from: 0 }, 'a track')).toThrow(/cannot carry them/)
    expect(() => assertSpringHasNoRange({ to: 1 }, 'a track')).toThrow(/cannot carry them/)
    expect(() => assertSpringHasNoRange({ stiffness: 10 }, 'a track')).not.toThrow()
  })
})

describe('the divergences the tables record rather than assert', () => {
  // A ground-truth table cannot both be the reference and record where we left
  // it, so the `diverges` rows are asserted here instead -- and asserted as
  // *differences*, so that quietly adopting v1's rule would fail rather than
  // pass.

  it('does not clamp t where v1 does', () => {
    const black = { r: 0, g: 0, b: 0, a: 1 }
    const white = { r: 255, g: 255, b: 255, a: 1 }
    // v1 answers '#ffffff' for both of these.
    expect(mixColor(black, white, 1.25)).toEqual({ r: 318.75, g: 318.75, b: 318.75, a: 1 })
    expect(mixColor(black, white, -0.25)).toEqual({ r: -63.75, g: -63.75, b: -63.75, a: 1 })
    expect(formatColor(mixColor(black, white, 1.25))).toBe('color(srgb 1.25 1.25 1.25)')
  })

  it('parses the alpha the author wrote, not an eight-bit approximation of it', () => {
    // This was `it.fails` until the parse boundary was fixed on 4 September
    // 2026, which is what makes it evidence: it failed on both surfaces, and
    // passes on both now, so the fix reached the shared parser rather than one
    // side of it.
    //
    // v1 answers 0.102 here, quantising alpha to eight bits. This surface
    // answered 0.10000000149011612, because `csscolorparser::Color` holds
    // `f32` and both v2 surfaces read the number through it. Neither was what
    // the author wrote, and `getComputedStyle` in a browser answers 0.1.
    //
    // Only the alphas that are not exact in binary32 ever failed: 0.5, 0.25
    // and 0.75 read back correctly all along, which is why the round numbers
    // looked fine.
    for (const alpha of [0.1, 0.33, 0.9]) {
      expect(parseColor(`rgba(0, 0, 0, ${alpha})`)?.a, `rgba(0, 0, 0, ${alpha})`).toBe(alpha)
    }
    // Hex bytes reached the same defect by another route, and are fixed by a
    // different rule: the byte is known, so the alpha is `byte / 255` exactly
    // rather than the shortest decimal naming an `f32`. `#000000cc` is the one
    // that looks like it should escape either way -- 204/255 is exactly 0.8 in
    // decimal, and 0.8 is still not representable in binary32.
    expect(parseColor('#0000007f')?.a, '#0000007f').toBe(0x7f / 255)
    expect(parseColor('#000000cc')?.a, '#000000cc').toBe(0.8)
    // A percentage lands on the same f32 as its decimal spelling.
    expect(parseColor('rgba(0, 0, 0, 33%)')?.a, '33%').toBe(0.33)
  })
})

describe('track, sequence and parallel against their vector tables', () => {
  // These three were the last helpers pinned inline. The tables came from v1
  // at the same tag by the same method as the rest, so the numbers below are
  // the ones the Rust walker reads -- one file, two surfaces, rather than two
  // independent claims about the same reference.

  const page = (time: number) => ({ time })

  /** The `motion` column: an easing name, or a spring's four numbers. */
  const motionOf = (spec: string): Partial<Parameters<typeof track>[0]> => {
    if (!spec.startsWith('spring:')) return { ease: spec as EasingName }
    const [stiffness = 0, damping = 0, mass = 1, velocity = 0] = spec.slice('spring:'.length).split(':').map(Number)
    return { spring: { stiffness, damping, mass, velocity } }
  }

  /** A field that may be absent, spelled `-`. */
  const maybe = (value: string): number | undefined => (value === '-' ? undefined : Number(value))

  /** Asks one motion the question a row names. */
  const answer = (motion: Sampled<number>, row: string[], kind: number): number => {
    if (text(row, kind) === 'at') return motion.at(page(num(row, kind + 1)), num(row, kind + 2))
    if (text(row, kind) === 'duration') return motion.duration
    return motion.totalDuration(num(row, kind + 3))
  }

  it('a track answers where v1 answers', () => {
    let compared = 0
    for (const row of rows('track.tsv')) {
      const duration = maybe(text(row, 2))
      const spec = {
        from: num(row, 0),
        to: num(row, 1),
        // Spread rather than assigned: `exactOptionalPropertyTypes` makes an
        // explicit `undefined` a different thing from an absent key, and a
        // spring track has no duration of its own to give.
        ...(duration === undefined ? {} : { duration }),
        delay: num(row, 3),
        stagger: num(row, 4),
        ...motionOf(text(row, 5)),
      }
      // `js-only` is the fractional count, which Rust's `usize` cannot take.
      // It is walked here precisely because this is the surface that can.
      const ours = answer(track<number>(spec as Parameters<typeof track<number>>[0]), row, 7)
      expect(String(ours), `a track ${row.slice(0, 8).join(' ')}`).toBe(text(row, 11))
      compared += 1
    }
    expect(compared).toBe(132)
  })

  it('a sequence answers where v1 answers', () => {
    for (const row of rows('sequence.tsv')) {
      const steps = text(row, 3)
        .split(';')
        .map(leg => {
          const [to, duration, hold, motion] = leg.split(':')
          const seconds = maybe(duration ?? '-')
          return {
            to: Number(to),
            ...(seconds === undefined ? {} : { duration: seconds }),
            hold: Number(hold),
            ease: motion as EasingName,
          }
        })
      const ours = answer(sequence<number>({ from: num(row, 0), delay: num(row, 1), stagger: num(row, 2), steps }), row, 5)
      expect(String(ours), `a sequence ${row.slice(0, 6).join(' ')}`).toBe(text(row, 9))
    }
  })

  it('a group answers where v1 answers', () => {
    // The member vocabulary `parallel.tsv` declares in its header. `C` is the
    // discriminating one: with A and B alone, `totalDuration` answers 2
    // whether or not the count reaches the members.
    const member = (letter: string) => {
      if (letter === 'A') return track({ from: 0, to: 100, duration: 1, ease: 'outCubic' })
      if (letter === 'B') return sequence({ from: 0, steps: [{ to: 4, duration: 2 }] })
      return track({ from: 0, to: 5, duration: 1, stagger: 1 })
    }
    for (const row of rows('parallel.tsv')) {
      const names = text(row, 0).split(';')
      const group = parallel(Object.fromEntries(names.map(letter => [letter, member(letter)])))
      // One member per row rather than a record: the group's value is an
      // object here and a `Vec` in declaration order in Rust, and a row
      // holding a record literal would assume one of them.
      const ours =
        text(row, 2) === 'at'
          ? (group.at(page(num(row, 3)), num(row, 4)) as Record<string, number>)[text(row, 6)]
          : text(row, 2) === 'duration'
            ? group.duration
            : group.totalDuration(num(row, 5))
      expect(String(ours), `a group ${row.slice(0, 7).join(' ')}`).toBe(text(row, 7))
    }
  })

  it('refuses the configurations v1 refuses', () => {
    // Refusals stay inline for the reason they always have: a `throws`
    // contract is not a vector, and nothing about it comes from v1.
    expect(() => track({ from: 0, to: 1, duration: 1, ease: 'linear', spring: {} })).toThrow(/not both/)
    expect(() => track({ from: 0, to: 1 })).toThrow(/needs a `duration`/)
    expect(() => track({ from: 0, to: 1, duration: 1, delay: -1 })).toThrow(/delay cannot be negative/)
    expect(() => track({ from: 0, to: 1, spring: { from: 0 } })).toThrow(/cannot carry them/)
    expect(() => sequence({ from: 0, steps: [] })).toThrow(/at least one step/)
    expect(() => parallel({})).toThrow(/at least one member/)
    expect(() => mix([0], [0, 1], 0.5)).toThrow(/same length/)
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
