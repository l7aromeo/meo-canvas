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

import { jest } from '@jest/globals'

// ---------------------------------------------------------------------------
// Shared mock fns — declared before unstable_mockModule so they're in scope
// ---------------------------------------------------------------------------

const mockLoadImage = jest.fn<(src: any) => Promise<any>>()
const mockFileTypeFromBuffer = jest.fn<(buf: any) => Promise<any>>()
const mockFileTypeFromFile = jest.fn<(path: any) => Promise<any>>()

const mockReadDiskCache = jest.fn<(key: string) => Promise<Buffer | null>>()
const mockWriteDiskCache = jest.fn<(key: string, data: Buffer) => Promise<void>>()
const mockDeleteDiskCache = jest.fn<(key: string) => Promise<void>>()
const mockHashBuffer = jest.fn<(buf: Buffer) => string>()

// Mock global fetch for HTTP image sources
const mockFetch = jest.fn<(url: string) => Promise<Response>>()
global.fetch = mockFetch as any

// ---------------------------------------------------------------------------
// Module mocks
// ---------------------------------------------------------------------------

jest.unstable_mockModule('skia-canvas', () => ({
  loadImage: mockLoadImage,
  Image: jest.fn(),
  Canvas: jest.fn(function (this: any, w: number, h: number) {
    this.width = w
    this.height = h
    this.getContext = jest.fn(() => ({
      scale: jest.fn(),
      save: jest.fn(),
      restore: jest.fn(),
      beginPath: jest.fn(),
      moveTo: jest.fn(),
      lineTo: jest.fn(),
      arc: jest.fn(),
      closePath: jest.fn(),
      rect: jest.fn(),
      fill: jest.fn(),
      stroke: jest.fn(),
      clip: jest.fn(),
      fillStyle: '',
      strokeStyle: '',
      lineWidth: 0,
      globalAlpha: 1,
      globalCompositeOperation: '',
      imageSmoothingEnabled: true,
      imageSmoothingQuality: 'high',
      createLinearGradient: jest.fn(() => ({ addColorStop: jest.fn() })),
      createRadialGradient: jest.fn(() => ({ addColorStop: jest.fn() })),
      setLineDash: jest.fn(),
      measureText: jest.fn(() => ({ width: 0, height: 0 })),
      fillText: jest.fn(),
      strokeText: jest.fn(),
      drawImage: jest.fn(),
      font: '',
      textAlign: 'left',
      textBaseline: 'alphabetic',
    }))
    this.toBufferSync = jest.fn(() => Buffer.from(''))
  }),
  FontLibrary: { use: jest.fn() },
}))

jest.unstable_mockModule('file-type', () => ({
  fileTypeFromBuffer: mockFileTypeFromBuffer,
  fileTypeFromFile: mockFileTypeFromFile,
}))

jest.unstable_mockModule('fs', () => ({
  promises: { readFile: jest.fn(() => Promise.reject(new Error('not found'))) },
}))

jest.unstable_mockModule('@/util/disk.cache.js', () => ({
  readDiskCache: mockReadDiskCache,
  writeDiskCache: mockWriteDiskCache,
  deleteDiskCache: mockDeleteDiskCache,
  hashBuffer: mockHashBuffer,
}))

// Minimal yoga-layout mock so BoxNode construction works
jest.unstable_mockModule('yoga-layout', () => {
  const node = {
    setWidth: jest.fn(),
    setHeight: jest.fn(),
    setAspectRatio: jest.fn(),
    getComputedWidth: jest.fn(() => 100),
    getComputedHeight: jest.fn(() => 100),
    getComputedLayout: jest.fn(() => ({ left: 0, top: 0, width: 100, height: 100 })),
    getComputedPadding: jest.fn(() => 0),
    getComputedBorder: jest.fn(() => 0),
    getBorder: jest.fn(() => 0),
    calculateLayout: jest.fn(),
    insertChild: jest.fn(),
    isDirty: jest.fn(() => false),
    markDirty: jest.fn(),
    setFlexDirection: jest.fn(),
    setAlignItems: jest.fn(),
    setJustifyContent: jest.fn(),
    setFlex: jest.fn(),
    setFlexGrow: jest.fn(),
    setFlexShrink: jest.fn(),
    setFlexBasis: jest.fn(),
    setMargin: jest.fn(),
    setPadding: jest.fn(),
    setBorder: jest.fn(),
    setGap: jest.fn(),
    setOverflow: jest.fn(),
    setDisplay: jest.fn(),
    setPosition: jest.fn(),
    setPositionType: jest.fn(),
    setMinWidth: jest.fn(),
    setMinHeight: jest.fn(),
    setMaxWidth: jest.fn(),
    setMaxHeight: jest.fn(),
    setBoxSizing: jest.fn(),
    setDirection: jest.fn(),
    getBoxSizing: jest.fn(() => 0),
    free: jest.fn(),
  }
  return {
    default: { Node: { create: jest.fn(() => node) } },
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

jest.unstable_mockModule('node:worker_threads', () => ({
  Worker: class {
    on() {}
    postMessage() {}
    terminate() {}
  },
}))
jest.unstable_mockModule('node:fs', () => ({
  existsSync: jest.fn(() => false),
  promises: { mkdir: jest.fn(), readFile: jest.fn(() => Promise.reject()), writeFile: jest.fn(), unlink: jest.fn() },
}))
jest.unstable_mockModule('node:path', () => ({
  join: (...args: string[]) => args.join('/'),
  dirname: (p: string) => p.split('/').slice(0, -1).join('/'),
  resolve: (...args: string[]) => args.join('/'),
}))

// ---------------------------------------------------------------------------
// Types loaded after mocks
// ---------------------------------------------------------------------------

let ImageNode: typeof import('@/canvas/image.canvas.util.js').ImageNode
let RootNode: typeof import('@/canvas/root.canvas.util.js').RootNode
let configure: typeof import('@/canvas/root.canvas.util.js').configure

const MOCK_IMAGE = { width: 200, height: 100 }
const TEST_KEY = 'abc123deadbeef'

beforeEach(async () => {
  jest.resetModules()

  jest.unstable_mockModule('skia-canvas', () => ({
    loadImage: mockLoadImage,
    Image: jest.fn(),
    Canvas: jest.fn(function (this: any, w: number, h: number) {
      this.width = w
      this.height = h
      this.getContext = jest.fn(() => ({
        scale: jest.fn(),
        save: jest.fn(),
        restore: jest.fn(),
        beginPath: jest.fn(),
        moveTo: jest.fn(),
        lineTo: jest.fn(),
        arc: jest.fn(),
        closePath: jest.fn(),
        rect: jest.fn(),
        fill: jest.fn(),
        stroke: jest.fn(),
        clip: jest.fn(),
        fillStyle: '',
        strokeStyle: '',
        lineWidth: 0,
        globalAlpha: 1,
        globalCompositeOperation: '',
        imageSmoothingEnabled: true,
        imageSmoothingQuality: 'high',
        createLinearGradient: jest.fn(() => ({ addColorStop: jest.fn() })),
        createRadialGradient: jest.fn(() => ({ addColorStop: jest.fn() })),
        setLineDash: jest.fn(),
        measureText: jest.fn(() => ({ width: 0, height: 0 })),
        fillText: jest.fn(),
        strokeText: jest.fn(),
        font: '',
        textAlign: 'left',
        textBaseline: 'alphabetic',
      }))
      this.toBufferSync = jest.fn(() => Buffer.from(''))
    }),
    FontLibrary: { use: jest.fn() },
  }))

  jest.unstable_mockModule('file-type', () => ({
    fileTypeFromBuffer: mockFileTypeFromBuffer,
    fileTypeFromFile: mockFileTypeFromFile,
  }))

  jest.unstable_mockModule('fs', () => ({
    promises: { readFile: jest.fn(() => Promise.reject(new Error('not found'))) },
  }))

  jest.unstable_mockModule('@/util/disk.cache.js', () => ({
    readDiskCache: mockReadDiskCache,
    writeDiskCache: mockWriteDiskCache,
    deleteDiskCache: mockDeleteDiskCache,
    hashBuffer: mockHashBuffer,
  }))

  const imageMod = await import('@/canvas/image.canvas.util.js')
  ImageNode = imageMod.ImageNode

  const rootMod = await import('@/canvas/root.canvas.util.js')
  RootNode = rootMod.RootNode
  configure = rootMod.configure
  configure({ workerMode: false })

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

    const memCache = new Map() as import('@/canvas/image.canvas.util.js').RenderImageCache
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
    jest.spyOn(root as any, 'finalizeLayout').mockImplementation(() => {
      throw new Error('layout error')
    })

    await expect(root.render()).rejects.toThrow('layout error')
    expect(mockDeleteDiskCache).toHaveBeenCalledWith(TEST_KEY)
  })
})
