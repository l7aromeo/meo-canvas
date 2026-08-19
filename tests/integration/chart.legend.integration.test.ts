import { Root } from '@/canvas/root.canvas.js'
import { Box, Row } from '@/canvas/layout.canvas.js'
import { Chart } from '@/canvas/chart.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { Style } from '@/constant/common.const.js'
import { integrationFontFamily, integrationRootBase } from './helpers/integration-font.js'

const WIDTH = 400
const HEIGHT = 300

/** A colour no slice uses, so finding it proves the custom item drew rather than the built-in one. */
const MARKER = { r: 0, g: 255, b: 0 }

const render = (custom: boolean) =>
  Root({
    ...integrationRootBase,
    width: WIDTH,
    height: HEIGHT,
    workerMode: false,
    gpu: false,
    children: [
      Chart({
        type: 'doughnut',
        width: '100%',
        height: '100%',
        fontFamily: integrationFontFamily,
        data: [
          { label: 'Red', value: 300, color: '#FF6384' },
          { label: 'Blue', value: 50, color: '#36A2EB' },
        ],
        options: {
          innerRadius: 0.7,
          ...(custom && {
            // Exactly the shape the README documents: factories, which return descriptors.
            renderLegendItem: ({ item }: { item: { label: string; value: number } }) =>
              Row({
                alignItems: Style.Align.Center,
                children: [
                  Box({ width: 12, height: 12, backgroundColor: '#00ff00' }),
                  Text(`${item.label}: ${item.value}`, { fontSize: 14, fontFamily: integrationFontFamily, margin: { Left: 8 } }),
                ],
              }),
          }),
        },
      }),
    ],
  })

const countMarker = (canvas: Awaited<ReturnType<typeof render>>) => {
  const { data } = canvas.getContext('2d').getImageData(0, 0, WIDTH, HEIGHT)
  let found = 0
  for (let i = 0; i < data.length; i += 4) {
    if (data[i] === MARKER.r && data[i + 1] === MARKER.g && data[i + 2] === MARKER.b) found++
  }
  return found
}

describe('custom chart legend', () => {
  it('draws items returned as descriptors, which is the only form a caller can build', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})

    const custom = await render(true)
    const marker = countMarker(custom)

    // The tree used to reject a descriptor and warn, leaving the legend silently absent.
    expect(warn).not.toHaveBeenCalled()
    warn.mockRestore()

    // Two swatches at 12x12, less any anti-aliased edge pixels that land off the exact colour.
    expect(marker).toBeGreaterThan(2 * 12 * 12 * 0.5)
  })

  it('draws the built-in legend when no item callback is given', async () => {
    // The marker colour belongs to the custom item alone, so the default legend must not show it.
    expect(countMarker(await render(false))).toBe(0)
  })
})
