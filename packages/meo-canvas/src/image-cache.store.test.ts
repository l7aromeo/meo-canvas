// The two stores' own contracts, case by case: the memory store's
// configuration and accounting, and the disk store's files changed behind its
// back. The crate's `memory.rs` and `disk.rs` tests hold the same cases.

import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs'
import { unlink } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'

import { createDiskStore, diskImageStore, encode, fileName, TEMP_FILE_MAX_AGE } from './image-cache-disk.js'
import { ENTRY_OVERHEAD_BYTES, ImageStoreError, memoryImageStore, type ImageCacheEntry } from './image-cache.js'

/** A plain entry whose body is `body`. */
function entry(body: Uint8Array | number[]): ImageCacheEntry {
  return { bytes: Uint8Array.from(body), etag: '"e"', storedAt: 1_000_000, expiresAt: 1_060_000, staleIfErrorUntil: 2_000_000, revalidateAlways: false }
}

const ones = (n: number): number[] => Array.from({ length: n }, () => 1)

describe('memoryImageStore', () => {
  it.each([0, -1, 1.5, Number.NaN, Number.POSITIVE_INFINITY])('refuses maxBytes %d', maxBytes => {
    expect(() => memoryImageStore({ maxBytes })).toThrow(RangeError)
  })

  it('refuses maxEntryBytes above maxBytes rather than clamping it', () => {
    expect(() => memoryImageStore({ maxBytes: 100, maxEntryBytes: 101 })).toThrow(/above `maxBytes`/)
    expect(() => memoryImageStore({ maxBytes: 100, maxEntryBytes: -1 })).toThrow(RangeError)
    expect(() => memoryImageStore({ maxBytes: 100, maxEntryBytes: 100 })).not.toThrow()
  })

  it('bounds many tiny entries by the overhead', async () => {
    // A three-byte key and a one-byte body cost `4 + overhead`. Without the
    // overhead all hundred would fit in the 400 bytes they hold.
    const each = 4 + ENTRY_OVERHEAD_BYTES
    const store = memoryImageStore({ maxBytes: 4 * each })
    for (let n = 0; n < 100; n += 1) await store.set(`k${String(n).padStart(2, '0')}`, entry([1]))
    expect(store.stats?.()).toEqual({ stored: 100, evicted: 96, bytes: 4 * each, entries: 4 })
  })

  it('adjusts the total by the difference when an entry is replaced', async () => {
    const store = memoryImageStore({ maxBytes: 10_000 })
    await store.set('a', entry(ones(100)))
    await store.set('a', entry(ones(40)))
    expect(store.stats?.()).toMatchObject({ entries: 1, bytes: 40 + 1 + ENTRY_OVERHEAD_BYTES })
    await store.delete('a')
    await store.delete('a')
    expect(store.stats?.()).toMatchObject({ entries: 0, bytes: 0 })
  })

  it('makes an entry the newest when it is read', async () => {
    const one = 100 + 1 + ENTRY_OVERHEAD_BYTES
    const store = memoryImageStore({ maxBytes: 2 * one })
    await store.set('a', entry(ones(100)))
    await store.set('b', entry(ones(100)))
    expect(await store.get('a')).toBeDefined()
    await store.set('c', entry(ones(100)))
    expect(await store.get('a')).toBeDefined()
    expect(await store.get('b')).toBeUndefined()
  })

  it('does not store an entry over its limits, and evicts nothing for it', async () => {
    const store = memoryImageStore({ maxBytes: 10_000, maxEntryBytes: 50 })
    await store.set('small', entry(ones(10)))
    await store.set('big', entry(ones(51)))
    expect(await store.get('big')).toBeUndefined()
    expect(await store.get('small')).toBeDefined()
    const tight = memoryImageStore({ maxBytes: 60 })
    await tight.set('k', entry(ones(50)))
    expect(tight.stats?.().entries).toBe(0)
  })

  it('empties the total on clear', async () => {
    const store = memoryImageStore({ maxBytes: 10_000 })
    await store.set('a', entry(ones(10)))
    await store.clear()
    expect(store.stats?.()).toMatchObject({ entries: 0, bytes: 0 })
    await store.set('a', entry(ones(10)))
    expect(store.stats?.().bytes).toBe(10 + 1 + ENTRY_OVERHEAD_BYTES)
  })
})

describe('diskImageStore', () => {
  const dirs: string[] = []
  afterEach(() => {
    for (const dir of dirs.splice(0)) {
      try {
        chmodSync(dir, 0o755)
      } catch {
        // Already gone.
      }
      rmSync(dir, { recursive: true, force: true })
    }
  })
  const scratch = (): string => {
    const dir = mkdtempSync(join(tmpdir(), 'meo-canvas-disk-store-'))
    dirs.push(dir)
    return dir
  }
  const sized = (key: string, body: number): number => encode(key, entry(ones(body))).byteLength
  const age = (dir: string, key: string, at: number): void => utimesSync(join(dir, fileName(key)), at / 1000, at / 1000)
  const corrupt = async (pending: Promise<unknown>): Promise<boolean> =>
    pending.then(
      () => false,
      (error: unknown) => error instanceof ImageStoreError && error.corrupt,
    )

  it.each([0, -1, 1.5, Number.NaN, Number.POSITIVE_INFINITY])('refuses maxBytes %d', maxBytes => {
    expect(() => diskImageStore({ dir: scratch(), maxBytes })).toThrow(RangeError)
  })

  it('refuses maxEntryBytes above maxBytes, and a missing dir', () => {
    expect(() => diskImageStore({ dir: scratch(), maxBytes: 400, maxEntryBytes: 401 })).toThrow(RangeError)
    expect(() => diskImageStore({ dir: '', maxBytes: 400 })).toThrow(TypeError)
  })

  it('persists an entry across two store instances', async () => {
    const dir = scratch()
    await diskImageStore({ dir, maxBytes: 1 << 20 }).set('k', entry([1, 2, 3]))
    const again = diskImageStore({ dir, maxBytes: 1 << 20 })
    expect([...((await again.get('k'))?.bytes ?? [])]).toEqual([1, 2, 3])
    expect(again.stats?.().bytes).toBe(statSync(join(dir, fileName('k'))).size)
  })

  it('treats a second delete as harmless', async () => {
    const store = diskImageStore({ dir: scratch(), maxBytes: 1 << 20 })
    await store.set('k', entry([1]))
    await store.delete('k')
    await store.delete('k')
    expect(await store.get('k')).toBeUndefined()
    expect(store.stats?.()).toMatchObject({ entries: 0, bytes: 0 })
  })

  it("clears only the store's own files", async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    await store.set('a', entry([1]))
    await store.set('b', entry([2]))
    for (const kept of ['notes.txt', 'a.entry', 'tmp-x']) writeFileSync(join(dir, kept), "the caller's")
    writeFileSync(join(dir, `tmp-${fileName('c').slice(0, 64)}-0123456789abcdef`), 'partial')
    await store.clear()
    expect(readdirSync(dir).sort()).toEqual(['a.entry', 'notes.txt', 'tmp-x'])
    expect(store.stats?.().entries).toBe(0)
  })

  it('evicts the oldest by modification time', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 3 * sized('k1', 100) })
    for (const key of ['k1', 'k2', 'k3']) await store.set(key, entry(ones(100)))
    const base = Date.now() - 600_000
    age(dir, 'k2', base)
    age(dir, 'k3', base + 10_000)
    age(dir, 'k1', base + 20_000)
    await store.set('k4', entry(ones(100)))
    expect(await store.get('k2')).toBeUndefined()
    expect(await store.get('k3')).toBeDefined()
    expect(await store.get('k1')).toBeDefined()
    expect(await store.get('k4')).toBeDefined()
    expect(store.stats?.().evicted).toBe(1)
  })

  it('makes an entry the newest when it is read', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 2 * sized('k1', 100) })
    await store.set('k1', entry(ones(100)))
    await store.set('k2', entry(ones(100)))
    const old = Date.now() - 600_000
    age(dir, 'k1', old)
    age(dir, 'k2', old + 1000)
    expect(await store.get('k1')).toBeDefined()
    await store.set('k3', entry(ones(100)))
    expect(await store.get('k2')).toBeUndefined()
    expect(await store.get('k1')).toBeDefined()
  })

  it('evicts equal modification times in file-name order', async () => {
    const dir = scratch()
    const keys = ['k1', 'k2', 'k3']
    const store = diskImageStore({ dir, maxBytes: 3 * sized('k1', 100) })
    for (const key of keys) await store.set(key, entry(ones(100)))
    const same = Date.now() - 600_000
    for (const key of keys) age(dir, key, same)
    const first = [...keys].sort((a, b) => (fileName(a) < fileName(b) ? -1 : 1))[0] ?? ''
    await store.set('k4', entry(ones(100)))
    expect(await store.get(first)).toBeUndefined()
    let left = 0
    for (const key of keys) if ((await store.get(key)) !== undefined) left += 1
    expect(left).toBe(2)
  })

  it('skips a file another process already removed when evicting', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 2 * sized('k1', 100) })
    await store.set('k1', entry(ones(100)))
    await store.set('k2', entry(ones(100)))
    await unlink(join(dir, fileName('k1')))
    await store.set('k3', entry(ones(100)))
    expect(await store.get('k2')).toBeDefined()
    expect(await store.get('k3')).toBeDefined()
    expect(store.stats?.().evicted).toBe(0)
  })

  it('does not store an entry larger than maxBytes or maxEntryBytes', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 400 })
    await store.set('small', entry(ones(10)))
    await store.set('big', entry(ones(400)))
    expect(await store.get('big')).toBeUndefined()
    expect(await store.get('small')).toBeDefined()
    const limited = diskImageStore({ dir, maxBytes: 400, maxEntryBytes: 5 })
    await limited.set('six', entry(ones(6)))
    expect(await limited.get('six')).toBeUndefined()
  })

  it('treats an entry file deleted outside as a plain miss', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    await store.set('k', entry([1, 2, 3]))
    expect(store.stats?.().bytes).toBeGreaterThan(0)
    await unlink(join(dir, fileName('k')))
    expect(await store.get('k')).toBeUndefined()
    expect(store.stats?.()).toMatchObject({ entries: 0, bytes: 0 })
    await store.set('k', entry([1, 2, 3]))
    expect(await store.get('k')).toBeDefined()
  })

  it('treats the whole dir deleted as a miss, and creates it again on the next write', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    await store.set('k', entry([1]))
    rmSync(dir, { recursive: true })
    expect(await store.get('k')).toBeUndefined()
    await store.clear()
    await store.set('k', entry([1]))
    expect(await store.get('k')).toBeDefined()
  })

  it('reads a truncated file as corrupt', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    await store.set('k', entry([1, 2, 3, 4, 5, 6]))
    const path = join(dir, fileName('k'))
    writeFileSync(path, readFileSync(path).subarray(0, -1))
    expect(await corrupt(store.get('k'))).toBe(true)
  })

  it('reads a file of garbage as corrupt', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    await store.set('k', entry([1]))
    writeFileSync(join(dir, fileName('k')), '<html>not an entry</html>')
    expect(await corrupt(store.get('k'))).toBe(true)
  })

  it('reads one flipped byte as corrupt', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    await store.set('k', entry([1, 2, 3, 4, 5, 6]))
    const path = join(dir, fileName('k'))
    const whole = readFileSync(path)
    whole[whole.length - 1] = (whole[whole.length - 1] ?? 0) ^ 1
    writeFileSync(path, whole)
    expect(await corrupt(store.get('k'))).toBe(true)
  })

  it('reads another version as corrupt', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    const file = encode('k', entry([1]))
    file[8] = 2
    mkdirSync(dir, { recursive: true })
    writeFileSync(join(dir, fileName('k')), file)
    expect(await corrupt(store.get('k'))).toBe(true)
  })

  it('treats a file for another key as a miss, and keeps it', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    await store.set('a', entry([1]))
    copyFileSync(join(dir, fileName('a')), join(dir, fileName('b')))
    expect(await store.get('b')).toBeUndefined()
    expect(existsSync(join(dir, fileName('b')))).toBe(true)
  })

  it('reads a file deleted between opening and reading it whole', async () => {
    const dir = scratch()
    const hooks = { afterOpen: (path: string) => unlink(path) }
    const store = createDiskStore({ dir, maxBytes: 1 << 20 }, hooks)
    await store.set('k', entry([1, 2, 3, 4, 5, 6]))
    const read = await store.get('k')
    expect([...(read?.bytes ?? [])]).toEqual([1, 2, 3, 4, 5, 6])
    expect(existsSync(join(dir, fileName('k')))).toBe(false)
  })

  it('leaves one whole file when two writers write one key', async () => {
    const dir = scratch()
    const [first, second] = [diskImageStore({ dir, maxBytes: 1 << 24 }), diskImageStore({ dir, maxBytes: 1 << 24 })]
    const bodies = [new Uint8Array(64 * 1024).fill(1), new Uint8Array(96 * 1024).fill(2)]
    const writes = [first, second].map(async (store, at) => {
      for (let n = 0; n < 30; n += 1) await store.set('k', entry(bodies[at] ?? []))
    })
    const reads = (async () => {
      for (let n = 0; n < 60; n += 1) {
        const found = await first.get('k')
        if (found !== undefined)
          expect(bodies.some(body => body.byteLength === found.bytes.byteLength && body.every((byte, i) => found.bytes[i] === byte))).toBe(true)
      }
    })()
    await Promise.all([...writes, reads])
    expect(await first.get('k')).toBeDefined()
    expect(readdirSync(dir).filter(name => name.startsWith('tmp-'))).toEqual([])
  })

  it('stores nothing and reports nothing when the rename is refused', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    // A directory with something in it where the entry would go: no rename
    // replaces it, on any platform.
    mkdirSync(join(dir, fileName('k'), 'held'), { recursive: true })
    await store.set('k', entry([1]))
    expect(readdirSync(dir).filter(name => name.startsWith('tmp-'))).toEqual([])
    expect(store.stats?.().stored).toBe(0)
  })

  it('removes the temp file and rejects when the disk is full', async () => {
    const dir = scratch()
    const hooks: { failWrites?: Error } = { failWrites: Object.assign(new Error('no space left on device (injected)'), { code: 'ENOSPC' }) }
    const store = createDiskStore({ dir, maxBytes: 1 << 20 }, hooks)
    await expect(store.set('k', entry([1]))).rejects.toThrow(/no space left/)
    expect(readdirSync(dir)).toEqual([])
    delete hooks.failWrites
    await store.set('k', entry([1]))
    expect(await store.get('k')).toBeDefined()
  })

  it('removes temp files older than an hour when it opens, and keeps younger ones', async () => {
    const dir = scratch()
    const hash = fileName('k').slice(0, 64)
    const [old, young] = [`tmp-${hash}-00000000000000aa`, `tmp-${hash}-00000000000000bb`]
    for (const name of [old, young]) writeFileSync(join(dir, name), 'partial')
    const now = Date.now()
    utimesSync(join(dir, old), (now - TEMP_FILE_MAX_AGE - 60_000) / 1000, (now - TEMP_FILE_MAX_AGE - 60_000) / 1000)
    utimesSync(join(dir, young), (now - TEMP_FILE_MAX_AGE + 60_000) / 1000, (now - TEMP_FILE_MAX_AGE + 60_000) / 1000)
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    expect(await store.get('k')).toBeUndefined()
    expect(readdirSync(dir)).toEqual([young])
  })

  it.skipIf(process.platform === 'win32')('serves lookups from a read-only dir and rejects writes', async () => {
    const dir = scratch()
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    await store.set('k', entry([1]))
    chmodSync(dir, 0o555)
    // Root ignores the mode, and nothing here could be refused then.
    try {
      writeFileSync(join(dir, 'probe'), '')
      return
    } catch {
      // Refused, as it should be.
    }
    expect(await store.get('k')).toBeDefined()
    await expect(store.set('j', entry([1]))).rejects.toThrow()
    await expect(store.delete('k')).rejects.toThrow()
  })

  it.skipIf(process.platform === 'win32')('rejects a write when the dir cannot be created again', async () => {
    const parent = scratch()
    const dir = join(parent, 'cache')
    const store = diskImageStore({ dir, maxBytes: 1 << 20 })
    await store.set('k', entry([1]))
    rmSync(dir, { recursive: true })
    chmodSync(parent, 0o555)
    try {
      mkdirSync(join(parent, 'probe'))
      return
    } catch {
      // Refused, as it should be.
    }
    await expect(store.set('k', entry([1]))).rejects.toThrow()
    expect(await store.get('k')).toBeUndefined()
  })
})
