import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { join } from 'node:path'
import { integrationRootBase } from './helpers/integration-font.js'

/** A 30x30 tile: a red frame three pixels thick around a white middle. */
const TILE = join(process.cwd(), 'tests/fixtures/images/tile-30.svg')

const W = 100
const H = 50

/**
 * A 30px tile in a 100x50 box leaves 10px over across and 20 down, which is what separates the
 * repeat modes. Chrome, given the same tile and box:
 *
 *   repeat     four columns, the last clipped; two rows, the second clipped
 *   repeat-x   one row across
 *   repeat-y   one column down
 *   no-repeat  one tile at the origin
 *   space      three whole tiles, first and last flush to the edges, 5px gaps between
 *   round      three columns and two rows, each tile stretched to fill exactly
 */
async function render(background: NonNullable<BoxProps['backgroundImage']>) {
  const canvas = await Root({
    ...integrationRootBase,
    width: W,
    height: H,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Box({ width: W, height: H, backgroundImage: background })],
  })

  const ctx = canvas.getContext('2d')
  return (x: number, y: number) => {
    const { data } = ctx.getImageData(x, y, 1, 1)
    return data[0] > 180 && data[1] < 120 && data[2] < 120 ? 'tile' : 'bare'
  }
}

describe('backgroundImage', () => {
  it('tiles both ways by default', async () => {
    const at = await render({ src: TILE, size: 30 })

    expect(at(1, 1)).toBe('tile')
    expect(at(31, 1)).toBe('tile')
    expect(at(61, 1)).toBe('tile')
    expect(at(1, 31)).toBe('tile')
  })

  it('tiles one way for repeat-x and repeat-y', async () => {
    const across = await render({ src: TILE, size: 30, repeat: Style.BackgroundRepeat.RepeatX })
    const down = await render({ src: TILE, size: 30, repeat: Style.BackgroundRepeat.RepeatY })

    expect(across(31, 1)).toBe('tile')
    expect(across(1, 31)).toBe('bare')

    expect(down(1, 31)).toBe('tile')
    expect(down(31, 1)).toBe('bare')
  })

  it('draws once for no-repeat', async () => {
    const at = await render({ src: TILE, size: 30, repeat: Style.BackgroundRepeat.NoRepeat })

    expect(at(1, 1)).toBe('tile')
    expect(at(31, 1)).toBe('bare')
    expect(at(1, 31)).toBe('bare')
  })

  it('spreads the slack between whole tiles for space', async () => {
    // Three 30px tiles in 100px leaves 10px, shared as two 5px gaps: tiles at 0, 35 and 70, the
    // first and last flush to the edges. A canvas pattern cannot express this, which is why the
    // tiling is done here rather than through one.
    const at = await render({ src: TILE, size: 30, repeat: Style.BackgroundRepeat.Space })

    expect(at(1, 1)).toBe('tile')
    expect(at(32, 1)).toBe('bare')
    expect(at(36, 1)).toBe('tile')
    expect(at(71, 1)).toBe('tile')
    expect(at(98, 1)).toBe('tile')
  })

  it('stretches the tile to a whole number for round', async () => {
    // 100/30 rounds to three columns of 33.3, and 50/30 to two rows of 25 — so the tiles reach both
    // edges with nothing clipped, and a second row starts at 25 rather than 30.
    const at = await render({ src: TILE, size: 30, repeat: Style.BackgroundRepeat.Round })

    expect(at(1, 1)).toBe('tile')
    expect(at(35, 1)).toBe('tile')
    expect(at(68, 1)).toBe('tile')
    expect(at(1, 26)).toBe('tile')
  })

  it('sizes a tile from one length, following the picture’s proportions', async () => {
    const at = await render({ src: TILE, size: 20, repeat: Style.BackgroundRepeat.NoRepeat })

    // The tile is square, so a width of 20 makes it 20 tall as well: its frame is ink, its middle
    // is not, and past its far corner there is nothing at all.
    expect(at(1, 1)).toBe('tile')
    expect(at(10, 10)).toBe('bare')
    expect(at(1, 18)).toBe('tile')
    expect(at(24, 24)).toBe('bare')
  })

  it('places the first tile where position says', async () => {
    const at = await render({ src: TILE, size: 30, repeat: Style.BackgroundRepeat.NoRepeat, position: { x: 40, y: 10 } })

    expect(at(1, 1)).toBe('bare')
    expect(at(41, 11)).toBe('tile')
  })

  it('reads a percentage position as CSS does, lining the picture up with the box', async () => {
    // `'100%'` puts the tile's right edge on the box's right edge — not 100px further along, which
    // would place it outside the box entirely.
    const at = await render({ src: TILE, size: 30, repeat: Style.BackgroundRepeat.NoRepeat, position: { x: '100%', y: 0 } })

    expect(at(98, 1)).toBe('tile')
    expect(at(1, 1)).toBe('bare')
  })

  it('covers and contains against the box', async () => {
    const cover = await render({ src: TILE, size: Style.BackgroundSize.Cover, repeat: Style.BackgroundRepeat.NoRepeat })
    const contain = await render({ src: TILE, size: Style.BackgroundSize.Contain, repeat: Style.BackgroundRepeat.NoRepeat })

    // The tile is square, so covering a wide box means matching its width and overflowing the
    // height; containing means matching the height and leaving space across.
    expect(cover(98, 25)).toBe('tile')
    expect(contain(98, 25)).toBe('bare')
    expect(contain(1, 48)).toBe('tile')
  })

  it('leaves the render standing when the picture cannot be loaded', async () => {
    const at = await render({ src: join(process.cwd(), 'tests/fixtures/images/not-here.png') })

    expect(at(1, 1)).toBe('bare')
  })
})
