/**
 * Fetched images kept across renders: an {@link ImageCache} holding the policy,
 * and an {@link ImageStore} holding the bytes — {@link memoryImageStore},
 * {@link diskImageStore}, or one of the caller's own.
 *
 * The freshness rules here and `meo-canvas-core`'s `image_cache` answer every
 * row of the shared `freshness.tsv` alike.
 * @packageDocumentation
 */

import { httpDate } from './http-date.js'
import type { Diagnostic } from './index.js'

/** Milliseconds in a second. */
const MS_PER_SECOND = 1000

/**
 * How long past expiry a response with no `stale-if-error` of its own may be
 * served when revalidating it fails, in milliseconds: 24 hours.
 *
 * A day outlasts an origin outage of the length incidents usually run, a
 * night included, so a render does not lose its images to one; and it bounds
 * how long an image the origin has since replaced can keep being drawn while
 * the origin cannot be reached. Set per cache with
 * {@link ImageCacheOptions.staleIfError}.
 */
export const DEFAULT_STALE_IF_ERROR = 24 * 60 * 60 * MS_PER_SECOND

/**
 * What one stored entry costs a {@link memoryImageStore} beyond its body and its
 * key, in bytes: 256.
 *
 * It stands for the metadata and the map slot: five times and a flag, a
 * typical `Last-Modified` (29 bytes) and quoted `ETag` (under 70), and a map
 * slot. Counting it is what stops many tiny entries from growing the store past
 * its `maxBytes` while their bodies stay under it. The crate's `MemoryStore`
 * uses the same number.
 */
export const ENTRY_OVERHEAD_BYTES = 256

/**
 * The largest body a store keeps when the caller names no `maxEntryBytes`, in
 * bytes: 32 MiB, the largest image a render fetches, so by default every image
 * that can be fetched can be kept.
 */
export const DEFAULT_MAX_ENTRY_BYTES = 32 * 1024 * 1024

/**
 * The longest lifetime a response can claim, in seconds: RFC 9111 §1.2.2 tells
 * a cache to read any larger `delta-seconds` as 2^31.
 */
const MAX_DELTA_SECONDS = 2 ** 31

/**
 * The share of a response's age since `Last-Modified` it stays fresh for when
 * nothing else says: RFC 9111 §4.2.2's "10%", as Chrome uses it
 * (`http-cache.tsv`: "heuristic, age 90 of 100" and "age 110 of 100").
 */
const HEURISTIC_DIVISOR = 10

/** `200 OK`, the one status stored. */
const OK = 200

/** `304 Not Modified`, the answer to a revalidation that keeps the bytes. */
const NOT_MODIFIED = 304

/** The first status a stale-if-error fallback answers: a 5xx. */
const SERVER_ERROR = 500

/**
 * One cached response: its body, and what decides when it is used again.
 *
 * A store keeps it whole and hands it back whole; only the {@link ImageCache}
 * reads the times. The fields match the crate's `ImageCacheEntry`, and
 * {@link diskImageStore} writes them in a format both surfaces read.
 */
export interface ImageCacheEntry {
  /**
   * The response body. **Not to be modified**: stores and the cache share it
   * rather than copying up to 32 MiB per hit, so a change here corrupts the
   * entry.
   */
  readonly bytes: Uint8Array
  /** The response's `ETag`, sent back as `If-None-Match` to revalidate. */
  readonly etag?: string
  /** The response's `Last-Modified`, sent back as `If-Modified-Since`. */
  readonly lastModified?: string
  /** The response's `Cache-Control`, kept so a `304` that names none leaves it in force, as Chrome keeps the stored header (`http-cache.tsv`: "304 with no Cache-Control"). */
  readonly cacheControl?: string
  /** The response's `Expires`, kept for the same reason. */
  readonly expires?: string
  /** The response's `Pragma`, kept for the same reason. */
  readonly pragma?: string
  /** The response's `Date`, kept so a `304` without one ages the entry from the stored date, as Chrome does (`http-cache.tsv`: "304 with no Date over a stored old Date"). */
  readonly date?: string
  /** The response's `Age`, kept so a `304` without one keeps it, as Chrome does (`http-cache.tsv`: "304 with no Age over a stored Age"). */
  readonly age?: string
  /** When the response was received, in milliseconds since the epoch. */
  readonly storedAt: number
  /**
   * Fresh until this instant, in milliseconds since the epoch; at it and past
   * it, the entry is revalidated before it is used.
   */
  readonly expiresAt: number
  /**
   * A failed revalidation may serve the entry until this instant, in
   * milliseconds since the epoch.
   */
  readonly staleIfErrorUntil: number
  /** The response said `no-cache`: revalidate on every use, even while fresh. */
  readonly revalidateAlways: boolean
}

/** What a store holds, for {@link ImageCache.stats}: the store's own numbers. */
export interface ImageStoreStats {
  /** Entries the store accepted. */
  readonly stored: number
  /** Entries it removed to stay under its size. */
  readonly evicted: number
  /** What it holds now, in bytes, as the store counts them. */
  readonly bytes: number
  /** How many entries it holds now. */
  readonly entries: number
}

/**
 * Where an {@link ImageCache} keeps bytes: get, set, delete and clear, and no
 * policy.
 *
 * {@link memoryImageStore} and {@link diskImageStore} are the two this package
 * ships; a caller may write another, over a shared cache server for instance.
 *
 * **What a store must uphold:**
 *
 * - **A missing key resolves `undefined`**, never rejects.
 * - **An entry comes back whole or not at all**: the bytes and every field as
 *   `set` was given them, the stored headers included: a `304` merges its own
 *   headers into those, so a store that drops one changes how long a
 *   revalidated entry stays fresh. An entry that exists and cannot be read
 *   rejects with an {@link ImageStoreError} whose `corrupt` is true, which
 *   makes the cache delete it.
 * - **`delete` is idempotent**: deleting a key that is not there resolves.
 * - **It may decline to keep an entry**, too large for instance, by resolving
 *   `set` without storing it.
 * - **The bytes `get` returns are not modified** by the cache or the render;
 *   a store may return the very array it was given.
 *
 * A rejection from any method is reported as a diagnostic on the render that
 * met it, and the render goes on as though the store were empty.
 *
 * ```ts
 * import { createImageCache, type ImageCacheEntry, type ImageStore } from 'meo-canvas'
 *
 * // Every entry, kept forever: a minimal store.
 * const entries = new Map<string, ImageCacheEntry>()
 * const unbounded: ImageStore = {
 *   get: async key => entries.get(key),
 *   set: async (key, entry) => void entries.set(key, entry),
 *   delete: async key => void entries.delete(key),
 *   clear: async () => entries.clear(),
 * }
 * const cache = createImageCache({ store: unbounded })
 * ```
 */
export interface ImageStore {
  /** The entry stored under `key`, or `undefined`. */
  get(key: string): Promise<ImageCacheEntry | undefined>
  /** Stores `entry` under `key`, replacing any entry already there. */
  set(key: string, entry: ImageCacheEntry): Promise<void>
  /** Removes the entry under `key`, if there is one. */
  delete(key: string): Promise<void>
  /** Removes every entry. */
  clear(): Promise<void>
  /**
   * What the store holds, for {@link ImageCache.stats}. A store without it
   * reports zeros; both shipped stores have it.
   */
  stats?(): ImageStoreStats
}

/**
 * Why a store could not do what it was asked. `corrupt` marks an entry that
 * exists and cannot be read as one, which the cache then deletes.
 *
 * **A store error never fails a render**: the cache treats it as a miss,
 * fetches, and reports it on {@link Canvas.diagnostics}.
 */
export class ImageStoreError extends Error {
  /** Whether the entry was unreadable rather than unreachable. */
  readonly corrupt: boolean

  /** An error described by `message`; `corrupt` when the entry itself is bad. */
  constructor(message: string, options: { readonly corrupt?: boolean; readonly cause?: unknown } = {}) {
    super(message, options.cause === undefined ? undefined : { cause: options.cause })
    this.name = 'ImageStoreError'
    this.corrupt = options.corrupt ?? false
  }
}

/**
 * What a cache has done, from {@link ImageCache.stats}.
 *
 * The first five count this cache's requests by how each was answered; a
 * request that joined another one for the same key already in flight counts as
 * that one did, so eight renders sharing one fetch count eight. The last four
 * are the store's, from {@link ImageStore.stats}, and are all zero for a store
 * without it.
 */
export interface ImageCacheStats {
  /** Served fresh from the store, with no request. */
  readonly hits: number
  /** Fetched with nothing usable stored, whether the fetch succeeded or not. */
  readonly misses: number
  /** Revalidated with a `304 Not Modified`: the stored bytes were used. */
  readonly revalidated: number
  /** Revalidated with a `200`: the new bytes replaced the stored ones. */
  readonly replaced: number
  /** Revalidation failed, and the expired bytes were served under stale-if-error. */
  readonly staleServed: number
  /** Entries the store accepted. */
  readonly stored: number
  /** Entries the store removed to stay under its size. */
  readonly evicted: number
  /** What the store holds now, in bytes. */
  readonly bytes: number
  /** How many entries the store holds now. */
  readonly entries: number
}

/** What {@link createImageCache} takes. */
export interface ImageCacheOptions {
  /** Where the bytes live: {@link memoryImageStore}, {@link diskImageStore}, or a caller's own. */
  readonly store: ImageStore
  /**
   * How long a response that names no lifetime of its own stays fresh, in
   * milliseconds, replacing the heuristic of a tenth of its age since
   * `Last-Modified`. It does not override `max-age`, `Expires` or `no-cache`.
   * Absent leaves the heuristic.
   */
  readonly fallbackTtl?: number
  /**
   * How long past expiry a failed revalidation may serve an entry whose
   * response names no `stale-if-error`, in milliseconds; a response's own
   * `stale-if-error` wins over it. Defaults to {@link DEFAULT_STALE_IF_ERROR},
   * 24 hours. `0` means never serve stale: this cache then ignores even a
   * response's own `stale-if-error`.
   */
  readonly staleIfError?: number
}

/** A non-negative finite number of milliseconds, or the refusal naming it. */
function milliseconds(name: string, value: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    throw new RangeError(`\`${name}\` is ${String(value)}; it takes a finite number of milliseconds, 0 or more`)
  }
  return value
}

/**
 * Fetched images kept across renders, revalidated as an HTTP cache would.
 *
 * Created by {@link createImageCache}, once per process, and passed to every
 * render as `RootProps.imageCache`. A render given none fetches every URL as it
 * always has.
 *
 * **Freshness.** The rules are those of a private HTTP cache,
 * [RFC 9111](https://www.rfc-editor.org/rfc/rfc9111), as Chrome applies them,
 * with stale-if-error from [RFC 5861](https://www.rfc-editor.org/rfc/rfc5861):
 *
 * - **What is stored**: a `200` within the 32 MiB fetch limit, unless it says
 *   `Cache-Control: no-store`. A failure, a timeout, any other status and a
 *   body too large are never stored. `private` is stored, since this is a
 *   private cache.
 * - **Fresh** for `max-age`, else `Expires` less `Date`, less the response's
 *   age on arrival. With neither, for `fallbackTtl` if set, else a tenth of the
 *   time since `Last-Modified`, else not at all. `Expires: 0` is already
 *   expired, and an `Expires` that is no date at all counts as absent, as in
 *   Chrome. A fresh entry is used with no request.
 * - **Dates** in `Date`, `Expires` and `Last-Modified` are read as Chrome reads
 *   them, more leniently than RFC 9110 asks. A value is a date when it names a
 *   month, a day and a year, in any order: the three forms RFC 9110 names, ISO
 *   8601 (`1994-11-06T08:49:37Z`), three numbers joined by `/` or `-` (month
 *   first unless the first is above 12, year first when it is above 31), and a
 *   month by the first three letters of its name in any case. A one- or
 *   two-digit year below 70 is in the 2000s, and a year past 32767 is no date.
 *   The time is optional, midnight when absent, and may carry seconds, a
 *   fraction, `Z`, or AM or PM. The zone is optional, GMT when absent: GMT, UT,
 *   EST, EDT, CST, CDT, MST, MDT, PST, PDT, AST, NST, BST, MET, EET, JST, or an
 *   offset such as `+0130`; `UTC` is not one. A value past its range carries
 *   over, as a calendar does (`31 Feb` is 3 March), and anything else in the
 *   header is ignored.
 * - **Expired**, the entry is revalidated with `If-None-Match` and
 *   `If-Modified-Since` from its `ETag` and `Last-Modified`; a `200` replaces
 *   the bytes, and a `304` keeps them and replaces the stored headers it sends,
 *   one by one, keeping the rest and the stored `ETag`, as Chrome does. With
 *   neither validator the entry is fetched again.
 * - **`no-cache`** stores the entry and revalidates it on every use.
 * - **Stale-if-error**: when revalidating fails with a transport error, a
 *   timeout or a 5xx, the expired bytes are drawn, with a diagnostic, until the
 *   response's own `stale-if-error`, else `staleIfError`, past expiry.
 *   `must-revalidate` and `no-cache` allow none, and a cache whose
 *   `staleIfError` is 0 never serves stale.
 * - **A clock that moves back** makes an entry stored "in the future" expired,
 *   never fresh for longer.
 *
 * The key is the URL with its request headers, so an authenticated request
 * never answers an anonymous one and `Vary` needs no handling.
 *
 * **Concurrency and deletion.** Overlapping renders requesting one key share
 * one fetch. A {@link ImageCache.delete} or {@link ImageCache.clear} while a
 * fetch for that key is in flight stops that fetch from storing what it brings
 * back, so a delete is never undone by a request already on the wire.
 *
 * **A failing store never fails a render.** A store error is a miss: the image
 * is fetched, and the error is reported on {@link Canvas.diagnostics} with the
 * URL as its `path`. A corrupt entry is also deleted.
 *
 * ```ts
 * import { createImageCache, memoryImageStore, Root, Image } from 'meo-canvas'
 *
 * // Once, where the server starts.
 * const imageCache = createImageCache({ store: memoryImageStore({ maxBytes: 256 * 1024 * 1024 }) })
 *
 * // Every render.
 * const canvas = await Root({ width: 200, height: 200, imageCache, children: Image({ src: { url: 'https://example.com/a.png' } }) })
 * ```
 */
export class ImageCache {
  readonly #store: ImageStore
  readonly #settings: Settings
  readonly #flights = new Map<string, Flight>()
  readonly #counts = { hits: 0, misses: 0, revalidated: 0, replaced: 0, staleServed: 0 }

  /** Use {@link createImageCache}. */
  constructor(options: ImageCacheOptions) {
    if (typeof options !== 'object' || options === null || typeof options.store !== 'object' || options.store === null) {
      throw new TypeError('`createImageCache` needs a `store`: `memoryImageStore`, `diskImageStore` or an `ImageStore` of your own')
    }
    this.#store = options.store
    this.#settings = {
      fallbackTtl: options.fallbackTtl === undefined ? undefined : milliseconds('fallbackTtl', options.fallbackTtl),
      staleIfError: options.staleIfError === undefined ? DEFAULT_STALE_IF_ERROR : milliseconds('staleIfError', options.staleIfError),
    }
  }

  /**
   * The key a request is stored under: the URL, then each header as
   * `name: value` on its own line, lower-cased and sorted as `Headers` gives
   * them. `httpOptions` is what the request sends: a source's options over the
   * scene's.
   */
  static key(url: string, httpOptions?: RequestInit): string {
    const headers = [...new Headers(httpOptions?.headers)].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    return headers.length === 0 ? url : `${url}\n${headers.map(([name, value]) => `${name}: ${value}`).join('\n')}`
  }

  /** What this cache has done, and what its store holds. */
  stats(): ImageCacheStats {
    const store = this.#store.stats?.() ?? { stored: 0, evicted: 0, bytes: 0, entries: 0 }
    return { ...this.#counts, stored: store.stored, evicted: store.evicted, bytes: store.bytes, entries: store.entries }
  }

  /**
   * Removes the entry for `url` requested with `httpOptions`, and stops a fetch
   * for it already in flight from storing its result. Rejects with the store's
   * error when the entry exists and cannot be removed.
   */
  async delete(url: string, httpOptions?: RequestInit): Promise<void> {
    const key = ImageCache.key(url, httpOptions)
    const flight = this.#flights.get(key)
    if (flight !== undefined) flight.cancelled = true
    await this.#store.delete(key)
  }

  /**
   * Removes every entry, and stops every fetch in flight from storing its
   * result. Rejects with the store's error when an entry cannot be removed.
   */
  async clear(): Promise<void> {
    for (const flight of this.#flights.values()) flight.cancelled = true
    await this.#store.clear()
  }

  /**
   * The bytes for one request, from the store or through `network`, sharing a
   * request already in flight for the same key. Internal to `Root`.
   * @internal
   */
  async fetch(url: string, key: string, network: Network): Promise<Outcome> {
    for (;;) {
      const joined = this.#flights.get(key)
      if (joined !== undefined) {
        const outcome = await joined.done
        // Another render's caller aborted its own request; that says nothing
        // about this one, which asks again.
        if (outcome.kind === 'aborted') continue
        this.#count(outcome.answer)
        return outcome
      }
      const flight: Flight = { cancelled: false, done: Promise.resolve({ kind: 'aborted', error: undefined }) }
      this.#flights.set(key, flight)
      flight.done = this.#answer(url, key, network, flight)
      try {
        const outcome = await flight.done
        if (outcome.kind !== 'aborted') this.#count(outcome.answer)
        return outcome
      } finally {
        if (this.#flights.get(key) === flight) this.#flights.delete(key)
      }
    }
  }

  #count(answer: Answer): void {
    this.#counts[answer] += 1
  }

  /** One request, answered: store, then network, then stale-if-error. */
  async #answer(url: string, key: string, network: Network, flight: Flight): Promise<Outcome> {
    const notes: Diagnostic[] = []
    const now = Date.now()
    const looked = await this.#lookup(url, key, now, notes)
    if (looked.fresh !== undefined) return { kind: 'bytes', bytes: looked.fresh, notes, answer: 'hits' }
    const stored = looked.stored

    const conditional: [string, string][] = []
    if (stored?.etag !== undefined) conditional.push(['if-none-match', stored.etag])
    if (stored?.lastModified !== undefined) conditional.push(['if-modified-since', stored.lastModified])

    const request = Date.now()
    const fetched = await network(conditional)
    const response = Date.now()

    if (fetched.kind === 'aborted') return fetched
    if (fetched.kind === 'failed') return this.#failed(url, fetched, stored, now, notes)
    if (fetched.status === NOT_MODIFIED && stored !== undefined && conditional.length > 0) {
      // The stored headers, each replaced by the 304's where it sends one, and
      // the stored ETag kept whatever the 304 sends (RFC 9111 §4.3.4), as
      // Chrome does in `http-cache.tsv`'s "304 with ..." rows.
      const headers = new Headers(fetched.headers)
      headers.delete('etag')
      for (const [field, name] of STORED_HEADERS) {
        const value = stored[field]
        if (!headers.has(name) && value !== undefined) headers.set(name, value)
      }
      const decided = freshness(headers, request, response, this.#settings)
      const renewed = entryFrom(stored.bytes, headers, response, decided)
      if (decided.storable) await this.#keep(url, key, renewed, flight, notes)
      else await this.#delete(url, key, notes)
      return { kind: 'bytes', bytes: stored.bytes, notes, answer: 'revalidated' }
    }
    if (fetched.status < OK || fetched.status >= 300) {
      return this.#failed(url, { kind: 'failed', attempt: fetched.failure, error: fetched.error }, stored, now, notes)
    }
    if (fetched.status === OK) {
      const decided = freshness(fetched.headers, request, response, this.#settings)
      if (decided.storable) await this.#keep(url, key, entryFrom(fetched.bytes, fetched.headers, response, decided), flight, notes)
      else if (stored !== undefined) await this.#delete(url, key, notes)
    }
    // A 200 to a conditional request replaces; with no validator to send, an
    // expired entry is simply fetched again.
    return { kind: 'bytes', bytes: fetched.bytes, notes, answer: conditional.length === 0 ? 'misses' : 'replaced' }
  }

  /**
   * The fresh bytes stored for `key`, or the entry to revalidate if one is
   * still of use. A store error is a miss with a diagnostic, and a corrupt entry
   * is also deleted.
   */
  async #lookup(url: string, key: string, now: number, notes: Diagnostic[]): Promise<{ fresh?: Uint8Array; stored?: ImageCacheEntry }> {
    let entry: ImageCacheEntry | undefined
    try {
      entry = await this.#store.get(key)
    } catch (error) {
      if (error instanceof ImageStoreError && error.corrupt) {
        notes.push({ path: url, detail: `the image cache's entry is corrupt (${reason(error)}), so it is deleted and the image fetched` })
        await this.#delete(url, key, notes)
      } else {
        notes.push({ path: url, detail: `the image cache could not read its entry (${reason(error)}), so the image is fetched` })
      }
      return {}
    }
    if (entry === undefined) return {}
    if (isFresh(entry.storedAt, entry.expiresAt, entry.revalidateAlways, now)) return { fresh: entry.bytes }
    // Expired, unrevalidatable and past its stale bound: it can never be used
    // again, so it goes now rather than when LRU reaches it.
    const validates = entry.etag !== undefined || entry.lastModified !== undefined
    if (!validates && !staleAllowed(entry.staleIfErrorUntil, this.#settings.staleIfError, now)) {
      await this.#delete(url, key, notes)
      return {}
    }
    return { stored: entry }
  }

  /**
   * A fetch that failed: the stored bytes under stale-if-error, when the failure
   * is one a retry might fix and the entry allows it, else the failure.
   */
  #failed(url: string, failed: Failed, stored: ImageCacheEntry | undefined, now: number, notes: Diagnostic[]): Outcome {
    const attempt = failed.attempt
    const retryable = attempt.failure === 'transport' || (attempt.failure === 'status' && attempt.status >= SERVER_ERROR)
    if (stored !== undefined && retryable && staleAllowed(stored.staleIfErrorUntil, this.#settings.staleIfError, now)) {
      notes.push({
        path: url,
        detail: `revalidating the cached image failed (${attempt.detail}), so the expired copy is drawn, as stale-if-error allows`,
      })
      return { kind: 'bytes', bytes: stored.bytes, notes, answer: 'staleServed' }
    }
    return { kind: 'failed', attempt, error: failed.error, notes, answer: 'misses' }
  }

  /** Stores `entry` unless a delete or clear cancelled this flight, before or during the write. */
  async #keep(url: string, key: string, entry: ImageCacheEntry, flight: Flight, notes: Diagnostic[]): Promise<void> {
    if (flight.cancelled) return
    try {
      await this.#store.set(key, entry)
    } catch (error) {
      notes.push({ path: url, detail: `the image cache could not store the image (${reason(error)}); the render draws the fetched bytes` })
      return
    }
    // A delete that arrived during the write marked the flight after the check
    // above; undoing the write here is what keeps it deleted.
    if (flight.cancelled) await this.#delete(url, key, notes)
  }

  async #delete(url: string, key: string, notes: Diagnostic[]): Promise<void> {
    try {
      await this.#store.delete(key)
    } catch (error) {
      notes.push({ path: url, detail: `the image cache could not delete its entry (${reason(error)})` })
    }
  }
}

/** What an error says, for a diagnostic. */
function reason(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/**
 * Fetched images kept across renders: the cache a server creates once and
 * passes to every render as `RootProps.imageCache`. Throws a `TypeError`
 * without a `store`, and a `RangeError` for a `fallbackTtl` or `staleIfError`
 * that is not a finite number of milliseconds, 0 or more.
 *
 * ```ts
 * import { createImageCache, diskImageStore } from 'meo-canvas'
 *
 * const imageCache = createImageCache({
 *   store: diskImageStore({ dir: '/var/cache/cards', maxBytes: 2 * 1024 ** 3 }),
 *   fallbackTtl: 60 * 60 * 1000,
 * })
 * ```
 */
export function createImageCache(options: ImageCacheOptions): ImageCache {
  return new ImageCache(options)
}

/** What {@link memoryImageStore} takes. */
export interface MemoryImageStoreOptions {
  /** The most the store holds, in bytes: a whole number above 0. Required, since no size suits every server. */
  readonly maxBytes: number
  /**
   * The largest body it keeps, in bytes, at most `maxBytes`. Defaults to the
   * smaller of `maxBytes` and {@link DEFAULT_MAX_ENTRY_BYTES}.
   */
  readonly maxEntryBytes?: number
}

/** A whole number of bytes, above 0 unless `zero`, or the refusal naming it. */
function byteCount(name: string, value: number, zero: boolean): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < (zero ? 0 : 1)) {
    throw new RangeError(`\`${name}\` is ${String(value)}; it takes a whole number of bytes${zero ? ', 0 or more' : ' above 0'}`)
  }
  return value
}

/**
 * Checks a store's sizes: `maxBytes` a whole number above 0, `maxEntryBytes` a
 * whole number no larger, refused rather than clamped so numbers that disagree
 * are found where they were written.
 * @internal
 */
export function storeLimits(options: { readonly maxBytes: number; readonly maxEntryBytes?: number }): { maxBytes: number; maxEntryBytes: number } {
  const maxBytes = byteCount('maxBytes', options.maxBytes, false)
  if (options.maxEntryBytes === undefined) return { maxBytes, maxEntryBytes: Math.min(DEFAULT_MAX_ENTRY_BYTES, maxBytes) }
  const maxEntryBytes = byteCount('maxEntryBytes', options.maxEntryBytes, true)
  if (maxEntryBytes > maxBytes) throw new RangeError(`\`maxEntryBytes\` (${maxEntryBytes}) is above \`maxBytes\` (${maxBytes})`)
  return { maxBytes, maxEntryBytes }
}

/** UTF-8 bytes in a string, as the crate's `str::len` counts them. */
const utf8 = new TextEncoder()

/**
 * Entries in this process's memory, evicted least recently used first once
 * they pass `maxBytes`. Lost when the process ends; see {@link diskImageStore}
 * for entries that survive a restart.
 *
 * Throws a `RangeError` when `maxBytes` is not a whole number above 0 (`0`, a
 * negative, `NaN` and `Infinity` all are not) or `maxEntryBytes` is above it.
 *
 * **What it guarantees:**
 *
 * - **Size.** An entry costs its body, its key and
 *   {@link ENTRY_OVERHEAD_BYTES}, so many tiny entries cannot grow it past
 *   `maxBytes` while their bodies stay under it. The total never passes
 *   `maxBytes`.
 * - **Eviction.** Storing an entry evicts the least recently used ones until it
 *   fits. A lookup or a store makes an entry the most recently used. The entry
 *   being stored is never the one evicted.
 * - **Refusal.** A body over `maxEntryBytes`, or an entry whose cost passes
 *   `maxBytes`, is not stored: `set` resolves, and the render draws the bytes
 *   it fetched.
 * - **Accounting.** Replacing an entry adjusts the total by the difference;
 *   `delete` and `clear` subtract exactly what was added, and deleting twice is
 *   harmless.
 * - **Bytes.** Entries share their bytes with the render rather than copying
 *   them, which the renderer never modifies. A caller of `get` must not modify
 *   them either.
 *
 * It does not react to the process's memory pressure: `maxBytes` is the whole
 * contract.
 *
 * ```ts
 * import { createImageCache, memoryImageStore } from 'meo-canvas'
 *
 * // Once, at start-up: 256 MiB, and no entry over 8 MiB.
 * const imageCache = createImageCache({
 *   store: memoryImageStore({ maxBytes: 256 * 1024 * 1024, maxEntryBytes: 8 * 1024 * 1024 }),
 * })
 * ```
 */
export function memoryImageStore(options: MemoryImageStoreOptions): ImageStore {
  const { maxBytes, maxEntryBytes } = storeLimits(options)
  // Insertion order is recency: a use deletes and re-inserts.
  const entries = new Map<string, { entry: ImageCacheEntry; cost: number }>()
  let total = 0
  let stored = 0
  let evicted = 0

  const remove = (key: string): void => {
    const held = entries.get(key)
    if (held === undefined) return
    entries.delete(key)
    total -= held.cost
  }

  return {
    async get(key) {
      const held = entries.get(key)
      if (held === undefined) return undefined
      entries.delete(key)
      entries.set(key, held)
      return held.entry
    },
    async set(key, entry) {
      const cost = entry.bytes.byteLength + utf8.encode(key).byteLength + ENTRY_OVERHEAD_BYTES
      if (entry.bytes.byteLength > maxEntryBytes || cost > maxBytes) return
      remove(key)
      for (const [oldest, held] of entries) {
        if (total + cost <= maxBytes) break
        entries.delete(oldest)
        total -= held.cost
        evicted += 1
      }
      entries.set(key, { entry, cost })
      total += cost
      stored += 1
    },
    async delete(key) {
      remove(key)
    },
    async clear() {
      entries.clear()
      total = 0
    },
    stats() {
      return { stored, evicted, bytes: total, entries: entries.size }
    },
  }
}

/** The cache settings the rules read. */
interface Settings {
  readonly fallbackTtl: number | undefined
  readonly staleIfError: number
}

/** How one request was answered: the counter it adds to. */
type Answer = 'hits' | 'misses' | 'revalidated' | 'replaced' | 'staleServed'

/**
 * What a request through the network brought back, as `Root` classifies it.
 * @internal
 */
export type Fetched =
  | {
      readonly kind: 'response'
      readonly status: number
      readonly headers: Headers
      /** Empty unless the status is a success. */
      readonly bytes: Uint8Array
      /** What a non-success status fails as. */
      readonly failure: FailedAttempt
      readonly error: TypeError
    }
  | Failed
  | { readonly kind: 'aborted'; readonly error: unknown; readonly answer?: undefined }

/** A request that produced no usable response. */
interface Failed {
  readonly kind: 'failed'
  readonly attempt: FailedAttempt
  readonly error: TypeError
}

/** A failed attempt, in the shape `Root` records it. */
type FailedAttempt =
  | { readonly url: string; readonly detail: string; readonly failure: 'status'; readonly status: number }
  | {
      readonly url: string
      readonly detail: string
      readonly failure: 'host-not-found' | 'bad-url' | 'transport' | 'too-large' | 'other'
      readonly status?: undefined
    }

/**
 * One request, with `extra` headers sent after the caller's.
 * @internal
 */
export type Network = (extra: readonly (readonly [string, string])[]) => Promise<Fetched>

/**
 * What {@link ImageCache.fetch} resolves with.
 * @internal
 */
export type Outcome =
  | { readonly kind: 'bytes'; readonly bytes: Uint8Array; readonly notes: readonly Diagnostic[]; readonly answer: Answer }
  | { readonly kind: 'failed'; readonly attempt: FailedAttempt; readonly error: TypeError; readonly notes: readonly Diagnostic[]; readonly answer: Answer }
  | { readonly kind: 'aborted'; readonly error: unknown; readonly answer?: undefined }

/** One request in flight, which overlapping requests for its key wait on. */
interface Flight {
  /** Set by a delete or a clear, so what this flight fetches is not stored. */
  cancelled: boolean
  done: Promise<Outcome>
}

/** The entry for a response received at `response`. */
function entryFrom(bytes: Uint8Array, headers: Headers, response: number, decided: Freshness): ImageCacheEntry {
  const kept: Record<string, string> = {}
  for (const [field, name] of STORED_HEADERS) {
    const value = headers.get(name)
    if (value !== null) kept[field] = value
  }
  return {
    bytes,
    ...kept,
    storedAt: response,
    expiresAt: decided.expiresAt,
    staleIfErrorUntil: decided.staleIfErrorUntil,
    revalidateAlways: decided.revalidateAlways,
  }
}

/** The headers an entry keeps, by field and by header name. */
const STORED_HEADERS = [
  ['etag', 'etag'],
  ['lastModified', 'last-modified'],
  ['cacheControl', 'cache-control'],
  ['expires', 'expires'],
  ['pragma', 'pragma'],
  ['date', 'date'],
  ['age', 'age'],
] as const

/**
 * What one response decided about its own caching, at the time it arrived.
 * @internal
 */
export interface Freshness {
  /** Whether it may be stored at all: not `no-store`. */
  readonly storable: boolean
  /** Fresh until this instant; at it and past it, revalidate. */
  readonly expiresAt: number
  /** A failed revalidation may serve the entry until this instant. */
  readonly staleIfErrorUntil: number
  /** `no-cache`: revalidate on every use, however fresh. */
  readonly revalidateAlways: boolean
}

/** `Cache-Control` or `Pragma` directives: lower-cased names, and each value as written if it had one. */
function directives(value: string | null): (readonly [string, string | undefined])[] {
  return (value ?? '')
    .split(',')
    .map(item => item.trim())
    .filter(item => item !== '')
    .map(item => {
      const at = item.indexOf('=')
      return at < 0 ? ([item.toLowerCase(), undefined] as const) : ([item.slice(0, at).trim().toLowerCase(), item.slice(at + 1).trim()] as const)
    })
}

/** A `delta-seconds` value in milliseconds: digits only, capped at {@link MAX_DELTA_SECONDS}. */
function deltaSeconds(value: string): number | undefined {
  if (!/^[0-9]+$/.test(value)) return undefined
  return Math.min(Number(value), MAX_DELTA_SECONDS) * MS_PER_SECOND
}

/** The first directive named `name` that carries a valid `delta-seconds`. */
function secondsOf(found: readonly (readonly [string, string | undefined])[], name: string): number | undefined {
  for (const [held, value] of found) {
    if (held !== name || value === undefined) continue
    const seconds = deltaSeconds(value)
    if (seconds !== undefined) return seconds
  }
  return undefined
}

/** Whether a bare directive `name` is present: `no-cache="set-cookie"` is not `no-cache`, as in Chrome (`http-cache.tsv`: `no-cache="set-cookie" with max-age`). */
function has(found: readonly (readonly [string, string | undefined])[], name: string): boolean {
  return found.some(([held, value]) => held === name && value === undefined)
}

/**
 * Freshness for a response requested at `request` and received at `response`,
 * per RFC 9111 §4.2 as Chrome applies it — the same rules as the crate's
 * `freshness`, which documents them and names the `http-cache.tsv` rows each
 * choice RFC 9111 leaves open rests on.
 * @internal
 */
export function freshness(headers: Headers, request: number, response: number, settings: Settings): Freshness {
  const control = directives(headers.get('cache-control'))
  const pragma = directives(headers.get('pragma'))
  const noCache = has(control, 'no-cache') || has(pragma, 'no-cache')
  const mustRevalidate = has(control, 'must-revalidate')
  const date = httpDate(headers.get('date')) ?? response

  let lifetime: number
  const maxAge = secondsOf(control, 'max-age')
  const expires = httpDate(headers.get('expires'))
  if (noCache) lifetime = 0
  else if (maxAge !== undefined) lifetime = maxAge
  else if (headers.get('expires')?.trim() === '0') lifetime = 0
  else if (expires !== undefined) lifetime = Math.max(0, expires - date)
  else if (settings.fallbackTtl !== undefined) lifetime = settings.fallbackTtl
  else {
    const modified = httpDate(headers.get('last-modified'))
    lifetime = modified !== undefined && !mustRevalidate && modified <= date ? Math.trunc((date - modified) / HEURISTIC_DIVISOR) : 0
  }

  const ageHeader = headers.get('age')
  const age = (ageHeader === null ? undefined : deltaSeconds(ageHeader.trim())) ?? 0
  const apparent = Math.max(0, response - date)
  const initial = Math.max(apparent, age + (response - request))
  const expiresAt = response + lifetime - initial

  const staleIfErrorUntil = noCache || mustRevalidate ? expiresAt : expiresAt + (secondsOf(control, 'stale-if-error') ?? settings.staleIfError)

  return { storable: !has(control, 'no-store'), expiresAt, staleIfErrorUntil, revalidateAlways: noCache }
}

/**
 * Whether an entry stored at `storedAt` may be served at `now` with no request.
 * One stored in the future means the clock moved back, and it is expired.
 * @internal
 */
export function isFresh(storedAt: number, expiresAt: number, revalidateAlways: boolean, now: number): boolean {
  return !revalidateAlways && storedAt <= now && now < expiresAt
}

/**
 * Whether a failed revalidation at `now` may serve the entry. A cache whose own
 * `staleIfError` is 0 never does, whatever the entry recorded.
 * @internal
 */
export function staleAllowed(staleIfErrorUntil: number, setting: number, now: number): boolean {
  return setting !== 0 && now < staleIfErrorUntil
}

export { httpDate }
