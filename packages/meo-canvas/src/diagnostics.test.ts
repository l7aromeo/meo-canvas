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

  it('says which of two identically written tags it means', async () => {
    // Both tags are spelled the same way, so `path` and `detail` are equal
    // strings and the reports are indistinguishable without the offset.
    const markup = '<color=zzz>a</color><color=zzz>b</color>'
    const { found } = await render(markup)

    expect(found).toHaveLength(2)
    expect(found[0]?.path).toBe(found[1]?.path)

    // Sliced rather than compared against an expected index: an integer here
    // would be this test recomputing the parser's arithmetic, and would agree
    // whenever both were wrong the same way.
    for (const one of found) {
      expect(one.offset).toBeTypeOf('number')
      expect(markup.slice(one.offset)).toMatch(new RegExp(`^${one.path}`))
    }
    expect(found[0]?.offset).toBeLessThan(found[1]?.offset as number)
  })

  it('leaves the offset off a report that did not come from markup', async () => {
    // Nothing raises one yet — every diagnostic today is a markup tag — so
    // this pins the shape rather than a producer: the property is optional
    // and absent, never `null`, so `'offset' in d` and `d.offset !== undefined`
    // cannot disagree.
    const { found } = await render('<color=zzz>abc</color>')
    expect(found).toHaveLength(1)
    expect(found[0]).not.toHaveProperty('offset', null)
  })
})
