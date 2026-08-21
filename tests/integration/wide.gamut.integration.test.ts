import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { CanvasRenderingContext2D, ColorSpace } from 'meo-skia-canvas'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * Colour must survive an offscreen.
 *
 * Every grouped effect here draws through a canvas built by `mirrorEngine`, which copies the
 * page's `colorType` and `colorSpace` precisely so a float or wide-gamut page is not composited
 * through a narrower one. That only holds if the renderer also keeps the source's format when it
 * rasterizes a canvas to draw it, and until meo-skia-canvas 5.6.6 it did not: the picture behind
 * a source canvas was handed over as a deferred image fixed at eight bits and sRGB. A `display-p3`
 * page read `[234, 51, 35]` through a backdrop where it held `[255, 0, 0]` -- sRGB red converted
 * back up, with everything the smaller gamut cannot name already gone.
 *
 * Gamut only. The same defect hit depth, but `getImageData` on an `RGBAF32` canvas does not hand
 * back eight-bit RGBA, so the comparison below would be reading something else entirely.
 *
 * Read off the rendered canvas rather than compared to a reference file, since the point is the
 * relationship between two regions of one drawing: what the backdrop covers must match what it
 * does not.
 */
const pixel = (ctx: CanvasRenderingContext2D, x: number, y: number) => Array.from(ctx.getImageData(x, y, 1, 1).data.slice(0, 3))

async function render(props: { colorSpace?: ColorSpace }) {
  // `workerMode: false` picks the overload that resolves with a real canvas, so the context is
  // reachable without a cast -- which is the whole point of reading pixels back here.
  const canvas = await Root({
    ...integrationRootBase,
    width: 200,
    height: 100,
    scale: 1,
    gpu: false,
    backgroundColor: 'color(display-p3 1 0 0)',
    ...props,
    children: [
      Box({
        width: 200,
        height: 100,
        children: [
          Box({
            positionType: Style.PositionType.Absolute,
            position: { Top: 20, Left: 20 },
            width: 160,
            height: 60,
            backdropFilter: 'blur(1px)',
          }),
        ],
      }),
    ],
    workerMode: false,
  })
  return canvas.getContext('2d')
}

describe('wide-gamut pages through an offscreen', () => {
  it('keeps display-p3 red through a backdrop filter', async () => {
    const ctx = await render({ colorSpace: 'display-p3' })
    const outside = pixel(ctx, 2, 2)
    const through = pixel(ctx, 100, 50)

    // The blur is over flat colour, so the two regions hold the same paint and must agree.
    expect(through).toEqual(outside)
    // And specifically: not sRGB red converted back up, which is what the round trip produced.
    expect(through).not.toEqual([234, 51, 35])
  })

  it('leaves an ordinary sRGB page alone', async () => {
    const ctx = await render({})
    expect(pixel(ctx, 100, 50)).toEqual(pixel(ctx, 2, 2))
  })
})
