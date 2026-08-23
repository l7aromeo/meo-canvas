// Whether Chrome fits a dotted border to its side, and what it does at a corner.
//
// Every question settled for dashed is open for dotted, and none of the answers
// transfer by analogy: a dot is a round cap on a zero-length segment where a
// dash is a stroked run, and the two go through different code before they
// reach the same path. So this measures the same five things again rather than
// assuming the dashed reading holds --
//
//   1. the period: is a dot `w` wide with a gap of `w`, at every width
//   2. per-side fitting: does a side end flush at BOTH ends, and where does the
//      remainder go -- into the gaps, or into the dots
//   3. the corner: where the phase is anchored, read against the CSS Backgrounds
//      3 section 4.4 division between the two edges
//   4. the radius threshold: whether `radius > min(w_a, w_b)` switches a dotted
//      border from per-side fitting to one run round the closed path, as it does
//      for dashed
//   5. what an arc carries below the threshold -- solid ink, or dots
//
// The instrument is `borders.mjs`'s, deliberately: same box, same threshold,
// same sixteen samples per pixel on a curve, same `Math.floor` on a sample point
// and `Math.round` on a browser-reported origin. A second instrument measuring
// the same family would make the two tables incomparable, which is the whole
// value of having both.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { read, pixel } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/dotted-rhythm.tsv')

/**
 * The boxes every straight-edge case is drawn in.
 *
 * **A sweep of edge lengths rather than one box, because the rule being
 * measured is a rounding rule and one box lands on one point of it.** An open
 * run of `n` dots has `n - 1` gaps, so it covers `(2n - 1)w` -- which makes the
 * exact count `(edge / w + 1) / 2`, and 240 puts that on an exact half at every
 * width here (120.5, 60.5, 40.5, 30.5, 15.5). **A tie is precisely where a
 * rounding rule is undetermined**, so a table built on 240 alone reports the
 * one case that cannot distinguish rounding up from rounding down -- and it
 * does not even answer consistently, which is the tell. The other lengths are
 * chosen to land away from a half at as many widths as possible.
 */
const BOXES = [131, 137, 149, 163, 179, 211, 240].map(width => ({ width, height: 48 }))

/** The box the closed-path walks use. */
const LOOP_BOX = { width: 137, height: 120 }

/** The widths measured. */
const WIDTHS = [1, 2, 3, 4, 8]

/**
 * Ink is a red channel under this.
 *
 * The same 128 `border-rhythm.tsv` reads at, and it matters more here: a dot is
 * a circle, so its edge pixels are partial coverage over a larger share of the
 * mark than a dash's are. A run measured at one threshold and compared against a
 * table measured at another would differ by a pixel per dot for no reason.
 */
const THRESHOLD = 128

/** Runs of ink along one row, as `on:N off:N`, from `x0` to `x1` inclusive. */
function runs(shot, y, x0, x1, offsets) {
  const out = []
  for (let x = x0; x <= x1; x += 1) {
    const ink = pixel(shot, x, y)[0] < THRESHOLD
    const last = out.at(-1)
    if (last && last.ink === ink) last.n += 1
    else out.push({ ink, n: 1, at: x - x0 })
  }
  return out.map(run => `${run.ink ? 'on' : 'off'}:${run.n}${offsets ? `@${run.at}` : ''}`).join(' ')
}

/**
 * The header every reader of this table meets first.
 *
 * Three of these lines are constraints rather than description, and each cost a
 * wrong reading before it was written down.
 */
const HEADER = [
  '# Chrome, through `just conformance`. The rhythm of a dotted border.',
  '#',
  '# A dot is `w` wide and the gap between two dots is `w`, at every width here.',
  '# A side carries an OPEN run: `n` dots and `n - 1` gaps, covering `(2n - 1)w`,',
  '# flush at both ends. So the exact count is `(edge / w + 1) / 2` and Chrome',
  '# takes it to the NEAREST integer -- 0 mismatches in 30 rows, against 10 for',
  '# ceiling and 14 for floor.',
  '#',
  '# **Do not build a fixture on a 240-wide box.** `(240 / w + 1) / 2` is an',
  '# exact half at every width measured here -- 120.5, 60.5, 40.5, 30.5, 15.5 --',
  '# and a tie is precisely where a rounding rule is undetermined. The five tie',
  '# rows do not even agree with each other: 120 at width 1 and 61 at width 2,',
  '# down then up. A rule read off a tie is a coin toss recorded as a',
  '# measurement. 131, 137, 149, 163, 179 and 211 all leave a remainder.',
  '#',
  '# **A `dotted-loop` row reports RAW marks and has to be joined twice before',
  '# it is a dot count.** Once for the wrap, because a closed walk begins',
  '# partway through a mark and ends in the same one; and once for SPLIT marks,',
  '# because a curved band crossing a sample row can chop one dot into pieces --',
  '# at radius 8 one dot reads as three, separated by two gaps of 0.9 where',
  '# every real gap on that walk is 3.1 or more. `dots == gaps` does NOT catch',
  '# this: a split inserts a gap as well as a mark, so the invariant holds while',
  '# the count is two too high. Join on the gap length, which separates cleanly.',
  '#',
  '# **An arc below radius 8 cannot say anything about the fitting branch.** A',
  '# quarter of the centreline is `(pi/2)(r - w/2)`, which is 4.7 at radius 5 and',
  '# 6.3 at radius 6 -- shorter than one period of 8, so a single mark is all it',
  '# can ever show. Dots appear on the arc from radius 8 because that is the',
  '# first quarter longer than a period. Arithmetic about length, not a branch.',
  '#',
  "# `walk-length` is the walk's own path, not the geometric centreline: the",
  '# extents are pixel indices, so it runs four short round a loop.',
  '#',
  '# name\twidth\ty or radius\twindow\tthreshold\treading',
]

const browser = await open()
const rows = [...HEADER]
try {
  for (const BOX of BOXES)
    for (const width of WIDTHS) {
      const geometry = await browser.page.evaluate(
        ({ box, width }) => {
          document.body.innerHTML = ''
          const element = document.createElement('div')
          element.style.cssText = `position:absolute;left:0;top:0;width:${box.width - 2 * width}px;height:${box.height - 2 * width}px;border:${width}px dotted #000000;background:#ffffff;`
          document.body.append(element)
          const rect = element.getBoundingClientRect()
          return { left: rect.left, top: rect.top }
        },
        { box: BOX, width },
      )
      await settle(browser.page)
      await browser.page.setViewportSize(BOX)
      const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))

      // Floor for a sample point, round for a value the browser reported. The two
      // are the same expression and different quantities -- see AGENTS.md.
      const y = Math.floor(geometry.top + width / 2)
      const from = Math.round(geometry.left)

      // Forty pixels clear of each corner: section 4.4 divides a corner between
      // its two edges, and a run that includes one measures the join rather than
      // the rhythm.
      rows.push(
        [`dotted-band-${BOX.width}`, width, y, `${from + 40}-${from + BOX.width - 40}`, THRESHOLD, runs(shot, y, from + 40, from + BOX.width - 40, false)].join(
          '\t',
        ),
      )

      // The whole top edge, offsets kept, so the two ends can be read for
      // flushness and the remainder can be located.
      rows.push(
        [`dotted-top-edge-${BOX.width}`, width, y, `${from}-${from + BOX.width - 1}`, THRESHOLD, runs(shot, y, from, from + BOX.width - 1, true)].join('\t'),
      )

      // Where the first and last ink sit on the edge. `first@0` and
      // `last@${width - 1}` from the far end is flush at both; anything else says
      // where the slack went.
      let first = null
      let last = null
      for (let x = from; x <= from + BOX.width - 1; x += 1) {
        if (pixel(shot, x, y)[0] < THRESHOLD) {
          first ??= x - from
          last = x - from
        }
      }
      rows.push(
        [
          `dotted-span-${BOX.width}`,
          width,
          y,
          `${from}-${from + BOX.width - 1}`,
          THRESHOLD,
          `first@${first} last@${last} edge=${BOX.width} trailing=${BOX.width - 1 - last}`,
        ].join('\t'),
      )
    }
  // ------------------------------------------------------------------
  // The radius threshold, and what an arc carries either side of it.
  //
  // For dashed the branch is `radius > min(w_a, w_b)`: at or below it the inner
  // radius `r - w` is non-positive, the inner corner is square, and each side is
  // fitted on its own; above it one pattern runs round the closed path. Nothing
  // says dotted takes the same branch, so it is measured rather than assumed.
  //
  // The discriminator is the one the dashed work settled on: a side fitted on
  // its own is FLUSH at both tangents, and a closed run is not.
  // ------------------------------------------------------------------
  for (const radius of [0, 1, 2, 3, 4, 5, 6, 8, 12, 24]) {
    const width = 4
    const box = { width: 240, height: 48 }
    const geometry = await browser.page.evaluate(
      ({ box, width, radius }) => {
        document.body.innerHTML = ''
        const element = document.createElement('div')
        element.style.cssText = `position:absolute;left:0;top:0;width:${box.width - 2 * width}px;height:${box.height - 2 * width}px;border:${width}px dotted #000000;border-radius:${radius}px;background:#ffffff;`
        document.body.append(element)
        const rect = element.getBoundingClientRect()
        return { left: rect.left, top: rect.top }
      },
      { box, width, radius },
    )
    await settle(browser.page)
    await browser.page.setViewportSize(box)
    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...box } }))
    const y = Math.floor(geometry.top + width / 2)
    const from = Math.round(geometry.left)

    // The straight portion runs between the two tangents.
    const straightFrom = from + radius
    const straightTo = from + box.width - 1 - radius
    let first = null
    let last = null
    for (let x = straightFrom; x <= straightTo; x += 1) {
      if (pixel(shot, x, y)[0] < THRESHOLD) {
        first ??= x - straightFrom
        last = x - straightFrom
      }
    }
    const length = straightTo - straightFrom + 1
    rows.push(
      [
        `dotted-radius-${radius}`,
        width,
        y,
        `${straightFrom}-${straightTo}`,
        THRESHOLD,
        `straight@${radius} straight-length=${length} first@${first} last@${last} trailing=${length - 1 - last} ${runs(shot, y, from, from + box.width - 1, true)}`,
      ].join('\t'),
    )

    // The top-left arc, walked along the centreline of the band rather than
    // across it: a band row crosses a curve obliquely and can only ever show one
    // short mark. Sixteen samples per pixel, because four cannot tell two radii
    // apart on a curve.
    if (radius > 0) {
      const r = radius - width / 2
      const centre = [from + radius, Math.round(geometry.top) + radius]
      const quarter = (Math.PI / 2) * r
      const steps = Math.max(1, Math.round(quarter * 16))
      const marks = []
      let ink = null
      let start = 0
      for (let step = 0; step <= steps; step += 1) {
        const angle = Math.PI + (step / steps) * (Math.PI / 2)
        const point = [centre[0] + r * Math.cos(angle), centre[1] + r * Math.sin(angle)]
        const here = pixel(shot, Math.floor(point[0]), Math.floor(point[1]))[0] < THRESHOLD
        if (ink === null) {
          ink = here
          continue
        }
        if (here !== ink) {
          marks.push(`${ink ? 'on' : 'off'}:${(((step - start) / steps) * quarter).toFixed(1)}`)
          ink = here
          start = step
        }
      }
      marks.push(`${ink ? 'on' : 'off'}:${(((steps - start) / steps) * quarter).toFixed(1)}`)
      rows.push(
        [`dotted-arc-${radius}-top-left`, width, r, 'walked along the centreline', THRESHOLD, `quarter=${quarter.toFixed(1)} ${marks.join(' ')}`].join('\t'),
      )
    }
  }

  // ------------------------------------------------------------------
  // The closed-path walk, which is what actually separates the two branches.
  //
  // **The straight-portion reading above cannot do it**, and the rows show why:
  // at radius 8 the band row at `y = 2` carries ink from `x = 5` where the
  // tangent is at `x = 8`, so a window starting at the tangent reads a mark that
  // belongs to the arc and reports flushness that is not fitting. The dashed
  // work hit exactly this and settled it by counting marks round the whole loop
  // instead, where the two hypotheses predict different totals.
  // ------------------------------------------------------------------
  for (const radius of [5, 6, 8, 12, 24]) {
    const width = 4
    const geometry = await browser.page.evaluate(
      ({ box, width, radius }) => {
        document.body.innerHTML = ''
        const element = document.createElement('div')
        element.style.cssText = `position:absolute;left:0;top:0;width:${box.width - 2 * width}px;height:${box.height - 2 * width}px;border:${width}px dotted #000000;border-radius:${radius}px;background:#ffffff;`
        document.body.append(element)
        const rect = element.getBoundingClientRect()
        return { left: rect.left, top: rect.top }
      },
      { box: LOOP_BOX, width, radius },
    )
    await settle(browser.page)
    await browser.page.setViewportSize(LOOP_BOX)
    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...LOOP_BOX } }))

    const inset = width / 2
    const r = radius - inset
    const left = Math.round(geometry.left) + inset
    const top = Math.round(geometry.top) + inset
    const right = Math.round(geometry.left) + LOOP_BOX.width - 1 - inset
    const bottom = Math.round(geometry.top) + LOOP_BOX.height - 1 - inset
    const straight = (x0, y0, x1, y1) => ({ length: Math.hypot(x1 - x0, y1 - y0), at: u => [x0 + (x1 - x0) * u, y0 + (y1 - y0) * u] })
    const arc = (cx, cy, from0) => ({
      length: (Math.PI / 2) * r,
      at: u => {
        const a = from0 + u * (Math.PI / 2)
        return [cx + r * Math.cos(a), cy + r * Math.sin(a)]
      },
    })
    const path = [
      straight(left + r, top, right - r, top),
      arc(right - r, top + r, -Math.PI / 2),
      straight(right, top + r, right, bottom - r),
      arc(right - r, bottom - r, 0),
      straight(right - r, bottom, left + r, bottom),
      arc(left + r, bottom - r, Math.PI / 2),
      straight(left, bottom - r, left, top + r),
      arc(left + r, top + r, Math.PI),
    ]
    // The length this walk covers, which is NOT the geometric centreline: the
    // extents are pixel indices, so it runs one short per axis and four short
    // round the loop. Named for what it is -- see AGENTS.md on a field named for
    // a geometric quantity holding an instrument's internal one.
    const walkLength = path.reduce((sum, part) => sum + part.length, 0)
    const total = Math.round(walkLength * 16)
    const marks = []
    let ink = null
    let start = 0
    for (let step = 0; step <= total; step += 1) {
      let rest = (step / total) * walkLength
      let point = null
      for (const part of path) {
        if (rest <= part.length || part === path[path.length - 1]) {
          point = part.at(Math.min(rest / part.length, 1))
          break
        }
        rest -= part.length
      }
      const here = pixel(shot, Math.floor(point[0]), Math.floor(point[1]))[0] < THRESHOLD
      if (ink === null) {
        ink = here
        continue
      }
      if (here !== ink) {
        marks.push(`${ink ? 'on' : 'off'}:${(((step - start) / total) * walkLength).toFixed(1)}`)
        ink = here
        start = step
      }
    }
    marks.push(`${ink ? 'on' : 'off'}:${(((total - start) / total) * walkLength).toFixed(1)}`)
    // Reported raw. A closed walk begins partway through a mark and ends in the
    // same one, so the first and last runs are one mark split by the start point
    // whenever both are ink -- join them before counting, and check the join by
    // `gaps == marks`, which only holds on a closed loop after joining.
    const dots = marks.filter(mark => mark.startsWith('on')).length
    rows.push(
      [
        `dotted-loop-r${radius}`,
        width,
        r,
        'walked clockwise from the top-left tangent',
        THRESHOLD,
        `box=${LOOP_BOX.width}x${LOOP_BOX.height} walk-length=${walkLength.toFixed(1)} raw-marks=${dots} ${marks.join(' ')}`,
      ].join('\t'),
    )
  }

  await writeFile(DESTINATION, table(rows))
} finally {
  await browser.close()
}
console.log(`dotted rhythm: ${rows.length} rows`)
