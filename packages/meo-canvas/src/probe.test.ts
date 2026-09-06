/**
 * The v1 9.0.1 layout sweep: what this renderer does, measured.
 *
 * v1 shipped thirteen layout fixes on 2026-08-22 and each names a behaviour
 * rather than a defect, so a commit's existence is not evidence that we lack
 * it. Every entry here is a **measurement of ours**, and v1's source is read
 * only where a measurement says the two differ.
 *
 * A verdict of "we match" is kept rather than deleted. It is what stops the
 * same behaviour being re-checked next month, and it is the half of a sweep
 * that usually goes unrecorded.
 *
 * Measured in **rendered pixels** through the real addon. That is the only
 * currency here that is not downstream of this project's own arithmetic — a
 * comparison against the case fixture shares the encoder's assumptions, and a
 * comparison against a scene shares the layout's.
 *
 * @packageDocumentation
 */

import { describe, expect, it } from 'vitest'

import { Box } from './node.js'
import type { ContainerProps } from './node.js'
import { Root } from './root.js'

/** Colours these scenes are drawn in, so a run reads as a name. */
const RED = '220,40,40'
const BLUE = '40,80,220'
const WHITE = '255,255,255'

/**
 * One row of a rendered scene, as `start-end:r,g,b` runs.
 *
 * Runs rather than single points: a run says where an edge is, and an edge in
 * the wrong place is what every one of these behaviours is about.
 */
async function row(props: ContainerProps, width: number, height: number, y: number): Promise<string> {
  const canvas = await Root({ width, height, gpu: false, children: Box(props) })
  const raw = canvas.toBufferSync('raw')
  canvas.release()

  const pixels: string[] = []
  for (let x = 0; x < width; x += 1) {
    const at = (y * width + x) * 4
    pixels.push(`${raw[at]},${raw[at + 1]},${raw[at + 2]}`)
  }

  const runs: string[] = []
  let colour = pixels[0] as string
  let start = 0
  for (let x = 1; x < width; x += 1) {
    if (pixels[x] !== colour) {
      runs.push(`${start}-${x - 1}:${colour}`)
      colour = pixels[x] as string
      start = x
    }
  }
  runs.push(`${start}-${width - 1}:${colour}`)
  return runs.join(' ')
}

describe('we match', () => {
  it('d4443a9 breaks a stacking tie by tree order', async () => {
    // Two overlapping positioned siblings. CSS 2.1 Appendix E paints equal
    // z-indexes in tree order, so the later one wins — and an explicit higher
    // index wins whatever the order.
    const pair = (first: number | undefined, second: number | undefined): ContainerProps => ({
      width: 60,
      height: 20,
      positionType: 'relative',
      backgroundColor: '#ffffff',
      children: [
        Box({
          positionType: 'absolute',
          position: { top: 0, left: 0 },
          width: 40,
          height: 20,
          backgroundColor: '#dc2828',
          ...(first === undefined ? {} : { zIndex: first }),
        }),
        Box({
          positionType: 'absolute',
          position: { top: 0, left: 10 },
          width: 40,
          height: 20,
          backgroundColor: '#2850dc',
          ...(second === undefined ? {} : { zIndex: second }),
        }),
      ],
    })
    // Sampled inside the overlap, at x=25.
    const at = async (a: number | undefined, b: number | undefined): Promise<string> => {
      const runs = await row(pair(a, b), 60, 20, 10)
      const found = runs
        .split(' ')
        .map(run => run.split(':'))
        .find(([span]) => {
          const [start, end] = (span as string).split('-').map(Number)
          return (start as number) <= 25 && 25 <= (end as number)
        })
      return found?.[1] ?? ''
    }

    expect(await at(undefined, undefined)).toBe(BLUE)
    expect(await at(0, 0)).toBe(BLUE)
    expect(await at(2, 2)).toBe(BLUE)
    expect(await at(2, 1)).toBe(RED)
  })

  it("3781610 runs a grid's tracks from its own box", async () => {
    // The grid is inset 40 from the left by a margin. Two 30-wide columns must
    // start at the grid's own left edge, not at the page's.
    const scene: ContainerProps = {
      width: 100,
      height: 20,
      backgroundColor: '#ffffff',
      children: Box({
        margin: { left: 20 },
        width: 60,
        height: 20,
        display: 'grid',
        gridTemplateColumns: [30, 30],
        children: [Box({ height: 20, backgroundColor: '#dc2828' }), Box({ height: 20, backgroundColor: '#2850dc' })],
      }),
    }

    expect(await row(scene, 100, 20, 10)).toBe(`0-19:${WHITE} 20-49:${RED} 50-79:${BLUE} 80-99:${WHITE}`)
  })

  it('6f87c5e sizes a bordered box by its box-sizing', async () => {
    // Content-box: the width is the content's and the border grows the box
    // around it, so 40 with a 4 border occupies 48. Border-box: the width
    // includes it, so the same box occupies 40.
    //
    // Measured with a corner radius, because a square-cornered border does not
    // paint correctly at all — see the pinned defect below. The sizing question
    // and the painting question are separate and only one of them is ours.
    const sized = (boxSizing: 'content-box' | 'border-box'): ContainerProps => ({
      width: 80,
      height: 60,
      backgroundColor: '#ffffff',
      children: Box({
        boxSizing,
        width: 40,
        height: 40,
        border: 4,
        borderStyle: 'solid',
        borderRadius: 4,
        borderColor: '#2850dc',
        backgroundColor: '#dc2828',
      }),
    })
    const ends = async (boxSizing: 'content-box' | 'border-box'): Promise<number> => {
      const runs = (await row(sized(boxSizing), 80, 60, 20)).split(' ')
      const white = runs[runs.length - 1] as string
      return Number(white.split('-')[0]) - 1
    }

    expect(await ends('content-box'), '40 of content plus 4 of border each side').toBe(47)
    expect(await ends('border-box'), 'the border is inside the 40').toBe(39)
  })

  it('paints a square-cornered border as a border', async () => {
    // Not one of the ten. Found by probing `6f87c5e` and fixed in the painter:
    // `box_path` built a square box with Skia's `add_rect` and a rounded one by
    // extending a path, while `ring_path`'s inner contour always took the
    // second. Mixed, the two contours joined into one self-intersecting path
    // and the even-odd fill left a diagonal wedge across half the box.
    //
    // A square box is now a rounded one with every radius at zero, so both
    // contours are built the same way. All three radii are kept here because
    // the failure was one branch of a two-branch function: a check on the
    // square case alone would pass if the rounded branch broke instead.
    const box = (radius: number): ContainerProps => ({
      width: 60,
      height: 60,
      backgroundColor: '#ffffff',
      children: Box({
        width: 40,
        height: 40,
        border: 4,
        borderStyle: 'solid',
        borderRadius: radius,
        borderColor: '#2850dc',
        backgroundColor: '#dc2828',
      }),
    })
    const correct = `0-3:${BLUE} 4-35:${RED} 36-39:${BLUE} 40-59:${WHITE}`

    expect(await row(box(0), 60, 60, 20), 'square').toBe(correct)
    expect(await row(box(1), 60, 60, 20), 'a radius of one').toBe(correct)
    expect(await row(box(12), 60, 60, 20), 'a radius of twelve').toBe(correct)
  })

  it('1b99d67 lets a page direction reach its children', async () => {
    // `direction: rtl` on the page and nothing on the child: the row must start
    // from the right edge, which is only true if the child inherited it.
    const scene: ContainerProps = {
      width: 80,
      height: 20,
      direction: 'rtl',
      backgroundColor: '#ffffff',
      children: Box({ width: 20, height: 20, backgroundColor: '#2850dc' }),
    }

    expect(await row(scene, 80, 20, 10)).toBe(`0-59:${WHITE} 60-79:${BLUE}`)
  })

  it('9e4f173 does not make a grid item a containing block it never asked to be', async () => {
    // The grid item names no `positionType`, so it is static and is not a
    // containing block: the absolute grandchild resolves against the relative
    // outer box at x=0 rather than against the item at x=40.
    //
    // One defect with `d6bfe23` rather than two, which is why it is measured
    // here rather than fixed separately -- both are an absolute node
    // resolving against its nearest positioned ancestor, and the grid item is
    // only the parent that happens to be in front of it.
    const scene: ContainerProps = {
      width: 100,
      height: 20,
      positionType: 'relative',
      backgroundColor: '#ffffff',
      children: Box({
        margin: { left: 40 },
        width: 60,
        height: 20,
        display: 'grid',
        gridTemplateColumns: [60],
        children: Box({
          height: 20,
          backgroundColor: '#dcdcdc',
          children: Box({
            positionType: 'absolute',
            position: { top: 0, left: 0 },
            width: 20,
            height: 20,
            backgroundColor: '#2850dc',
          }),
        }),
      }),
    }

    expect(await row(scene, 100, 20, 10)).toBe(`0-19:${BLUE} 20-39:${WHITE} 40-99:220,220,220`)
  })

  it('d6bfe23 resolves an absolute node against its containing block, not its parent', async () => {
    // The middle box names no `positionType`, so it is static and is not a
    // containing block: the absolute child's `left: 0` resolves against the
    // relative grandparent, putting it at x=0.
    //
    // The middle is offset by a margin rather than an inset, because a static
    // box ignores its inset and the two answers would otherwise coincide at
    // x=0 — a probe that cannot tell them apart.
    const scene: ContainerProps = {
      width: 100,
      height: 20,
      positionType: 'relative',
      backgroundColor: '#ffffff',
      children: Box({
        margin: { left: 30 },
        width: 40,
        height: 20,
        backgroundColor: '#dcdcdc',
        children: Box({
          positionType: 'absolute',
          position: { top: 0, left: 0 },
          width: 20,
          height: 20,
          backgroundColor: '#2850dc',
        }),
      }),
    }
    const correct = `0-19:${BLUE} 20-29:${WHITE} 30-69:220,220,220 70-99:${WHITE}`

    expect(await row(scene, 100, 20, 10)).toBe(correct)
  })
})

describe('we differ', () => {
  it('923594e composes a transform in the wrong order', async () => {
    // **PINNED DEFECT.** `translateX(20px) rotate(90deg)` about the top-left
    // corner. A CSS list applies right to left — rotate first, then translate —
    // so a box at the origin swings to x=-20..0 and comes back to 0..20. We
    // translate first and then rotate, which sends it to x=-20..0 and off the
    // canvas.
    //
    // The discriminator is chosen so the two answers are "at the origin" and
    // "not on the canvas at all", rather than two positions a rounding argument
    // could separate. Measured alongside it, so the probe is known to be
    // sound: with only the translate the box is at 20..39, and with only the
    // rotation it is off-canvas — which also says the origin is honoured,
    // since a rotation about the default centre would leave it in place.
    const scene: ContainerProps = {
      width: 60,
      height: 60,
      backgroundColor: '#ffffff',
      children: Box({
        width: 20,
        height: 20,
        backgroundColor: '#2850dc',
        transform: { translateX: 20, rotate: 90, originX: 0, originY: 0 },
      }),
    }
    const correct = `0-19:${BLUE} 20-59:${WHITE}`

    // Change this to `toBe(correct)` when the composition order is CSS's.
    expect(await row(scene, 60, 60, 10)).not.toBe(correct)
    expect(await row(scene, 60, 60, 10), 'the box is nowhere on the canvas').toBe(`0-59:${WHITE}`)
  })

  it("fd81f7e runs a grid's tracks from the left whatever the direction", async () => {
    // **PINNED DEFECT.** Under `direction: rtl` the inline axis reverses, so
    // the first track belongs at the right. The grid box itself does move —
    // it sits at x=40..99 rather than 20..79 — so the direction reaches the
    // layout; it is the track order inside it that does not.
    const scene: ContainerProps = {
      width: 100,
      height: 20,
      direction: 'rtl',
      backgroundColor: '#ffffff',
      children: Box({
        margin: { left: 20 },
        width: 60,
        height: 20,
        display: 'grid',
        gridTemplateColumns: [30, 30],
        children: [Box({ height: 20, backgroundColor: '#dc2828' }), Box({ height: 20, backgroundColor: '#2850dc' })],
      }),
    }

    // Change this to the commented form when the tracks follow the direction.
    expect(await row(scene, 100, 20, 10)).toBe(`0-39:${WHITE} 40-69:${RED} 70-99:${BLUE}`)
    // expect(await row(scene, 100, 20, 10)).toBe(`0-39:${WHITE} 40-69:${BLUE} 70-99:${RED}`)
  })
})
