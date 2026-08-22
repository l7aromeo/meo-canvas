import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { Style } from '@/constant/common.const.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * Where a border is painted under each `boxSizing`.
 *
 * `boxSizing` decides how `width` and `height` are resolved into a box: `BorderBox` counts the
 * padding and the border inside the number given, `ContentBox` adds them to it. Once that box
 * exists the border is painted inside it either way, which is what CSS does under both — the
 * `content-box` model changes the size of the border box, not the side of its edge the border sits
 * on.
 *
 * It used to be drawn outward under `ContentBox`, so the node covered a border width more of the
 * page than it had been laid out for and its own background painted over the inner edge of the ring.
 */
const PAGE = 300
const WIDTH = 100
const HEIGHT = 40
const PADDING = 10
const BORDER = 5

/** The runs of colour across one row of the page, as `[start, end, 'r,g,b']`. */
async function scanline(props: BoxProps) {
  const canvas = await Root({
    ...integrationRootBase,
    width: PAGE,
    height: 120,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [Box({ backgroundColor: '#cc2222', borderColor: '#000000', ...props })],
  })

  const { data } = canvas.getContext('2d').getImageData(0, HEIGHT / 2, PAGE, 1)
  const runs: { from: number; to: number; colour: string }[] = []
  for (let x = 0; x < PAGE; x++) {
    const colour = `${data[x * 4]},${data[x * 4 + 1]},${data[x * 4 + 2]}`
    const last = runs[runs.length - 1]
    if (last && last.colour === colour) last.to = x
    else runs.push({ from: x, to: x, colour })
  }
  return runs
}

const BLACK = '0,0,0'
const RED = '204,34,34'
const WHITE = '255,255,255'

describe('a border under each boxSizing', () => {
  it('paints inside a border-box node, which is the width it was given', async () => {
    const runs = await scanline({ width: WIDTH, height: HEIGHT, padding: PADDING, border: BORDER, boxSizing: Style.BoxSizing.BorderBox })

    expect(runs.map(run => run.colour)).toEqual([BLACK, RED, BLACK, WHITE])
    expect(runs[0]).toMatchObject({ from: 0, to: BORDER - 1 })
    expect(runs[2]).toMatchObject({ from: WIDTH - BORDER, to: WIDTH - 1 })
  })

  it('paints inside a content-box node, whose box grew by its padding and border', async () => {
    // The reported failure: the ring was drawn outside the box, so the left edge fell off the page
    // and the right edge sat a border width beyond where the node had been laid out.
    const total = WIDTH + PADDING * 2 + BORDER * 2
    const runs = await scanline({ width: WIDTH, height: HEIGHT, padding: PADDING, border: BORDER, boxSizing: Style.BoxSizing.ContentBox })

    expect(runs.map(run => run.colour)).toEqual([BLACK, RED, BLACK, WHITE])
    expect(runs[0]).toMatchObject({ from: 0, to: BORDER - 1 })
    expect(runs[2]).toMatchObject({ from: total - BORDER, to: total - 1 })
    expect(runs[3].from, 'the node reached past the box it was laid out in').toBe(total)
  })

  it('leaves the same border box whichever model sized it', async () => {
    const borderBox = await scanline({
      width: WIDTH + PADDING * 2 + BORDER * 2,
      height: HEIGHT,
      padding: PADDING,
      border: BORDER,
      boxSizing: Style.BoxSizing.BorderBox,
    })
    const contentBox = await scanline({ width: WIDTH, height: HEIGHT, padding: PADDING, border: BORDER, boxSizing: Style.BoxSizing.ContentBox })

    expect(borderBox).toEqual(contentBox)
  })
})
