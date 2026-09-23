/**
 * What every example here shares: where it writes, in what formats and at what
 * size, so the nine differ only in what they draw, and the Rust examples beside
 * them only in syntax.
 */

import { mkdir } from 'node:fs/promises'
import { dirname } from 'node:path'

import type { Canvas, Format } from 'meo-canvas'

/**
 * The formats every example writes: raster, vector and raw pixels, the same list
 * for every example, so a refusal surfaces as an error naming the format.
 */
export const FORMATS: readonly Format[] = ['png', 'jpg', 'webp', 'avif', 'bmp', 'tiff', 'svg', 'raw']

/**
 * The formats only a multi-page scene has anything to say in.
 *
 * A single-page example writing a GIF would write a one-frame animation, which
 * says nothing the PNG does not. These are exercised by `pages` alone.
 */
export const PAGED_FORMATS: readonly Format[] = ['pdf', 'gif', 'apng', 'ico']

/**
 * Writes a rendered canvas in every format `formats` names, stopping at the first
 * refusal and naming the format: which parts work is what the directory shows.
 */
export async function draw(name: string, canvas: Canvas, formats: readonly Format[] = FORMATS): Promise<void> {
  const directory = `out/${name}`
  await mkdir(directory, { recursive: true })

  for (const format of formats) {
    const path = `${directory}/${name}.${format}`
    await mkdir(dirname(path), { recursive: true })
    try {
      await canvas.toFile(path)
    } catch (cause) {
      throw new Error(`${name}: writing ${format} failed`, { cause })
    }
  }

  canvas.release()
  console.log(`${name}: ${formats.length} formats`)
}
