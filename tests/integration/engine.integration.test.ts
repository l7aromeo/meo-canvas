import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { integrationRootBase } from './helpers/integration-font.js'

const SIZE = 32

/** PNG IHDR carries the bit depth in the byte after the four-byte width and height. */
const pngBitDepth = (buffer: Buffer) => buffer[buffer.indexOf(Buffer.from('IHDR')) + 4 + 8]

const render = (props: Record<string, unknown>) =>
  Root({
    ...integrationRootBase,
    width: SIZE,
    height: SIZE,
    workerMode: false,
    children: [Box({ width: SIZE, height: SIZE, gradient: { type: 'linear', direction: 'to-right', colors: ['#000000', '#ffffff'] } })],
    ...props,
  } as never)

/**
 * These options are requests the engine may decline, so each case asserts what the canvas reports
 * rather than what was asked for. A test that only checked the prop reached the constructor would
 * pass on a machine where the answer was different.
 */
describe('canvas engine options', () => {
  it('renders on the GPU by default where one is available', async () => {
    const canvas = await render({})
    // Not asserted as `true`: CI runners have no GPU, and the point is that the report is honest.
    expect(typeof canvas.gpu).toBe('boolean')
    expect(canvas.engine.renderer).toMatch(/^(CPU|GPU)$/)
  })

  it('forces the CPU backend when asked', async () => {
    const canvas = await render({ gpu: false })

    expect(canvas.gpu).toBe(false)
    expect(canvas.engine.renderer).toBe('CPU')
  })

  it('composites in the colour type it is given', async () => {
    const canvas = await render({ colorType: 'RGBAF32' })
    expect(canvas.colorType).toBe('RGBAF32')
  })

  it('falls back to the CPU for a float canvas, whatever gpu asked for', async () => {
    // No GPU composites float, so the engine overrides the request rather than failing it.
    const canvas = await render({ colorType: 'RGBAF32', gpu: true })

    expect(canvas.colorType).toBe('RGBAF32')
    expect(canvas.gpu).toBe(false)
  })

  it('carries the depth through to the encoder', async () => {
    const eight = await render({})
    const float = await render({ colorType: 'RGBAF32' })

    expect(pngBitDepth(await eight.toBuffer('png'))).toBe(8)
    expect(pngBitDepth(await float.toBuffer('png'))).toBe(16)
  })

  it('composites in the colour space it is given', async () => {
    const canvas = await render({ colorSpace: 'display-p3' })
    expect(canvas.colorSpace).toBe('display-p3')
  })

  it('leaves the engine to its own defaults when nothing is named', async () => {
    const canvas = await render({})

    expect(canvas.colorType).toBe('rgba')
    expect(canvas.colorSpace).toBe('srgb')
  })

  it('applies to a paged render too', async () => {
    const canvas = await Root({
      ...integrationRootBase,
      width: SIZE,
      height: SIZE,
      workerMode: false,
      gpu: false,
      colorType: 'RGBAF32',
      pages: 3,
      fps: 10,
      children: ({ index }: { index: number }) => Box({ width: SIZE, height: SIZE, backgroundColor: index ? '#1d4ed8' : '#b91c1c' }),
    } as never)

    expect(canvas.pages).toHaveLength(3)
    expect(canvas.colorType).toBe('RGBAF32')
    expect(canvas.gpu).toBe(false)
  })

  it('keeps a masked node at the root colour type', async () => {
    // The mask composites through an offscreen canvas; an eight-bit one under a float root would
    // clip the colour the float was chosen to keep.
    const canvas = await Root({
      ...integrationRootBase,
      width: SIZE,
      height: SIZE,
      workerMode: false,
      colorType: 'RGBAF32',
      children: [
        Box({
          width: SIZE,
          height: SIZE,
          backgroundColor: '#ff0000',
          mask: { gradient: { type: 'linear', direction: 'to-bottom', colors: ['#000000ff', '#00000000'] } },
        } as never),
      ],
    } as never)

    expect(canvas.colorType).toBe('RGBAF32')
    expect(pngBitDepth(await canvas.toBuffer('png'))).toBe(16)
  })
})
