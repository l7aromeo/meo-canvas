import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps, CanvasElement } from '@/canvas/canvas.type.js'
import { integrationRootBase, integrationFontFamily } from './helpers/integration-font.js'

const CANVAS = 200
const BOX = { left: 70, top: 70, size: 60 }

/**
 * A node carrying `filter` or `mixBlendMode` is drawn into an offscreen so the effect applies to
 * its subtree as one picture. That offscreen is an implementation detail and must not be
 * observable: CSS clips a subtree only under `overflow: hidden`, and Chrome renders each case below
 * identically with and without the effect.
 *
 * Counted outside the node's own box, which is exactly what an offscreen cut to the box would lose.
 */
async function inkOutsideBox(children: CanvasElement[], group: Partial<BoxProps> = {}) {
  const canvas = await Root({
    ...integrationRootBase,
    width: CANVAS,
    height: CANVAS,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [
      Box({
        width: CANVAS,
        height: CANVAS,
        children: [
          Box({
            positionType: Style.PositionType.Absolute,
            position: { Top: BOX.top, Left: BOX.left },
            width: BOX.size,
            height: BOX.size,
            ...group,
            children,
          }),
        ],
      }),
    ],
  })

  const { data } = canvas.getContext('2d').getImageData(0, 0, CANVAS, CANVAS)
  let outside = 0
  for (let y = 0; y < CANVAS; y++) {
    for (let x = 0; x < CANVAS; x++) {
      const inside = x >= BOX.left && x < BOX.left + BOX.size && y >= BOX.top && y < BOX.top + BOX.size
      if (inside) continue
      const i = (y * CANVAS + x) * 4
      if (data[i] < 240 || data[i + 1] < 240 || data[i + 2] < 240) outside++
    }
  }
  return outside
}

/** Each group effect reaches the offscreen through its own branch, so each is worth checking. */
const GROUPS: Array<[string, Partial<BoxProps>]> = [
  ['filter', { filter: 'saturate(1.5)' }],
  ['backdropFilter', { backdropFilter: 'blur(10px)' }],
  ['mixBlendMode', { mixBlendMode: Style.BlendMode.Multiply }],
]

const OVERFLOWING: Array<[string, () => CanvasElement[]]> = [
  ['a child translated outside', () => [Box({ width: 40, height: 40, backgroundColor: '#0066cc', transform: { translateX: 60 } })]],
  ['a child rotated past the corner', () => [Box({ width: 56, height: 56, backgroundColor: '#0066cc', transform: { rotate: 45 } })]],
  ['a child pulled out by a negative margin', () => [Box({ width: 40, height: 40, margin: { Left: -30, Top: -30 }, backgroundColor: '#0066cc' })]],
  [
    'a child casting its own shadow',
    () => [Box({ width: 40, height: 40, margin: 10, backgroundColor: '#0066cc', boxShadow: { offsetY: 10, blur: 14, color: 'rgba(0,0,0,0.9)' } })],
  ],
  ['a child with its own filter', () => [Box({ width: 40, height: 40, backgroundColor: '#0066cc', filter: 'blur(6px)' })]],
  ['text too long for the box', () => [Text('overflowing text well past the box', { fontFamily: integrationFontFamily, fontSize: 16, color: '#000000' })]],
]

describe('a grouped node does not clip what it draws', () => {
  describe.each(OVERFLOWING)('%s', (_label, make) => {
    it.each(GROUPS)('survives %s', async (_effect, group) => {
      const plain = await inkOutsideBox(make())
      const grouped = await inkOutsideBox(make(), group)

      expect(plain).toBeGreaterThan(0)
      expect(grouped).toBeGreaterThanOrEqual(plain * 0.9)
    })
  })

  it('keeps a node’s own shadow soft rather than cutting it at the box', async () => {
    // The reported case: the shadow became a hard-edged rectangle ending at the node's box. A cut
    // shadow still has ink outside — the offset carries it there — so counting pixels is not
    // enough; this reads the falloff below the box and requires it to fade rather than stop.
    const shadow = { offsetY: 8, blur: 12, color: 'rgba(0,0,0,0.9)' }
    const column = async (group: Partial<BoxProps>) => {
      const canvas = await Root({
        ...integrationRootBase,
        width: 120,
        height: 160,
        workerMode: false,
        gpu: false,
        backgroundColor: '#ffffff',
        children: [
          Box({
            width: 120,
            height: 160,
            children: [Box({ width: 60, height: 60, margin: 30, borderRadius: 9999, backgroundColor: '#E7000B', boxShadow: shadow, ...group })],
          }),
        ],
      })
      const { data } = canvas.getContext('2d').getImageData(60, 0, 1, 160)
      return Array.from({ length: 160 }, (_, y) => data[y * 4])
    }

    const plain = await column({})
    const filtered = await column({ filter: 'saturate(1.5)' })

    // Down the middle, past the circle: both must darken and then return to white gradually.
    const tail = (values: number[]) => values.slice(95, 140)
    const darkest = (values: number[]) => Math.min(...tail(values))
    const steps = (values: number[]) => tail(values).filter((v, i, all) => i > 0 && Math.abs(v - all[i - 1]) > 60).length

    expect(darkest(filtered)).toBeLessThan(250)
    expect(Math.abs(darkest(filtered) - darkest(plain))).toBeLessThanOrEqual(12)
    // A shadow cut at the box edge leaves a step in the profile; a faded one does not.
    expect(steps(filtered)).toBe(0)
  })

  it('does not grow the offscreen for an inset shadow', async () => {
    // Inset shadows draw within the box, so they must not pad a group — otherwise every filtered
    // node with one pays for pixels nothing reaches.
    const inset = { inset: true, offsetY: 8, blur: 12, color: 'rgba(0,0,0,0.9)' }
    const outside = await inkOutsideBox([Box({ width: 60, height: 60, backgroundColor: '#E7000B', boxShadow: inset })], { filter: 'saturate(1.5)' })

    expect(outside).toBe(0)
  })

  it('takes its padding from the largest shadow, not the first', async () => {
    const shadows = [
      { offsetY: 1, blur: 1, color: 'rgba(0,0,0,0.9)' },
      { offsetY: 24, blur: 16, color: 'rgba(0,0,0,0.9)' },
    ]
    const outside = await inkOutsideBox([Box({ width: 40, height: 40, margin: 10, backgroundColor: '#E7000B', boxShadow: shadows })], {
      filter: 'saturate(1.5)',
    })
    const plain = await inkOutsideBox([Box({ width: 40, height: 40, margin: 10, backgroundColor: '#E7000B', boxShadow: shadows })])

    expect(outside).toBeGreaterThanOrEqual(plain * 0.9)
  })
})
