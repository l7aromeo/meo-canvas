import { createRequire } from 'node:module'

import { describe, expect, it } from 'vitest'

import type { SideValue } from './arena.js'
import type { NativeCanvas } from './canvas.js'
import { Box, Image, Text } from './node.js'
import type { ColorType } from './index.js'
import { Root, fetchDeadline, type PageInfo, type RootDependencies, type RootProps } from './root.js'

/**
 * Slot index of the page count: magic, version, three geometry floats, and the
 * three discriminants of the surface block, and the one slot `onImageError`
 * always occupies.
 *
 * Named rather than written at each use. The header has changed twice and each
 * time the failure was five assertions reading one slot too early, which reads
 * as five bugs rather than as one moved field.
 */
const PAGE_COUNT = 2 + 4 + 3 + 1 + 1

/** A renderer that records what it was handed and paints nothing. */
function fakeRenderer() {
  const painted: { slots: Float64Array; values: readonly SideValue[]; fonts: unknown }[] = []
  const native: NativeCanvas = {
    encode: () => Buffer.from([1]),
    encodeAsync: async () => Buffer.from([1]),
    write: () => undefined,
    writeAsync: async () => undefined,
    release: () => undefined,
    gpu: true,
    engine: 'cpu',
    pageCount: 1,
    scale: 1,
    warnings: [],
    diagnostics: [],
  }
  const dependencies: RootDependencies = {
    renderer: {
      paint: (slots, values, options) => {
        painted.push({ slots, values, fonts: options.fonts })
        return native
      },
    },
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

    // Magic, version, width, height, the content-height flag, scale, then the
    // three surface discriminants, then the page count. `PAGE_COUNT` names the
    // last of those so a header change moves one constant rather than every
    // index here.
    expect([...slots.slice(2, 6)]).toEqual([520, 180, 0, 2])
    expect(slots[PAGE_COUNT]).toBe(1)
  })

  it('defaults the scale to one', async () => {
    const { slots } = await arenaFor({ width: 10, height: 10 })

    expect(slots[5]).toBe(1)
  })

  it('asks for a content height when no height is given', async () => {
    // The flag and the floor are one answer, so both are read. A caller who
    // states neither gets "as tall as the content, and at least nothing".
    const { slots } = await arenaFor({ width: 520 })

    expect([...slots.slice(2, 6)]).toEqual([520, 0, 1, 1])
  })

  it('takes a stated minHeight as the floor of a content height', async () => {
    const { slots } = await arenaFor({ width: 520, minHeight: 90 })

    expect([...slots.slice(2, 6)]).toEqual([520, 90, 1, 1])
  })

  it('does not ask for a content height when a height is given', async () => {
    // The control. A stated height must not set the flag, or the height would
    // become a floor and a page would grow past what the caller asked for --
    // which is the one failure this pair exists to catch.
    const { slots } = await arenaFor({ width: 520, height: 180 })

    expect([...slots.slice(2, 6)]).toEqual([520, 180, 0, 1])
  })

  it('says nothing about the surface unless the caller does', async () => {
    // Three absent flags, then the page count. "The caller said nothing" is a
    // distinct thing from "the caller said true": the renderer decides when
    // nothing was said, and a default written here would take that decision
    // away from it silently.
    const { slots } = await arenaFor({ width: 10, height: 10 })

    expect([...slots.slice(6, 9)]).toEqual([0, 0, 0])
    expect(slots[PAGE_COUNT]).toBe(1)
  })

  it('carries the surface options in the arena, not beside it', async () => {
    // Asserted against the arena rather than against the paint options, because
    // the arena is where these three travel. A test against the fake renderer
    // can only check that a value was copied from one object to another, which
    // is true whether or not the addon reads it.
    const { slots } = await arenaFor({
      width: 10,
      height: 10,
      gpu: false,
      colorType: 'RGBAF32',
      colorSpace: 'display-p3',
    })

    // Present flag, value, three times over — `gpu` false, `'RGBAF32'` which is
    // the scene's `F32` at 2, and `'display-p3'` which is `DisplayP3`, also 2.
    expect([...slots.slice(6, 12)]).toEqual([1, 0, 1, 2, 1, 2])
  })

  it('passes the families through to be registered', async () => {
    const fonts = [{ family: 'Fixture', paths: ['a.ttf'] }]
    const { dependencies, painted } = fakeRenderer()

    await Root({ width: 10, height: 10, fonts }, dependencies)

    expect(painted[0]?.fonts).toBe(fonts)
  })
})

describe('a url source', () => {
  // **These pin `'throw'` rather than changing in substance.** They are about
  // what a fetch does — the status it reports, the signal it keeps, the body
  // it bounds — and the default is now `'placeholder'`, under which a failure
  // is a warning and the render finishes. That would leave every assertion
  // below with nothing to read.
  // **Every test here stubs `fetch`.** A test that dials out is a test that
  // fails on an aeroplane, and worse, one whose failure mode is a DNS error
  // dressed up as a renderer error — which is exactly what the two tests this
  // replaced did once the surface started fetching.
  const withFetch = (handler: typeof fetch) => {
    const real = globalThis.fetch
    globalThis.fetch = handler
    return () => {
      globalThis.fetch = real
    }
  }

  it('fetches the bytes and sends those, never the url', async () => {
    const png = Uint8Array.from([137, 80, 78, 71, 13, 10, 26, 10])
    const asked: string[] = []
    const restore = withFetch(async (input: RequestInfo | URL) => {
      asked.push(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url)
      return new Response(png, { status: 200 })
    })

    try {
      const { dependencies, painted } = fakeRenderer()
      await Root({ width: 10, height: 10, onImageError: 'throw', children: Image({ src: { url: 'https://example.invalid/a.png' } }) }, dependencies)

      expect(asked).toEqual(['https://example.invalid/a.png'])
      expect(painted).toHaveLength(1)
      // The arena the addon received carries no url at all: the bytes were
      // substituted at encode time, so nothing downstream knows a network was
      // involved and `meo-canvas-core` needs no `net` feature to draw it.
      const [first] = painted
      if (first === undefined) throw new Error('nothing reached the renderer')
      expect(JSON.stringify(first.values)).not.toContain('example.invalid')
    } finally {
      restore()
    }
  })

  it('asks once for a url two nodes share', async () => {
    const asked: string[] = []
    const restore = withFetch(async (input: RequestInfo | URL) => {
      asked.push(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url)
      return new Response(Uint8Array.from([137, 80, 78, 71]), { status: 200 })
    })

    try {
      const { dependencies } = fakeRenderer()
      await Root(
        {
          width: 10,
          height: 10,
          children: Box({
            children: [Image({ src: { url: 'https://a.invalid/1.png' } }), Image({ src: { url: 'https://a.invalid/1.png' } })],
          }),
        },
        dependencies,
      )

      expect(asked).toEqual(['https://a.invalid/1.png'])
    } finally {
      restore()
    }
  })

  it('passes httpOptions through to fetch', async () => {
    let seen: RequestInit | undefined
    const restore = withFetch(async (_input: RequestInfo | URL, init?: RequestInit) => {
      seen = init
      return new Response(Uint8Array.from([137, 80]), { status: 200 })
    })

    try {
      const { dependencies } = fakeRenderer()
      await Root(
        {
          width: 10,
          height: 10,
          httpOptions: { headers: { authorization: 'Bearer t' } },
          children: Image({ src: { url: 'https://a.invalid/1.png' } }),
        },
        dependencies,
      )

      expect(seen?.headers).toEqual({ authorization: 'Bearer t' })
    } finally {
      restore()
    }
  })

  it('caps the wait rather than defaulting it, so no signal can extend past ours', async () => {
    // **The distinction the whole design rests on.** A bound `httpOptions`
    // could raise is this defect with a supported spelling, so the caller's
    // signal may only tighten. Both directions, because either alone passes on
    // an implementation that got the other one wrong.
    const never = new AbortController().signal
    const ours = fetchDeadline(never, 5)
    await new Promise(resolve => setTimeout(resolve, 30))
    expect(ours.signal.aborted).toBe(true)
    expect(never.aborted).toBe(false)

    const early = AbortSignal.abort(new Error('the caller was quicker'))
    const theirs = fetchDeadline(early, 60_000)
    expect(theirs.signal.aborted).toBe(true)
    expect(theirs.ceiling.aborted).toBe(false)
  })

  it('bounds the body, counting rather than trusting content-length', async () => {
    // **Counted while reading.** The header claims one byte and the stream
    // sends thirty-three mebibytes, which is the shape of the attack a cap
    // built on `content-length` does not stop.
    const chunk = new Uint8Array(1024 * 1024)
    const restore = withFetch(
      async () =>
        new Response(
          new ReadableStream({
            pull(controller) {
              controller.enqueue(chunk)
            },
          }),
          { status: 200, headers: { 'content-length': '1' } },
        ),
    )

    try {
      const { dependencies } = fakeRenderer()
      await expect(
        Root({ width: 10, height: 10, onImageError: 'throw', children: Image({ src: { url: 'https://a.invalid/big.png' } }) }, dependencies),
      ).rejects.toThrow(/larger than the 32 MiB this renderer fetches/)
    } finally {
      restore()
    }
  })

  it('keeps the caller signal rather than replacing it with the deadline', async () => {
    // The bound is added to the caller's control, not taken from it: a signal
    // they passed still aborts, and the message is theirs rather than ours.
    // Without `AbortSignal.any` this test passes only by accident, because the
    // deadline would have overwritten `signal` and nothing would abort at all.
    const controller = new AbortController()
    let composed: AbortSignal | undefined
    const restore = withFetch(async (_input: RequestInfo | URL, init?: RequestInit) => {
      composed = init?.signal ?? undefined
      controller.abort(new Error('the caller changed their mind'))
      throw composed?.reason instanceof Error ? composed.reason : new Error('aborted')
    })

    try {
      const { dependencies } = fakeRenderer()
      await expect(
        Root({ width: 10, height: 10, httpOptions: { signal: controller.signal }, children: Image({ src: { url: 'https://a.invalid/1.png' } }) }, dependencies),
      ).rejects.toThrow(/the caller changed their mind/)
      expect(composed?.aborted).toBe(true)
      // Ours did not fire, so the message must not claim a timeout.
      await expect(
        Root({ width: 10, height: 10, httpOptions: { signal: controller.signal }, children: Image({ src: { url: 'https://a.invalid/1.png' } }) }, dependencies),
      ).rejects.not.toThrow(/seconds this renderer waits/)
    } finally {
      restore()
    }
  })

  // The status is named rather than swallowed: a 404 that reached the decoder
  // as an HTML error page would fail as "undecodable image", which sends the
  // reader looking at the picture instead of at the server.
  it('names the status when the server refuses', async () => {
    const restore = withFetch(async () => new Response('nope', { status: 404, statusText: 'Not Found' }))

    try {
      const { dependencies } = fakeRenderer()
      await expect(
        Root({ width: 10, height: 10, onImageError: 'throw', children: Image({ src: { url: 'https://a.invalid/1.png' } }) }, dependencies),
      ).rejects.toThrow(/cannot fetch "https:\/\/a.invalid\/1.png": 404 Not Found/)
    } finally {
      restore()
    }
  })

  it('names the url when the fetch itself throws', async () => {
    const restore = withFetch(async () => {
      throw new TypeError('network unreachable')
    })

    try {
      const { dependencies } = fakeRenderer()
      await expect(
        Root({ width: 10, height: 10, onImageError: 'throw', children: Image({ src: { url: 'https://a.invalid/1.png' } }) }, dependencies),
      ).rejects.toThrow(/cannot fetch "https:\/\/a.invalid\/1.png".*network unreachable/s)
    } finally {
      restore()
    }
  })

  // A path and bytes are what this surface could always draw, and neither is
  // disturbed: a scene with no url in it never runs the fetch pass, and never
  // encodes twice.
  it('leaves a path alone', async () => {
    const { dependencies, painted } = fakeRenderer()

    await Root({ width: 10, height: 10, onImageError: 'throw', children: Image({ src: 'local.png' }) }, dependencies)

    expect(painted).toHaveLength(1)
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
    await expect(Root({ width: 10, height: 10, onImageError: 'throw', children: builder }, dependencies)).rejects.toThrow(
      /page builder needs `pages` or `duration`/,
    )
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

  it('resolves a percentage against the box rather than against a hundred times it', async () => {
    // Measured in pixels, on purpose. The scene stores a percentage as a
    // fraction where `1.0` is 100%, and this surface wrote `'50%'` as `50` for
    // a while — five thousand per cent — while **every** test agreed: the case
    // fixture probes each percentage property with `1`, and `'1%'` written as
    // `1` is exactly Rust's `Percent(1.0)`. The round trip and the byte
    // comparison both passed against the one value where the bug is invisible.
    //
    // So this asserts a rendered width. A comparison against Rust's bytes
    // cannot catch a units error that Rust's own probe shares.
    const covered = async (width: number | `${number}%`): Promise<number> => {
      const canvas = await Root({
        width: 200,
        height: 40,
        gpu: false,
        children: Box({ width, height: 40, backgroundColor: '#ffffff' }),
      })
      const raw = canvas.toBufferSync('raw')
      canvas.release()

      let lit = 0
      for (let x = 0; x < 200; x += 1) if ((raw[x * 4 + 3] ?? 0) > 0) lit += 1
      return lit
    }

    expect(await covered('50%')).toBe(100)
    expect(await covered('10%')).toBe(20)
    expect(await covered(100)).toBe(100)
  })

  it('reports the CPU when a float layout forces it, whatever was asked', async () => {
    // v1 documents that a float `colorType` falls back to the CPU because no
    // GPU composites float, and this is the only check that says the alias
    // reaches a float variant at all: comparing buffers cannot, since a float
    // layout changes compositing depth whether or not the engine fell back.
    //
    // **It claims only that.** `RGBAF16` and `RGBAF32` both report `cpu`, so
    // swapping the two in the alias table would pass this. What it pins is that
    // each names *a* float layout rather than an integer one.
    const engine = async (colorType?: ColorType): Promise<string> => {
      const canvas = await Root({ width: 8, height: 8, gpu: true, ...(colorType === undefined ? {} : { colorType }) })
      const settled = canvas.engine
      canvas.release()
      return settled
    }

    // Vacuous without a GPU backend compiled: everything reports `cpu` and the
    // assertion holds for a reason that has nothing to do with colour type.
    if ((await engine()) !== 'gpu') return

    expect(await engine('RGBAF32')).toBe('cpu')
    expect(await engine('RGBAF16')).toBe('cpu')
    expect(await engine('RGBAF16Norm')).toBe('cpu')
    expect(await engine('rgba')).toBe('gpu')
  })

  it('draws different pixels on the two rasterisers', async () => {
    // The check a fake renderer cannot satisfy. An assertion against a fake can
    // only say that a value was copied from one object to another, which stays
    // true when nothing on the far side reads it.
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

describe('an image source that cannot be resolved', () => {
  // Port 1 is on Node's blocked-port list, so `fetch` refuses it before it
  // reaches the network and the failure is "bad port" rather than a refused
  // connection. 49151 is unassigned and nothing listens, which is the case
  // being tested.
  const DEAD = 'http://127.0.0.1:49151/never.png'

  it('is an array even when nothing failed, so the check needs no guard', async () => {
    const canvas = await Root({ width: 10, height: 10, children: Box({}) })
    expect(canvas.warnings).toEqual([])
    expect(canvas.warnings.length === 0).toBe(true)
    canvas.release()
  })

  it('lets the render finish and records which URL failed', async () => {
    const canvas = await Root({
      width: 60,
      height: 60,
      children: Image({ src: { url: DEAD }, width: 40, height: 40 }),
    })
    expect(canvas.warnings).toHaveLength(1)
    // Identity is the requirement: the URL is what separates "not uploaded
    // yet" from "this path has never been right".
    expect(canvas.warnings[0]?.url).toBe(DEAD)
    expect(canvas.warnings[0]?.failure).toBe('transport')
    expect(canvas.warnings[0]?.nodes).toBe(1)
    canvas.release()
  })

  it("still records the warning under 'ignore'", async () => {
    const canvas = await Root({
      width: 60,
      height: 60,
      onImageError: 'ignore',
      children: Image({ src: { url: DEAD }, width: 40, height: 40 }),
    })
    expect(canvas.warnings).toHaveLength(1)
    canvas.release()
  })

  it("fails the whole render under 'throw', as every earlier version did", async () => {
    await expect(
      Root({
        width: 60,
        height: 60,
        onImageError: 'throw',
        children: Image({ src: { url: DEAD }, width: 40, height: 40 }),
      }),
    ).rejects.toThrow(/49151/)
  })
})
