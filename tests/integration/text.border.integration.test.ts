import { Root } from '@/canvas/root.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import type { TextProps } from '@/canvas/canvas.type.js'
import { integrationFontFamily, integrationRootBase } from './helpers/integration-font.js'

const WIDTH = 320
const FONT_SIZE = 20
const PHRASE = 'Sphinx of quartz'

/**
 * Where Chrome puts the first glyph's ink, in pixels from the left of the border box.
 *
 * Measured against this same Roboto file served over HTTP, `width: 320px`, `box-sizing: border-box`,
 * `font: 20px/normal`, on a block-level element — which is what a `Text` is, being a flex item. Each
 * figure is the element's content-box left plus the glyph's own left side bearing.
 */
const CHROME_INK_LEFT = {
  'border only': 5,
  'border and padding': 13,
  'padding only': 9,
  'a rule down one side': 15,
} as const

const render = (text: string, props: Partial<TextProps>) =>
  Root({
    ...integrationRootBase,
    width: WIDTH,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Text(text, { width: WIDTH, fontSize: FONT_SIZE, fontFamily: integrationFontFamily, color: '#000000', borderColor: '#2563eb', ...props })],
  })

/** Leftmost column carrying text ink. The border is blue, so a dark blue channel is required too. */
async function inkLeft(canvas: Awaited<ReturnType<typeof render>>) {
  const height = canvas.height
  const { data } = canvas.getContext('2d').getImageData(0, 0, WIDTH, height)
  let left = WIDTH
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < left; x++) {
      const i = (y * WIDTH + x) * 4
      if (data[i] < 120 && data[i + 2] < 120) left = x
    }
  }
  return left
}

describe('a Text with a border of its own', () => {
  it('insets its text by the border, not just the padding', async () => {
    // The node's own border used to be ignored: `x`/`y`/`width`/`height` handed to the content pass
    // are the border box, and only the padding was subtracted, so the text was drawn over the rule.
    expect(await inkLeft(await render(PHRASE, { border: 4 }))).toBe(CHROME_INK_LEFT['border only'])
  })

  it('insets by the border and the padding together', async () => {
    expect(await inkLeft(await render(PHRASE, { border: 4, padding: 8 }))).toBe(CHROME_INK_LEFT['border and padding'])
  })

  it('is unchanged when there is no border', async () => {
    // The case that was always right, kept so a fix to the border cannot move it.
    expect(await inkLeft(await render(PHRASE, { padding: 8 }))).toBe(CHROME_INK_LEFT['padding only'])
  })

  it('reads each edge on its own', async () => {
    const canvas = await render(PHRASE, { border: { Top: 2, Right: 6, Bottom: 10, Left: 14 } })
    expect(await inkLeft(canvas)).toBe(CHROME_INK_LEFT['a rule down one side'])
  })

  it('wraps against the width the border leaves, so layout and drawing agree', async () => {
    const BORDER = 40
    const SENTENCE = 'Sphinx of black quartz judge my vow the five boxing wizards'
    const canvas = await render(SENTENCE, { fontSize: 18, border: BORDER })

    // Yoga sized the box from the measure pass, which always subtracted the border. The drawing pass
    // did not, so it wrapped against the full width and laid the same text out in fewer lines than
    // the box had been built for -- text past the right border, and a line's worth of empty space at
    // the bottom. Counting the lines actually drawn is what catches that; the box height cannot,
    // because it was right the whole time.
    const { data } = canvas.getContext('2d').getImageData(0, 0, WIDTH, canvas.height)
    const inkedRow = (y: number) => {
      for (let x = 0; x < WIDTH; x++) {
        const i = (y * WIDTH + x) * 4
        if (data[i] < 120 && data[i + 2] < 120) return true
      }
      return false
    }

    let lines = 0
    for (let y = 0; y < canvas.height; y++) if (inkedRow(y) && !inkedRow(y - 1)) lines++

    expect(lines).toBe(3)
  })
})
