/**
 * What every example in this directory shares: where it writes and in what.
 *
 * Each example is a scene and nothing else. This decides the formats, the paths
 * and the size, so the nine of them differ only in what they draw — and so the
 * Rust half beside them can differ only in syntax.
 */

import { mkdir } from 'node:fs/promises'
import { dirname } from 'node:path'

import type { Canvas, Format } from '@l7aromeo/meo-canvas'

/**
 * The formats every example writes.
 *
 * One raster family, one vector, and the raw pixels. A format that refuses a
 * scene is a finding rather than something to skip, so this list is the same
 * for every example and a refusal surfaces as an error naming the format.
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
 * Writes a rendered canvas in every format `formats` names.
 *
 * Stops at the first refusal, naming the format. A format that cannot encode a
 * scene is a result worth stopping on rather than skipping: the point of the
 * directory is to say which parts work.
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
