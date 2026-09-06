import { describe, expect, it } from 'vitest'

import { Box, Text } from './node.js'
import { Root } from './root.js'

/**
 * A failure names the property the caller wrote.
 *
 * These go through a real render rather than asserting a Rust `Display`
 * directly, and that is the whole point of them. The Rust-side unit tests
 * prove the message *renders* a property once it has one; they cannot prove
 * the decoder was handed the right name, because they supply the name
 * themselves. Only a decode reads the table.
 *
 * Measured: a unit test asserting the rendered form passes unchanged when the
 * table is mutated to `borderColorAll`, which is the defect these exist for.
 */
describe('a value the renderer cannot read', () => {
  const message = async (build: () => Awaited<ReturnType<typeof Root>> | ReturnType<typeof Root>) => {
    try {
      const canvas = await build()
      await canvas.toBuffer('raw')
      canvas.release()
      return 'no error'
    } catch (error) {
      return String(error).split('\n')[0]
    }
  }

  it('names the property a caller wrote, not the field the scene stores', async () => {
    // `borderColor` is stored as `border_color_all`, and a name derived from
    // the field would send a caller looking for a property that does not
    // exist on the surface.
    const text = await message(() => Root({ width: 8, height: 8, children: Box({ border: 2, borderStyle: 'solid', borderColor: 'potato' }) }))

    expect(text).toContain('borderColor is "potato"')
    expect(text).not.toContain('borderColorAll')
    expect(text).not.toContain('slot')
  })

  it('says the same thing wherever the node sits', async () => {
    // The slot index moved with the rest of the scene, so the same mistake in
    // two trees produced two different sentences and neither could be searched
    // for.
    const shallow = await message(() => Root({ width: 8, height: 8, children: Box({ backgroundColor: 'potato' }) }))
    const nested = await message(() => Root({ width: 8, height: 8, children: Box({ children: Box({ backgroundColor: 'potato' }) }) }))

    expect(shallow).toContain('backgroundColor is "potato"')
    expect(nested).toEqual(shallow)
  })

  it('names a text colour as `color`', async () => {
    const text = await message(() => Root({ width: 8, height: 8, children: Text('x', { color: 'potato' }) }))

    expect(text).toContain('color is "potato"')
  })
})
