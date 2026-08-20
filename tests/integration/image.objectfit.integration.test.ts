import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Image } from '@/canvas/image.canvas.js'
import type { ImageProps } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

/** 40x20: a green stripe across the top, red left half, blue right half. */
const IMAGE = join(dirname(fileURLToPath(import.meta.url)), '../fixtures/images/objectfit-40x20.png')
const W = 120
const H = 80
const PAPER = { r: 238, g: 238, b: 238 }

/**
 * Where CSS paints a 40x20 image inside a 120x80 box, clipped to the box.
 *
 * The image is twice as wide as it is tall and the box is one and a half times, so every mode lands
 * somewhere different — which is the point: with the node reshaped to the image's own ratio, as it
 * used to be, `contain`, `cover` and `fill` all produced the same rectangle.
 */
const CSS_RECT = {
  fill: '0,0 120x80',
  contain: '0,10 120x60',
  cover: '0,0 120x80',
  none: '40,30 40x20',
  'scale-down': '40,30 40x20',
} as const

async function paintedRect(props: Partial<ImageProps>) {
  const canvas = await Root({
    ...integrationRootBase,
    width: W,
    height: H,
    workerMode: false,
    gpu: false,
    backgroundColor: '#eeeeee',
    children: [Box({ width: W, height: H, children: [Image({ src: IMAGE, width: W, height: H, ...props })] })],
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, W, H)
  let left = W
  let top = H
  let right = -1
  let bottom = -1
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const i = (y * W + x) * 4
      if (data[i] === PAPER.r && data[i + 1] === PAPER.g && data[i + 2] === PAPER.b) continue
      if (x < left) left = x
      if (x > right) right = x
      if (y < top) top = y
      if (y > bottom) bottom = y
    }
  }
  return `${left},${top} ${right - left + 1}x${bottom - top + 1}`
}

describe('objectFit', () => {
  it.each(Object.entries(CSS_RECT))('places the image where CSS does: %s', async (objectFit, expected) => {
    expect(await paintedRect({ objectFit: objectFit as ImageProps['objectFit'] })).toBe(expected)
  })
})

describe('an image given both a width and a height', () => {
  it('keeps the size it was given rather than the one its own proportions imply', async () => {
    // The intrinsic ratio used to be handed to Yoga unconditionally, which let it override the
    // width: a box declared 120x80 was laid out 160x80, and every fit mode then measured itself
    // against a box the caller never asked for.
    const canvas = await Root({
      ...integrationRootBase,
      width: 400,
      height: 200,
      workerMode: false,
      gpu: false,
      backgroundColor: '#eeeeee',
      children: [Box({ width: 400, height: 200, children: [Image({ src: IMAGE, width: W, height: H, objectFit: 'contain' })] })],
    })

    const { data } = canvas.getContext('2d').getImageData(0, 0, 400, 200)
    let right = -1
    for (let y = 0; y < 200; y++) {
      for (let x = 0; x < 400; x++) {
        const i = (y * 400 + x) * 4
        if (data[i] === PAPER.r && data[i + 1] === PAPER.g && data[i + 2] === PAPER.b) continue
        if (x > right) right = x
      }
    }
    // Its content box is 120 wide, so nothing it draws may reach past that.
    expect(right + 1).toBeLessThanOrEqual(W)
  })
})
