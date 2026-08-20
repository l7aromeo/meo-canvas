import { Root } from '@/canvas/root.canvas.js'
import { Box, Row } from '@/canvas/layout.canvas.js'
import { Image } from '@/canvas/image.canvas.js'
import type { CanvasElement } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'
import { join } from 'node:path'

const IMAGE = join(process.cwd(), 'tests/fixtures/images/objectfit-40x20.png')
const SCALE = 2
const CELL = 40

/**
 * Renders a row of cells at `scale: 2` and reports the colour at the middle of each.
 *
 * The scale is the point of the test. An `Image` always clips to its own content box, and a clip
 * inside an opacity layer used to be applied under the transform twice — so at any scale above 1
 * the clip landed at `user × scale²`, missing every image except one at the origin, and they
 * rendered nothing at all.
 */
async function cellColours(children: CanvasElement[]) {
  const canvas = await Root({
    ...integrationRootBase,
    width: CELL * children.length,
    height: CELL,
    scale: SCALE,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Row({ width: CELL * children.length, height: CELL, children })],
  })

  const ctx = canvas.getContext('2d')
  return children.map((_, index) => {
    const x = Math.round((index * CELL + CELL / 2) * SCALE)
    const y = Math.round((CELL / 2) * SCALE)
    const { data } = ctx.getImageData(x, y, 1, 1)
    return `rgb(${data[0]},${data[1]},${data[2]})`
  })
}

const WHITE = 'rgb(255,255,255)'
const image = (opacity?: number) => Image({ src: IMAGE, width: CELL, height: CELL, opacity })

describe('opacity under a scaled root', () => {
  it('draws a lone image with opacity, lighter than the same image opaque', async () => {
    const [faded] = await cellColours([image(0.5)])
    const [solid] = await cellColours([image()])

    expect(faded).not.toBe(WHITE)
    expect(faded).not.toBe(solid)
  })

  it('draws both when two images carry opacity', async () => {
    expect(await cellColours([image(0.5), image(0.5)])).not.toContain(WHITE)
  })

  it('draws both when only the second carries opacity', async () => {
    // The reported failure: the first image drew and the second vanished, because the doubled clip
    // is anchored at the origin and only grows outward — a child laid out further along missed it.
    expect(await cellColours([image(), image(0.6)])).not.toContain(WHITE)
  })

  it('draws all ten of a row of images with opacity', async () => {
    const row = Array.from({ length: 10 }, () => image(0.5))
    expect(await cellColours(row)).not.toContain(WHITE)
  })

  it('draws a box and an image that share an opacity', async () => {
    const box = Box({ width: CELL, height: CELL, backgroundColor: '#11aa11', opacity: 0.5 })
    expect(await cellColours([box, image(0.5)])).not.toContain(WHITE)
  })
})
