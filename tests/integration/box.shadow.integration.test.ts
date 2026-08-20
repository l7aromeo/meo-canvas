import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxShadowProps } from '@/canvas/canvas.type.js'
import { integrationFontFamily, integrationRootBase } from './helpers/integration-font.js'

const W = 240
const H = 160
const BOX = { left: 40, top: 30, width: 160, height: 100 }
const PANEL = '#dbeafe'
/** The same colour as the sampler reports it. */
const PANEL_RGB = 'rgb(219,234,254)'

/**
 * Chrome's colour at the centre of the box for each background, with a shadow behind it.
 *
 * CSS clips an outer shadow to outside the border box, so it never darkens the box itself — which
 * is only observable when the background lets something through.
 */
const CHROME_CENTRE = {
  opaque: 'rgb(51,102,204)',
  transparent: 'rgb(255,255,255)',
  semiTransparent: 'rgb(153,178,229)',
} as const

async function render(shadow: BoxShadowProps, boxProps: Record<string, unknown> = {}) {
  return Root({
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
        alignItems: Style.Align.FlexStart,
        children: [
          Box({
            width: BOX.width,
            height: BOX.height,
            margin: { Left: BOX.left, Top: BOX.top },
            backgroundColor: PANEL,
            boxShadow: shadow,
            ...boxProps,
          }),
        ],
      }),
    ],
  })
}

async function sampler(canvas: Awaited<ReturnType<typeof render>>) {
  const { data } = canvas.getContext('2d').getImageData(0, 0, W, H)
  return (x: number, y: number) => {
    const i = (y * W + x) * 4
    return `rgb(${data[i]},${data[i + 1]},${data[i + 2]})`
  }
}

const centre = { x: BOX.left + BOX.width / 2, y: BOX.top + BOX.height / 2 }

describe('an outset shadow', () => {
  const OFFSET: BoxShadowProps = { offsetX: 30, offsetY: 20, blur: 0, color: 'rgba(0,0,0,1)' }

  it('draws where the offset puts it', async () => {
    const at = await sampler(await render(OFFSET))
    expect(at(BOX.left + BOX.width + 15, BOX.top + BOX.height + 10)).toBe('rgb(0,0,0)')
  })

  it.each([
    ['an opaque background', '#3366cc', CHROME_CENTRE.opaque],
    ['no background', undefined, CHROME_CENTRE.transparent],
    ['a semi-transparent background', 'rgba(51,102,204,0.5)', CHROME_CENTRE.semiTransparent],
  ])('is never painted under the box: %s', async (_label, backgroundColor, expected) => {
    // The reason this matters is the transparent case: a shadow drawn behind the box would show
    // straight through it, and CSS knocks it out instead.
    const at = await sampler(await render(OFFSET, { backgroundColor }))
    expect(at(centre.x, centre.y)).toBe(expected)
  })
})

describe('an inset shadow', () => {
  it('draws at all', async () => {
    // It used to stroke a path with `strokeStyle = 'transparent'` and rely on the shadow of that
    // stroke. Nothing is painted by a transparent stroke, so nothing cast a shadow either.
    const at = await sampler(await render({ inset: true, offsetX: 20, offsetY: 20, blur: 0, color: '#000000' }))
    expect(at(BOX.left + 10, BOX.top + 10)).toBe('rgb(0,0,0)')
  })

  it('darkens the side the offset comes from, not the side it points at', async () => {
    const at = await sampler(await render({ inset: true, offsetX: 20, offsetY: 20, blur: 0, color: '#000000' }))

    expect(at(BOX.left + 10, centre.y)).toBe('rgb(0,0,0)')
    expect(at(BOX.left + BOX.width - 10, centre.y)).toBe(PANEL_RGB)
  })

  it('leaves the middle of the box alone', async () => {
    const at = await sampler(await render({ inset: true, offsetX: 20, offsetY: 20, blur: 0, color: '#000000' }))
    expect(at(centre.x, centre.y)).toBe(PANEL_RGB)
  })

  it('stays inside the box', async () => {
    const at = await sampler(await render({ inset: true, offsetX: 20, offsetY: 20, blur: 0, color: '#000000' }))
    expect(at(BOX.left - 10, centre.y)).toBe('rgb(255,255,255)')
  })

  it('reaches in from every edge when spread', async () => {
    const at = await sampler(await render({ inset: true, offsetX: 0, offsetY: 0, blur: 0, spread: 20, color: '#000000' }))

    expect(at(BOX.left + 10, centre.y)).toBe('rgb(0,0,0)')
    expect(at(BOX.left + BOX.width - 10, centre.y)).toBe('rgb(0,0,0)')
    expect(at(centre.x, centre.y)).toBe(PANEL_RGB)
  })
})

describe('spread', () => {
  it('grows the shadow beyond the box', async () => {
    const at = await sampler(await render({ offsetX: 0, offsetY: 0, blur: 0, spread: 20, color: '#000000' }))
    expect(at(BOX.left - 10, centre.y)).toBe('rgb(0,0,0)')
  })

  it('keeps a square corner square, as CSS does', async () => {
    // A radius of zero stays zero however far the shadow spreads, so the ring's corner is filled
    // rather than curved away.
    const at = await sampler(await render({ offsetX: 0, offsetY: 0, blur: 0, spread: 20, color: '#000000' }))
    expect(at(BOX.left - 18, BOX.top - 18)).toBe('rgb(0,0,0)')
  })
})

describe('a Text', () => {
  it('takes a boxShadow like any other node', async () => {
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
          alignItems: Style.Align.FlexStart,
          children: [
            Text('Sphinx', {
              fontSize: 24,
              fontFamily: integrationFontFamily,
              color: '#0f172a',
              backgroundColor: PANEL,
              padding: 12,
              margin: { Left: 20, Top: 30 },
              boxShadow: { offsetX: 60, offsetY: 0, blur: 0, color: '#000000' },
            }),
          ],
        }),
      ],
    })

    const { data } = canvas.getContext('2d').getImageData(0, 0, W, H)
    let hasShadow = false
    for (let i = 0; i < data.length; i += 4) {
      if (data[i] === 0 && data[i + 1] === 0 && data[i + 2] === 0) hasShadow = true
    }
    expect(hasShadow).toBe(true)
  })
})
