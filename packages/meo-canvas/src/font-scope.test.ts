import { createHash } from 'node:crypto'
import { createRequire } from 'node:module'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Root, Text } from './index.js'

/**
 * That registering a font changes the process rather than the render, as
 * {@link FontRegistration} tells a caller and `font_scope.rs` pins from Rust. One
 * test rather than four, since the assertions are about order in this process and
 * vitest may reorder tests; its own file keeps the registration out of other suites.
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
