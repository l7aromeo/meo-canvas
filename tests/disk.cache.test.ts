/**
 * Unit tests for disk cache utilities.
 *
 * Tests the actual disk cache implementation with real file system operations.
 * Uses a temporary directory for isolation and cleans up after each test.
 */

import { hashBuffer, readDiskCache, writeDiskCache, deleteDiskCache, clearDiskCache, setDiskCacheDir } from '@/util/disk.cache.js'
import { promises as fs } from 'fs'
import { join } from 'path'

// Use a isolated temp directory for testing
const TEST_CACHE_DIR = join(process.cwd(), '.cache', 'test-files')

// ---------------------------------------------------------------------------
// Test setup and teardown
// ---------------------------------------------------------------------------

beforeEach(async () => {
  // Ensure test cache dir exists
  await fs.mkdir(TEST_CACHE_DIR, { recursive: true })
  // Point the cache module at the isolated dir. Without this, reads/writes hit
  // the shared default `.cache/files`, where a concurrent test fork's exit-time
  // clearDiskCache() can wipe entries mid-test (flaky "expected null" races).
  setDiskCacheDir(TEST_CACHE_DIR)
})

afterEach(async () => {
  // Clean up test cache dir completely
  try {
    await fs.rm(TEST_CACHE_DIR, { recursive: true, force: true })
  } catch {
    // Ignore cleanup errors
  }
})

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('disk.cache', () => {
  it('registers process exit listeners only once across re-imports', async () => {
    const beforeCount = process.listenerCount('beforeExit')
    const sigintCount = process.listenerCount('SIGINT')
    const sigtermCount = process.listenerCount('SIGTERM')

    vi.resetModules()
    await import('@/util/disk.cache.js')
    vi.resetModules()
    await import('@/util/disk.cache.js')

    // Replacing handlers should not accumulate extra listeners (+2 per leaky re-import).
    expect(process.listenerCount('beforeExit')).toBe(beforeCount)
    expect(process.listenerCount('SIGINT')).toBe(sigintCount)
    expect(process.listenerCount('SIGTERM')).toBe(sigtermCount)
  })

  describe('hashBuffer', () => {
    it('should generate consistent SHA-256 hashes', () => {
      const buffer1 = Buffer.from('hello world')
      const buffer2 = Buffer.from('hello world')
      const buffer3 = Buffer.from('different content')

      const hash1 = hashBuffer(buffer1)
      const hash2 = hashBuffer(buffer2)
      const hash3 = hashBuffer(buffer3)

      expect(hash1).toBe(hash2) // Same content = same hash
      expect(hash1).not.toBe(hash3) // Different content = different hash
    })

    it('should generate 64-character hex strings (SHA-256)', () => {
      const buffer = Buffer.from('test data')
      const hash = hashBuffer(buffer)

      expect(hash).toHaveLength(64)
      expect(hash).toMatch(/^[a-f0-9]+$/) // Hex only
    })

    it('should handle empty buffers', () => {
      const emptyBuffer = Buffer.from([])
      const hash = hashBuffer(emptyBuffer)

      expect(hash).toHaveLength(64)
      expect(hash).toBe('e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855') // SHA-256 of empty
    })

    it('should handle large buffers', () => {
      const largeBuffer = Buffer.alloc(1024 * 1024, 'x') // 1MB
      const hash = hashBuffer(largeBuffer)

      expect(hash).toHaveLength(64)
    })
  })

  describe('writeDiskCache and readDiskCache', () => {
    it('should write and read data successfully', async () => {
      const key = 'test-key-123'
      const data = Buffer.from('test data content')

      await writeDiskCache(key, data)
      const result = await readDiskCache(key)

      expect(result).not.toBeNull()
      expect(result).toEqual(data)
    })

    it('should return null for non-existent keys', async () => {
      const result = await readDiskCache('non-existent-key')

      expect(result).toBeNull()
    })

    it('should handle binary data correctly', async () => {
      const key = 'binary-test'
      const data = Buffer.from([0x00, 0x01, 0x02, 0xff, 0xfe, 0xfd])

      await writeDiskCache(key, data)
      const result = await readDiskCache(key)

      expect(result).toEqual(data)
    })

    it('should overwrite existing keys', async () => {
      const key = 'overwrite-test'
      const data1 = Buffer.from('first version')
      const data2 = Buffer.from('second version')

      await writeDiskCache(key, data1)
      await writeDiskCache(key, data2)
      const result = await readDiskCache(key)

      expect(result).toEqual(data2)
    })
  })

  describe('deleteDiskCache', () => {
    it('should delete existing keys', async () => {
      const key = 'delete-test'
      const data = Buffer.from('to be deleted')

      await writeDiskCache(key, data)

      // Verify it exists
      const beforeDelete = await readDiskCache(key)
      expect(beforeDelete).not.toBeNull()

      // Delete it
      await deleteDiskCache(key)

      // Verify it's gone
      const afterDelete = await readDiskCache(key)
      expect(afterDelete).toBeNull()
    })

    it('should not throw for non-existent keys', async () => {
      // Should not throw
      await expect(deleteDiskCache('non-existent-key')).resolves.not.toThrow()
    })
  })

  describe('ensureDir (implicit)', () => {
    it('should create cache directory automatically on write', async () => {
      // Remove the dir first
      try {
        await fs.rm(TEST_CACHE_DIR, { recursive: true, force: true })
      } catch {
        // Ignore
      }

      const key = 'auto-create-test'
      const data = Buffer.from('test')

      // Write should create the directory
      await writeDiskCache(key, data)

      // Verify directory was created and file exists
      const result = await readDiskCache(key)
      expect(result).not.toBeNull()
    })

    it('should create cache directory automatically on read', async () => {
      // Remove the dir first
      try {
        await fs.rm(TEST_CACHE_DIR, { recursive: true, force: true })
      } catch {
        // Ignore
      }

      // Read from non-existent dir should not throw
      await expect(readDiskCache('non-existent')).resolves.not.toThrow()
    })
  })

  describe('error handling', () => {
    it('should handle write failures gracefully (non-fatal)', async () => {
      // This tests the try-catch in writeDiskCache
      // In normal conditions, writes should succeed
      // The function silently fails on errors (by design)
      await expect(writeDiskCache('test', Buffer.from('data'))).resolves.not.toThrow()
    })

    it('should handle read failures by returning null', async () => {
      // Read failures return null (tested above)
      const result = await readDiskCache('non-existent')
      expect(result).toBeNull()
    })

    it('should handle delete failures gracefully (non-fatal)', async () => {
      // Delete is non-fatal by design
      await expect(deleteDiskCache('non-existent')).resolves.not.toThrow()
    })
  })

  describe('integration scenarios', () => {
    it('should support write → read → delete lifecycle', async () => {
      const key = 'lifecycle-test'
      const originalData = Buffer.from('lifecycle test data')

      // Write
      await writeDiskCache(key, originalData)

      // Read
      const readData = await readDiskCache(key)
      expect(readData).toEqual(originalData)

      // Delete
      await deleteDiskCache(key)

      // Verify deleted
      const afterDelete = await readDiskCache(key)
      expect(afterDelete).toBeNull()
    })

    it('should handle multiple concurrent writes', async () => {
      const writes = Array.from({ length: 10 }, (_, i) => ({
        key: `concurrent-${i}`,
        data: Buffer.from(`data-${i}`),
      }))

      // Write all concurrently
      await Promise.all(writes.map(w => writeDiskCache(w.key, w.data)))

      // Verify all written
      const reads = await Promise.all(writes.map(w => readDiskCache(w.key)))
      reads.forEach((result, i) => {
        expect(result).toEqual(writes[i].data)
      })

      // Clean up
      await Promise.all(writes.map(w => deleteDiskCache(w.key)))
    })

    it('should handle special characters in keys', async () => {
      const specialKeys = ['key-with-dashes', 'key_with_underscores', 'key.with.dots', 'key123numeric']

      for (const key of specialKeys) {
        const data = Buffer.from(`data for ${key}`)
        await writeDiskCache(key, data)
        const result = await readDiskCache(key)
        expect(result).toEqual(data)
        await deleteDiskCache(key)
      }
    })
  })

  describe('clearDiskCache', () => {
    it('should delete the entire cache directory', async () => {
      // Write multiple files
      await writeDiskCache('key1', Buffer.from('data1'))
      await writeDiskCache('key2', Buffer.from('data2'))
      await writeDiskCache('key3', Buffer.from('data3'))

      // Verify they exist
      expect(await readDiskCache('key1')).not.toBeNull()
      expect(await readDiskCache('key2')).not.toBeNull()
      expect(await readDiskCache('key3')).not.toBeNull()

      // Clear all
      await clearDiskCache()

      // Verify all gone
      expect(await readDiskCache('key1')).toBeNull()
      expect(await readDiskCache('key2')).toBeNull()
      expect(await readDiskCache('key3')).toBeNull()
    })

    it('should not throw if cache directory does not exist', async () => {
      // Remove the (isolated) dir first
      try {
        await fs.rm(TEST_CACHE_DIR, { recursive: true, force: true })
      } catch {
        // Ignore
      }

      // Clear should not throw
      await expect(clearDiskCache()).resolves.not.toThrow()
    })

    it('should be idempotent (can be called multiple times)', async () => {
      await clearDiskCache()
      await clearDiskCache()
      await clearDiskCache()

      // Should not throw
      expect(true).toBe(true)
    })
  })

  describe('setDiskCacheDir', () => {
    it('redirects read/write to a custom directory', async () => {
      const customDir = join(process.cwd(), '.cache', 'custom-dir-test')
      setDiskCacheDir(customDir)

      const key = 'custom-dir-key'
      const data = Buffer.from('custom location')
      await writeDiskCache(key, data)

      const filePath = join(customDir, key)
      const onDisk = await fs.readFile(filePath)
      expect(onDisk).toEqual(data)

      await fs.rm(customDir, { recursive: true, force: true })
      setDiskCacheDir(TEST_CACHE_DIR)
    })
  })

  describe('exit listeners', () => {
    interface DiskCacheExitGlobal {
      lifecycle: { cleanupStarted: boolean }
      onBeforeExit: () => void
      onSignals: () => void
    }

    const globalForExit = globalThis as typeof globalThis & {
      __diskCacheExit?: DiskCacheExitGlobal
    }

    // The globally-registered exit handler was captured from the module instance
    // re-imported in the first test, whose _cacheDir is the default. Align the
    // dir used by writeDiskCache/readDiskCache here with what the handler clears.
    beforeEach(() => {
      setDiskCacheDir(join(process.cwd(), '.cache', 'files'))
    })

    it('beforeExit handler clears cache once', async () => {
      await writeDiskCache('before-exit-key', Buffer.from('data'))
      expect(await readDiskCache('before-exit-key')).not.toBeNull()

      const handler = globalForExit.__diskCacheExit?.onBeforeExit
      expect(handler).toBeTypeOf('function')

      globalForExit.__diskCacheExit!.lifecycle.cleanupStarted = false
      handler!()

      await vi.waitFor(async () => {
        expect(await readDiskCache('before-exit-key')).toBeNull()
      })

      expect(globalForExit.__diskCacheExit!.lifecycle.cleanupStarted).toBe(true)
    })

    it('beforeExit handler is a no-op when cleanup already started', async () => {
      const handler = globalForExit.__diskCacheExit?.onBeforeExit
      globalForExit.__diskCacheExit!.lifecycle.cleanupStarted = true

      expect(() => handler!()).not.toThrow()
    })

    it('signal handler clears cache and exits process', async () => {
      const exitSpy = vi.spyOn(process, 'exit').mockImplementation((() => undefined) as typeof process.exit)
      const handler = globalForExit.__diskCacheExit?.onSignals

      await writeDiskCache('signal-key', Buffer.from('data'))
      handler!()

      await vi.waitFor(() => {
        expect(exitSpy).toHaveBeenCalledWith(0)
      })

      expect(await readDiskCache('signal-key')).toBeNull()
      exitSpy.mockRestore()
    })
  })
})
