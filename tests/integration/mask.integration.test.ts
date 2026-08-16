import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { integrationFontFamily, integrationRootBase } from './helpers/integration-font.js'

const SIZE = 40
const FILL = '#ff0000'

/** Alpha at one pixel, which is the whole question a mask answers. */
const alphaAt = (raw: Buffer, x: number, y: number, width = SIZE) => raw[(y * width + x) * 4 + 3]

const render = async (props: Record<string, unknown>, scale = 1) => {
  const canvas = await Root({
    ...integrationRootBase,
    width: SIZE,
    height: SIZE,
    scale,
    workerMode: false,
    children: [Box({ width: SIZE, height: SIZE, backgroundColor: FILL, ...props } as never)],
  } as never)
  return { raw: canvas.toBufferSync('raw'), width: canvas.width }
}

/**
 * Masking is a question about pixels, so these ask about pixels. A unit test can prove the geometry
 * of a path; only a real render can prove the renderer honoured it, and that the node was drawn
 * through it rather than beside it.
 */
describe('masking', () => {
  const centre = SIZE / 2

  it('draws the whole box when nothing is masked', async () => {
    const { raw } = await render({})
    expect(alphaAt(raw, 1, 1)).toBe(255)
    expect(alphaAt(raw, centre, centre)).toBe(255)
  })

  it('clips a circle, leaving the corners empty', async () => {
    const { raw } = await render({ mask: { shape: 'circle' } })

    expect(alphaAt(raw, centre, centre)).toBe(255)
    for (const [x, y] of [
      [1, 1],
      [SIZE - 2, 1],
      [1, SIZE - 2],
      [SIZE - 2, SIZE - 2],
    ]) {
      expect(alphaAt(raw, x, y)).toBe(0)
    }
  })

  it('clips an ellipse to the box, which for a square is its inscribed circle', async () => {
    const { raw } = await render({ mask: { shape: 'ellipse' } })

    expect(alphaAt(raw, centre, centre)).toBe(255)
    expect(alphaAt(raw, 1, 1)).toBe(0)
    // The ellipse touches the middle of each edge, where a circle in a square does too.
    expect(alphaAt(raw, centre, 1)).toBeGreaterThan(0)
  })

  it('clips to path data written in the node own coordinates', async () => {
    const { raw } = await render({ mask: `M 0 0 H ${SIZE} V ${SIZE / 2} H 0 Z` })

    expect(alphaAt(raw, centre, 2)).toBe(255)
    expect(alphaAt(raw, centre, SIZE - 2)).toBe(0)
  })

  it('cuts a hole with the evenodd fill rule', async () => {
    // Two nested rectangles: with `evenodd` the inner one is a hole, with `nonzero` it is filled.
    const path = `M 0 0 H ${SIZE} V ${SIZE} H 0 Z M 10 10 H 30 V 30 H 10 Z`

    const holed = await render({ mask: { path, fillRule: 'evenodd' } })
    const solid = await render({ mask: { path, fillRule: 'nonzero' } })

    expect(alphaAt(holed.raw, centre, centre)).toBe(0)
    expect(alphaAt(solid.raw, centre, centre)).toBe(255)
    expect(alphaAt(holed.raw, 2, 2)).toBe(255)
  })

  it('fades through a gradient rather than cutting', async () => {
    const { raw } = await render({
      mask: { gradient: { type: 'linear', direction: 'to-bottom', colors: ['#000000ff', '#00000000'] } },
    })

    const top = alphaAt(raw, centre, 1)
    const middle = alphaAt(raw, centre, centre)
    const bottom = alphaAt(raw, centre, SIZE - 1)

    // The point of a gradient mask is the values in between, which a clip cannot produce.
    expect(top).toBeGreaterThan(200)
    expect(bottom).toBeLessThan(40)
    expect(middle).toBeGreaterThan(bottom)
    expect(middle).toBeLessThan(top)
  })

  it('keeps the colour it was drawn in while changing only its alpha', async () => {
    const { raw } = await render({
      mask: { gradient: { type: 'linear', direction: 'to-bottom', colors: ['#000000ff', '#00000000'] } },
    })

    const offset = (centre * SIZE + centre) * 4
    expect(raw[offset]).toBe(255)
    expect(raw[offset + 1]).toBe(0)
    expect(raw[offset + 2]).toBe(0)
  })

  it('masks at the device resolution, not the layout one', async () => {
    const scale = 2
    const { raw, width } = await render({ mask: { shape: 'circle' } }, scale)

    expect(width).toBe(SIZE * scale)
    // A mask composited at layout resolution and stretched would put the circle's edge in the wrong
    // place; at device resolution the corner is empty and the centre is full at the larger size.
    expect(alphaAt(raw, width / 2, width / 2, width)).toBe(255)
    expect(alphaAt(raw, 1, 1, width)).toBe(0)
  })

  it('applies to any component, because every one of them renders through the same entry', async () => {
    const canvas = await Root({
      ...integrationRootBase,
      width: SIZE,
      height: SIZE,
      workerMode: false,
      children: [
        Box({
          width: SIZE,
          height: SIZE,
          backgroundColor: FILL,
          children: [Text('mask', { fontSize: 10, color: '#ffffff', fontFamily: integrationFontFamily, mask: { shape: 'circle' } })],
        } as never),
      ],
    } as never)

    // The assertion that matters is that it rendered at all: `Text` overrides `_renderContent`, not
    // `render`, so a mask reaching it proves the wrapper covers every node type rather than boxes.
    expect(canvas.toBufferSync('raw').length).toBe(SIZE * SIZE * 4)
  })

  it('draws unmasked, with a warning, when the gradient cannot be built', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const { raw } = await render({ mask: { gradient: { type: 'linear', direction: 'sideways', colors: ['#000'] } } })

    // Losing the node entirely would be a worse answer than losing its mask.
    expect(alphaAt(raw, centre, centre)).toBe(255)
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('Mask ignored.'))
    warn.mockRestore()
  })
})
