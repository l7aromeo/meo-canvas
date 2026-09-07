import { Root, Column, Row, Box, Image, Style } from '@/index.js'
import type { CanvasElement } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'
import { join } from 'node:path'

/**
 * The size an image takes when the layout does not decide one for it.
 *
 * CSS uses a replaced element's intrinsic dimensions as its used size, so an `<img>` carrying no
 * width or height is as big as the file says. Only the aspect ratio reached Yoga here, and a ratio
 * is a relationship between the axes rather than a size: naming one axis gave the other, and naming
 * neither gave nothing at all — an unsized image in a `Row`, or in a box shrink-wrapping around it,
 * laid out at zero and drew nothing.
 *
 * The source is 40 by 20, so every expectation below is a number Chrome gives for the same tree.
 * The stretch case is the one that keeps the fix honest: an intrinsic size is one the layout may
 * still overrule, so writing a width instead of measuring would have taken 200x100 down to 40x20.
 */
const PAGE = 200
const NATURAL = { width: 40, height: 20 }
const SOURCE = join(process.cwd(), 'tests/fixtures/images/objectfit-40x20.png')

/** The rectangle the image covers, read as anything that is not the page's white. */
async function drawn(tree: CanvasElement) {
  const canvas = await Root({
    ...integrationRootBase,
    width: PAGE,
    height: PAGE,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [tree],
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, PAGE, PAGE)
  let left = Infinity
  let top = Infinity
  let right = -Infinity
  let bottom = -Infinity
  for (let y = 0; y < PAGE; y++) {
    for (let x = 0; x < PAGE; x++) {
      const i = (y * PAGE + x) * 4
      if (!(data[i] > 250 && data[i + 1] > 250 && data[i + 2] > 250)) {
        if (x < left) left = x
        if (x > right) right = x
        if (y < top) top = y
        if (y > bottom) bottom = y
      }
    }
  }
  return right < 0 ? null : { width: right - left + 1, height: bottom - top + 1 }
}

describe('an image the layout does not size', () => {
  it('lays out at the size of its source inside a row', async () => {
    // The reported failure: nothing rendered, because a row gives its items no main-axis size and
    // the image offered none of its own.
    expect(await drawn(Row({ children: Image({ src: SOURCE }) }))).toEqual(NATURAL)
  })

  it('lays out at the size of its source inside a box shrink-wrapping it', async () => {
    const wrapped = Box({ children: Image({ src: SOURCE }) })
    expect(await drawn(Column({ alignItems: Style.Align.FlexStart, children: wrapped }))).toEqual(NATURAL)
  })

  it('still stretches where the layout does decide, rather than holding its source size', async () => {
    // An intrinsic size is one the layout may overrule. A column stretches its items across, and the
    // height follows the ratio — which is what CSS does with a replaced element here too.
    expect(await drawn(Column({ children: Image({ src: SOURCE }) }))).toEqual({ width: PAGE, height: PAGE / 2 })
  })

  it('takes the axis it was given, and the other from the ratio', async () => {
    expect(await drawn(Column({ children: Image({ src: SOURCE, width: 80 }) }))).toEqual({ width: 80, height: 40 })
    expect(await drawn(Row({ children: Image({ src: SOURCE, height: 20 }) }))).toEqual(NATURAL)
  })

  it('keeps both axes the caller named, ratio or no ratio', async () => {
    const sized = Image({ src: SOURCE, width: 120, height: 80 })
    expect(await drawn(Column({ alignItems: Style.Align.FlexStart, children: sized }))).toEqual({ width: 120, height: 80 })
  })
})
