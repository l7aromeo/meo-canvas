import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * The order the parts of a `transform` compose in.
 *
 * CSS reads a transform list left to right, each function written in the coordinate system the ones
 * before it left behind, so `scale(2) translateY(30px)` moves the box 60 device pixels: the
 * translation is inside the scale. A translation applied first instead moves it 30 whatever the
 * scale says, which is what this used to do — the box came out 30 short of where Chrome puts it.
 *
 * The same question decides where a rotation leaves a translation, so a rotated chain is pinned
 * here too: `scale(2) rotate(90deg) translateX(30px)` walks the box down the page, not across it.
 *
 * Lengths are resolved against the untransformed border box either way, which is why the pixel and
 * the percentage cases below are asserted to land on the same box.
 */
const PAGE = 400
const OUTER = { left: 100, top: 100, width: 200, height: 100 }
const BOX = { width: 100, height: 50 }

/** The rectangle the red box covers. */
async function redBounds(transform: BoxProps['transform']) {
  const canvas = await Root({
    ...integrationRootBase,
    width: PAGE,
    height: PAGE,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [
      Box({
        margin: OUTER.left,
        width: OUTER.width,
        height: OUTER.height,
        backgroundColor: '#dddddd',
        children: [Box({ ...BOX, backgroundColor: '#cc2222', transform })],
      }),
    ],
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, PAGE, PAGE)
  let left = Infinity
  let top = Infinity
  let right = -Infinity
  let bottom = -Infinity

  for (let y = 0; y < PAGE; y++) {
    for (let x = 0; x < PAGE; x++) {
      const i = (y * PAGE + x) * 4
      if (data[i] > 150 && data[i + 1] < 100 && data[i + 2] < 100) {
        if (x < left) left = x
        if (x > right) right = x
        if (y < top) top = y
        if (y > bottom) bottom = y
      }
    }
  }

  return right < 0 ? null : { x: left, y: top, width: right - left + 1, height: bottom - top + 1 }
}

/** A pixel of slack for the rasteriser's edges; the placement itself is not negotiable. */
const TOLERANCE = 1

function expectBox(actual: Awaited<ReturnType<typeof redBounds>>, want: { x: number; y: number; width: number; height: number }) {
  expect(actual, 'the box did not render').not.toBeNull()
  for (const key of ['x', 'y', 'width', 'height'] as const) {
    expect(Math.abs(actual![key] - want[key]), `${key} was ${actual![key]}, wanted ${want[key]}`).toBeLessThanOrEqual(TOLERANCE)
  }
}

// The box before any transform, and the origin every chain turns about.
const UNTRANSFORMED = { x: OUTER.left, y: OUTER.top, ...BOX }
const CENTRE = { x: OUTER.left + BOX.width / 2, y: OUTER.top + BOX.height / 2 }

/** Where a `scale(s)` about the centre leaves the box, before any translation moves it. */
function scaled(scale: number) {
  return {
    x: CENTRE.x - (BOX.width * scale) / 2,
    y: CENTRE.y - (BOX.height * scale) / 2,
    width: BOX.width * scale,
    height: BOX.height * scale,
  }
}

describe('a transform composes as a CSS transform list', () => {
  it('leaves an untransformed box where it was laid out', async () => {
    expectBox(await redBounds(undefined), UNTRANSFORMED)
  })

  it('scales about the centre with no translation in the chain', async () => {
    expectBox(await redBounds({ scale: 2 }), scaled(2))
  })

  it('moves a scaled box by the scaled distance down the page', async () => {
    // The reported failure: the box moved 30 rather than 60, landing 30 above Chrome's answer.
    const want = scaled(2)
    expectBox(await redBounds({ scale: 2, translateY: 30 }), { ...want, y: want.y + 30 * 2 })
  })

  it('moves a scaled box by the scaled distance across the page', async () => {
    const want = scaled(2)
    expectBox(await redBounds({ scale: 2, translateX: 30 }), { ...want, x: want.x + 30 * 2 })
  })

  it('resolves a percentage translation against the untransformed box, then scales it', async () => {
    // 60% of a 50-tall box is 30, which the scale then carries to 60 — the same box as the pixel
    // case above, not 60% of the scaled height.
    expect(await redBounds({ scale: 2, translateY: '60%' })).toEqual(await redBounds({ scale: 2, translateY: 30 }))
  })

  it('resolves a percentage translation on the inline axis the same way', async () => {
    expect(await redBounds({ scale: 2, translateX: '30%' })).toEqual(await redBounds({ scale: 2, translateX: 30 }))
  })

  it('moves a translation along the axis the rotation turned it onto', async () => {
    // `scale(2) rotate(90deg) translateX(30px)`: the rotation puts the box's own inline axis down
    // the page, so 30 across becomes 60 down, and the scaled box stands on end.
    const upright = scaled(2)
    expectBox(await redBounds({ scale: 2, rotate: 90, translateX: 30 }), {
      x: CENTRE.x - upright.height / 2,
      y: CENTRE.y - upright.width / 2 + 30 * 2,
      width: upright.height,
      height: upright.width,
    })
  })
})
