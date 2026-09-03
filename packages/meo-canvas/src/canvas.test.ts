import { describe, expect, it } from 'vitest'

import { Canvas, type EncodeOptions, type Format, type NativeCanvas } from './canvas.js'

/** A native surface that records what it was asked for and returns its bytes. */
function fake(bytes = Buffer.from([1, 2, 3])) {
  const calls: { format: Format; options: EncodeOptions }[] = []
  let released = 0
  const native: NativeCanvas = {
    encode(format, options) {
      calls.push({ format, options })
      return bytes
    },
    release() {
      released += 1
    },
    gpu: true,
    engine: 'cpu',
    pageCount: 1,
    scale: 2,
  }
  return { native, calls, released: () => released }
}

/** A canvas over a fake surface and a fake filesystem. */
function canvasOver(native: NativeCanvas) {
  const written: { path: string; bytes: Uint8Array }[] = []
  const canvas = new Canvas(
    native,
    async (path, bytes) => {
      written.push({ path, bytes })
    },
    (path, bytes) => {
      written.push({ path, bytes })
    },
  )
  return { canvas, written }
}

describe('encoding', () => {
  it('defaults to png and passes the format through', async () => {
    const surface = fake()
    const { canvas } = canvasOver(surface.native)

    await canvas.toBuffer()
    canvas.toBufferSync('jpg')

    expect(surface.calls.map(call => call.format)).toEqual(['png', 'jpg'])
  })

  it('encodes once per call, so two formats cost two encodes and one paint', async () => {
    const surface = fake()
    const { canvas } = canvasOver(surface.native)

    await canvas.toBuffer('png')
    await canvas.toBuffer('jpg')

    // Two encodes. The paint is not here at all -- it happened in `Root`, which
    // is the whole reason these are separate calls.
    expect(surface.calls).toHaveLength(2)
  })

  it('hands the options to the surface unchanged', () => {
    const surface = fake()
    const { canvas } = canvasOver(surface.native)

    canvas.toBufferSync('gif', { fps: 24, loop: 3 })

    expect(surface.calls[0]?.options).toEqual({ fps: 24, loop: 3 })
  })
})

describe('writing a file', () => {
  it('takes the format from the extension', async () => {
    const surface = fake()
    const { canvas, written } = canvasOver(surface.native)

    await canvas.toFile('out.webp')
    canvas.toFileSync('out.JPEG')

    expect(surface.calls.map(call => call.format)).toEqual(['webp', 'jpg'])
    expect(written.map(entry => entry.path)).toEqual(['out.webp', 'out.JPEG'])
  })

  it('refuses a path whose extension names no format', async () => {
    const { canvas } = canvasOver(fake().native)

    // Defaulting to png would turn a typo into a file whose name lies about
    // its contents.
    await expect(canvas.toFile('out')).rejects.toThrow(/cannot tell the format/)
    expect(() => canvas.toFileSync('out.docx')).toThrow(/cannot tell the format/)
  })
})

describe('data urls', () => {
  it('carries the format’s media type and base64 bytes', () => {
    const { canvas } = canvasOver(fake(Buffer.from([0, 16, 131])).native)

    expect(canvas.toURLSync('png')).toBe('data:image/png;base64,ABCD')
  })

  it('resolves with the same url as the blocking form', async () => {
    // The awaited form exists because v1's did. There is no I/O in an encode,
    // so it is the same call without the `await` and this says so.
    const { canvas } = canvasOver(fake(Buffer.from([0, 16, 131])).native)

    await expect(canvas.toURL()).resolves.toBe(canvas.toURLSync('png'))
  })

  it('pads a length that is not a multiple of three', () => {
    const one = canvasOver(fake(Buffer.from([77])).native).canvas
    const two = canvasOver(fake(Buffer.from([77, 97])).native).canvas

    expect(one.toURLSync('png').endsWith('TQ==')).toBe(true)
    expect(two.toURLSync('png').endsWith('TWE=')).toBe(true)
  })

  it('agrees with the platform’s own encoder', () => {
    // Base64 is written by hand in `canvas.ts` rather than taken from
    // `Buffer`, so it is worth checking against an encoder that did not come
    // from this package. `btoa` is the platform's, and needs no dependency.
    const bytes = Buffer.from([0, 1, 2, 250, 251, 252, 253])
    const { canvas } = canvasOver(fake(bytes).native)

    const mine = canvas.toURLSync('raw').split(',')[1]
    expect(mine).toBe(btoa(String.fromCharCode(...bytes)))
  })

  it('spells toDataURL the way the DOM does', () => {
    const surface = fake()
    const { canvas } = canvasOver(surface.native)

    // Synchronous, and a quality rather than an options object, because the
    // method it is named after is both.
    const url: string = canvas.toDataURL('jpg', 0.5)

    expect(url.startsWith('data:image/jpeg;base64,')).toBe(true)
    expect(surface.calls[0]?.options).toEqual({ quality: 0.5 })
  })

  it('sends no quality when none was given', () => {
    const surface = fake()
    canvasOver(surface.native).canvas.toDataURL('png')

    expect(surface.calls[0]?.options).toEqual({})
  })
})

describe('release', () => {
  it('frees the surface once, however often it is called', () => {
    const surface = fake()
    const { canvas } = canvasOver(surface.native)

    canvas.release()
    canvas.release()

    expect(surface.released()).toBe(1)
    expect(canvas.released).toBe(true)
  })

  it('refuses an encode afterwards rather than reading freed memory', () => {
    const { canvas } = canvasOver(fake().native)
    canvas.release()

    expect(() => canvas.toBufferSync('png')).toThrow(/was released/)
    expect(() => canvas.toDataURL()).toThrow(/was released/)
  })

  it('is not required — a canvas never released still encodes', () => {
    const surface = fake()
    const { canvas } = canvasOver(surface.native)

    canvas.toBufferSync('png')

    expect(surface.released()).toBe(0)
    expect(canvas.released).toBe(false)
  })
})

describe('what the paint settled on', () => {
  it('reports the request and the outcome apart', () => {
    // v1 reports both because they disagree, and the disagreement is the
    // useful reading: a build with no GPU backend, a driver that declines and
    // a float `colorType` all rasterise on the CPU whatever was asked for.
    const { canvas } = canvasOver(fake().native)

    expect(canvas.gpu).toBe(true)
    expect(canvas.engine).toBe('cpu')
  })

  it('is still readable after the surface is freed', () => {
    // Each of the four is a fact about a paint that has already happened, so a
    // caller holding bytes it has released can still say which rasteriser drew
    // them. A method would have to answer from a surface that is gone.
    const { canvas } = canvasOver(fake().native)
    canvas.release()

    expect(canvas.released).toBe(true)
    expect(canvas.engine).toBe('cpu')
    expect(canvas.pageCount).toBe(1)
    expect(canvas.scale).toBe(2)
  })
})

describe('the surface v1 had', () => {
  it('carries no saveAs, which v1 deprecated', () => {
    const { canvas } = canvasOver(fake().native)

    // A deprecated name reintroduced in a rewrite is one nobody gets to remove.
    expect('saveAs' in canvas).toBe(false)
    expect('saveAsSync' in canvas).toBe(false)
  })

  it('carries a getter per format, each the buffer of that format', async () => {
    const formats = ['png', 'jpg', 'webp', 'avif', 'bmp', 'ico', 'tiff', 'gif', 'apng', 'svg', 'pdf', 'raw'] as const
    const surface = fake()
    const { canvas } = canvasOver(surface.native)

    for (const format of formats) {
      await expect(canvas[format]).resolves.toEqual(Buffer.from([1, 2, 3]))
    }
    expect(surface.calls.map(call => call.format)).toEqual([...formats])
  })

  it('does not encode when the canvas is merely inspected', () => {
    // A getter on the prototype is what makes that true: a spread copies own
    // enumerable properties and finds none of these, so logging a canvas does
    // not start twelve encodes.
    const surface = fake()
    const { canvas } = canvasOver(surface.native)

    expect({ ...canvas }).toEqual({})
    expect(surface.calls).toEqual([])
  })

  it('carries every method a ported script writes a file with', () => {
    const { canvas } = canvasOver(fake().native)

    for (const method of ['toBuffer', 'toBufferSync', 'toFile', 'toFileSync', 'toURL', 'toURLSync', 'toDataURL', 'release']) {
      expect(typeof (canvas as unknown as Record<string, unknown>)[method]).toBe('function')
    }
  })
})
