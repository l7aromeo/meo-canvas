import { vi } from 'vitest'
import { paintBackgroundImage } from '@/canvas/background.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { CanvasRenderingContext2D, Image as CanvasImage } from 'meo-skia-canvas'

/** A 30x30 picture, the size the placement arithmetic is easiest to read against. */
const IMAGE = { width: 30, height: 30 } as CanvasImage

const BOX = { x: 0, y: 0, width: 100, height: 50 }
const NO_RADII = { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 }

/** Captures where each tile was drawn, which is the whole of what this module decides. */
function record() {
  const drawn: Array<[number, number, number, number]> = []
  const ctx = {
    save: vi.fn(),
    restore: vi.fn(),
    clip: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    arcTo: vi.fn(),
    closePath: vi.fn(),
    rect: vi.fn(),
    drawImage: vi.fn((_image: unknown, x: number, y: number, w: number, h: number) => {
      drawn.push([x, y, w, h])
    }),
  } as unknown as CanvasRenderingContext2D

  return { ctx, drawn }
}

const round = (tiles: Array<[number, number, number, number]>) => tiles.map(t => t.map(v => Math.round(v * 10) / 10))

describe('paintBackgroundImage', () => {
  it('covers the box and past its edges when tiling both ways', () => {
    const { ctx, drawn } = record()

    paintBackgroundImage(ctx, IMAGE, { src: 'x', size: 30 }, BOX, NO_RADII)

    // Four columns across 100px and two rows down 50, the last of each running past the edge —
    // the clip cuts them, which is what a browser draws.
    expect(drawn.filter(([, y]) => y === 0).map(([x]) => x)).toEqual([0, 30, 60, 90])
    expect([...new Set(drawn.map(([, y]) => y))]).toEqual([0, 30])
  })

  it('lays one line of tiles for repeat-x and repeat-y', () => {
    const across = record()
    paintBackgroundImage(across.ctx, IMAGE, { src: 'x', size: 30, repeat: Style.BackgroundRepeat.RepeatX }, BOX, NO_RADII)

    const down = record()
    paintBackgroundImage(down.ctx, IMAGE, { src: 'x', size: 30, repeat: Style.BackgroundRepeat.RepeatY }, BOX, NO_RADII)

    expect([...new Set(across.drawn.map(([, y]) => y))]).toEqual([0])
    expect([...new Set(down.drawn.map(([x]) => x))]).toEqual([0])
    expect(down.drawn.map(([, y]) => y)).toEqual([0, 30])
  })

  it('shares the slack out as equal gaps for space', () => {
    const { ctx, drawn } = record()

    paintBackgroundImage(ctx, IMAGE, { src: 'x', size: 30, repeat: Style.BackgroundRepeat.Space }, BOX, NO_RADII)

    // Three 30px tiles in 100px leave 10px, so two 5px gaps: 0, 35, 70 — first and last flush.
    // Only one row fits in 50px, so the vertical axis places a single tile rather than spacing.
    expect(round(drawn)).toEqual([
      [0, 0, 30, 30],
      [35, 0, 30, 30],
      [70, 0, 30, 30],
    ])
  })

  it('stretches tiles to a whole number for round', () => {
    const { ctx, drawn } = record()

    paintBackgroundImage(ctx, IMAGE, { src: 'x', size: 30, repeat: Style.BackgroundRepeat.Round }, BOX, NO_RADII)

    // 100/30 rounds to three columns of 33.3, 50/30 to two rows of 25 — nothing clipped, both
    // edges reached, and the tile no longer square.
    expect(round(drawn)).toEqual([
      [0, 0, 33.3, 25],
      [0, 25, 33.3, 25],
      [33.3, 0, 33.3, 25],
      [33.3, 25, 33.3, 25],
      [66.7, 0, 33.3, 25],
      [66.7, 25, 33.3, 25],
    ])
  })

  it('takes the second edge from the picture when given one length', () => {
    const { ctx, drawn } = record()

    paintBackgroundImage(ctx, IMAGE, { src: 'x', size: 20, repeat: Style.BackgroundRepeat.NoRepeat }, BOX, NO_RADII)

    expect(drawn).toEqual([[0, 0, 20, 20]])
  })

  it('scales to the box for cover and contain', () => {
    const cover = record()
    paintBackgroundImage(cover.ctx, IMAGE, { src: 'x', size: 'cover', repeat: Style.BackgroundRepeat.NoRepeat }, BOX, NO_RADII)

    const contain = record()
    paintBackgroundImage(contain.ctx, IMAGE, { src: 'x', size: 'contain', repeat: Style.BackgroundRepeat.NoRepeat }, BOX, NO_RADII)

    // The picture is square: covering a 100x50 box means 100x100, containing it means 50x50.
    expect(cover.drawn).toEqual([[0, 0, 100, 100]])
    expect(contain.drawn).toEqual([[0, 0, 50, 50]])
  })

  it('reads a percentage position as a share of the slack, as CSS does', () => {
    const { ctx, drawn } = record()

    paintBackgroundImage(ctx, IMAGE, { src: 'x', size: 30, repeat: Style.BackgroundRepeat.NoRepeat, position: { x: '100%', y: '100%' } }, BOX, NO_RADII)

    // Not 100px along, which would put the tile outside the box — the far edges line up: the box
    // is 100 wide and 50 tall, the tile 30, so 70 and 20.
    expect(drawn).toEqual([[70, 20, 30, 30]])
  })

  it('draws nothing for an empty box or a sizeless picture', () => {
    const emptyBox = record()
    paintBackgroundImage(emptyBox.ctx, IMAGE, { src: 'x' }, { x: 0, y: 0, width: 0, height: 50 }, NO_RADII)

    const emptyImage = record()
    paintBackgroundImage(emptyImage.ctx, { width: 0, height: 0 } as CanvasImage, { src: 'x' }, BOX, NO_RADII)

    expect(emptyBox.drawn).toEqual([])
    expect(emptyImage.drawn).toEqual([])
  })
})
