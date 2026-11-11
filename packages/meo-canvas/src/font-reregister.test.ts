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

/** A registry adding a typeface per registration moves these GPU pixels from the fourth on (l7aromeo/meo-skia-canvas#611). */
const RENDERS = 12

/**
 * Where a GPU engine is expected. The raster backend draws the same pixels however often a
 * face is registered, so elsewhere a render that comes up on the CPU would check nothing.
 */
const GPU_PLATFORM = 'darwin'

describe('naming the same fonts on every render', () => {
  it('draws the same pixels each time on the GPU engine', async ({ skip }) => {
    // Printed as well as passed to `skip`: vitest's CI output names neither a skipped
    // test's note nor a passing test, and a console line it always shows.
    if (process.platform !== GPU_PLATFORM) {
      const reason = `a GPU engine is expected only on macOS, not ${process.platform}, and on the CPU this checks nothing`
      console.info(`skipped: ${reason}`)
      skip(reason)
    }
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
      const engine = canvas.engine
      if (engine !== 'gpu') canvas.release()
      expect(engine, `render ${render} came up on the ${engine} engine, where this checks nothing; macOS is expected to give the GPU`).toBe('gpu')
      hashes.push(
        createHash('sha256')
          .update(await canvas.toBuffer('raw'))
          .digest('hex'),
      )
      canvas.release()
    }
    console.info(`ran ${RENDERS} renders on the gpu engine`)
    expect(hashes.filter(hash => hash !== hashes[0])).toEqual([])
  })
})
