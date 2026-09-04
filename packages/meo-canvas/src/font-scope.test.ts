import { createHash } from 'node:crypto'
import { createRequire } from 'node:module'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Root, Text } from './index.js'

/**
 * That registering a font changes the process rather than the render.
 *
 * A family registered by one render stays registered for every render after it,
 * nothing unregisters anything, and a render naming a family it never
 * registered uses whatever an earlier one left behind instead of failing. The
 * registry is `meo-skia-canvas`'s and neither surface can scope it;
 * `crates/meo-canvas-core/tests/font_scope.rs` pins the same behaviour from the
 * Rust side, and {@link FontRegistration} is where a caller is told about it.
 *
 * **Deliberately one test rather than four.** Every assertion here is about the
 * order things happened in *this process*, and vitest is free to run separate
 * tests in an order it chooses — split up, the control that has to come first
 * might not, and the file would report on whichever order it happened to pick.
 * A single file also keeps the registration out of every other suite: the pool
 * gives each file its own process, and this one deliberately dirties its own.
 */

const HERE = dirname(fileURLToPath(import.meta.url))

/** A face nothing else in this package registers, so the name is ours to spend. */
const FACE = resolve(HERE, '../../../crates/meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf')

/** Fails loudly rather than skipping: a green run that drew nothing proves nothing. */
function requireAddon(): void {
  try {
    createRequire(import.meta.url)('../meo-canvas.node')
  } catch (cause) {
    throw new Error('the addon is not built; run `just addon`. This is the only check that a registration outlives the render that made it.', { cause })
  }
}

/** Draws one line in `family`, registering `paths` first when there are any. */
async function draw(family: string, paths?: readonly string[]): Promise<string> {
  const canvas = await Root({
    width: 240,
    height: 60,
    backgroundColor: '#ffffff',
    ...(paths === undefined ? {} : { fonts: [{ family, paths }] }),
    children: [Text('Hamburgefonstiv', { fontFamily: family, fontSize: 24, color: '#000000' })],
  })
  const bytes = await canvas.toBuffer('png')
  canvas.release()
  return createHash('sha256').update(bytes).digest('hex')
}

describe('a font registration', () => {
  it('outlives the render that made it, and is not undone by leaving it out', async () => {
    requireAddon()

    // **First, and it has to be first.** An unregistered family is refused, so
    // everything below is about a guard that works rather than one that never
    // did. Running this after the registration would assert nothing.
    await expect(draw('ScopeProbe')).rejects.toThrow(/font family "ScopeProbe" is not registered/)

    const registered = await draw('ScopeProbe', [FACE])

    // The registration was for that render and outlived it: this call passes no
    // fonts at all, and not only succeeds where the first one was refused but
    // draws the same pixels.
    expect(await draw('ScopeProbe')).toBe(registered)

    // The control in the other direction. Without it a renderer that had
    // quietly started drawing every family with a fallback would pass every
    // line above, and the file would be reporting on nothing.
    await expect(draw('ScopeProbeNeverRegistered')).rejects.toThrow(/font family "ScopeProbeNeverRegistered" is not registered/)
  })
})
