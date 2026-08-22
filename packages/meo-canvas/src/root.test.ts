import { createRequire } from 'node:module'

import { describe, expect, it } from 'vitest'

import type { SideValue } from './arena.js'
import type { NativeCanvas } from './canvas.js'
import { Box, Text } from './node.js'
import { Root, type PageInfo, type RootDependencies, type RootProps } from './root.js'

/**
 * Slot index of the page count: magic, version, three geometry floats, and the
 * three discriminants of the surface block.
 *
 * Named rather than written at each use. The header has changed twice and each
 * time the failure was five assertions reading one slot too early, which reads
 * as five bugs rather than as one moved field.
 */
const PAGE_COUNT = 2 + 3 + 3

/** A renderer that records what it was handed and paints nothing. */
function fakeRenderer() {
  const painted: { slots: Float64Array; values: readonly SideValue[]; fonts: unknown }[] = []
  const native: NativeCanvas = {
    encode: () => new Uint8Array([1]),
    release: () => undefined,
  }
  const dependencies: RootDependencies = {
    renderer: {
      paint: (slots, values, options) => {
        painted.push({ slots, values, fonts: options.fonts })
        return native
      },
    },
    writeFile: async () => undefined,
    writeFileSync: () => undefined,
  }
  return { dependencies, painted }
}

/** The arena `Root` handed across for `props`. */
async function arenaFor(props: RootProps): Promise<{ slots: Float64Array; values: readonly SideValue[] }> {
  const { dependencies, painted } = fakeRenderer()
  await Root(props, dependencies)

  const only = painted[0]
  if (only === undefined) throw new Error('Root painted nothing')
  return only
}

describe('the canvas Root describes', () => {
  it('carries the size and the scale it was given', async () => {
    const { slots } = await arenaFor({ width: 520, height: 180, scale: 2 })

    // Magic, version, width, height, scale, then the three surface
    // discriminants, then the page count. `PAGE_COUNT` names the last of those
    // so a header change moves one constant rather than every index here.
    expect([...slots.slice(2, 5)]).toEqual([520, 180, 2])
    expect(slots[PAGE_COUNT]).toBe(1)
  })

  it('defaults the scale to one', async () => {
    const { slots } = await arenaFor({ width: 10, height: 10 })

    expect(slots[4]).toBe(1)
  })

  it('says nothing about the surface unless the caller does', async () => {
    // Three absent flags, then the page count. "The caller said nothing" is a
    // distinct thing from "the caller said true": the renderer decides when
    // nothing was said, and a default written here would take that decision
    // away from it silently.
    const { slots } = await arenaFor({ width: 10, height: 10 })

    expect([...slots.slice(5, 8)]).toEqual([0, 0, 0])
    expect(slots[PAGE_COUNT]).toBe(1)
  })

  it('carries the surface options in the arena, not beside it', async () => {
    // Where this has to be asserted, and why: `gpu` used to travel in the
    // paint options object and the addon stopped reading it there. A test
    // against the fake renderer stayed green while the flag reached nothing.
    const { slots } = await arenaFor({
      width: 10,
      height: 10,
      gpu: false,
      colorType: 'RGBAF32',
      colorSpace: 'display-p3',
    })

    // Present flag, value, three times over — `gpu` false, `'RGBAF32'` which is
    // the scene's `F32` at 2, and `'display-p3'` which is `DisplayP3`, also 2.
    expect([...slots.slice(5, 11)]).toEqual([1, 0, 1, 2, 1, 2])
  })

  it('passes the families through to be registered', async () => {
    const fonts = [{ family: 'Fixture', paths: ['a.ttf'] }]
    const { dependencies, painted } = fakeRenderer()

    await Root({ width: 10, height: 10, fonts }, dependencies)

    expect(painted[0]?.fonts).toBe(fonts)
  })
})

describe('a sequence', () => {
  it('is one page when nothing says otherwise', async () => {
    const { slots } = await arenaFor({ width: 10, height: 10, children: Text('x') })

    expect(slots[PAGE_COUNT]).toBe(1)
  })

  it('takes its length as a page count', async () => {
    const { slots } = await arenaFor({ width: 10, height: 10, pages: 3, children: () => Text('x') })

    expect(slots[PAGE_COUNT]).toBe(3)
  })

  it('derives the page count from a duration and a rate', async () => {
    // `ceil(duration * fps)`, as v1 derives it: a second at thirty is thirty
    // pages, and a fraction of a page is still a page that has to be drawn.
    const { slots } = await arenaFor({ width: 10, height: 10, duration: 1, children: () => Text('x') })
    const rounded = await arenaFor({ width: 10, height: 10, duration: 0.1, fps: 24, children: () => Text('x') })

    expect(slots[PAGE_COUNT]).toBe(30)
    expect(rounded.slots[PAGE_COUNT]).toBe(3)
  })

  it('tells each page where it sits', async () => {
    const seen: PageInfo[] = []
    const { dependencies } = fakeRenderer()

    await Root(
      {
        width: 10,
        height: 10,
        pages: 4,
        fps: 10,
        children: page => {
          seen.push(page)
          return Text('x')
        },
      },
      dependencies,
    )

    expect(seen.map(page => page.index)).toEqual([0, 1, 2, 3])
    expect(seen.map(page => page.count)).toEqual([4, 4, 4, 4])
    // `progress` reaches one on the last page; `cycle` never does, because the
    // page after the last is the next loop's first.
    expect(seen.map(page => page.progress)).toEqual([0, 1 / 3, 2 / 3, 1])
    expect(seen.map(page => page.cycle)).toEqual([0, 0.25, 0.5, 0.75])
    expect(seen.map(page => page.time)).toEqual([0, 0.1, 0.2, 0.3])
  })

  it('reports both curves as zero for a page that is the whole render', async () => {
    const seen: PageInfo[] = []
    const { dependencies } = fakeRenderer()

    await Root({ width: 10, height: 10, pages: 1, children: page => (seen.push(page), Text('x')) }, dependencies)

    expect(seen[0]).toEqual({ index: 0, count: 1, progress: 0, cycle: 0, time: 0 })
  })

  it('builds a page at a time rather than all at once', async () => {
    // A builder may fetch, and a thousand-page render firing a thousand
    // requests at once is a denial of service the caller did not ask for.
    const order: string[] = []
    const { dependencies } = fakeRenderer()

    await Root(
      {
        width: 10,
        height: 10,
        pages: 3,
        children: async page => {
          order.push(`start ${page.index}`)
          await Promise.resolve()
          order.push(`end ${page.index}`)
          return Text('x')
        },
      },
      dependencies,
    )

    expect(order).toEqual(['start 0', 'end 0', 'start 1', 'end 1', 'start 2', 'end 2'])
  })
})

describe('a sequence that contradicts itself', () => {
  it('is refused rather than resolved by precedence', async () => {
    const { dependencies } = fakeRenderer()
    const builder = (): ReturnType<typeof Text> => Text('x')

    await expect(Root({ width: 10, height: 10, pages: 2, duration: 1, children: builder }, dependencies)).rejects.toThrow(/`pages` or `duration`, not both/)
    await expect(Root({ width: 10, height: 10, children: builder }, dependencies)).rejects.toThrow(/page builder needs `pages` or `duration`/)
    await expect(Root({ width: 10, height: 10, pages: 2, children: Text('x') }, dependencies)).rejects.toThrow(
      /`children` has to be a function that builds one/,
    )
    await expect(Root({ width: 10, height: 10, pages: 0, children: builder }, dependencies)).rejects.toThrow(/at least one page/)
    await expect(Root({ width: 10, height: 10, pages: 1.5, children: builder }, dependencies)).rejects.toThrow(/is not a count/)
  })
})

describe('the renderer Root reaches for when told nothing', () => {
  // Against the real addon, and not skipped when it is absent: a default that
  // is never exercised is a default nobody has checked. Run `just addon`.
  it('is the addon, and the canvas it returns encodes', async () => {
    const canvas = await Root({ width: 8, height: 4, gpu: false, backgroundColor: '#101014' })

    const png = await canvas.png
    // The eight-byte PNG signature, which says the bytes came from an encoder
    // rather than from a stub that returned something.
    expect([...png.slice(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])

    canvas.release()
    expect(canvas.released).toBe(true)
  })

  it('draws different pixels on the two rasterisers', async () => {
    // The check a fake renderer cannot satisfy, and the one that would have
    // caught this: `gpu` travelled in the paint options object, the addon
    // stopped reading it there, and the unit test asserting the flag had been
    // copied from one object to another stayed green while it reached nothing.
    //
    // Two real renders that must differ cannot pass by copying a field. When no
    // GPU backend is compiled in they are both the CPU and this says so rather
    // than passing for the wrong reason.
    //
    // **A rounded box, and the curve is the whole point.** The two rasterisers
    // resolve anti-aliased edges a level or two apart and agree exactly on a
    // picture that has none, so the scene has to contain a curve for this to
    // mean anything. A curve always does; text does not reliably.
    //
    // Measured, on this scene at 200×80: text differs at `fontSize: 23` and
    // `24` and is **byte-identical at 16, 20, 22, 28, 32 and 48** — a narrow
    // window rather than a threshold, which is why text is the wrong choice
    // however large it is made. A rounded box differs at every radius from 8 to
    // 30 and at every width tried. A square box agrees, as it should.
    //
    // A scene without a curve makes this fail rather than pass quietly, which
    // is the right way round — but it fails for a reason that has nothing to do
    // with the GPU, so change the scene knowing that.
    const of = async (gpu: boolean): Promise<Uint8Array> => {
      const canvas = await Root({
        width: 200,
        height: 80,
        gpu,
        children: Box({ width: 120, height: 60, borderRadius: 24, backgroundColor: '#ffffff' }),
      })
      const bytes = canvas.toBufferSync('png')
      canvas.release()
      return bytes
    }

    const [on, off] = [await of(true), await of(false)]
    const native = createRequire(import.meta.url)('../meo-canvas.node') as { backend(): { active: string } }

    if (native.backend().active === 'cpu') expect(on).toEqual(off)
    else expect(on).not.toEqual(off)
  })

  it('paints once, however many formats are asked for', async () => {
    // The claim the retained surface exists for. Two encodes, one paint — and
    // the second returning different bytes for a different container is what
    // says the surface was encoded twice rather than cached once.
    const canvas = await Root({ width: 8, height: 4, gpu: false })

    const [png, webp] = [canvas.toBufferSync('png'), canvas.toBufferSync('webp')]

    expect([...png.slice(0, 4)]).toEqual([0x89, 0x50, 0x4e, 0x47])
    expect([...webp.slice(0, 4)]).toEqual([0x52, 0x49, 0x46, 0x46])
    canvas.release()
  })
})
