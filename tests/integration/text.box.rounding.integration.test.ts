import { Root } from '@/canvas/root.canvas.js'
import { Box, Column } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import type { CanvasElement } from '@/canvas/canvas.type.js'
import { integrationRootBase, integrationFontFamily } from './helpers/integration-font.js'

/**
 * A box shrink-wrapping text is exactly as tall as the text inside it.
 *
 * Setting a measure function makes a node `NodeType::Text` to Yoga, and Yoga rounds such a node's
 * edges *up* when they fall between pixels, so the glyphs are never cut. A plain parent rounds to
 * the nearest instead. Three lines measuring 63.3 tall therefore gave the text 64 and the box
 * wrapped around it 63: the text's own box — its background, and anything clipping to it — spilled a
 * row past the parent's.
 *
 * The measure returns whole pixels now, so both round the same number and there is nothing left to
 * disagree about. The sentence below is chosen to wrap to three lines and land on a fraction; a
 * whole-pixel measurement would pass this test without exercising anything.
 */
const WIDTH = 300
const HEIGHT = 200
const MEASURE = 200
const FONT_SIZE = 16
const SENTENCE = 'wrapping text that must break inside a narrow measure'
const BAND = { r: 238, g: 238, b: 136 } // #eeee88

/** The rows the background covers, counted where no glyph can reach. */
async function bandRows(node: CanvasElement) {
  const canvas = await Root({
    ...integrationRootBase,
    width: WIDTH,
    height: HEIGHT,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    fontFamily: integrationFontFamily,
    children: [Column({ children: node })],
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, WIDTH, HEIGHT)
  let top = Infinity
  let bottom = -Infinity
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const i = (y * WIDTH + x) * 4
      if (data[i] === BAND.r && data[i + 1] === BAND.g && data[i + 2] === BAND.b) {
        if (y < top) top = y
        if (y > bottom) bottom = y
      }
    }
  }
  return bottom < 0 ? null : { y: top, height: bottom - top + 1 }
}

describe('a box shrink-wrapping text', () => {
  it('is exactly as tall as the text it holds', async () => {
    // The reported failure: 64 against 63, so the text overflowed the box by a row.
    const own = await bandRows(Text(SENTENCE, { fontSize: FONT_SIZE, width: MEASURE, backgroundColor: '#eeee88' }))
    const wrapped = await bandRows(Box({ width: MEASURE, backgroundColor: '#eeee88', children: Text(SENTENCE, { fontSize: FONT_SIZE }) }))

    expect(own, 'the text did not render').not.toBeNull()
    expect(wrapped).toEqual(own)
  })

  it('leaves the two the same height once padded', async () => {
    const padding = 12
    const own = await bandRows(Text(SENTENCE, { fontSize: FONT_SIZE, width: MEASURE, padding, backgroundColor: '#eeee88' }))
    const wrapped = await bandRows(Box({ width: MEASURE, padding, backgroundColor: '#eeee88', children: Text(SENTENCE, { fontSize: FONT_SIZE }) }))

    expect(wrapped).toEqual(own)
  })

  it('holds a sibling directly under the text, with no gap and no overlap', async () => {
    // What the disagreement cost downstream: the row after the text started before the text ended.
    const band = await bandRows(
      Column({
        children: [Text(SENTENCE, { fontSize: FONT_SIZE, width: MEASURE }), Box({ width: MEASURE, height: 10, backgroundColor: '#eeee88' })],
      }),
    )
    const text = await bandRows(Text(SENTENCE, { fontSize: FONT_SIZE, width: MEASURE, backgroundColor: '#eeee88' }))

    expect(band, 'the sibling did not render').not.toBeNull()
    expect(band!.y, 'the sibling started before the text ended').toBe(text!.height)
  })
})
