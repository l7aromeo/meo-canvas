import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { integrationRootBase } from './helpers/integration-font.js'

/**
 * Which descendants `overflow: hidden` reaches.
 *
 * It clips a node's content, and an absolutely positioned node is not its content unless it is
 * also its containing block. CSS clips such a node only where the clipper is that containing
 * block, or lies between it and one — so a static box clips its in-flow children and lets an
 * absolute one through. Each expectation below is what Chrome rendered for the equivalent markup.
 *
 * The child under test sits at x=120, entirely outside the 100-wide clipper, so it is visible only
 * if the clip did not reach it and the clipper's own background cannot hide it. That last part
 * matters: an earlier version of this harness put the child on top of the clipper, where nothing
 * could be concluded either way.
 */
const PAGE = 300
const CLIP = 100
const OUTSIDE = 150

/** Red is (221,17,17) and the page is white — both have a high red channel, so read all three. */
const isRed = (data: Uint8ClampedArray) => data[0] > 150 && data[1] < 100 && data[2] < 100

async function reaches(clipper: Partial<BoxProps>, child: Partial<BoxProps>) {
  const canvas = await Root({
    ...integrationRootBase,
    width: PAGE,
    height: 40,
    workerMode: false,
    gpu: false,
    backgroundColor: '#ffffff',
    children: [
      Box({
        width: PAGE,
        height: 40,
        positionType: Style.PositionType.Relative,
        children: [
          Box({
            width: CLIP,
            height: 40,
            overflow: Style.Overflow.Hidden,
            ...clipper,
            children: [Box({ width: 60, height: 40, flexShrink: 0, backgroundColor: '#dd1111', ...child })],
          }),
        ],
      }),
    ],
  })
  return isRed(canvas.getContext('2d').getImageData(OUTSIDE, 20, 1, 1).data) ? 'escapes' : 'clipped'
}

const ABSOLUTE: Partial<BoxProps> = { positionType: Style.PositionType.Absolute, position: { Top: 0, Left: 120 } }
const IN_FLOW: Partial<BoxProps> = { margin: { Left: 120 } }

describe('overflow against an absolute child', () => {
  it('lets one through a static clipper, which is not its containing block', async () => {
    expect(await reaches({}, ABSOLUTE)).toBe('escapes')
  })

  it('lets one through a clipper that names Static explicitly', async () => {
    expect(await reaches({ positionType: Style.PositionType.Static }, ABSOLUTE)).toBe('escapes')
  })

  it('clips one inside a relative clipper, which is its containing block', async () => {
    expect(await reaches({ positionType: Style.PositionType.Relative }, ABSOLUTE)).toBe('clipped')
  })

  it('clips one inside an absolute clipper', async () => {
    expect(await reaches({ positionType: Style.PositionType.Absolute, position: { Top: 0, Left: 0 } }, ABSOLUTE)).toBe('clipped')
  })
})

describe('overflow against an in-flow child', () => {
  it('clips it inside a static clipper', async () => {
    expect(await reaches({}, IN_FLOW)).toBe('clipped')
  })

  it('clips it inside a relative clipper', async () => {
    expect(await reaches({ positionType: Style.PositionType.Relative }, IN_FLOW)).toBe('clipped')
  })

  it('clips it even when it names a zIndex that lifts it', async () => {
    expect(await reaches({ positionType: Style.PositionType.Relative }, { ...IN_FLOW, zIndex: 5 })).toBe('clipped')
  })
})
