import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterAll, describe, expect, it } from 'vitest'

import { Box, Root } from './index.js'

/**
 * That `toFile` writes the file `toBuffer` would have handed back. They are
 * separate calls into the encoder, and a writer ignoring `page` would write a whole
 * animation, a plausible GIF, so these compare the two paths byte for byte.
 */

/**
 * Three pages in different flat colours: identical pages would pass against a
 * writer that always wrote the first page, or the last.
 */
const COLOURS = ['#ff0000', '#00ff00', '#0000ff']

/**
 * The colour for a page, refusing rather than returning `undefined`, which would
 * render transparent pages that compare equal while drawing nothing.
 */
function colourFor(index: number): string {
  const found = COLOURS[index]
  if (found === undefined) throw new Error(`no colour for page ${index}`)
  return found
}

async function threeColours() {
  return await Root({
    width: 4,
    height: 4,
    pages: COLOURS.length,
    children: page => Box({ width: 4, height: 4, backgroundColor: colourFor(page.index) }),
  })
}

const directory = mkdtempSync(join(tmpdir(), 'meo-canvas-write-'))

afterAll(() => {
  rmSync(directory, { recursive: true, force: true })
})

describe('toFile writes what toBuffer returns', () => {
  // A spanning format and a still one, each with no page named and with one
  // named. The middle page rather than the first: naming page 0 would pass
  // against a writer that always wrote the first page.
  const cases = [
    { format: 'gif' as const, options: {} },
    { format: 'gif' as const, options: { page: 1 } },
    { format: 'png' as const, options: {} },
    { format: 'png' as const, options: { page: 1 } },
  ]

  for (const { format, options } of cases) {
    const name = `${format} ${JSON.stringify(options)}`

    it(`agrees for ${name}, asynchronously`, async () => {
      const canvas = await threeColours()
      const expected = await canvas.toBuffer(format, options)

      const path = join(directory, `async-${format}-${options.page ?? 'all'}.${format}`)
      await canvas.toFile(path, options)

      expect(readFileSync(path).equals(expected)).toBe(true)
    })

    it(`agrees for ${name}, synchronously`, async () => {
      const canvas = await threeColours()
      const expected = canvas.toBufferSync(format, options)

      const path = join(directory, `sync-${format}-${options.page ?? 'all'}.${format}`)
      canvas.toFileSync(path, options)

      expect(readFileSync(path).equals(expected)).toBe(true)
    })
  }

  it('writes a different file when a different page is named, so the comparisons above can fail', async () => {
    // The control. If every page encoded alike, each case above would pass
    // against a writer that always wrote the same one.
    const canvas = await threeColours()
    const first = await canvas.toBuffer('gif', { page: 0 })
    const second = await canvas.toBuffer('gif', { page: 1 })
    const every = await canvas.toBuffer('gif', {})

    expect(first.equals(second)).toBe(false)
    expect(every.equals(first)).toBe(false)
    expect(every.length).toBeGreaterThan(first.length)
  })

  it('refuses a path whose extension names no format before writing anything', async () => {
    const canvas = await threeColours()
    await expect(canvas.toFile(join(directory, 'out'))).rejects.toThrow(/cannot tell the format/)
    expect(() => canvas.toFileSync(join(directory, 'out.docx'))).toThrow(/cannot tell the format/)
  })
})
