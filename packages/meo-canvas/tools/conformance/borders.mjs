// The rhythm Chrome draws a dashed or a dotted border in.
//
// CSS does not specify one — it says a dashed border is dashes and leaves the
// lengths to the implementation — so this is a **behaviour** measurement
// rather than a conformance one, and the browser is the baseline for
// behaviour. Our current rhythm is v1's: `max(2, w * 1.5)` on and `max(1, w)`
// off for dashed, and a zero-length dash with round caps and a gap of twice
// the width for dotted. Whether Chrome scales either with the border width is
// the question, and it is not derivable from anything: it has to be read off a
// painted edge.
//
// Read along the TOP edge, away from both corners, because CSS Backgrounds 3
// §4.4 divides a corner between its two edges and a run that includes one is
// measuring the join rather than the rhythm.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/border-rhythm.tsv')

/** The box every case is drawn in, wide enough to hold many periods. */
const BOX = { width: 240, height: 48 }

/**
 * The widths to ask about.
 *
 * Two doublings, so a rhythm that scales shows -- plus **3**, because the
 * measured ratios differ between 2 and 4 (`3w` on, `2w` off at and below one;
 * `2w` on, `w` off at and above the other) and the step has to sit somewhere in
 * between. Without this row the step is a guess.
 */
const WIDTHS = [1, 2, 3, 4, 8]

/** The two styles that have a rhythm at all. */
const STYLES = ['dashed', 'dotted']

/**
 * How far from each corner the run is read.
 *
 * Forty pixels, which is five times the widest border here: §4.4 gives each
 * corner a wedge of its two edges, and a run that starts inside one measures
 * the join rather than the rhythm.
 */
const MARGIN = 40

/** A pixel counts as ink below this in the red channel. */
const THRESHOLD = 128

const browser = await open()
try {
  const rows = []
  await browser.page.setViewportSize(BOX)

  for (const style of STYLES) {
    for (const width of WIDTHS) {
      const geometry = await browser.page.evaluate(
        ({ box, style, width }) => {
          document.body.innerHTML = ''
          const element = document.createElement('div')
          element.style.cssText = `position:absolute;left:0;top:0;width:${box.width - 2 * width}px;height:${box.height - 2 * width}px;border:${width}px ${style} #000000;background:#ffffff;`
          document.body.append(element)
          // The rectangle the browser reports, so the row read is derived from
          // where the border actually is rather than from where it was asked
          // to be.
          const rect = element.getBoundingClientRect()
          return { left: rect.left, top: rect.top, width: rect.width, height: rect.height }
        },
        { box: BOX, style, width },
      )

      const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))
      // The middle row of the top border band, from the reported rectangle.
      const y = Math.floor(geometry.top + width / 2)
      const from = Math.round(geometry.left) + MARGIN
      const to = Math.round(geometry.left + geometry.width) - MARGIN

      const runs = []
      let ink = pixel(shot, from, y)[0] < THRESHOLD
      let start = from
      for (let x = from; x <= to; x += 1) {
        const here = pixel(shot, x, y)[0] < THRESHOLD
        if (here !== ink) {
          runs.push(`${ink ? 'on' : 'off'}:${x - start}`)
          ink = here
          start = x
        }
      }
      runs.push(`${ink ? 'on' : 'off'}:${to - start + 1}`)

      rows.push([style, width, y, `${from}-${to}`, THRESHOLD, runs.join(' ')].join('\t'))
    }
  }

  // A run that INCLUDES a corner, which every row above is deliberately clear
  // of. CSS Backgrounds 3 §4.4 divides a corner between its two edges along the
  // diagonal, so for a uniform border the top edge's straight portion begins at
  // `left + width`. Where the first dash sits relative to that line -- flush,
  // centred, or mid-gap -- is what a rhythm needs in order to fit a whole
  // number of periods to the side, and it is exactly what the clearance
  // removes.
  for (const width of [2, 4]) {
    const geometry = await browser.page.evaluate(
      ({ box, width }) => {
        document.body.innerHTML = ''
        const element = document.createElement('div')
        element.style.cssText = `position:absolute;left:0;top:0;width:${box.width - 2 * width}px;height:${box.height - 2 * width}px;border:${width}px dashed #000000;background:#ffffff;`
        document.body.append(element)
        const rect = element.getBoundingClientRect()
        return { left: rect.left, top: rect.top, width: rect.width }
      },
      { box: BOX, width },
    )

    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))
    const y = Math.floor(geometry.top + width / 2)
    const from = Math.round(geometry.left)
    const to = from + 60
    const division = from + width

    const runs = []
    let ink = pixel(shot, from, y)[0] < THRESHOLD
    let start = from
    for (let x = from; x <= to; x += 1) {
      const here = pixel(shot, x, y)[0] < THRESHOLD
      if (here !== ink) {
        runs.push(`${ink ? 'on' : 'off'}:${x - start}@${start - from}`)
        ink = here
        start = x
      }
    }
    runs.push(`${ink ? 'on' : 'off'}:${to - start + 1}@${start - from}`)

    rows.push(['dashed-corner-left', width, y, `${from}-${to}`, THRESHOLD, `division@${division - from} ${runs.join(' ')}`].join('\t'))

    // The **other** corner of the same edge, read inward from the right. Our
    // renderer strokes the whole rounded-rect path and clips to each edge's
    // wedge, so the phase at a later corner is whatever arrived along the
    // path; Chrome may instead anchor each side. If both ends of one edge
    // start ink flush at their own corner, the phase is per side and a dash
    // array alone cannot reproduce it.
    const rightEdge = Math.round(geometry.left + geometry.width) - 1
    const back = rightEdge - 60
    const backRuns = []
    let backInk = pixel(shot, rightEdge, y)[0] < THRESHOLD
    let backStart = rightEdge
    for (let x = rightEdge; x >= back; x -= 1) {
      const here = pixel(shot, x, y)[0] < THRESHOLD
      if (here !== backInk) {
        backRuns.push(`${backInk ? 'on' : 'off'}:${backStart - x}@${rightEdge - backStart}`)
        backInk = here
        backStart = x
      }
    }
    backRuns.push(`${backInk ? 'on' : 'off'}:${backStart - back + 1}@${rightEdge - backStart}`)

    rows.push(['dashed-corner-right', width, y, `${back}-${rightEdge}`, THRESHOLD, `division@${width} ${backRuns.join(' ')}`].join('\t'))
  }

  // **The discriminator.** Every reading above is along the TOP edge, and at
  // its far corner a continuous phase and a per-side fit predict the same
  // picture whenever the side nearly divides by the period -- which 240 does at
  // both widths. A VERTICAL edge is the far end of the top edge's whole travel
  // plus two corner arcs: a fresh `on` at offset 0 means each side restarts at
  // its own corner, and anything mid-period means the phase carried round.
  //
  // The second box is 137 wide, which does not divide evenly by any of these
  // periods. That removes the coincidence rather than assuming it away, and it
  // also shows how a lot of slack is spread.
  for (const box of [BOX, { width: 137, height: 48 }]) {
    const width = 4
    const geometry = await browser.page.evaluate(
      ({ box, width }) => {
        document.body.innerHTML = ''
        const element = document.createElement('div')
        element.style.cssText = `position:absolute;left:0;top:0;width:${box.width - 2 * width}px;height:${box.height - 2 * width}px;border:${width}px dashed #000000;background:#ffffff;`
        document.body.append(element)
        const rect = element.getBoundingClientRect()
        return { left: rect.left, top: rect.top, width: rect.width, height: rect.height }
      },
      { box, width },
    )

    await browser.page.setViewportSize(box)
    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...box } }))

    // Down the right border band, from the box's own top edge.
    const x = Math.round(geometry.left + geometry.width) - 1 - Math.floor(width / 2)
    const top = Math.round(geometry.top)
    const bottom = Math.min(top + 60, box.height - 1)
    const down = []
    let ink = pixel(shot, x, top)[0] < THRESHOLD
    let start = top
    for (let y = top; y <= bottom; y += 1) {
      const here = pixel(shot, x, y)[0] < THRESHOLD
      if (here !== ink) {
        down.push(`${ink ? 'on' : 'off'}:${y - start}@${start - top}`)
        ink = here
        start = y
      }
    }
    down.push(`${ink ? 'on' : 'off'}:${bottom - start + 1}@${start - top}`)
    rows.push([`dashed-right-edge-${box.width}`, width, x, `${top}-${bottom}`, THRESHOLD, down.join(' ')].join('\t'))

    // And the far end of the TOP edge of the same box, for the uneven case.
    const rightEdge = Math.round(geometry.left + geometry.width) - 1
    const back = rightEdge - 60
    const bandY = Math.floor(geometry.top + width / 2)
    const backRuns = []
    let backInk = pixel(shot, rightEdge, bandY)[0] < THRESHOLD
    let backStart = rightEdge
    for (let px = rightEdge; px >= back; px -= 1) {
      const here = pixel(shot, px, bandY)[0] < THRESHOLD
      if (here !== backInk) {
        backRuns.push(`${backInk ? 'on' : 'off'}:${backStart - px}@${rightEdge - backStart}`)
        backInk = here
        backStart = px
      }
    }
    backRuns.push(`${backInk ? 'on' : 'off'}:${backStart - back + 1}@${rightEdge - backStart}`)
    rows.push([`dashed-top-far-${box.width}`, width, bandY, `${back}-${rightEdge}`, THRESHOLD, backRuns.join(' ')].join('\t'))
  }
  await browser.page.setViewportSize(BOX)

  // A **radiused** box, which is the last unknown in the rhythm: a side of a
  // rounded box is not a line, and whether the corner arc's length enters the
  // side's fit or the arc is dashed on its own has never been measured. Read
  // along the top band from the box's outer left edge, with the offset at
  // which the STRAIGHT portion begins -- `left + radius` -- named in the row.
  for (const radius of [1, 2, 3, 4, 5, 6, 8, 12, 24]) {
    const width = 4
    const geometry = await browser.page.evaluate(
      ({ box, width, radius }) => {
        document.body.innerHTML = ''
        const element = document.createElement('div')
        element.style.cssText = `position:absolute;left:0;top:0;width:${box.width - 2 * width}px;height:${box.height - 2 * width}px;border:${width}px dashed #000000;border-radius:${radius}px;background:#ffffff;`
        document.body.append(element)
        const rect = element.getBoundingClientRect()
        return { left: rect.left, top: rect.top }
      },
      { box: BOX, width, radius },
    )

    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))
    const y = Math.floor(geometry.top + width / 2)
    const from = Math.round(geometry.left)
    const to = from + BOX.width - 1

    const runs = []
    let ink = pixel(shot, from, y)[0] < THRESHOLD
    let start = from
    for (let x = from; x <= to; x += 1) {
      const here = pixel(shot, x, y)[0] < THRESHOLD
      if (here !== ink) {
        runs.push(`${ink ? 'on' : 'off'}:${x - start}@${start - from}`)
        ink = here
        start = x
      }
    }
    runs.push(`${ink ? 'on' : 'off'}:${to - start + 1}@${start - from}`)

    rows.push(
      [
        `dashed-radius-${radius}`,
        width,
        y,
        `${from}-${to}`,
        THRESHOLD,
        `straight@${radius} box=${BOX.width} straight-length=${BOX.width - 2 * radius} ${runs.join(' ')}`,
      ].join('\t'),
    )

    // **The span, which needs no run arithmetic at all.** Summing runs
    // accumulates a rounding error at every dash end -- eighteen dashes have
    // thirty-six of them -- so a sum three pixels short of the straight length
    // is equally well explained by a different fitted length and by
    // anti-aliased ends falling under the threshold. The offset of the first
    // ink pixel and of the last one measure the fitted length directly.
    //
    // Reported at two thresholds for the same reason: the strict one was
    // chosen for square corners, where every dash end is a hard edge.
    for (const level of [THRESHOLD, 200]) {
      const straightFrom = from + radius
      const straightTo = from + BOX.width - 1 - radius
      let firstInk = null
      let lastInk = null
      for (let x = straightFrom; x <= straightTo; x += 1) {
        if (pixel(shot, x, y)[0] < level) {
          firstInk = firstInk ?? x
          lastInk = x
        }
      }
      const span = firstInk === null ? 'absent' : `${lastInk - firstInk + 1}`
      rows.push(
        [
          `dashed-radius-${radius}-span`,
          width,
          y,
          `${straightFrom}-${straightTo}`,
          level,
          `first@${firstInk === null ? '-' : firstInk - from} last@${lastInk === null ? '-' : lastInk - from} span=${span} straight-length=${BOX.width - 2 * radius}`,
        ].join('\t'),
      )
    }

    // **Along the arc rather than across it, at BOTH corners of this edge.**
    // A band row crosses the curve obliquely and can only ever show one short
    // mark; walked along the centreline the runs are arc lengths, and the two
    // corners can be compared to each other. If a single run were fitted to
    // the whole side the two ends would be mirror images, and at radius 12
    // they are not -- 5 to 12 at the left against 217 to 229 at the right.
    const along = radius - width / 2
    const quarter = (Math.PI / 2) * along
    const steps = 600
    const walkArc = (cx, cy, from0) => {
      const runs = []
      let ink = null
      let start = 0
      let first = null
      for (let step = 0; step <= steps; step += 1) {
        const angle = from0 + (step / steps) * (Math.PI / 2)
        // Floored, never rounded: a pixel index is the cell a point falls in,
        // and `Math.round(0.5)` is 1 -- which put a half-pixel inset one row
        // off a one-pixel band and reported a painted border as blank.
        const px = Math.floor(cx + along * Math.cos(angle))
        const py = Math.floor(cy + along * Math.sin(angle))
        const here = pixel(shot, px, py)[0] < THRESHOLD
        if (ink === null) {
          ink = here
          if (here) first = 0
          continue
        }
        if (here !== ink) {
          runs.push(`${ink ? 'on' : 'off'}:${(((step - start) / steps) * quarter).toFixed(1)}`)
          if (here && first === null) first = ((step / steps) * quarter).toFixed(1)
          ink = here
          start = step
        }
      }
      runs.push(`${ink ? 'on' : 'off'}:${(((steps - start) / steps) * quarter).toFixed(1)}`)
      return { runs, first }
    }

    const left = walkArc(from + radius, Math.round(geometry.top) + radius, Math.PI)
    const rightCentre = from + BOX.width - 1 - radius
    const right = walkArc(rightCentre, Math.round(geometry.top) + radius, -Math.PI / 2)

    rows.push(
      [
        `dashed-arc-${radius}-top-left`,
        width,
        along,
        'walked along the centreline',
        THRESHOLD,
        `quarter=${quarter.toFixed(1)} first-ink@${left.first} ${left.runs.join(' ')}`,
      ].join('\t'),
    )
    rows.push(
      [
        `dashed-arc-${radius}-top-right`,
        width,
        along,
        'walked along the centreline',
        THRESHOLD,
        `quarter=${quarter.toFixed(1)} first-ink@${right.first} ${right.runs.join(' ')}`,
      ].join('\t'),
    )
  }

  // **Where Chrome stops fitting per side and starts running the path.** At
  // width 4 the change sits in `5 < r <= 6`: the ink spans the straight
  // portion exactly up to radius 5 and falls short from 6. Two rules pass
  // through that point and disagree everywhere else -- `r >= w + 2` and
  // `r >= 1.5w` -- so the sweep runs at three widths rather than one. A
  // constant taken from a single width would be wrong at every other width,
  // which is the same failure as branching at zero, one level up.
  for (const width of [2, 4, 8]) {
    for (const radius of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]) {
      if (radius * 2 > BOX.height - 2 * width) continue

      const geometry = await browser.page.evaluate(
        ({ box, width, radius }) => {
          document.body.innerHTML = ''
          const element = document.createElement('div')
          element.style.cssText = `position:absolute;left:0;top:0;width:${box.width - 2 * width}px;height:${box.height - 2 * width}px;border:${width}px dashed #000000;border-radius:${radius}px;background:#ffffff;`
          document.body.append(element)
          const rect = element.getBoundingClientRect()
          return { left: rect.left, top: rect.top }
        },
        { box: BOX, width, radius },
      )

      const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))
      const y = Math.floor(geometry.top + width / 2)
      const from = Math.round(geometry.left)
      const straightFrom = from + radius
      const straightTo = from + BOX.width - 1 - radius

      let firstInk = null
      let lastInk = null
      for (let x = straightFrom; x <= straightTo; x += 1) {
        if (pixel(shot, x, y)[0] < THRESHOLD) {
          firstInk = firstInk ?? x
          lastInk = x
        }
      }
      const span = firstInk === null ? -1 : lastInk - firstInk + 1
      const straightLength = BOX.width - 2 * radius
      rows.push(
        [
          'branch-sweep',
          width,
          y,
          `radius=${radius}`,
          THRESHOLD,
          `span=${span} straight-length=${straightLength} flush=${span === straightLength ? 'yes' : 'no'}`,
        ].join('\t'),
      )
    }
  }

  // **The whole perimeter, walked as one path.** Everything above reads an edge
  // or a corner; a closed run can only be checked for a SEAM by going round.
  // No seam means the loop was fitted as a loop, the slack spread all the way
  // round, and where the run starts is unobservable rather than unknown. A
  // seam -- one gap unlike its neighbours, two dashes butting, a short mark --
  // means the run is fitted from an anchor, and the seam is where the anchor
  // is.
  for (const [width, radius] of [
    [4, 0],
    [4, 4],
    [4, 5],
    [4, 6],
    [4, 8],
    [4, 12],
    [4, 24],
    // The same bisect at another width: whether the threshold is a constant or
    // scales is still open, and it has to be asked with the instrument that
    // works rather than with flushness.
    [8, 4],
    [8, 6],
    [8, 7],
    [8, 8],
    [8, 9],
    [8, 10],
    [8, 12],
  ]) {
    // **A taller box than the rest of this file uses.** At radius 24 in a box
    // 48 tall the corners consume the whole height -- the vertical sides have
    // no straight portion at all, the arcs meet, and a walk of that shape
    // measures a path the box does not have. 120 leaves 72 of straight side.
    // 137 rather than 240, so the perimeter is NOT a whole number of periods:
    // a square box 240 wide has a perimeter of 576 and a period of 12, which
    // divides exactly, and a continuous loop would then start every side flush
    // by arithmetic rather than by policy. 137x120 divides no better than any
    // other number.
    const tall = { width: 137, height: 120 }
    const geometry = await browser.page.evaluate(
      ({ box, width, radius }) => {
        document.body.innerHTML = ''
        const element = document.createElement('div')
        element.style.cssText = `position:absolute;left:0;top:0;width:${box.width - 2 * width}px;height:${box.height - 2 * width}px;border:${width}px dashed #000000;border-radius:${radius}px;background:#ffffff;`
        document.body.append(element)
        const rect = element.getBoundingClientRect()
        return { left: rect.left, top: rect.top }
      },
      { box: tall, width, radius },
    )
    await browser.page.setViewportSize(tall)
    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...tall } }))

    // The centreline of the border band: a rounded rect inset by half the
    // width, so its own radius is `radius - width / 2`.
    const inset = width / 2
    const r = radius - inset
    const left = Math.round(geometry.left) + inset
    const top = Math.round(geometry.top) + inset
    const right = Math.round(geometry.left) + tall.width - 1 - inset
    const bottom = Math.round(geometry.top) + tall.height - 1 - inset

    // Eight segments in path order: four straights and four quarter arcs,
    // clockwise from the top-left tangent.
    const straight = (x0, y0, x1, y1) => ({
      length: Math.hypot(x1 - x0, y1 - y0),
      at: u => [x0 + (x1 - x0) * u, y0 + (y1 - y0) * u],
    })
    const arc = (cx, cy, from0) => ({
      length: (Math.PI / 2) * r,
      at: u => {
        const angle = from0 + u * (Math.PI / 2)
        return [cx + r * Math.cos(angle), cy + r * Math.sin(angle)]
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
    const perimeter = path.reduce((sum, part) => sum + part.length, 0)

    // Sixteen samples per pixel of path. Four was enough along a straight edge
    // and not along an arc: the walk rounds each sample to a pixel, and on a
    // curve several consecutive samples land on the same pixel, which reads as
    // ink continuing. The corner is exactly where the signal lives here, so
    // the sampling has to be finer than the feature being distinguished.
    const total = Math.round(perimeter * 16)
    const runs = []
    let ink = null
    let start = 0
    let walked = 0
    for (let step = 0; step <= total; step += 1) {
      const distance = (step / total) * perimeter
      let rest = distance
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
        runs.push(`${ink ? 'on' : 'off'}:${(((step - start) / total) * perimeter).toFixed(1)}`)
        ink = here
        start = step
      }
      walked = step
    }
    runs.push(`${ink ? 'on' : 'off'}:${(((total - start) / total) * perimeter).toFixed(1)}`)
    const marks = runs.filter(run => run.startsWith('on')).length

    rows.push(
      [
        `dashed-perimeter-w${width}-r${radius}`,
        width,
        r,
        'walked clockwise from the top-left tangent',
        THRESHOLD,
        `box=${tall.width}x${tall.height} perimeter=${perimeter.toFixed(1)} marks=${marks} ${runs.join(' ')}`,
      ].join('\t'),
    )
    const _ = walked
  }
  await browser.page.setViewportSize(BOX)

  // **A corner where the two edges have different widths.** The branch is
  // `radius > width`, and where the widths differ the corner is degenerate
  // when the inner radius fails in EITHER direction (`max`) or only when it
  // fails in both (`min`). Radius 6 with a 4-wide top and an 8-wide left is
  // above one and below the other, so the two rules disagree there and
  // nowhere else.
  //
  // Read as the longest ink run spanning the corner, one pixel inside the
  // outer boundary so both widths are covered. Per-side fitting fills the
  // corner and butts a dash from each edge against it, which is a long run;
  // a continuous fit puts an ordinary dash there. The two uniform boxes are
  // the references: at radius 6 a 4-wide border is above its threshold and an
  // 8-wide one is below it.
  for (const [top, left, radius] of [
    [4, 4, 6],
    [8, 8, 6],
    [4, 8, 6],
    // The second point for the `min` rule: 6 is above 4 and far below 12, so
    // a corner that behaves as rounded here cannot be a near-threshold
    // artefact of 8 being close to 6.
    [4, 12, 6],
    // The uniform control for the row above. Worth taking even though its
    // answer is predictable: it is what makes the mixed reading a comparison
    // rather than a number, and the first mixed row was only legible because
    // its controls sat beside it.
    [12, 12, 6],
    // A width-1 row is labelled WEAK in its own output. Its dash is 2, so ink
    // of 3.0 at a tangent is 1.5x a dash where the classifier wants 1.3x, and
    // at that width antialiasing moves every quantity: it is a reading at the
    // edge of what this instrument can resolve. Left in and labelled rather
    // than dropped -- a weak row labelled weak documents where the instrument
    // runs out, which is the thing nobody records.
    //
    // The extreme of the `min` rule rather than another point along it: a
    // ratio of twenty, where the thick side's own geometry is nowhere near its
    // threshold. A rule fitted at ratios of two and three and failing at
    // twenty is the near-even-240 shape again -- right in the regime measured,
    // silently wrong outside it.
    [1, 1, 2],
    [20, 20, 2],
    [1, 20, 2],
  ]) {
    const geometry = await browser.page.evaluate(
      ({ box, top, left, radius }) => {
        document.body.innerHTML = ''
        const element = document.createElement('div')
        element.style.cssText = `position:absolute;left:0;top:0;width:${box.width - 2 * left}px;height:${box.height - 2 * top}px;border-style:dashed;border-color:#000000;border-top-width:${top}px;border-bottom-width:${top}px;border-left-width:${left}px;border-right-width:${left}px;border-radius:${radius}px;background:#ffffff;`
        document.body.append(element)
        const rect = element.getBoundingClientRect()
        return { left: rect.left, top: rect.top }
      },
      { box: BOX, top, left, radius },
    )

    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))
    const x0 = Math.round(geometry.left)
    const y0 = Math.round(geometry.top)
    // Half the thinner border rather than a fixed pixel: at width 1 the band
    // IS the outer pixel, so an inset of one samples past it and finds no ink
    // at all -- an instrument that cannot see the case it was pointed at.
    //
    // And the samples are FLOORED to a pixel rather than rounded, because
    // `Math.round(0.5)` is 1: with a half-pixel inset a rounded sample lands
    // one row inside the band and a 1-wide border reads as blank. The first
    // run of this row reported zero ink for exactly that reason.
    const inset = Math.min(1, Math.min(top, left) / 2)
    const r = radius - inset
    // Up the left edge to the tangent, round the arc, along the top edge.
    const reach = 40
    const parts = [
      { length: reach, at: u => [x0 + inset, y0 + radius + reach * (1 - u)] },
      {
        length: (Math.PI / 2) * r,
        at: u => [x0 + radius + r * Math.cos(Math.PI + u * (Math.PI / 2)), y0 + radius + r * Math.sin(Math.PI + u * (Math.PI / 2))],
      },
      { length: reach, at: u => [x0 + radius + reach * u, y0 + inset] },
    ]
    const total = parts.reduce((sum, part) => sum + part.length, 0)
    const samples = Math.round(total * 16)
    // The runs themselves, not only the longest: where the ink starts and
    // stops relative to the two tangents is what separates "one long dash from
    // the wider edge" from "a dash from each edge butting through a filled
    // corner", and a single maximum cannot.
    const runs = []
    let ink = null
    let start = 0
    let longest = 0
    let current = 0
    for (let step = 0; step <= samples; step += 1) {
      let rest = (step / samples) * total
      let point = null
      for (const part of parts) {
        if (rest <= part.length || part === parts[parts.length - 1]) {
          point = part.at(Math.min(rest / part.length, 1))
          break
        }
        rest -= part.length
      }
      const here = pixel(shot, Math.floor(point[0]), Math.floor(point[1]))[0] < THRESHOLD
      if (here) {
        current += total / samples
        longest = Math.max(longest, current)
      } else {
        current = 0
      }
      if (ink === null) {
        ink = here
        continue
      }
      if (here !== ink) {
        runs.push(`${ink ? 'on' : 'off'}:${(((step - start) / samples) * total).toFixed(1)}`)
        ink = here
        start = step
      }
    }
    runs.push(`${ink ? 'on' : 'off'}:${(((samples - start) / samples) * total).toFixed(1)}`)

    rows.push(
      [
        'corner-run',
        `top=${top} left=${left}`,
        radius,
        'one pixel inside the outer boundary',
        THRESHOLD,
        `${Math.min(top, left) <= 1 ? 'WEAK ' : ''}longest-ink=${longest.toFixed(1)} dash-top=${2 * top} dash-left=${2 * left} tangent-at=${reach.toFixed(1)} arc-ends=${(reach + (Math.PI / 2) * r).toFixed(1)} ${runs.join(' ')}`,
      ].join('\t'),
    )
  }

  const header = [
    '# Chrome, through `just conformance`. The rhythm of a dashed or dotted border.',
    '#',
    `# Box ${BOX.width}x${BOX.height}. The run is read along the TOP border band, at the`,
    '# row named in the `y` column, between the two `x` bounds -- both derived from the',
    `# rectangle Chrome reported, and both ${MARGIN} pixels clear of a corner, because CSS`,
    '# Backgrounds 3 §4.4 divides a corner between its two edges and a run that',
    '# includes one measures the join rather than the rhythm.',
    '#',
    `# A pixel is ink below ${THRESHOLD} in the red channel. The first and last runs are cut`,
    '# short by the bounds and are not whole periods.',
    '#',
    '#',
    "# The last row is the one case that INCLUDES a corner: read from the box's own",
    '# left edge outward, with each run tagged `@offset` from that edge and the §4.4',
    "# division line named. It says whether a side's first dash starts flush at the",
    '# division, centres the run, or starts mid-gap -- the thing the clearance above',
    '# removes and the thing a fitting rule needs.',
    '#',
    '# style\twidth\ty\tx range\tthreshold\truns',
  ]
  await writeFile(DESTINATION, table([...header, ...rows]), 'utf8')
  process.stderr.write(`border rhythm: ${rows.length} cases -> ${DESTINATION}\n`)
} finally {
  await browser.close()
}
