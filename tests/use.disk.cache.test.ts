/**
 * Tests for the useDiskCache per-render disk cache system.
 *
 * Covers:
 *  - ImageNode: disk read/write only happens when diskCacheKeys is provided
 *  - ImageNode: written keys are tracked in the caller's Set
 *  - RootNode: deleteDiskCache called for each written key when useDiskCache: true
 *  - RootNode: no disk operations when useDiskCache: false (default)
 *  - RootNode: only this render's own keys are deleted (isolation)
 */

import { vi } from 'vitest'

import { createMockCanvas } from './helpers/mock-canvas-context.js'

// ---------------------------------------------------------------------------
// Shared mock fns — declared before unstable_mockModule so they're in scope
// ---------------------------------------------------------------------------

const mockLoadImage = vi.fn<(src: any) => Promise<any>>()
const mockFileTypeFromBuffer = vi.fn<(buf: any) => Promise<any>>()
const mockFileTypeFromFile = vi.fn<(path: any) => Promise<any>>()

const mockReadDiskCache = vi.fn<(key: string) => Promise<Buffer | null>>()
const mockWriteDiskCache = vi.fn<(key: string, data: Buffer) => Promise<void>>()
const mockDeleteDiskCache = vi.fn<(key: string) => Promise<void>>()
const mockHashBuffer = vi.fn<(buf: Buffer) => string>()

// Mock global fetch for HTTP image sources
const mockFetch = vi.fn<(url: string) => Promise<Response>>()
global.fetch = mockFetch as any

// ---------------------------------------------------------------------------
// Module mocks
// ---------------------------------------------------------------------------

vi.mock('skia-canvas', () => ({
  loadImage: mockLoadImage,
  Image: vi.fn(),
  Canvas: createMockCanvas(),
  FontLibrary: { use: vi.fn() },
}))

vi.mock('file-type', () => ({
  fileTypeFromBuffer: mockFileTypeFromBuffer,
  fileTypeFromFile: mockFileTypeFromFile,
}))

vi.mock('fs', () => ({
  promises: { readFile: vi.fn(() => Promise.reject(new Error('not found'))) },
}))

vi.mock('@/util/disk.cache.js', () => ({
  readDiskCache: mockReadDiskCache,
  writeDiskCache: mockWriteDiskCache,
  deleteDiskCache: mockDeleteDiskCache,
  hashBuffer: mockHashBuffer,
}))

// Minimal yoga-layout mock so BoxNode construction works
vi.mock('yoga-layout', () => {
  const createNode = () => ({
    setWidth: vi.fn(),
    setHeight: vi.fn(),
    setAspectRatio: vi.fn(),
    getComputedWidth: vi.fn(() => 100),
    getComputedHeight: vi.fn(() => 100),
    getComputedLayout: vi.fn(() => ({ left: 0, top: 0, width: 100, height: 100 })),
    getComputedPadding: vi.fn(() => 0),
    getComputedBorder: vi.fn(() => 0),
    getBorder: vi.fn(() => 0),
    calculateLayout: vi.fn(),
    insertChild: vi.fn(),
    isDirty: vi.fn(() => false),
    markDirty: vi.fn(),
    setFlexDirection: vi.fn(),
    setAlignItems: vi.fn(),
    setJustifyContent: vi.fn(),
    setFlex: vi.fn(),
    setFlexGrow: vi.fn(),
    setFlexShrink: vi.fn(),
    setFlexBasis: vi.fn(),
    setMargin: vi.fn(),
    setPadding: vi.fn(),
    setBorder: vi.fn(),
    setGap: vi.fn(),
    setOverflow: vi.fn(),
    setDisplay: vi.fn(),
    setPosition: vi.fn(),
    setPositionType: vi.fn(),
    setMinWidth: vi.fn(),
    setMinHeight: vi.fn(),
    setMaxWidth: vi.fn(),
    setMaxHeight: vi.fn(),
    setBoxSizing: vi.fn(),
    setDirection: vi.fn(),
    getBoxSizing: vi.fn(() => 0),
    free: vi.fn(),
  })
  return {
    default: { Node: { create: vi.fn(createNode) } },
    Direction: { LTR: 0, RTL: 1, Inherit: 2 },
    FlexDirection: { Column: 0, Row: 1 },
    Align: { FlexStart: 0, Center: 1, FlexEnd: 2, Stretch: 3 },
    Justify: { FlexStart: 0, Center: 1, FlexEnd: 2, SpaceBetween: 3 },
    Overflow: { Hidden: 0 },
    Display: { Flex: 0, None: 1 },
    PositionType: { Relative: 0, Absolute: 1 },
    Edge: { Left: 0, Top: 1, Right: 2, Bottom: 3, All: 4 },
    Gutter: { All: 0, Column: 1, Row: 2 },
    Wrap: { NoWrap: 0, Wrap: 1 },
    BoxSizing: { BorderBox: 0, ContentBox: 1 },
  }
})

vi.mock('node:worker_threads', () => ({
  Worker: class {
    on() {}
    postMessage() {}
    terminate() {}
  },
}))
vi.mock('node:fs', () => ({
  existsSync: vi.fn(() => false),
  promises: { mkdir: vi.fn(), readFile: vi.fn(() => Promise.reject()), writeFile: vi.fn(), unlink: vi.fn() },
}))
vi.mock('node:path', () => ({
  join: (...args: string[]) => args.join('/'),
  dirname: (p: string) => p.split('/').slice(0, -1).join('/'),
  resolve: (...args: string[]) => args.join('/'),
}))

// ---------------------------------------------------------------------------
// Types loaded after mocks
// ---------------------------------------------------------------------------

let ImageNode: typeof import('@/canvas/image.canvas.js').ImageNode
let RootNode: typeof import('@/canvas/root.canvas.js').RootNode

const MOCK_IMAGE = { width: 200, height: 100 }
const TEST_KEY = 'abc123deadbeef'

beforeEach(async () => {
  vi.resetModules()

  vi.doMock('skia-canvas', () => ({
    loadImage: mockLoadImage,
    Image: vi.fn(),
    Canvas: createMockCanvas(),
    FontLibrary: { use: vi.fn() },
  }))

  vi.doMock('file-type', () => ({
    fileTypeFromBuffer: mockFileTypeFromBuffer,
    fileTypeFromFile: mockFileTypeFromFile,
  }))

  vi.doMock('fs', () => ({
    promises: { readFile: vi.fn(() => Promise.reject(new Error('not found'))) },
  }))

  vi.doMock('@/util/disk.cache.js', () => ({
    readDiskCache: mockReadDiskCache,
    writeDiskCache: mockWriteDiskCache,
    deleteDiskCache: mockDeleteDiskCache,
    hashBuffer: mockHashBuffer,
  }))

  const imageMod = await import('@/canvas/image.canvas.js')
  ImageNode = imageMod.ImageNode

  const rootMod = await import('@/canvas/root.canvas.js')
  RootNode = rootMod.RootNode

  mockLoadImage.mockReset()
  mockFileTypeFromBuffer.mockReset()
  mockFileTypeFromFile.mockReset()
  mockReadDiskCache.mockReset()
  mockWriteDiskCache.mockReset()
  mockDeleteDiskCache.mockReset()
  mockHashBuffer.mockReset()

  mockLoadImage.mockResolvedValue(MOCK_IMAGE)
  mockFileTypeFromFile.mockResolvedValue({ mime: 'image/png' })
  mockFileTypeFromBuffer.mockResolvedValue({ mime: 'image/png' })
  mockReadDiskCache.mockResolvedValue(null) // disk miss by default
  mockWriteDiskCache.mockResolvedValue(undefined)
  mockDeleteDiskCache.mockResolvedValue(undefined)
  mockHashBuffer.mockImplementation(() => TEST_KEY)

  // Mock fetch to return a buffer for HTTP URLs
  mockFetch.mockResolvedValue({
    ok: true,
    arrayBuffer: async () => new ArrayBuffer(10),
  } as any)
})

// ---------------------------------------------------------------------------
// Section 1: ImageNode disk cache behavior
// ---------------------------------------------------------------------------

describe('ImageNode — disk cache via diskCacheKeys', () => {
  it('does not read or write disk when diskCacheKeys is not provided', async () => {
    const node = new ImageNode({ src: 'test.png' })
    await node.load()

    expect(mockReadDiskCache).not.toHaveBeenCalled()
    expect(mockWriteDiskCache).not.toHaveBeenCalled()
  })

  it('reads disk cache when diskCacheKeys is provided', async () => {
    const diskCacheKeys = new Set<string>()
    const node = new ImageNode({ src: 'test.png' })
    await node.load(undefined, diskCacheKeys)

    expect(mockReadDiskCache).toHaveBeenCalledWith(TEST_KEY)
  })

  it('loads from disk and skips fetch on disk hit', async () => {
    const diskBuffer = Buffer.from('fake-png-data')
    mockReadDiskCache.mockResolvedValue(diskBuffer)

    const diskCacheKeys = new Set<string>()
    const node = new ImageNode({ src: 'test.png' })
    await node.load(undefined, diskCacheKeys)

    // loadImage called with the disk buffer, no network fetch
    expect(mockLoadImage).toHaveBeenCalledWith(diskBuffer)
    expect(mockWriteDiskCache).not.toHaveBeenCalled()
  })

  it('writes to disk and adds key to diskCacheKeys on disk miss', async () => {
    mockReadDiskCache.mockResolvedValue(null)

    const diskCacheKeys = new Set<string>()
    // Use HTTP URL to trigger fetch path which has contentBuffer available
    const node = new ImageNode({ src: 'http://example.com/test.png' })
    await node.load(undefined, diskCacheKeys)

    expect(mockWriteDiskCache).toHaveBeenCalledWith(TEST_KEY, expect.any(Buffer))
    expect(diskCacheKeys.has(TEST_KEY)).toBe(true)
  })

  it('does not write to disk when diskCacheKeys is not provided even on fresh fetch', async () => {
    const node = new ImageNode({ src: 'test.png' })
    await node.load()

    expect(mockWriteDiskCache).not.toHaveBeenCalled()
  })

  it('deduplicates disk writes via per-render memory cache', async () => {
    mockReadDiskCache.mockResolvedValue(null)

    const memCache = new Map() as import('@/canvas/image.canvas.js').RenderImageCache
    const diskCacheKeys = new Set<string>()

    // Use HTTP URL to trigger fetch path which has contentBuffer available
    const node1 = new ImageNode({ src: 'http://example.com/test.png' })
    const node2 = new ImageNode({ src: 'http://example.com/test.png' })

    await node1.load(memCache, diskCacheKeys)
    await node2.load(memCache, diskCacheKeys)

    // Only one fetch and one disk write despite two nodes
    expect(mockWriteDiskCache).toHaveBeenCalledTimes(1)
    expect(diskCacheKeys.size).toBe(1)
  })
})

// ---------------------------------------------------------------------------
// Section 2: RootNode useDiskCache prop
// ---------------------------------------------------------------------------

describe('RootNode — useDiskCache prop', () => {
  function makeRootWithImageChild(useDiskCache?: boolean) {
    const root = new RootNode({ width: 100, useDiskCache })
    // Inject a mock ImageNode child that populates diskCacheKeys when disk is enabled
    // Use HTTP URL to trigger fetch path which has contentBuffer available for disk caching
    const child = new ImageNode({ src: 'http://example.com/test.png', width: 50, height: 50 })
    root.children.push(child as any)
    return root
  }

  it('does not call deleteDiskCache when useDiskCache is false (default)', async () => {
    const root = makeRootWithImageChild(false)
    await root.render()

    expect(mockDeleteDiskCache).not.toHaveBeenCalled()
  })

  it('does not call deleteDiskCache when useDiskCache is omitted', async () => {
    const root = makeRootWithImageChild()
    await root.render()

    expect(mockDeleteDiskCache).not.toHaveBeenCalled()
  })

  it('calls deleteDiskCache for each key written during render when useDiskCache is true', async () => {
    mockReadDiskCache.mockResolvedValue(null) // disk miss → triggers write

    const root = makeRootWithImageChild(true)
    await root.render()

    // The image fetch wrote TEST_KEY to disk — render's finally must clean it up
    expect(mockDeleteDiskCache).toHaveBeenCalledWith(TEST_KEY)
  })

  it('only deletes keys written by this render, not unrelated keys', async () => {
    mockReadDiskCache.mockResolvedValue(null)

    const root = makeRootWithImageChild(true)
    await root.render()

    // Exactly the keys tracked in diskCacheKeys are deleted — nothing more
    const deletedKeys = mockDeleteDiskCache.mock.calls.map(([k]) => k)
    expect(deletedKeys).toEqual([TEST_KEY])
  })

  it('calls deleteDiskCache in finally even if render throws after image load', async () => {
    mockReadDiskCache.mockResolvedValue(null)

    const root = makeRootWithImageChild(true)
    // Force a throw after image loading by breaking the canvas setup
    vi.spyOn(root as any, 'finalizeLayout').mockImplementation(() => {
      throw new Error('layout error')
    })

    await expect(root.render()).rejects.toThrow('layout error')
    expect(mockDeleteDiskCache).toHaveBeenCalledWith(TEST_KEY)
  })
})
