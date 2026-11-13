/**
 * {@link diskImageStore}: one file per entry in a directory, in the format the
 * crate's `DiskStore` reads and writes, which its module documentation lays
 * out byte by byte. `entry.tsv` and the file beside it pin the layout on both
 * surfaces.
 * @packageDocumentation
 */

import { createHash, randomBytes } from 'node:crypto'
import { mkdir, open, readdir, rename, stat, unlink, utimes } from 'node:fs/promises'
import { join } from 'node:path'

import { type ImageCacheEntry, type ImageStore, type ImageStoreStats, ImageStoreError, storeLimits } from './image-cache.js'

/** The first bytes of every entry file. */
const MAGIC = new TextEncoder().encode('MEOIMGC\n')

/** The layout this build reads and writes. */
const VERSION = 1

/** Magic, version, the two lengths and the checksum. */
const HEADER_BYTES = 8 + 4 + 4 + 8 + 32

/** The flag bit for an entry that revalidates on every use. */
const REVALIDATE_ALWAYS = 1

/** The flag bit for an entry with an `ETag`. */
const HAS_ETAG = 2

/** The flag bit for an entry with a `Last-Modified`. */
const HAS_LAST_MODIFIED = 4

/** The flag bit for each stored header after `Last-Modified`, in file order. */
const HAS_HEADER = [8, 16, 32, 64, 128] as const

/** The headers an entry keeps after its validators, in file order. */
const HEADER_FIELDS = ['cacheControl', 'expires', 'pragma', 'date', 'age'] as const

/** An entry file's name: a SHA-256 in lower-case hex, then `.entry`. */
const ENTRY = /^[0-9a-f]{64}\.entry$/

/** A file being written, which no lookup ever reads: `tmp-`, the entry's hash, a unique part. */
const TEMP = /^tmp-[0-9a-f]{64}-[0-9a-f]{16}$/

/**
 * How old a temp file must be before a disk store removes it when it opens, in
 * milliseconds: an hour. A younger one may belong to a writer in another
 * process; writing one entry, at most 32 MiB to a local disk, takes seconds, so
 * an hour old is a crash. The crate's `TEMP_FILE_MAX_AGE` is the same.
 */
export const TEMP_FILE_MAX_AGE = 60 * 60 * 1000

/** What {@link diskImageStore} takes. */
export interface DiskImageStoreOptions {
  /** The directory the entries live in, created if missing. */
  readonly dir: string
  /** The most the files may total, in bytes: a whole number above 0. Required, since no size suits every server. */
  readonly maxBytes: number
  /**
   * The largest body it keeps, in bytes, at most `maxBytes`. Defaults to the
   * smaller of `maxBytes` and `DEFAULT_MAX_ENTRY_BYTES`.
   */
  readonly maxEntryBytes?: number
}

/** The file an entry for `key` lives in. @internal */
export function fileName(key: string): string {
  return `${createHash('sha256').update(key, 'utf8').digest('hex')}.entry`
}

/** A Node filesystem error's code, if it has one. */
function code(error: unknown): string | undefined {
  return typeof error === 'object' && error !== null && 'code' in error && typeof error.code === 'string' ? error.code : undefined
}

/** Removes a file, counting one already gone as removed. */
async function remove(path: string): Promise<void> {
  try {
    await unlink(path)
  } catch (error) {
    if (code(error) !== 'ENOENT') throw error
  }
}

/** Milliseconds as the unsigned 64-bit integer the file holds, 0 before the epoch. */
function u64(ms: number): bigint {
  return ms <= 0 ? 0n : BigInt(Math.trunc(ms))
}

/** The entry file for `key`. @internal */
export function encode(key: string, entry: ImageCacheEntry): Uint8Array {
  const text = new TextEncoder()
  const strings = [key, entry.etag ?? '', entry.lastModified ?? '', ...HEADER_FIELDS.map(field => entry[field] ?? '')].map(string => text.encode(string))
  const metaLength = 8 * 3 + 1 + strings.reduce((sum, string) => sum + 4 + string.byteLength, 0)
  const file = new Uint8Array(HEADER_BYTES + metaLength + entry.bytes.byteLength)
  const view = new DataView(file.buffer)

  let at = HEADER_BYTES
  for (const when of [entry.storedAt, entry.expiresAt, entry.staleIfErrorUntil]) {
    view.setBigUint64(at, u64(when), true)
    at += 8
  }
  file[at] = HEADER_FIELDS.reduce(
    (flags, field, index) => flags | (entry[field] === undefined ? 0 : (HAS_HEADER[index] ?? 0)),
    (entry.revalidateAlways ? REVALIDATE_ALWAYS : 0) | (entry.etag === undefined ? 0 : HAS_ETAG) | (entry.lastModified === undefined ? 0 : HAS_LAST_MODIFIED),
  )
  at += 1
  for (const string of strings) {
    view.setUint32(at, string.byteLength, true)
    file.set(string, at + 4)
    at += 4 + string.byteLength
  }
  file.set(entry.bytes, at)

  file.set(MAGIC, 0)
  view.setUint32(8, VERSION, true)
  view.setUint32(12, metaLength, true)
  view.setBigUint64(16, BigInt(entry.bytes.byteLength), true)
  file.set(createHash('sha256').update(file.subarray(HEADER_BYTES)).digest(), 24)
  return file
}

/**
 * The entry in `file`, `undefined` if it holds another key, or a corrupt
 * {@link ImageStoreError} if it is not an entry file this build reads.
 * @internal
 */
export function decode(file: Uint8Array, key: string): ImageCacheEntry | undefined {
  const corrupt = (message: string): ImageStoreError => new ImageStoreError(message, { corrupt: true })
  if (file.byteLength < HEADER_BYTES) throw corrupt('the file is shorter than it says')
  if (MAGIC.some((byte, at) => file[at] !== byte)) throw corrupt('the file does not begin as an entry')
  const view = new DataView(file.buffer, file.byteOffset, file.byteLength)
  const version = view.getUint32(8, true)
  if (version !== VERSION) throw corrupt(`the file is version ${version}, and this build reads ${VERSION}`)
  const metaLength = view.getUint32(12, true)
  const bodyLength = view.getBigUint64(16, true)
  const rest = file.byteLength - HEADER_BYTES
  if (BigInt(metaLength) + bodyLength !== BigInt(rest)) {
    throw corrupt(`the file holds ${rest} bytes after its header where it says ${BigInt(metaLength) + bodyLength}`)
  }
  const checksum = createHash('sha256').update(file.subarray(HEADER_BYTES)).digest()
  if (checksum.some((byte, at) => file[24 + at] !== byte)) throw corrupt('the checksum does not match')

  let at = HEADER_BYTES
  const end = HEADER_BYTES + metaLength
  const take = (n: number): number => {
    if (at + n > end) throw corrupt('the file is shorter than it says')
    const was = at
    at += n
    return was
  }
  const when = (): number => Number(view.getBigUint64(take(8), true))
  const [storedAt, expiresAt, staleIfErrorUntil] = [when(), when(), when()]
  const flags = file[take(1)] ?? 0
  const strict = new TextDecoder('utf-8', { fatal: true })
  const text = (): string => {
    const length = view.getUint32(take(4), true)
    const start = take(length)
    try {
      return strict.decode(file.subarray(start, start + length))
    } catch {
      throw corrupt('a string in the file is not UTF-8')
    }
  }
  const [held, etag, lastModified] = [text(), text(), text()]
  const headers: Record<string, string> = {}
  for (const [index, field] of HEADER_FIELDS.entries()) {
    const value = text()
    if ((flags & (HAS_HEADER[index] ?? 0)) !== 0) headers[field] = value
  }
  if (at !== end) throw corrupt('the metadata runs past its fields')
  if (held !== key) return undefined
  return {
    bytes: file.subarray(end),
    ...((flags & HAS_ETAG) === 0 ? {} : { etag }),
    ...((flags & HAS_LAST_MODIFIED) === 0 ? {} : { lastModified }),
    ...headers,
    storedAt,
    expiresAt,
    staleIfErrorUntil,
    revalidateAlways: (flags & REVALIDATE_ALWAYS) !== 0,
  }
}

/**
 * What a test reaches into a disk store with.
 * @internal
 */
export interface DiskStoreHooks {
  /** Fails each write with this error instead of writing, so a test can fill the disk. */
  failWrites?: Error | undefined
  /** Runs between opening an entry file and reading it. */
  afterOpen?: ((path: string) => Promise<void>) | undefined
}

/**
 * Entries as files in one directory, evicted oldest first by modification time
 * once they pass `maxBytes`, and kept across restarts. A Rust process using the
 * crate's `DiskStore` can share the directory: the two write the same files.
 *
 * Throws a `RangeError` when `maxBytes` is not a whole number above 0 or
 * `maxEntryBytes` is above it. The directory is created and scanned on first
 * use: entries already there count toward `maxBytes`, the oldest are evicted
 * if they pass it, and temp files older than {@link TEMP_FILE_MAX_AGE} are
 * removed.
 *
 * **What it guarantees.** The files are the truth: the size the store keeps in
 * memory is an estimate that a directory scan corrects, and anything it finds
 * missing or wrong is a miss, never an error that reaches a render.
 *
 * - **Writes are whole.** An entry is written to a temp file in the same
 *   directory, synced, and renamed into place, so a partial file never has an
 *   entry's name. Two processes writing one key each rename their own file;
 *   the last wins, and a reader sees one whole file or the other.
 * - **Corrupt files are dropped.** A wrong magic, version, length or checksum
 *   is a miss with a diagnostic, and the file is deleted. A file holding another
 *   key is a miss and is left alone, since it may be valid for its own key.
 * - **Files removed from outside are misses.** Deleted by hand, by a cleanup
 *   job or by another process's eviction, an entry is fetched and stored again
 *   with no diagnostic. If the whole directory goes, the next write creates it
 *   again.
 * - **Eviction** removes the least recently used entries, by modification time,
 *   which a hit refreshes, until the total is under `maxBytes`; entries with the
 *   same time go in file-name order. A file another process already removed is
 *   skipped, and the entry just written is never the one evicted. An entry
 *   larger than `maxEntryBytes`, or than `maxBytes`, is not stored.
 * - **Failed writes cost nothing but the cache.** A full disk, a write error or
 *   a read-only directory removes the temp file and reports a diagnostic; the
 *   render draws the bytes it fetched. A rename the system refuses stores
 *   nothing and reports nothing; the next render tries again.
 * - **Crashes leave temp files**, named `tmp-...`, which no lookup reads.
 * - **`clear` removes only the store's own files**: entries and temp files,
 *   never anything else a caller keeps in the directory.
 *
 * The bytes `get` returns are read from the file for each call.
 *
 * ```ts
 * import { createImageCache, diskImageStore } from 'meo-canvas'
 *
 * // Once, at start-up: 2 GiB under the given directory.
 * const imageCache = createImageCache({
 *   store: diskImageStore({ dir: '/var/cache/cards', maxBytes: 2 * 1024 ** 3 }),
 * })
 * ```
 */
export function diskImageStore(options: DiskImageStoreOptions): ImageStore {
  return createDiskStore(options, {})
}

/** {@link diskImageStore}, with hooks a test reaches in through. @internal */
export function createDiskStore(options: DiskImageStoreOptions, hooks: DiskStoreHooks): ImageStore {
  if (typeof options?.dir !== 'string' || options.dir === '') throw new TypeError('`diskImageStore` needs a `dir`')
  const dir = options.dir
  const { maxBytes, maxEntryBytes } = storeLimits(options)
  // Each entry file's size, by name, and their sum: an estimate the scans correct.
  let sizes = new Map<string, number>()
  let total = 0
  let stored = 0
  let evicted = 0
  let ready: Promise<void> | undefined

  const record = (name: string, size: number): void => {
    total += size - (sizes.get(name) ?? 0)
    sizes.set(name, size)
  }
  const forget = (name: string): void => {
    total -= sizes.get(name) ?? 0
    sizes.delete(name)
  }

  /** Rescans the directory, correcting the estimate, then removes the oldest entries until the total is under `maxBytes`, sparing `keep`. */
  const evict = async (keep?: string): Promise<void> => {
    let names: string[]
    try {
      names = await readdir(dir)
    } catch (error) {
      if (code(error) !== 'ENOENT') throw error
      sizes = new Map()
      total = 0
      return
    }
    const found: { name: string; modified: number; size: number }[] = []
    for (const name of names) {
      if (!ENTRY.test(name)) continue
      try {
        const info = await stat(join(dir, name))
        found.push({ name, modified: info.mtimeMs, size: info.size })
      } catch {
        // Gone since the listing: another process evicted it.
      }
    }
    found.sort((a, b) => a.modified - b.modified || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    const kept = new Map(found.map(({ name, size }) => [name, size]))
    let sum = found.reduce((all, { size }) => all + size, 0)
    for (const { name, size } of found) {
      if (sum <= maxBytes) break
      if (name === keep) continue
      try {
        await unlink(join(dir, name))
        evicted += 1
      } catch (error) {
        // Held open on Windows, or not ours to remove: left for the next
        // eviction, and the total stays honest about it.
        if (code(error) !== 'ENOENT') continue
      }
      kept.delete(name)
      sum -= size
    }
    sizes = kept
    total = sum
  }

  /** Creates and scans the directory once, retried after a failure. */
  const opened = (): Promise<void> => {
    ready ??= (async () => {
      await mkdir(dir, { recursive: true })
      const now = Date.now()
      for (const name of await readdir(dir)) {
        if (!TEMP.test(name)) continue
        try {
          const info = await stat(join(dir, name))
          if (now - info.mtimeMs > TEMP_FILE_MAX_AGE) await remove(join(dir, name))
        } catch {
          // Another user's file, or one a writer just finished: either way
          // not this store's to insist on.
        }
      }
      await evict()
    })().catch((error: unknown) => {
      ready = undefined
      throw error
    })
    return ready
  }

  return {
    async get(key) {
      await opened()
      const name = fileName(key)
      const path = join(dir, name)
      let file: Uint8Array
      try {
        const handle = await open(path, 'r')
        try {
          await hooks.afterOpen?.(path)
          file = await handle.readFile()
        } finally {
          await handle.close()
        }
      } catch (error) {
        if (code(error) !== 'ENOENT') throw error
        forget(name)
        return undefined
      }
      const entry = decode(file, key)
      if (entry === undefined) return undefined
      // A hit makes the entry the newest. Failing to is not worth a
      // diagnostic: the entry is merely evicted a little sooner.
      const now = new Date()
      await utimes(path, now, now).catch(() => undefined)
      record(name, file.byteLength)
      return entry
    },

    async set(key, entry) {
      await opened()
      if (entry.bytes.byteLength > maxEntryBytes) return
      const file = encode(key, entry)
      if (file.byteLength > maxBytes) return
      const name = fileName(key)
      await mkdir(dir, { recursive: true })
      const temp = join(dir, `tmp-${name.slice(0, 64)}-${randomBytes(8).toString('hex')}`)
      try {
        const handle = await open(temp, 'wx')
        try {
          if (hooks.failWrites !== undefined) throw hooks.failWrites
          await handle.writeFile(file)
          await handle.sync()
        } finally {
          await handle.close()
        }
      } catch (error) {
        await remove(temp).catch(() => undefined)
        throw error
      }
      try {
        await rename(temp, join(dir, name))
      } catch {
        await remove(temp).catch(() => undefined)
        return
      }
      record(name, file.byteLength)
      stored += 1
      if (total > maxBytes) await evict(name)
    },

    async delete(key) {
      await opened()
      const name = fileName(key)
      await remove(join(dir, name))
      forget(name)
    },

    async clear() {
      await opened()
      let names: string[]
      try {
        names = await readdir(dir)
      } catch (error) {
        if (code(error) !== 'ENOENT') throw error
        sizes = new Map()
        total = 0
        return
      }
      let first: Error | undefined
      for (const name of names) {
        if (!ENTRY.test(name) && !TEMP.test(name)) continue
        try {
          await remove(join(dir, name))
          forget(name)
        } catch (error) {
          first ??= error instanceof Error ? error : new Error(String(error))
        }
      }
      if (first !== undefined) throw first
    },

    stats(): ImageStoreStats {
      return { stored, evicted, bytes: total, entries: sizes.size }
    },
  }
}
