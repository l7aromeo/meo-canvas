import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import type { TextProps } from '@/canvas/canvas.type.js'
import { integrationRootBase, integrationFontFamily } from './helpers/integration-font.js'

const SENTENCE = 'Flower of Paradise Lost'

/**
 * Ink measured to the last pixel drawn, which is where the ellipsis ends.
 *
 * Chrome, given the same Roboto file at 20px with `text-overflow: ellipsis`:
 *
 *   120px   Flower of P…
 *   140px   Flower of Par…
 *   160px   Flower of Paradi…
 *   180px   Flower of Paradise…
 *
 * It fills the line to the character, mid-word, rather than stopping at the last whole word that
 * fitted — which is what wrapping leaves behind if the line is not rebuilt.
 */
async function inkWidth(width: number, props: Partial<TextProps> = {}, text = SENTENCE) {
  const canvas = await Root({
    ...integrationRootBase,
    width: 300,
    height: 80,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Box({ width, height: 60, children: [Text(text, { fontFamily: integrationFontFamily, fontSize: 20, maxLines: 1, ellipsis: true, ...props })] })],
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, 300, 80)
  let last = -1
  for (let x = 0; x < 300; x++) {
    for (let y = 0; y < 80; y++) {
      const i = (y * 300 + x) * 4
      if (data[i] < 200) {
        last = x
        break
      }
    }
  }
  return last + 1
}

describe('ellipsis', () => {
  it('fills the line to the character, as CSS does', async () => {
    // Each wider box has to reach further into the sentence. Breaking at whole words instead gives
    // the same `Flower of…` at 120, 140 and 160 — three identical readings where a browser shows
    // three different amounts of text.
    const widths = [120, 140, 160, 180]
    const inks = []
    for (const width of widths) inks.push(await inkWidth(width))

    for (let i = 1; i < inks.length; i++) {
      expect(inks[i]).toBeGreaterThan(inks[i - 1])
    }

    // ...and each one uses most of the room it was given, rather than stopping a word early.
    for (let i = 0; i < widths.length; i++) {
      expect(inks[i]).toBeGreaterThan(widths[i] - 12)
      expect(inks[i]).toBeLessThanOrEqual(widths[i])
    }
  })

  it('always draws the ellipsis, making room for it by dropping characters', async () => {
    // The mark used to be skipped when the room ran out, so a truncated line gave no sign of being
    // truncated: at 180px this drew `Flower of Paradise` and stopped there.
    //
    // The control is that same text with nothing to truncate, so the difference between them is
    // the mark itself. Comparing against a shorter string instead would pass either way, since a
    // longer line is wider whether or not it ends in an ellipsis.
    const truncated = await inkWidth(180)
    const withoutMark = await inkWidth(180, { maxLines: undefined, ellipsis: false }, 'Flower of Paradise')

    expect(truncated).toBeGreaterThan(withoutMark + 3)
    expect(truncated).toBeLessThanOrEqual(180)
  })

  it('does not pull text up across a newline the caller wrote', async () => {
    // A soft wrap is the library's choice and can be undone to fill the line; a `\n` is the
    // caller's and cannot. The second line here must stay off the first.
    const hard = await inkWidth(200, { maxLines: 1 }, 'Flower\nof Paradise Lost')
    const soft = await inkWidth(200, { maxLines: 1 }, 'Flower of Paradise Lost')

    expect(hard).toBeLessThan(soft)
  })

  it('keeps a caller’s own ellipsis string', async () => {
    const custom = await inkWidth(140, { ellipsis: ' [more]' })
    const standard = await inkWidth(140)

    expect(custom).not.toBe(standard)
    expect(custom).toBeLessThanOrEqual(140)
  })
})
