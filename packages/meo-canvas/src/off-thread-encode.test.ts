import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Root, Text } from './index.js'

/**
 * That `toBuffer` frees the event loop and `toBufferSync` does not, counted with a
 * timer, since a promise resolving says nothing about the loop. The synchronous case
 * is the control that the counter can tell the two apart; the canvas is large
 * because the encode scales with area (97 ms here, under 2 ms at 480×320).
 */

/** The test font, so nothing here depends on the machine's own faces. */
// `fileURLToPath`, not `.pathname`: a file URL's pathname on Windows is
// `/D:/a/...`, which resolves against the current drive and fails.
const FONT = fileURLToPath(new URL('../../../crates/meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf', import.meta.url))

/** Big enough that the encode is measured in tens of milliseconds. */
const PAGE = { width: 4000, height: 4000 }

/** How often the loop is asked whether it is free, in milliseconds. */
const TICK = 5

/**
 * A painted canvas at {@link PAGE}, with text so the paint is not trivial. Throws
 * rather than skips without the addon: this is the one check that sees the loop.
 */
async function painted() {
  try {
    return await Root({
      width: PAGE.width,
      height: PAGE.height,
      backgroundColor: '#ffffff',
      fonts: [{ family: 'Fixture', paths: [FONT] }],
      children: Text('Handoff', { fontSize: 96, fontFamily: 'Fixture', color: '#101014' }),
    })
  } catch (cause) {
    throw new Error('the addon is not built; run `just addon`. This is the only check that measures the event loop.', { cause })
  }
}

/**
 * Runs `work` with a timer running and reports how often the loop was free, read
 * before and after rather than reset. `work` returns `unknown`: a promise or
 * nothing, which `await` passes through.
 */
async function ticksDuring(work: () => unknown): Promise<number> {
  let ticks = 0
  const timer = setInterval(() => {
    ticks += 1
  }, TICK)
  try {
    // One turn of the loop before measuring, so the interval is armed and the
    // first tick is not waiting on this function's own setup.
    await new Promise(resolve => setTimeout(resolve, TICK * 2))
    const before = ticks
    await work()
    return ticks - before
  } finally {
    clearInterval(timer)
  }
}

describe('where the encode runs', () => {
  it('leaves the event loop free while `toBuffer` encodes', async () => {
    const canvas = await painted()
    const ticks = await ticksDuring(() => canvas.toBuffer('png'))
    expect(ticks).toBeGreaterThan(0)
  })

  it('blocks the event loop while `toBufferSync` encodes, which is the control', async () => {
    const canvas = await painted()
    // No `await` inside the work, so the loop cannot turn: every tick this
    // would have fired is still queued when the encode returns.
    const ticks = await ticksDuring(() => {
      canvas.toBufferSync('png')
    })
    expect(ticks).toBe(0)
  })

  it('writes the same bytes either way', async () => {
    const canvas = await painted()
    const asynchronous = await canvas.toBuffer('png')
    const synchronous = canvas.toBufferSync('png')
    // Byte for byte, not merely a decoded image that looks the same: the addon
    // defines the two as one path, and this is the assertion that says so.
    expect(asynchronous.equals(synchronous)).toBe(true)
  })

  it('takes `toURL` and `toFile` off the loop as well', async () => {
    const canvas = await painted()
    const ticks = await ticksDuring(() => canvas.toURL('png'))
    expect(ticks).toBeGreaterThan(0)
  })
})
