import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterAll, describe, expect, it } from 'vitest'

import { Box, Root } from './index.js'

/**
 * That `toFile` writes the file `toBuffer` would have handed back.
 *
 * `toFile` no longer encodes to a `Buffer` and then writes it: the bytes never
 * come back through JavaScript, so a page-spanning format streams into the
 * file instead of existing whole in memory first. That is the point of the
 * change and it is invisible in the output — the file is the same either way.
 *
 * What is *not* guaranteed by construction is that the two paths still resolve
 * the same pages. They are two calls into the encoder now, and the case where
 * they can disagree is the one where two rules collide: an `EncodeOptions.page`
 * naming one frame of a format that otherwise gathers them all. A writer that
 * ignored `page` would produce a whole animation, which is a perfectly
 * plausible GIF; nothing about the file says which was asked for. So these
 * compare the paths rather than inspecting either.
 *
 * Byte equality rather than a decoded comparison, and no decoder is needed for
 * it: two files that differ in frame count differ in bytes, and two that agree
 * byte for byte agree about everything.
 */

/**
 * A scene of three pages, each a different flat colour.
 *
 * Different colours on purpose: identical pages encode to identical bytes, so
 * a comparison over them would pass against a writer that always wrote the
 * first page and against one that always wrote the last.
 */
const COLOURS = ['#ff0000', '#00ff00', '#0000ff']

/**
 * The colour for a page, refusing rather than returning `undefined`.
 *
 * An absent colour would render transparent on every page, which is how a
 * first version of this file passed its comparisons while drawing nothing: the
 * page builder is handed a `PageInfo`, not an index, and `COLOURS[page]` was
 * quietly undefined. The throw is what makes that a failure rather than a
 * green run over identical blank pages.
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
