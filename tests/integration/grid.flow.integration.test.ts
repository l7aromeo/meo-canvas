import { Root } from '@/canvas/root.canvas.js'
import { Box, Column } from '@/canvas/layout.canvas.js'
import { Grid } from '@/canvas/grid.canvas.js'
import type { CanvasElement } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * Where a grid's tracks land once the grid is not at the page origin.
 *
 * `Grid` places its items by making them absolute in Yoga, and Yoga resolves an absolute node
 * against the nearest ancestor that is not `Static`. A grid is static unless the caller said
 * otherwise, so the items resolved straight past it to the page: the tracks ran from the page's
 * origin rather than from the grid's own box, and a preceding sibling or an ancestor's padding
 * dropped out of every offset. A grid sitting at the origin anyway was the only case that looked
 * right, which is the case the earlier tests all used.
 *
 * Offsets are asserted, not merely that something rendered: each cell has its own colour and the
 * bounds of that colour are compared against the track the cell was placed on.
 */
const WIDTH = 300
const COLUMNS = 3
const GAP = 10
const CELL_HEIGHT = 20
const BANNER_HEIGHT = 24

const COLOURS = ['#ff0000', '#00ff00', '#0000ff', '#ffff00', '#ff00ff', '#00ffff'] as const

/** The rectangle one colour covers, or `null` where it never appears. */
function bounds(data: Uint8ClampedArray, width: number, height: number, colour: string) {
  const red = parseInt(colour.slice(1, 3), 16)
  const green = parseInt(colour.slice(3, 5), 16)
  const blue = parseInt(colour.slice(5, 7), 16)

  let left = Infinity
  let top = Infinity
  let right = -Infinity
  let bottom = -Infinity

  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4
      if (data[i] === red && data[i + 1] === green && data[i + 2] === blue) {
        if (x < left) left = x
        if (x > right) right = x
        if (y < top) top = y
        if (y > bottom) bottom = y
      }
    }
  }

  if (right < 0) return null
  return { x: left, y: top, width: right - left + 1, height: bottom - top + 1 }
}

const cells = () => COLOURS.map(backgroundColor => Box({ height: CELL_HEIGHT, backgroundColor }))

const grid = (): CanvasElement => Grid({ columns: COLUMNS, gap: { Row: GAP, Column: GAP }, children: cells() })

/** Renders one page and reports where each cell's colour ended up. */
async function cellBounds({ padding, banner }: { padding: number; banner: boolean }) {
  const gridHeight = CELL_HEIGHT * 2 + GAP
  const height = padding * 2 + (banner ? BANNER_HEIGHT + GAP : 0) + gridHeight

  const canvas = await Root({
    ...integrationRootBase,
    width: WIDTH,
    height,
    padding,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Column({ gap: GAP, children: banner ? [Box({ height: BANNER_HEIGHT, backgroundColor: '#333333' }), grid()] : [grid()] })],
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, WIDTH, height)
  return COLOURS.map(colour => bounds(data, WIDTH, height, colour))
}

/** Where the tracks should run for a grid whose content box starts at `left`, `top`. */
function expected(left: number, top: number) {
  const trackWidth = (WIDTH - left * 2 - GAP * (COLUMNS - 1)) / COLUMNS
  return COLOURS.map((_, index) => {
    const column = index % COLUMNS
    const row = Math.floor(index / COLUMNS)
    return {
      x: Math.round(left + column * (trackWidth + GAP)),
      y: top + row * (CELL_HEIGHT + GAP),
      width: Math.round(trackWidth),
      height: CELL_HEIGHT,
    }
  })
}

/** Rounding between the track arithmetic and the rasteriser is allowed a pixel, placement is not. */
const TOLERANCE = 1

function expectPlaced(actual: ReturnType<typeof bounds>[], want: ReturnType<typeof expected>) {
  actual.forEach((box, index) => {
    expect(box, `cell ${index} did not render`).not.toBeNull()
    expect(Math.abs(box!.x - want[index].x), `cell ${index} x was ${box!.x}, wanted ${want[index].x}`).toBeLessThanOrEqual(TOLERANCE)
    expect(Math.abs(box!.y - want[index].y), `cell ${index} y was ${box!.y}, wanted ${want[index].y}`).toBeLessThanOrEqual(TOLERANCE)
    expect(Math.abs(box!.width - want[index].width), `cell ${index} width was ${box!.width}`).toBeLessThanOrEqual(TOLERANCE)
    expect(box!.height, `cell ${index} height`).toBe(want[index].height)
  })
}

describe('a grid away from the page origin', () => {
  it('runs its tracks from below a preceding sibling', async () => {
    // The reported failure: the first row painted over the banner at the top of the page.
    const padding = 10
    const top = padding + BANNER_HEIGHT + GAP
    expectPlaced(await cellBounds({ padding, banner: true }), expected(padding, top))
  })

  it("runs its tracks from inside an ancestor's padding", async () => {
    expectPlaced(await cellBounds({ padding: 10, banner: false }), expected(10, 10))
  })

  it('leaves a grid alone in an unpadded parent where it was', async () => {
    expectPlaced(await cellBounds({ padding: 0, banner: false }), expected(0, 0))
  })
})
