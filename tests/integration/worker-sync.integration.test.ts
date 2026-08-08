import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * Loaded from `dist`, not from `src`, and this is load-bearing rather than a shortcut.
 *
 * The pool starts a worker by path — `render.worker.js` — which only exists once the package has
 * been built. Importing the sources instead would spawn a worker that never loads, and the render
 * would hang rather than fail. Running against the build is also the more honest test: it is the
 * artefact consumers actually install.
 */
const DIST = join(dirname(fileURLToPath(import.meta.url)), '../../dist/esm/index.js')
const built = existsSync(DIST)

/**
 * Exercises the synchronous worker channel against a real Canvas.
 *
 * The unit suite mocks the worker entirely, so it agreed happily with the bug this covers: worker
 * mode used to pre-encode one PNG at render time and return it from every sync method, whatever
 * format was asked for. `toBufferSync('svg')` handed back PNG bytes with no error anywhere, and a
 * caller writing them to a `.svg` got a corrupt file. Only a real render catches that, so these
 * assertions are on magic bytes rather than on call arguments.
 */
const magic = (buf: Buffer, n = 4) => Array.from(buf.subarray(0, n))

let Root: typeof import('@/canvas/root.canvas.js').Root
let terminate: typeof import('@/canvas/root.canvas.js').terminate
let Box: typeof import('@/canvas/layout.canvas.js').Box

const render = () =>
  Root({
    ...integrationRootBase,
    width: 120,
    height: 80,
    children: [Box({ width: '100%', height: '100%', backgroundColor: '#745557' })],
  })

describe.skipIf(!built)('worker-mode synchronous API', () => {
  beforeAll(async () => {
    ;({ Root, terminate, Box } = await import(DIST))
  })

  afterAll(async () => {
    await terminate()
  })

  it('encodes the format that was actually requested', async () => {
    const canvas = await render()

    expect(magic(canvas.toBufferSync('png'))).toEqual([0x89, 0x50, 0x4e, 0x47])
    expect(magic(canvas.toBufferSync('webp'))).toEqual([0x52, 0x49, 0x46, 0x46]) // "RIFF"
    expect(magic(canvas.toBufferSync('jpg'))).toEqual([0xff, 0xd8, 0xff, 0xe0])
    expect(canvas.toBufferSync('svg').subarray(0, 4).toString('utf8')).toBe('<?xm')

    canvas.release()
  })

  /**
   * Structured clone keeps the bytes but drops the subclass, so results arrive as plain
   * Uint8Arrays. Nothing throws — `.toString('utf8')` just quietly returns "60,63,120,109" instead
   * of "<?xm" — so this is asserted directly rather than left to be noticed downstream.
   */
  it('returns real Buffers, not bare Uint8Arrays', async () => {
    const canvas = await render()

    expect(Buffer.isBuffer(canvas.toBufferSync('png'))).toBe(true)
    expect(Buffer.isBuffer(await canvas.toBuffer('png'))).toBe(true)
    expect(Buffer.isBuffer(await canvas.webp)).toBe(true)

    canvas.release()
  })

  it("returns raw pixels for 'raw', not an encoded container", async () => {
    const canvas = await render()
    // 120x80 at the base scale, four channels. An encoded buffer would be orders of magnitude
    // smaller, which is exactly how the old behaviour hid.
    const raw = canvas.toBufferSync('raw')

    expect(raw.length).toBe(canvas.width * canvas.height * 4)

    canvas.release()
  })

  it('reports the matching mime type from toURLSync and toDataURL', async () => {
    const canvas = await render()

    expect(canvas.toURLSync('webp').startsWith('data:image/webp;base64,')).toBe(true)
    expect(canvas.toURLSync('png').startsWith('data:image/png;base64,')).toBe(true)
    expect(canvas.toDataURL('jpg').startsWith('data:image/jpeg;base64,')).toBe(true)

    canvas.release()
  })

  it('serves a repeated call from cache without changing the bytes', async () => {
    const canvas = await render()

    const first = canvas.toBufferSync('webp')
    const second = canvas.toBufferSync('webp')

    expect(second).toBe(first)
    // A different format must not be served the cached entry.
    expect(magic(canvas.toBufferSync('png'))).toEqual([0x89, 0x50, 0x4e, 0x47])

    canvas.release()
  })

  it('exposes gpu and engine, and refuses the members it cannot honour', async () => {
    const canvas = await render()

    expect(typeof canvas.gpu).toBe('boolean')
    expect(canvas.engine.renderer).toMatch(/^(CPU|GPU)$/)

    expect(() => canvas.getContext()).toThrow('not available in worker mode')
    expect(() => canvas.newPage()).toThrow('not available in worker mode')
    expect(() => canvas.pages).toThrow('not available in worker mode')

    canvas.release()
  })

  it('surfaces a worker-side failure as a thrown error, not a hang', async () => {
    const canvas = await render()

    expect(() => canvas.toBufferSync('not-a-format' as never)).toThrow()
    // The channel must still be usable afterwards — a failed call has to leave the port drained.
    expect(magic(canvas.toBufferSync('png'))).toEqual([0x89, 0x50, 0x4e, 0x47])

    canvas.release()
  })

  it('still resolves the async API', async () => {
    const canvas = await render()

    expect(magic(await canvas.toBuffer('webp'))).toEqual([0x52, 0x49, 0x46, 0x46])
    expect(magic(await canvas.png)).toEqual([0x89, 0x50, 0x4e, 0x47])

    canvas.release()
  })
})
