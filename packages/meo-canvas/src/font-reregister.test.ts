import { createHash } from 'node:crypto'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Root, Text } from './index.js'

/**
 * Naming the same `fonts` on every render, as {@link FontRegistration} tells a caller
 * to, leaves what a render draws unchanged. Its own file, since a registration lasts
 * the thread and this one has to start from a face nothing else registered.
 */

const HERE = dirname(fileURLToPath(import.meta.url))

const FACE = resolve(HERE, '../../../crates/meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf')

/** A family nothing else in this package registers. */
const FAMILY = 'MeoReregisterProbe'

/** On the GPU the pixels move every third registration here, so twelve crosses three moves. */
const RENDERS = 12

describe('naming the same fonts on every render', () => {
  it('draws the same pixels each time', async () => {
    const hashes: string[] = []
    for (let render = 0; render < RENDERS; render++) {
      const canvas = await Root({
        width: 420,
        height: 140,
        scale: 2.4,
        gpu: true,
        padding: 16,
        backgroundColor: '#ECEEF3',
        fonts: [{ family: FAMILY, paths: [FACE] }],
        children: Text('N.T.E', { fontFamily: FAMILY, fontSize: 96, fontWeight: 900, fontStyle: 'italic', letterSpacing: -2, color: '#3A4FD8' }),
      })
      // Vacuous without a GPU: the raster backend draws the same pixels however
      // often the face was registered.
      if (canvas.engine !== 'gpu') {
        canvas.release()
        return
      }
      hashes.push(
        createHash('sha256')
          .update(await canvas.toBuffer('raw'))
          .digest('hex'),
      )
      canvas.release()
    }
    expect(hashes.filter(hash => hash !== hashes[0])).toEqual([])
  })
})
