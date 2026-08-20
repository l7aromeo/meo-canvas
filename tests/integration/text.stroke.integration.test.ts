import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { TextProps } from '@/canvas/canvas.type.js'
import { integrationRootBase, integrationFontFamily } from './helpers/integration-font.js'

const W = 400
const H = 100
const FILL = '#ffd400'
const STROKE = { width: 6, color: '#110033' }

/**
 * Ink measurements over a rendered word: how much of the fill colour survives, how much stroke
 * colour there is, and how wide the whole mark is.
 *
 * Chrome, given the same Roboto file at the same size through its own canvas:
 *
 *   fill only          yellow 1530   dark 0      ink 124
 *   stroke over fill   yellow 0      dark 3076   ink 130
 *   stroke under fill  yellow 1455   dark 1349   ink 130
 *
 * A 6px stroke at 44px swallows the fill completely, which is the signature worth pinning: it is
 * what tells the two paint orders apart, and what tells either apart from no stroke at all.
 */
async function ink(props: Partial<TextProps>) {
  const canvas = await Root({
    ...integrationRootBase,
    width: W,
    height: H,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [
      Box({
        width: W,
        height: H,
        padding: 20,
        children: [Text('Stroke', { fontFamily: integrationFontFamily, fontSize: 44, color: FILL, ...props })],
      }),
    ],
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, W, H)
  let fill = 0
  let stroke = 0
  let first = -1
  let last = -1

  for (let x = 0; x < W; x++) {
    for (let y = 0; y < H; y++) {
      const i = (y * W + x) * 4
      if (data[i] > 200 && data[i + 1] > 150 && data[i + 2] < 100) fill++
      if (data[i] < 80 && data[i + 1] < 80 && data[i + 2] < 120) stroke++
      if (data[i] < 250 || data[i + 1] < 250 || data[i + 2] < 250) {
        if (first < 0 || x < first) first = x
        if (x > last) last = x
      }
    }
  }

  return { fill, stroke, width: last - first + 1 }
}

describe('textStroke', () => {
  it('paints the stroke over the fill by default, as CSS does', async () => {
    // The counterintuitive part of `-webkit-text-stroke`, and the reason `paintOrder` has to exist:
    // the stroke is centred on the outline, so half of it lands inside the letter. At this size a
    // 6px stroke covers the fill entirely — Chrome reads 0 fill pixels here too.
    const plain = await ink({})
    const stroked = await ink({ textStroke: STROKE })

    expect(plain.fill).toBeGreaterThan(1000)
    expect(plain.stroke).toBe(0)

    expect(stroked.fill).toBe(0)
    expect(stroked.stroke).toBeGreaterThan(2000)
  })

  it('leaves the letterform whole when the stroke is painted under', async () => {
    const plain = await ink({})
    const under = await ink({ textStroke: STROKE, paintOrder: Style.PaintOrder.Stroke })

    // Almost all the fill survives — it is only eaten where the glyph is thinner than the stroke.
    expect(under.fill).toBeGreaterThan(plain.fill * 0.9)
    expect(under.stroke).toBeGreaterThan(1000)
  })

  it('widens the mark by the stroke either way, since the stroke is centred', async () => {
    // Chrome: 124 unstroked, 130 with a 6px stroke, and the same 130 in both paint orders — the
    // outer edge does not care which was painted first. Ours reads a pixel narrower because of
    // where antialiasing crosses the threshold, which is why this allows one.
    const plain = await ink({})
    const over = await ink({ textStroke: STROKE })
    const under = await ink({ textStroke: STROKE, paintOrder: Style.PaintOrder.Stroke })

    expect(over.width - plain.width).toBeGreaterThanOrEqual(4)
    expect(over.width - plain.width).toBeLessThanOrEqual(6)
    expect(under.width).toBe(over.width)
  })

  it('takes the paintOrder enum and the plain string alike', async () => {
    const viaEnum = await ink({ textStroke: STROKE, paintOrder: Style.PaintOrder.Stroke })
    const viaString = await ink({ textStroke: STROKE, paintOrder: 'stroke' })

    expect(viaString).toEqual(viaEnum)
    // Both actually stroked, rather than both quietly doing nothing.
    expect(viaEnum.stroke).toBeGreaterThan(1000)
  })

  it('falls back to the text colour when the stroke names none', async () => {
    const plain = await ink({})
    const inherited = await ink({ textStroke: { width: 2 } })

    // Drawn in the fill colour, so no dark ink appears — but the mark still grows, which is what
    // separates an inherited stroke from no stroke at all.
    expect(inherited.stroke).toBe(0)
    expect(inherited.fill).toBeGreaterThan(plain.fill)
    expect(inherited.width).toBeGreaterThan(plain.width)
  })

  it('draws nothing extra for a zero or missing width', async () => {
    const plain = await ink({})

    expect(await ink({ textStroke: { width: 0, color: '#110033' } })).toEqual(plain)
    expect(await ink({ textStroke: { color: '#110033' } })).toEqual(plain)
  })

  it('strokes a line that had to be truncated', async () => {
    // The ellipsis path puts text down through its own call, so it has to go through the same
    // painting or a truncated line would lose its outline.
    const truncated = await ink({ textStroke: STROKE, maxLines: 1, ellipsis: true, width: 60 } as Partial<TextProps>)

    expect(truncated.stroke).toBeGreaterThan(500)
  })
})
