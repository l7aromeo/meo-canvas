import { describe, expect, it } from 'vitest'

import { Root } from './root.js'
import { Text } from './node.js'

/**
 * Markup a caller wrote that the renderer could not use.
 *
 * Through a real render, because the claim is that the report survives the
 * parse, the arena and the Neon boundary — none of which a unit test of the
 * parser touches.
 */
describe('markup the renderer could not use', () => {
  const render = async (markup: string) => {
    const canvas = await Root({
      width: 120,
      height: 24,
      backgroundColor: '#ffffff',
      children: Text(markup, { fontSize: 12, color: '#000000' }),
    })
    const raw = Buffer.from(await canvas.toBuffer('raw'))
    const found = canvas.diagnostics
    canvas.release()

    let ink = 0
    for (let at = 0; at < raw.length; at += 4) if ((raw[at] as number) < 200) ink += 1
    return { ink, found }
  }

  it('reports a tag it does not know, which the picture cannot show', async () => {
    const tagged = await render('<nope>abc</nope> def')
    const plain = await render('abc def')

    // The ink is the reason the channel has to exist: the two renders are
    // identical, so nothing a caller can see distinguishes them.
    expect(tagged.ink).toBe(plain.ink)

    expect(tagged.found).toHaveLength(1)
    expect(tagged.found[0]?.path).toBe('<nope>')
    expect(tagged.found[0]?.detail).toContain('not a tag this parser knows')
    expect(plain.found).toHaveLength(0)
  })

  it('reports a value it cannot read, and stays quiet on one it can', async () => {
    const bad = await render('<color=zzz>abc</color>')
    expect(bad.found).toHaveLength(1)
    expect(bad.found[0]?.path).toBe('<color=zzz>')

    // The control. Without it the assertion above passes on a renderer that
    // reports every tag, which is noise a caller learns to ignore.
    const good = await render('<color=#ff0000>abc</color>')
    expect(good.found).toHaveLength(0)
  })
})
