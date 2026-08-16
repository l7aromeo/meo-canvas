import { DEFAULT_FPS, resolvePageCount } from '@/canvas/page.plan.js'
import type { PageInfo, RootPropsWithWorker, RootPropsWithoutWorker } from '@/canvas/canvas.type.js'
import type { WorkerCanvas } from '@/canvas/root.canvas.js'
import { pageInfoAt } from '@/canvas/page.plan.js'

/**
 * `resolvePageCount` and `pageInfoAt` are the whole contract between "what the caller asked for"
 * and "how many times the builder runs, with what". They are pure, so they are tested directly
 * rather than inferred from a render — a render can only show the count, never the arithmetic.
 */
describe('resolvePageCount', () => {
  const builder = () => ({}) as never

  it('returns 1 for still content', () => {
    expect(resolvePageCount({ children: [] })).toBe(1)
    expect(resolvePageCount({})).toBe(1)
  })

  it('uses an explicit page count', () => {
    expect(resolvePageCount({ children: builder, pages: 7 })).toBe(7)
  })

  it('derives the count from duration and fps', () => {
    expect(resolvePageCount({ children: builder, duration: 2, fps: 30 })).toBe(60)
  })

  it('defaults fps when only duration is given', () => {
    expect(resolvePageCount({ children: builder, duration: 1 })).toBe(DEFAULT_FPS)
  })

  it('rounds a fractional duration up so the full span is covered', () => {
    expect(resolvePageCount({ children: builder, duration: 0.55, fps: 10 })).toBe(6)
  })

  describe('rejects contradictions', () => {
    it('a builder with neither pages nor duration', () => {
      expect(() => resolvePageCount({ children: builder })).toThrow(/pages.*duration/i)
    })

    it('both pages and duration', () => {
      expect(() => resolvePageCount({ children: builder, pages: 3, duration: 1 })).toThrow(/both/i)
    })

    it('pages below one', () => {
      expect(() => resolvePageCount({ children: builder, pages: 0 })).toThrow(/at least 1/i)
    })

    it('a non-integer page count', () => {
      expect(() => resolvePageCount({ children: builder, pages: 2.5 })).toThrow(/whole number/i)
    })

    it('a duration that is not positive', () => {
      expect(() => resolvePageCount({ children: builder, duration: 0 })).toThrow(/greater than 0/i)
    })

    it('fps that is not positive', () => {
      expect(() => resolvePageCount({ children: builder, duration: 1, fps: 0 })).toThrow(/greater than 0/i)
    })

    it('pages given without a builder', () => {
      expect(() => resolvePageCount({ children: [], pages: 3 })).toThrow(/function/i)
    })

    it('duration given without a builder', () => {
      expect(() => resolvePageCount({ children: [], duration: 1 })).toThrow(/function/i)
    })

    it('fps given without a builder', () => {
      expect(() => resolvePageCount({ children: [], fps: 10 })).toThrow(/function/i)
    })
  })
})

describe('pageInfoAt', () => {
  const COUNT = 5

  it('reports the index and total', () => {
    const info = pageInfoAt(2, COUNT, DEFAULT_FPS)
    expect(info.index).toBe(2)
    expect(info.count).toBe(COUNT)
  })

  it('spans progress from 0 to 1 inclusive', () => {
    expect(pageInfoAt(0, COUNT, DEFAULT_FPS).progress).toBe(0)
    expect(pageInfoAt(COUNT - 1, COUNT, DEFAULT_FPS).progress).toBe(1)
  })

  it('reports progress 0 for a single page rather than dividing by zero', () => {
    const info = pageInfoAt(0, 1, DEFAULT_FPS)
    expect(info.progress).toBe(0)
    expect(Number.isNaN(info.progress)).toBe(false)
  })

  it('derives time in seconds from the frame rate', () => {
    const fps = 10
    expect(pageInfoAt(0, COUNT, fps).time).toBe(0)
    expect(pageInfoAt(3, COUNT, fps).time).toBeCloseTo(0.3)
  })

  it('increases progress monotonically', () => {
    const values = Array.from({ length: COUNT }, (_, i) => pageInfoAt(i, COUNT, DEFAULT_FPS).progress)
    const sorted = [...values].sort((a, b) => a - b)
    expect(values).toEqual(sorted)
    expect(new Set(values).size).toBe(COUNT)
  })

  it('hands the builder every field the type promises', () => {
    const info: PageInfo = pageInfoAt(1, COUNT, DEFAULT_FPS)
    expect(Object.keys(info).sort()).toEqual(['count', 'cycle', 'index', 'progress', 'time'])
  })
})

/**
 * `cycle` exists because `progress` is the wrong curve for anything that repeats, and wrong in a
 * way that hides: the render looks right frame by frame, and only the wrap stutters. These assert
 * the property that makes a loop seamless — the page after the last must land exactly where page 0
 * did — rather than asserting the formula back at itself.
 */
describe('pageInfoAt cycle', () => {
  const COUNT = 90
  const TURN = 2 * Math.PI
  const at = (index: number) => pageInfoAt(index, COUNT, DEFAULT_FPS)

  it('starts at 0 and stops one step short of 1', () => {
    expect(at(0).cycle).toBe(0)
    expect(at(COUNT - 1).cycle).toBeCloseTo(1 - 1 / COUNT, 12)
    expect(at(COUNT - 1).cycle).toBeLessThan(1)
  })

  it('closes the loop: one page past the end is page 0 again', () => {
    // The page that would follow the last belongs to the next repeat, so it has to coincide with
    // the first. `progress` fails this — it reaches 1 on the last page and overshoots past it.
    expect(pageInfoAt(COUNT, COUNT, DEFAULT_FPS).cycle).toBe(1)
    expect(Math.sin(pageInfoAt(COUNT, COUNT, DEFAULT_FPS).cycle * TURN)).toBeCloseTo(Math.sin(at(0).cycle * TURN), 12)
  })

  it('keeps the wrap step equal to every other step', () => {
    // The stutter is exactly this: a wrap step of zero, where every other step advanced.
    const step = at(1).cycle - at(0).cycle
    const wrap = 1 - at(COUNT - 1).cycle
    expect(wrap).toBeCloseTo(step, 12)
  })

  it('never revisits a point on the circle, which progress does on the wrap', () => {
    // Measured as distance between points rather than as sines: a sine alone is symmetric about
    // its peak, so two different phases share a value and a duplicate would hide in the collision.
    // Distance, in turn, rather than formatted text, because a full turn lands on `sin(2π)` —
    // -2.4e-16 rather than 0, the same point with a different sign of zero.
    const point = (phase: number) => [Math.cos(phase * TURN), Math.sin(phase * TURN)] as const
    const apart = (a: number, b: number) => Math.hypot(point(a)[0] - point(b)[0], point(a)[1] - point(b)[1])

    // The bug, pinned: under `progress` the last page lands back on the first, so ninety pages
    // only ever occupy eighty-nine positions and one frame of every repeat stands still.
    expect(apart(at(COUNT - 1).progress, at(0).progress)).toBeCloseTo(0, 12)

    // Under `cycle` the last page is a full step away from the first, exactly like every other
    // pair of neighbours.
    const step = apart(at(1).cycle, at(0).cycle)
    expect(apart(at(COUNT - 1).cycle, at(0).cycle)).toBeCloseTo(step, 12)
  })

  it('reports 0 for a single page, without a guard', () => {
    expect(pageInfoAt(0, 1, DEFAULT_FPS).cycle).toBe(0)
  })

  it('increases monotonically', () => {
    const values = Array.from({ length: COUNT }, (_, i) => at(i).cycle)
    expect(values).toEqual([...values].sort((a, b) => a - b))
    expect(new Set(values).size).toBe(COUNT)
  })

  it('agrees with time, which already spans the loop half-open', () => {
    // Anyone already driving periodic motion from `time` was never affected; the two have to match
    // or the two ways of asking the same question would disagree.
    const fps = 30
    const duration = COUNT / fps
    for (const index of [0, 1, 45, COUNT - 1]) {
      expect(pageInfoAt(index, COUNT, fps).time / duration).toBeCloseTo(at(index).cycle, 12)
    }
  })
})

/**
 * Type-level guarantees and their runtime counterparts must agree: anything the compiler rejects
 * has to throw at runtime too, because JavaScript callers and `as any` bypass the types entirely.
 * Each case below asserts both halves.
 */
describe('type and runtime rejection stay in sync', () => {
  const builder = () => ({}) as never

  const cases: { name: string; props: Record<string, unknown>; runtime: RegExp }[] = [
    { name: 'builder without a count', props: { children: builder }, runtime: /pages.*duration/i },
    { name: 'pages and duration together', props: { children: builder, pages: 2, duration: 1 }, runtime: /both/i },
    { name: 'pages without a builder', props: { children: [], pages: 2 }, runtime: /function/i },
    { name: 'duration without a builder', props: { children: [], duration: 1 }, runtime: /function/i },
    { name: 'fps without a builder', props: { children: [], fps: 24 }, runtime: /function/i },
  ]

  it.each(cases)('$name throws at runtime', ({ props, runtime }) => {
    expect(() => resolvePageCount(props as never)).toThrow(runtime)
  })

  /**
   * Every `@ts-expect-error` below is itself checked: if the expression stopped being an error,
   * TypeScript reports the directive as unused and `bun run typecheck` fails. So these assert the
   * rejection is real, not merely that the file compiles.
   */
  it('rejects the same shapes at compile time', () => {
    // @ts-expect-error — a builder needs either `pages` or `duration`
    const noCount: RootPropsWithoutWorker = { width: 10, height: 10, workerMode: false, children: builder }

    // @ts-expect-error — `pages` and `duration` are mutually exclusive
    const bothCounts: RootPropsWithoutWorker = { width: 10, height: 10, workerMode: false, children: builder, pages: 2, duration: 1 }

    // @ts-expect-error — `pages` has no meaning without a builder
    const pagesWithoutBuilder: RootPropsWithoutWorker = { width: 10, height: 10, workerMode: false, children: [], pages: 2 }

    // @ts-expect-error — `duration` has no meaning without a builder
    const durationWithoutBuilder: RootPropsWithoutWorker = { width: 10, height: 10, workerMode: false, children: [], duration: 1 }

    // @ts-expect-error — `fps` has no meaning without a builder
    const fpsWithoutBuilder: RootPropsWithoutWorker = { width: 10, height: 10, workerMode: false, children: [], fps: 24 }

    // @ts-expect-error — `workers` belongs to worker mode only
    const workersOutsideWorkerMode: RootPropsWithoutWorker = { width: 10, height: 10, workerMode: false, workers: 2, children: [] }

    void [noCount, bothCounts, pagesWithoutBuilder, durationWithoutBuilder, fpsWithoutBuilder, workersOutsideWorkerMode]
    expect(true).toBe(true)
  })

  it('accepts every valid shape at compile time', () => {
    const still: RootPropsWithoutWorker = { width: 10, height: 10, workerMode: false, children: [] }
    const noChildren: RootPropsWithoutWorker = { width: 10, height: 10, workerMode: false }
    const byPages: RootPropsWithoutWorker = { width: 10, height: 10, workerMode: false, children: builder, pages: 3 }
    const byDuration: RootPropsWithoutWorker = { width: 10, height: 10, workerMode: false, children: builder, duration: 2, fps: 24 }

    // Worker mode is the default, and is the only mode in which `workers` is meaningful.
    const inWorker: RootPropsWithWorker = { width: 10, height: 10, workers: 2, children: builder, pages: 3 }
    const workerStill: RootPropsWithWorker = { width: 10, height: 10, children: [] }

    void [still, noChildren, byPages, byDuration, inWorker, workerStill]
    expect(true).toBe(true)
  })
})

/**
 * The renderer raises a `TypeError` when animation timing reaches a format that cannot animate.
 * These assertions check the signatures turn that into a compile error instead, and — just as
 * importantly — that the still formats keep accepting everything else.
 */
describe('export options are narrowed by format', () => {
  /**
   * The bodies below are typechecked but never invoked: the assertion is that they compile (or, for
   * the rejected cases, that they do not). Calling them would need a live worker canvas, which has
   * nothing to do with what is being asserted.
   */
  it('accepts animation timing only on animated formats', () => {
    const accepted = (canvas: WorkerCanvas) => {
      void canvas.toBuffer('gif', { fps: 30 })
      void canvas.toBuffer('apng', { loop: 0 })
      void canvas.toBuffer('gif', { frameDelays: [100, 200] })
      // WebP and AVIF animate as of the renderer's 5.2.0.
      void canvas.toBuffer('webp', { fps: 24 })
      void canvas.toBuffer('avif', { fps: 24, loop: 2 })
      void canvas.toBufferSync('gif', { fps: 30 })
      void canvas.toURL('apng', { fps: 30 })
      void canvas.toURLSync('gif', { loop: 2 })
    }

    expect(accepted).toBeTypeOf('function')
  })

  it('rejects animation timing on still formats', () => {
    const rejected = (canvas: WorkerCanvas) => {
      // @ts-expect-error — `png` encodes one page, so `fps` would do nothing
      void canvas.toBuffer('png', { fps: 30 })

      // @ts-expect-error — `pdf` gathers pages as sheets, with no timeline
      void canvas.toBuffer('pdf', { loop: 0 })

      // @ts-expect-error — `tiff` gathers pages as sheets, with no timeline
      void canvas.toBuffer('tiff', { frameDelays: [100] })

      // @ts-expect-error — the sync path is narrowed the same way
      void canvas.toBufferSync('jpg', { fps: 12 })

      // @ts-expect-error — and so is the URL path
      void canvas.toURL('svg', { loop: 1 })
    }

    expect(rejected).toBeTypeOf('function')
  })

  it('still accepts the shared export options on both kinds of format', () => {
    const shared = (canvas: WorkerCanvas) => {
      void canvas.toBuffer('png', { quality: 0.8, density: 2 })
      void canvas.toBuffer('gif', { quality: 0.8, density: 2, fps: 10 })
      void canvas.toBuffer('jpg', { downsample: true })
    }

    expect(shared).toBeTypeOf('function')
  })
})
