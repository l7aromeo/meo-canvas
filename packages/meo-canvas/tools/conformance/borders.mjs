// The rhythm Chrome draws a dashed or a dotted border in, read off a painted
// edge: CSS leaves dash lengths to the implementation, so this measures behaviour
// rather than conformance. Runs are read along the top edge, clear of both
// corners, since CSS Backgrounds 3 §4.4 splits a corner between its two edges.

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
 * Two doublings, so a rhythm that scales shows, plus 3: the measured ratios
 * differ between 2 and 4, and without this row the step between them is a guess.
 */
const WIDTHS = [1, 2, 3, 4, 8]

/** The two styles that have a rhythm at all. */
const STYLES = ['dashed', 'dotted']

/**
 * How far from each corner the run is read: five times the widest border, so no
 * run starts inside the wedge §4.4 gives a corner.
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

  // A run that includes a corner. §4.4 starts the straight portion of a uniform
  // top edge at `left + width`; where the first dash sits against that line --
  // flush, centred or mid-gap -- decides how a rhythm fits whole periods.
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

    // The other corner of the same edge, read inward from the right. If both ends
    // of one edge start ink flush at their own corner, the phase is per side and
    // a dash array alone cannot reproduce it.
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

  // A vertical edge separates a continuous phase from a per-side fit, which the
  // top edge's far corner cannot when the side nearly divides by the period: a
  // fresh `on` at offset 0 means each side restarts. The 137-wide box divides by
  // none of these periods, so the coincidence is removed rather than assumed.
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

  // A radiused box: whether the arc's length enters the side's fit or the arc is
  // dashed on its own. Read along the top band from the outer left edge; the
  // straight portion begins at `left + radius`, named in the row.
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

    // The span, from the first and last ink pixel: a sum of runs rounds at every
    // dash end, so a short sum cannot tell a different fitted length from faint
    // ends. Two thresholds, since the strict one was chosen for square corners.
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

    // Along the arc's centreline, at both corners of the edge: the runs are then
    // arc lengths, and a single run fitted to the whole side would make the two
    // ends mirror images. Floored at zero -- below `w/2` the centre path has no
    // arc, and a negative radius walks it backwards onto the far side.
    const along = Math.max(0, radius - width / 2)
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
        // `no-arc` rather than a walk of nothing, and rather than skipping the
        // row: a skipped row is invisible, and the question a later reader asks
        // is not *what did radius 1 measure* but *was radius 1 covered*. Only an
        // emitted row answers that.
        along > 0 ? `quarter=${quarter.toFixed(1)} first-ink@${left.first} ${left.runs.join(' ')}` : 'quarter=0 no-arc',
      ].join('\t'),
    )
    rows.push(
      [
        `dashed-arc-${radius}-top-right`,
        width,
        along,
        'walked along the centreline',
        THRESHOLD,
        along > 0 ? `quarter=${quarter.toFixed(1)} first-ink@${right.first} ${right.runs.join(' ')}` : 'quarter=0 no-arc',
      ].join('\t'),
    )
  }

  // Where Chrome stops fitting per side and runs the path. At width 4 the change
  // is in `5 < r <= 6`, which both `r >= w + 2` and `r >= 1.5w` pass through, so
  // three widths are swept to tell them apart.
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

  // The whole perimeter as one path, since only a loop can show a seam. No seam:
  // the loop was fitted as a loop. A seam -- an odd gap, two dashes butting, a
  // short mark -- marks the anchor the run is fitted from.
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
    // 120 tall, so radius 24 leaves 72 of straight side rather than arcs that meet.
    // 137 wide, so the perimeter is not a whole number of periods and a continuous
    // loop cannot start every side flush by arithmetic alone.
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

    // The centreline of the border band: a rounded rect inset by half the width,
    // radius `radius - width / 2`, floored at zero for the same reason as the arc
    // walk above -- every length the path model produces must be positive.
    const inset = width / 2
    const r = Math.max(0, radius - inset)
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
    // `walk-length`, not `perimeter`: the extents are pixel indices, so this runs
    // one short per axis -- 494.0 here against a 498.0 centreline. Right for
    // sampling pixel centres, wrong as geometry.
    const perimeter = path.reduce((sum, part) => sum + part.length, 0)

    // Sixteen samples per pixel of path: on an arc several samples round to one
    // pixel and read as ink continuing, and the corner is where the signal is.
    const total = Math.round(perimeter * 16)
    const runs = []
    let ink = null
    let start = 0
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
        `box=${tall.width}x${tall.height} walk-length=${perimeter.toFixed(1)} marks=${marks} ${runs.join(' ')}`,
      ].join('\t'),
    )
  }
  await browser.page.setViewportSize(BOX)

  // A corner whose edges differ in width. The branch is `radius > width`; radius 6
  // with a 4-wide top and an 8-wide left separates `max` from `min`. Read as the
  // longest ink run spanning the corner: per-side fitting leaves a long run, a
  // continuous fit an ordinary dash. The uniform boxes are the references.
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
    // Width-1 rows are labelled WEAK in their output: antialiasing moves every
    // quantity at that width, and a labelled row records where the instrument
    // runs out. Ratio twenty tests the `min` rule far outside the ratios of two
    // and three it was fitted at.
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
    // Half the thinner border, since at width 1 an inset of one samples past the
    // band. Floored rather than rounded: `Math.round(0.5)` is 1, which lands a
    // half-pixel inset one row inside the band and reads a 1-wide border blank.
    const inset = Math.min(1, Math.min(top, left) / 2)
    // Floored for the reason the other two sites are: a negative radius walks
    // the far side of the corner rather than a short arc.
    const r = Math.max(0, radius - inset)
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
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`border rhythm: ${rows.length} cases -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
