import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps, CanvasElement } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

const W = 120
const H = 40

const at = (x: number, y: number) => [x, y] as const

async function render(children: CanvasElement[], scale = 1) {
  const canvas = await Root({
    ...integrationRootBase,
    width: W,
    height: H,
    scale,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Box({ width: W, height: H, children })],
  })
  const ctx = canvas.getContext('2d')
  return {
    colourAt: ([x, y]: readonly [number, number]) => {
      const { data } = ctx.getImageData(Math.round(x * scale), Math.round(y * scale), 1, 1)
      return [data[0], data[1], data[2]] as [number, number, number]
    },
    row: (y: number) => ctx.getImageData(0, Math.round(y * scale), W * scale, 1).data,
  }
}

/** A black square on the left half, which is what the backdrop filters. */
const square = () =>
  Box({
    positionType: Style.PositionType.Absolute,
    position: { Top: 0, Left: 0 },
    width: 40,
    height: H,
    backgroundColor: '#000000',
  })

const panel = (props: Partial<BoxProps> = {}) =>
  Box({
    positionType: Style.PositionType.Absolute,
    position: { Top: 0, Left: 20 },
    width: 60,
    height: H,
    backdropFilter: 'blur(6px)',
    ...props,
  })

/** How far ink spreads along a row, in user px — the reach of the blur. */
function inkWidth(row: Uint8ClampedArray, canvasWidth: number, scale: number) {
  let first = -1
  let last = -1
  for (let x = 0; x < canvasWidth; x++) {
    if (row[x * 4] < 250) {
      if (first < 0) first = x
      last = x
    }
  }
  return (last - first + 1) / scale
}

describe('backdropFilter', () => {
  it('filters what is already painted behind the node', async () => {
    // The square's own edge is hard. Seen through the panel it is blurred, so a point just past
    // that edge picks up ink it would not otherwise have.
    const withPanel = await render([square(), panel()])
    const without = await render([square()])

    expect(without.colourAt(at(44, 20))).toEqual([255, 255, 255])
    expect(withPanel.colourAt(at(44, 20))[0]).toBeLessThan(250)
  })

  it('clips the filtered backdrop to the node’s own box', async () => {
    // Inside the panel the square's edge is softened; outside it the same square keeps its own
    // hard edge and the empty canvas stays white. Asserting only the outside would hold true of a
    // node that filtered nothing at all.
    const { colourAt } = await render([square(), panel()])

    const insideOverEdge = colourAt(at(38, 20))
    expect(insideOverEdge[0]).toBeGreaterThan(0)
    expect(insideOverEdge[0]).toBeLessThan(255)

    expect(colourAt(at(10, 20))).toEqual([0, 0, 0])
    expect(colourAt(at(100, 20))).toEqual([255, 255, 255])
  })

  it('paints the node’s own background over the filtered backdrop', async () => {
    // CSS order, and the whole point of frosted glass. Sampled just inside the square's edge,
    // where all three readings differ:
    //
    //   blurred backdrop, no tint   35    the backdrop alone
    //   tint, no backdrop filter   153    the tint alone
    //   both                       167    the tint over the filtered backdrop
    //
    // Painting the background under the backdrop instead would redraw over the tint and land back
    // at 35; leaving the backdrop unfiltered would land at 153.
    const tint = { backgroundColor: 'rgba(255,255,255,0.6)' }
    const both = await render([square(), panel(tint)])
    const backdropOnly = await render([square(), panel()])
    const tintOnly = await render([square(), panel({ ...tint, backdropFilter: undefined })])

    const sample = at(36, 20)
    expect(both.colourAt(sample)[0]).toBeGreaterThan(backdropOnly.colourAt(sample)[0] + 100)
    expect(both.colourAt(sample)[0]).toBeGreaterThan(tintOnly.colourAt(sample)[0] + 8)
  })

  it('does not filter a sibling declared after it', async () => {
    // Only what has been painted is a backdrop. A later sibling draws over the panel and keeps its
    // own hard edge.
    const later = () =>
      Box({
        positionType: Style.PositionType.Absolute,
        position: { Top: 0, Left: 60 },
        width: 20,
        height: H,
        backgroundColor: '#000000',
      })

    const { colourAt } = await render([square(), panel(), later()])

    // The later square is untouched...
    expect(colourAt(at(70, 20))).toEqual([0, 0, 0])
    expect(colourAt(at(82, 20))).toEqual([255, 255, 255])
    // ...while the one painted before the panel is still blurred through it.
    expect(colourAt(at(44, 20))[0]).toBeLessThan(250)
  })

  it('reaches the same distance in user px whatever the root scale', async () => {
    // A canvas applies `filter` in device pixels regardless of the transform, where CSS reads the
    // length in the element's own units. Unscaled, a `blur(6px)` covers three user px at
    // `scale: 2` instead of six, and the same tree exported at two scales is two pictures.
    const reach = async (scale: number) => {
      const { row } = await render([Box({ width: 40, height: H, backgroundColor: '#000000', filter: 'blur(6px)' })], scale)
      return inkWidth(row(20), W * scale, scale)
    }

    const [one, two, three] = [await reach(1), await reach(2), await reach(3)]

    expect(Math.abs(two - one)).toBeLessThanOrEqual(1)
    expect(Math.abs(three - one)).toBeLessThanOrEqual(1)
  })
})
