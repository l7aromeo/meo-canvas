import { describe, expect, it } from 'vitest'

import { Box, Image } from './node.js'
import { Root } from './root.js'

/**
 * What `EncodeOptions` documents of meo-skia-canvas, which reads them: a WebP is
 * VP8L and decodes to the drawn pixels under `lossless: true` or, with `lossless`
 * left out, at `quality: 1`, still and animated; `lossless: false` is lossy at any
 * quality. A change upstream fails here. The Rust side is `webp_lossless.rs`; the
 * CLI is `cli.rs`.
 */
describe('lossless WebP', () => {
  const WIDTH = 64
  const HEIGHT = 40

  /** Antialiased curves and a translucent fill, where a lossy encode moves pixels first. */
  const page = (index: number) =>
    Box({
      width: WIDTH,
      height: HEIGHT,
      backgroundColor: '#123456',
      children: Box({ width: 20 + 6 * index, height: 24, borderRadius: 9, backgroundColor: 'rgba(232, 40, 140, 0.7)' }),
    })

  /** The chunk tags at the top level of a RIFF WebP, and inside each `ANMF`. */
  const chunks = (webp: Buffer): string[] => {
    expect(webp.toString('latin1', 0, 4)).toBe('RIFF')
    expect(webp.toString('latin1', 8, 12)).toBe('WEBP')
    const out: string[] = []
    const walk = (at: number, end: number) => {
      while (at + 8 <= end) {
        const tag = webp.toString('latin1', at, at + 4)
        const length = webp.readUInt32LE(at + 4)
        // Sixteen bytes of frame header precede an `ANMF`'s own chunks.
        if (tag === 'ANMF') walk(at + 8 + 16, at + 8 + length)
        out.push(tag)
        at += 8 + length + (length & 1)
      }
    }
    walk(12, webp.length)
    return out
  }

  /** The pixels of `webp` as this renderer decodes them, drawn at their own size. */
  const decoded = async (webp: Buffer) => {
    const canvas = await Root({ width: WIDTH, height: HEIGHT, children: Image({ src: { bytes: webp }, width: WIDTH, height: HEIGHT }) })
    const raw = Buffer.from(await canvas.toBuffer('raw'))
    canvas.release()
    return raw
  }

  it('writes a quality-1 still as VP8L that decodes to the drawn pixels', async () => {
    const canvas = await Root({ width: WIDTH, height: HEIGHT, children: page(0) })
    const drawn = Buffer.from(await canvas.toBuffer('raw'))
    const webp = Buffer.from(await canvas.toBuffer('webp', { quality: 1 }))
    // The control: the default quality is lossy, so the comparison can fail.
    const lossy = Buffer.from(await canvas.toBuffer('webp'))
    canvas.release()

    expect(chunks(webp)).toContain('VP8L')
    expect(chunks(webp)).not.toContain('VP8 ')
    expect((await decoded(webp)).equals(drawn)).toBe(true)

    expect(chunks(lossy)).toContain('VP8 ')
    expect((await decoded(lossy)).equals(drawn)).toBe(false)
  })

  it('writes every frame of a quality-1 animation as VP8L', async () => {
    const canvas = await Root({ width: WIDTH, height: HEIGHT, pages: 3, children: ({ index }) => page(index) })
    const webp = Buffer.from(await canvas.toBuffer('webp', { quality: 1 }))
    canvas.release()
    const tags = chunks(webp)
    expect(tags.filter(tag => tag === 'ANMF')).toHaveLength(3)
    expect(tags.filter(tag => tag === 'VP8L')).toHaveLength(3)
    expect(tags).not.toContain('VP8 ')
  })

  it('writes lossless: true as VP8L that decodes to the drawn pixels, still and animated', async () => {
    const canvas = await Root({ width: WIDTH, height: HEIGHT, children: page(0) })
    const drawn = Buffer.from(await canvas.toBuffer('raw'))
    const webp = Buffer.from(await canvas.toBuffer('webp', { lossless: true }))
    canvas.release()
    expect(chunks(webp)).toContain('VP8L')
    expect(chunks(webp)).not.toContain('VP8 ')
    expect((await decoded(webp)).equals(drawn)).toBe(true)

    const sequence = await Root({ width: WIDTH, height: HEIGHT, pages: 3, children: ({ index }) => page(index) })
    const tags = chunks(Buffer.from(await sequence.toBuffer('webp', { lossless: true })))
    sequence.release()
    expect(tags.filter(tag => tag === 'VP8L')).toHaveLength(3)
    expect(tags).not.toContain('VP8 ')
  })

  it('writes lossless: false as lossy VP8, even at quality 1', async () => {
    const canvas = await Root({ width: WIDTH, height: HEIGHT, children: page(0) })
    const drawn = Buffer.from(await canvas.toBuffer('raw'))
    for (const options of [{ lossless: false }, { lossless: false, quality: 1 }]) {
      const webp = Buffer.from(await canvas.toBuffer('webp', options))
      expect(chunks(webp)).toContain('VP8 ')
      expect(chunks(webp)).not.toContain('VP8L')
      expect((await decoded(webp)).equals(drawn)).toBe(false)
    }
    canvas.release()
  })

  it("refuses lossless beside a quality below 1 with the encoder's reason", async () => {
    const canvas = await Root({ width: WIDTH, height: HEIGHT, children: page(0) })
    for (const format of ['webp', 'avif'] as const) {
      await expect(canvas.toBuffer(format, { lossless: true, quality: 0.5 })).rejects.toThrow(/asks for a lossy file/)
    }
    canvas.release()
  })

  it('encodes lossless AVIF', async () => {
    const canvas = await Root({ width: WIDTH, height: HEIGHT, children: page(0) })
    const avif = Buffer.from(await canvas.toBuffer('avif', { lossless: true }))
    canvas.release()
    expect(avif.toString('latin1', 4, 12)).toBe('ftypavif')
  })
})
