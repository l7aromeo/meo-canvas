import { vi } from 'vitest'

vi.mock('meo-skia-canvas', async () => (await import('@/__mocks__/meo-skia-canvas.js')).__mocks__)
vi.mock('@/canvas/layout.canvas.js', async () => (await import('@/__mocks__/layout.canvas.js')).__mocks__)
vi.mock('@/canvas/image.canvas.js', async () => (await import('@/__mocks__/image.canvas.js')).__mocks__)

const PAGE_COUNT = 4
const WIDTH = 100
const HEIGHT = 50

/**
 * Covers the two claims `renderPages` makes that a page count alone cannot show: that the expensive
 * work is shared across pages, and that each page's layout tree is released as soon as it is drawn.
 *
 * Both are why a long sequence stays affordable — an image referenced by every page is fetched once,
 * and memory does not grow with the page count — so neither is left to inspection.
 */
describe('renderPages', () => {
  let renderPages: typeof import('@/canvas/root.canvas.js').renderPages
  let RootNode: typeof import('@/canvas/root.canvas.js').RootNode

  const props = { width: WIDTH, height: HEIGHT, workerMode: false } as never
  const pages = Array.from({ length: PAGE_COUNT }, () => [])

  beforeEach(async () => {
    vi.resetModules()
    // The renderer mock is module-level and outlives `resetModules`, so call counts carry over
    // between tests unless they are cleared here.
    ;(await import('@/__mocks__/meo-skia-canvas.js')).__mocks__.reset()
    ;({ renderPages, RootNode } = await import('@/canvas/root.canvas.js'))
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders one page per entry', async () => {
    const canvas = await renderPages(props, pages)
    expect(canvas.pages).toHaveLength(PAGE_COUNT)
  })

  it('creates a single canvas and appends every later page to it', async () => {
    const { Canvas } = await import('meo-skia-canvas')
    // Cleared here rather than in `beforeEach`: the renderer mock is shared across this file's
    // module graph, so only a clear immediately before the call measures this render alone.
    vi.mocked(Canvas).mockClear()

    const canvas = await renderPages(props, pages)

    // One construction for the whole sequence; the rest arrive through `newPage`.
    expect(Canvas).toHaveBeenCalledTimes(1)
    expect(canvas.newPage).toHaveBeenCalledTimes(PAGE_COUNT - 1)
    expect(canvas.pages).toHaveLength(PAGE_COUNT)
  })

  it('shares one image cache across every page', async () => {
    const prepare = vi.spyOn(RootNode.prototype, 'prepare')

    await renderPages(props, pages)

    expect(prepare).toHaveBeenCalledTimes(PAGE_COUNT)
    const caches = prepare.mock.calls.map(([cache]) => cache)
    // Identity, not equality: a fresh empty Map per page would satisfy a deep comparison while
    // re-fetching every image the sequence shares.
    for (const cache of caches) {
      expect(cache).toBe(caches[0])
    }
  })

  it('releases each page layout tree, not just the last', async () => {
    const release = vi.spyOn(RootNode.prototype, 'releaseLayoutTree')

    await renderPages(props, pages)

    expect(release).toHaveBeenCalledTimes(PAGE_COUNT)
  })

  it('releases the layout tree even when a page fails to draw', async () => {
    const release = vi.spyOn(RootNode.prototype, 'releaseLayoutTree')
    const failure = new Error('layout exploded')
    vi.spyOn(RootNode.prototype, 'prepare').mockRejectedValueOnce(failure)

    await expect(renderPages(props, pages)).rejects.toThrow(failure)

    // The failing page still gives its Yoga nodes back; a leak here would outlive the whole render.
    expect(release).toHaveBeenCalledTimes(1)
  })

  it('registers fonts once for the whole sequence, not once per page', async () => {
    const registerFonts = vi.spyOn(RootNode.prototype, 'registerFonts')

    await renderPages(props, pages)

    expect(registerFonts).toHaveBeenCalledTimes(1)
  })
})
