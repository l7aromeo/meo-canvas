import { Root } from '@/canvas/root.canvas.js'
import { Box, Column } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * A page of backdrop-filtered pills, the shape a card of stat panels takes.
 *
 * Each one copies what is already on the canvas. A CPU surface used to keep such a copy as a
 * picture rather than pixels, so a backdrop taken after another replayed that one's work too and
 * the cost doubled with every node: 8 nodes 0.27s, 11 nodes 1.87s, 16 unfinished after five
 * minutes, while the GPU surface stayed flat. Fixed in meo-skia-canvas 5.6.4.
 *
 * The guard stays because the failure is invisible on a GPU surface, which is not where renders
 * run — a container has no GPU, and that is exactly where a card of stat panels would hang.
 */
async function renderPills(nodes: number) {
  const canvas = await Root({
    ...integrationRootBase,
    gpu: false,
    scale: 2,
    width: 903,
    height: 680,
    workerMode: false,
    backgroundColor: '#774422',
    children: [
      Box({
        width: 903,
        height: 680,
        backgroundColor: '#774422',
        children: [
          Column({
            children: Array.from({ length: nodes }, () =>
              Box({ width: 200, height: 20, margin: 2, backgroundColor: 'rgba(0,0,0,0.5)', backdropFilter: 'blur(5px)' }),
            ),
          }),
        ],
      }),
    ],
  })
  return canvas
}

describe('backdropFilter cost', () => {
  it('grows with the number of backdrops, not exponentially in it', async () => {
    // Deliberately loose: the point is to separate linear from doubling, which are orders of
    // magnitude apart by 16 nodes, not to pin a number that a slower machine would fail.
    const time = async (nodes: number) => {
      const started = performance.now()
      const canvas = await renderPills(nodes)
      canvas.toBufferSync('png')
      return performance.now() - started
    }

    const four = await time(4)
    const sixteen = await time(16)

    // Doubling per node puts sixteen at roughly 4096x four. Linear puts it near 4x.
    expect(sixteen).toBeLessThan(Math.max(four * 20, 5000))
  }, 120_000)

  it('finishes a page of thirty-two backdrops', async () => {
    // The reported production case: four cards of eight panels each, which never completed.
    const started = performance.now()
    const canvas = await renderPills(32)
    canvas.toBufferSync('png')

    expect(performance.now() - started).toBeLessThan(30_000)
  }, 120_000)

  it('still filters what an earlier backdrop wrote', async () => {
    // Two panels overlapping: the second's backdrop includes the first's own result, so tinting the
    // first red has to show through the second. Measured at the overlap — rgb(238,92,92) with the
    // red panel beneath, rgb(208,208,208) without it.
    const overlap = async (withFirst: boolean) => {
      const canvas = await Root({
        ...integrationRootBase,
        gpu: false,
        width: 120,
        height: 120,
        workerMode: false,
        backgroundColor: '#ffffff',
        children: [
          Box({
            width: 120,
            height: 120,
            children: [
              Box({ positionType: Style.PositionType.Absolute, position: { Top: 0, Left: 0 }, width: 60, height: 120, backgroundColor: '#000000' }),
              ...(withFirst
                ? [
                    Box({
                      positionType: Style.PositionType.Absolute,
                      position: { Top: 0, Left: 40 },
                      width: 60,
                      height: 30,
                      backdropFilter: 'blur(6px)',
                      backgroundColor: 'rgba(255,0,0,0.85)',
                    }),
                  ]
                : []),
              Box({
                positionType: Style.PositionType.Absolute,
                position: { Top: 20, Left: 40 },
                width: 60,
                height: 30,
                backdropFilter: 'blur(6px)',
                backgroundColor: 'rgba(255,255,255,0.15)',
              }),
            ],
          }),
        ],
      })
      const { data } = canvas.getContext('2d').getImageData(64, 25, 1, 1)
      return [data[0], data[1], data[2]] as [number, number, number]
    }

    const alone = await overlap(false)
    const overRed = await overlap(true)

    // Grey on its own; red once there is a red panel beneath for it to filter.
    expect(Math.abs(alone[0] - alone[2])).toBeLessThanOrEqual(4)
    expect(overRed[0] - overRed[2]).toBeGreaterThan(80)
  })
})
