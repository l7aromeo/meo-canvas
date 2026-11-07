import { describe, expect, it } from 'vitest'

import { Root } from './root.js'
import { Box, Text } from './node.js'

/**
 * Markup a caller wrote that the renderer could not use, through a real render: the
 * report has to survive the parse, the arena and the Neon boundary.
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
    // A refused filter chain is the report that does not: the property is
    // absent, never `null`, so `'offset' in d` and `d.offset !== undefined`
    // cannot disagree.
    const canvas = await Root({ width: 8, height: 8, children: Box({ width: 4, height: 4, filter: 'blur(4 px)' }) })
    const found = canvas.diagnostics
    canvas.release()
    expect(found).toHaveLength(1)
    expect(found[0]).not.toHaveProperty('offset')
  })
})

/**
 * A filter chain the renderer refuses is dropped, as Chrome drops an invalid
 * declaration, and reported: the render goes on as though it were not set. The
 * chains are ones meo-skia-canvas 0.17 refuses as Chrome 153 does.
 */
describe('a filter the renderer refuses', () => {
  const REFUSED = ['blur(4 px)', 'blur(-1px)', '']

  /** A red box over a blue child at `zIndex: -1`, which a stacking context would show. */
  const draw = async (style: { filter?: string; backdropFilter?: string }) => {
    const canvas = await Root({
      width: 60,
      height: 40,
      display: 'block',
      backgroundColor: '#ffffff',
      gpu: false,
      children: Box({
        positionType: 'relative',
        width: 40,
        height: 30,
        backgroundColor: '#dc2828',
        ...style,
        children: Box({ positionType: 'absolute', position: { top: 5, left: 5 }, width: 20, height: 20, backgroundColor: '#2850dc', zIndex: -1 }),
      }),
    })
    const raw = Buffer.from(await canvas.toBuffer('raw'))
    const found = canvas.diagnostics
    canvas.release()
    return { raw, found }
  }

  it('is drawn as no filter is, and reported under its property', async () => {
    const plain = await draw({})
    expect(plain.found).toEqual([])
    for (const chain of REFUSED) {
      for (const [key, path] of [
        ['filter', 'filter'],
        ['backdropFilter', 'backdrop-filter'],
      ] as const) {
        const { raw, found } = await draw({ [key]: chain })
        expect(found.map(one => one.path)).toEqual([path])
        expect(found[0]?.detail).toContain(JSON.stringify(chain))
        expect(raw.equals(plain.raw)).toBe(true)
      }
    }
  })

  it('keeps a chain it draws', async () => {
    const plain = await draw({})
    const blurred = await draw({ filter: 'blur(2px)' })
    expect(blurred.found).toEqual([])
    expect(blurred.raw.equals(plain.raw)).toBe(false)
  })
})
