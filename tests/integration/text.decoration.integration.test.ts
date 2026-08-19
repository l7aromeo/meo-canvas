import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import type { TextProps } from '@/canvas/canvas.type.js'
import { Style } from '@/constant/common.const.js'
import { integrationFontFamily, integrationRootBase } from './helpers/integration-font.js'

const WIDTH = 360
const HEIGHT = 120
const PHRASE = 'Sphinx of quartz'

const render = (text: string, props: Partial<TextProps>, boxProps: Record<string, unknown> = {}) =>
  Root({
    ...integrationRootBase,
    width: WIDTH,
    height: HEIGHT,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [
      Box({
        width: WIDTH,
        height: HEIGHT,
        backgroundColor: '#ffffff',
        ...boxProps,
        children: [Text(text, { width: WIDTH, fontSize: 24, fontFamily: integrationFontFamily, color: '#000000', ...props })],
      }),
    ],
  })

async function pixels(canvas: Awaited<ReturnType<typeof render>>) {
  const { data } = canvas.getContext('2d').getImageData(0, 0, WIDTH, HEIGHT)
  return {
    inked: (x: number, y: number) => data[(y * WIDTH + x) * 4] < 250,
    count: () => {
      let n = 0
      for (let i = 0; i < data.length; i += 4) if (data[i] < 250) n++
      return n
    },
  }
}

/**
 * The row carrying the most ink, which is the rule.
 *
 * Not the lowest row: in this face the descenders reach below the underline, so the bottom of the
 * drawing is a `q` rather than the line being measured.
 */
function busiestRow(inked: (x: number, y: number) => boolean) {
  let best = -1
  let bestCount = 0
  for (let y = 0; y < HEIGHT; y++) {
    let count = 0
    for (let x = 0; x < WIDTH; x++) if (inked(x, y)) count++
    if (count > bestCount) {
      bestCount = count
      best = y
    }
  }
  return best
}

/** Uninked pixels between the first and last ink on a row — the gaps a per-word rule leaves. */
function gapsAlong(row: number, inked: (x: number, y: number) => boolean) {
  let first = -1
  let last = -1
  for (let x = 0; x < WIDTH; x++) {
    if (!inked(x, row)) continue
    if (first < 0) first = x
    last = x
  }
  if (first < 0) return -1
  let gaps = 0
  for (let x = first; x <= last; x++) if (!inked(x, row)) gaps++
  return gaps
}

describe('textDecoration', () => {
  it('draws a line that plain text does not', async () => {
    const plain = (await pixels(await render(PHRASE, {}))).count()
    const underlined = (await pixels(await render(PHRASE, { textDecoration: 'underline' }))).count()

    expect(underlined).toBeGreaterThan(plain)
  })

  it('draws one unbroken rule across the spaces', async () => {
    const { inked } = await pixels(await render(PHRASE, { textDecoration: 'underline' }))
    const rule = busiestRow(inked)

    // The node draws a word at a time and synthesizes the gap between them, so a rule drawn per
    // call stops at every space. CSS draws one line under the whole run.
    expect(gapsAlong(rule, inked)).toBe(0)
  })

  it('keeps the rule unbroken on every line of wrapped text', async () => {
    const { inked } = await pixels(await render('Sphinx of black quartz judge my vow the five wizards', { textDecoration: 'underline', width: 200 }))
    const rule = busiestRow(inked)

    expect(rule).toBeGreaterThan(0)
    expect(gapsAlong(rule, inked)).toBe(0)
  })

  it('is inherited, so a parent decorates the text inside it', async () => {
    const plain = (await pixels(await render(PHRASE, {}))).count()
    const inherited = (await pixels(await render(PHRASE, {}, { textDecoration: 'underline' }))).count()

    expect(inherited).toBeGreaterThan(plain)
  })

  it('draws nothing for a value it cannot parse, rather than throwing', async () => {
    const plain = (await pixels(await render(PHRASE, {}))).count()
    const nonsense = (await pixels(await render(PHRASE, { textDecoration: 'not-a-decoration' }))).count()

    expect(nonsense).toBe(plain)
  })

  it('still draws rich text, which cannot take the single-call path', async () => {
    // Segments differ in style, so the line has to be drawn a word at a time; the rule is broken
    // there and that is the documented limit rather than a failure to draw.
    const rich = await pixels(await render('Plain <b>bold</b> plain', { textDecoration: 'underline' }))
    expect(rich.count()).toBeGreaterThan(0)
  })
})

describe('text overflow', () => {
  const TALL = 'Sphinx of black quartz judge my vow the five boxing wizards jump quickly at dawn again'

  /** Ink below the box, which only exists when the text was allowed out of it. */
  async function inkBelow(boxHeight: number, boxProps: Record<string, unknown>) {
    const canvas = await render(TALL, { width: 150 }, { height: boxHeight, ...boxProps })
    const { inked } = await pixels(canvas)
    let n = 0
    for (let y = boxHeight; y < HEIGHT; y++) {
      for (let x = 0; x < WIDTH; x++) if (inked(x, y)) n++
    }
    return n
  }

  it('lets text spill out of a box too short for it, as CSS does', async () => {
    expect(await inkBelow(40, {})).toBeGreaterThan(0)
  })

  it('clips it when the box asks to hide its overflow', async () => {
    expect(await inkBelow(40, { overflow: Style.Overflow.Hidden })).toBe(0)
  })
})
