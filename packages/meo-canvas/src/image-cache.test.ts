// The image cache through `Root`, against a local origin that counts the
// requests it answers: what is stored, what is reused, what revalidates, and
// what a failing store or origin does to a render. The crate's
// `tests/image_cache.rs` holds the same cases.

import { createServer, type IncomingHttpHeaders } from 'node:http'
import type { AddressInfo, Socket } from 'node:net'
import { mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'

import { diskImageStore } from './image-cache-disk.js'
import { createImageCache, ENTRY_OVERHEAD_BYTES, ImageCache, ImageStoreError, memoryImageStore, type ImageStore } from './image-cache.js'
import { Image } from './node.js'
import { Root } from './root.js'

/** A 1x1 PNG, so a response decodes and the render reaches its end. */
const DOT = Uint8Array.from([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06,
  0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, 0x03, 0x01, 0x01,
  0x00, 0x18, 0xdd, 0x8d, 0xb0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
])

/** One response: a status, headers, a body and a wait; or a hang-up with none. */
interface Reply {
  readonly status?: number
  readonly headers?: Record<string, string>
  readonly body?: Uint8Array
  readonly delay?: number
  readonly hangUp?: boolean
  /** Send the head and part of the body, then hold the socket for {@link Origin.cut}. */
  readonly partial?: boolean
}

const ok = (headers: Record<string, string>): Reply => ({ status: 200, headers, body: DOT })

/** A local origin that counts and records every request. */
interface Origin {
  url(path: string): string
  readonly requests: number
  readonly heads: readonly IncomingHttpHeaders[]
  /** Drops every socket a `partial` reply holds. */
  cut(): void
}

const closing: (() => Promise<void>)[] = []
afterEach(async () => {
  await Promise.all(closing.splice(0).map(close => close()))
})

async function origin(reply: (n: number) => Reply): Promise<Origin> {
  const heads: IncomingHttpHeaders[] = []
  const held: Socket[] = []
  let requests = 0
  const server = createServer((request, response) => {
    const n = requests
    requests += 1
    heads.push(request.headers)
    const answer = reply(n)
    setTimeout(() => {
      if (answer.hangUp === true) {
        request.socket.destroy()
        return
      }
      if (answer.partial === true) {
        held.push(request.socket)
        response.writeHead(answer.status ?? 200, { 'content-length': '100000', ...answer.headers })
        response.write(new Uint8Array(1000))
        return
      }
      const body = answer.body ?? new Uint8Array()
      response.writeHead(answer.status ?? 200, { 'content-length': String(body.byteLength), connection: 'close', ...answer.headers })
      response.end(body)
    }, answer.delay ?? 0)
  })
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve))
  const { port } = server.address() as AddressInfo
  closing.push(
    () =>
      new Promise(resolve => {
        server.closeAllConnections()
        server.close(() => resolve())
      }),
  )
  return {
    url: path => `http://127.0.0.1:${port}${path}`,
    get requests() {
      return requests
    },
    heads,
    cut: () => {
      for (const socket of held.splice(0)) socket.destroy()
    },
  }
}

/** One render of `url` through `imageCache`: its warnings and diagnostics. */
async function render(url: string, imageCache: ImageCache, httpOptions?: RequestInit, extra: { filter?: string } = {}) {
  const canvas = await Root({
    width: 8,
    height: 8,
    imageCache,
    ...(httpOptions === undefined ? {} : { httpOptions }),
    ...(extra.filter === undefined ? {} : { filter: extra.filter }),
    children: Image({ src: { url }, width: 8, height: 8 }),
  })
  const result = { warnings: canvas.warnings, diagnostics: canvas.diagnostics.map(found => `${found.path} ${found.detail}`) }
  canvas.release()
  return result
}

const memory = (): ImageCache => createImageCache({ store: memoryImageStore({ maxBytes: 1 << 24 }) })

describe('an image cache through Root', () => {
  it('serves a fresh hit with no request', async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    const cache = memory()
    for (let n = 0; n < 3; n += 1) {
      const found = await render(server.url('/a.png'), cache)
      expect(found).toEqual({ warnings: [], diagnostics: [] })
    }
    expect(server.requests).toBe(1)
    const stats = cache.stats()
    expect([stats.misses, stats.hits, stats.stored, stats.entries]).toEqual([1, 2, 1, 1])
  })

  it('fetches every render without one', async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    for (let n = 0; n < 2; n += 1) (await Root({ width: 8, height: 8, children: Image({ src: { url: server.url('/a.png') } }) })).release()
    expect(server.requests).toBe(2)
  })

  it('revalidates an expired entry with its ETag, and a 304 keeps it', async () => {
    const server = await origin(n =>
      n === 0 ? ok({ 'cache-control': 'max-age=0', etag: '"v1"' }) : { status: 304, headers: { 'cache-control': 'max-age=60' } },
    )
    const cache = memory()
    const url = server.url('/a.png')
    await render(url, cache)
    expect((await render(url, cache)).warnings).toEqual([])
    expect(server.heads[1]?.['if-none-match']).toBe('"v1"')
    await render(url, cache)
    expect(server.requests).toBe(2)
    const stats = cache.stats()
    expect([stats.misses, stats.revalidated, stats.hits]).toEqual([1, 1, 1])
  })

  it('revalidates with If-Modified-Since from Last-Modified', async () => {
    const lastModified = 'Thu, 22 Oct 2026 00:00:00 GMT'
    const server = await origin(n => (n === 0 ? ok({ 'cache-control': 'no-cache', 'last-modified': lastModified }) : { status: 304 }))
    const cache = memory()
    const url = server.url('/a.png')
    await render(url, cache)
    await render(url, cache)
    expect(server.heads[1]?.['if-modified-since']).toBe(lastModified)
    expect(cache.stats().revalidated).toBe(1)
  })

  it('replaces the entry with a 200 to a revalidation', async () => {
    const server = await origin(n => ok(n === 0 ? { 'cache-control': 'max-age=0', etag: '"v1"' } : { 'cache-control': 'max-age=60', etag: '"v2"' }))
    const cache = memory()
    const url = server.url('/a.png')
    for (let n = 0; n < 3; n += 1) await render(url, cache)
    expect(server.requests).toBe(2)
    const stats = cache.stats()
    expect([stats.misses, stats.replaced, stats.hits, stats.stored]).toEqual([1, 1, 1, 2])
  })

  it('fetches an expired entry with no validator again, plainly', async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=0' }))
    const cache = memory()
    const url = server.url('/a.png')
    await render(url, cache)
    await render(url, cache)
    expect(server.requests).toBe(2)
    expect(Object.keys(server.heads[1] ?? {}).filter(name => name.startsWith('if-'))).toEqual([])
    expect(cache.stats().misses).toBe(2)
  })

  it('fetches an image evicted by bytes again', async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    const url = (n: number): string => server.url(`/${n}.png`)
    const one = DOT.byteLength + url(0).length + ENTRY_OVERHEAD_BYTES
    const cache = createImageCache({ store: memoryImageStore({ maxBytes: 2 * one + 1 }) })
    for (let n = 0; n < 3; n += 1) await render(url(n), cache)
    expect(cache.stats().evicted).toBe(1)
    await render(url(0), cache)
    expect(server.requests).toBe(4)
    await render(url(2), cache)
    expect(server.requests).toBe(4)
  })

  it('does not store an entry over maxEntryBytes, and still draws it', async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    const cache = createImageCache({ store: memoryImageStore({ maxBytes: 1 << 20, maxEntryBytes: 10 }) })
    const url = server.url('/a.png')
    for (let n = 0; n < 2; n += 1) expect((await render(url, cache)).warnings).toEqual([])
    expect(server.requests).toBe(2)
    expect(cache.stats().entries).toBe(0)
  })

  it.each([
    ['a 404', { status: 404, headers: { 'cache-control': 'max-age=60' }, body: Uint8Array.of(1) }],
    ['a 503', { status: 503, headers: { 'cache-control': 'max-age=60' }, body: Uint8Array.of(1) }],
    ['a hang-up', { hangUp: true }],
  ] as const)('does not store %s', async (_, first: Reply) => {
    const server = await origin(n => (n === 0 ? first : ok({ 'cache-control': 'max-age=60' })))
    const cache = memory()
    const url = server.url('/a.png')
    expect((await render(url, cache)).warnings).toHaveLength(1)
    expect(cache.stats().entries).toBe(0)
    expect((await render(url, cache)).warnings).toEqual([])
    expect(server.requests).toBe(2)
  })

  it('does not store a body past the fetch limit', async () => {
    const server = await origin(() => ({ status: 200, headers: { 'cache-control': 'max-age=60' }, body: new Uint8Array(33 * 1024 * 1024) }))
    const cache = memory()
    expect((await render(server.url('/big.png'), cache)).warnings).toHaveLength(1)
    expect(cache.stats().entries).toBe(0)
  })

  it('does not store no-store', async () => {
    const server = await origin(() => ok({ 'cache-control': 'no-store, max-age=60' }))
    const cache = memory()
    const url = server.url('/a.png')
    await render(url, cache)
    await render(url, cache)
    expect(server.requests).toBe(2)
    expect(cache.stats().entries).toBe(0)
  })

  it('caches a 200 that will not decode as the response it was', async () => {
    // The cache holds responses, not images: the body is kept and served again,
    // and each render softens it to the warning the crate gives for the same
    // response, since the renderer is told the bytes were fetched.
    const server = await origin(() => ({ status: 200, headers: { 'cache-control': 'max-age=60' }, body: new TextEncoder().encode('<html>oops</html>') }))
    const cache = memory()
    const url = server.url('/a.png')
    for (let n = 0; n < 2; n += 1) {
      const found = await render(url, cache)
      expect(found.warnings).toEqual([{ url, failure: 'other', detail: 'the bytes fetched are not an image any decoder here reads', nodes: 1 }])
    }
    expect(server.requests).toBe(1)
  })

  it('keeps one entry per set of request headers', async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    const cache = memory()
    const url = server.url('/a.png')
    for (let n = 0; n < 2; n += 1) {
      await render(url, cache)
      await render(url, cache, { headers: { Authorization: 'Bearer t' } })
    }
    expect(server.requests).toBe(2)
    expect(cache.stats().entries).toBe(2)
  })

  it.each([
    ['a 503', { status: 503 }],
    ['a hang-up', { hangUp: true }],
    ['a body cut off partway', { partial: true }],
  ] as const)('serves the expired copy under stale-if-error after %s', async (_, failure: Reply) => {
    const server = await origin(n => (n === 0 ? ok({ 'cache-control': 'max-age=0, stale-if-error=600', etag: '"v1"' }) : failure))
    const cache = memory()
    const url = server.url('/a.png')
    await render(url, cache)
    // Cut once the head has arrived, so a partial reply fails the body read and
    // never `fetch` itself; for the other replies there is nothing to cut.
    const real = globalThis.fetch
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const response = await real(input, init)
      server.cut()
      return response
    }
    const found = await render(url, cache).finally(() => {
      globalThis.fetch = real
    })
    expect(found.warnings).toEqual([])
    expect(found.diagnostics).toHaveLength(1)
    expect(found.diagnostics[0]).toMatch(
      new RegExp(`^${url.replaceAll('.', '\\.')} revalidating the cached image failed \\(.*\\), so the expired copy is drawn, as stale-if-error allows$`),
    )
    expect(cache.stats().staleServed).toBe(1)
  })

  it('fails as without a cache past the stale-if-error bound', async () => {
    const server = await origin(n => (n === 0 ? ok({ 'cache-control': 'max-age=0, stale-if-error=0', etag: '"v1"' }) : { status: 503 }))
    const cache = memory()
    const url = server.url('/a.png')
    await render(url, cache)
    expect((await render(url, cache)).warnings).toHaveLength(1)
    expect(cache.stats().staleServed).toBe(0)
  })

  it('never serves stale from a cache whose staleIfError is 0', async () => {
    const server = await origin(n => (n === 0 ? ok({ 'cache-control': 'max-age=0, stale-if-error=600', etag: '"v1"' }) : { status: 503 }))
    const cache = createImageCache({ store: memoryImageStore({ maxBytes: 1 << 20 }), staleIfError: 0 })
    const url = server.url('/a.png')
    await render(url, cache)
    expect((await render(url, cache)).warnings).toHaveLength(1)
  })

  it('does not serve stale after a 4xx', async () => {
    const server = await origin(n => (n === 0 ? ok({ 'cache-control': 'max-age=0', etag: '"v1"' }) : { status: 404 }))
    const cache = memory()
    const url = server.url('/a.png')
    await render(url, cache)
    expect((await render(url, cache)).warnings).toHaveLength(1)
  })

  it('deletes an entry that can never be used again when it is looked up', async () => {
    const server = await origin(n => (n === 0 ? ok({ 'cache-control': 'max-age=0' }) : { status: 404 }))
    const cache = createImageCache({ store: memoryImageStore({ maxBytes: 1 << 20 }), staleIfError: 0 })
    const url = server.url('/a.png')
    await render(url, cache)
    expect(cache.stats().entries).toBe(1)
    await render(url, cache)
    expect(cache.stats().entries).toBe(0)
  })

  it('makes one request for overlapping renders of one key', async () => {
    const server = await origin(() => ({ ...ok({ 'cache-control': 'max-age=60' }), delay: 300 }))
    const cache = memory()
    const url = server.url('/a.png')
    const found = await Promise.all(Array.from({ length: 8 }, () => render(url, cache)))
    expect(found.every(one => one.warnings.length === 0)).toBe(true)
    expect(server.requests).toBe(1)
    const stats = cache.stats()
    expect(stats.misses + stats.hits).toBe(8)
  })

  it('persists a disk store across two instances', async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    const dir = mkdtempSync(join(tmpdir(), 'meo-canvas-cache-'))
    try {
      const url = server.url('/a.png')
      await render(url, createImageCache({ store: diskImageStore({ dir, maxBytes: 1 << 24 }) }))
      const again = createImageCache({ store: diskImageStore({ dir, maxBytes: 1 << 24 }) })
      expect((await render(url, again)).warnings).toEqual([])
      expect(server.requests).toBe(1)
      expect(again.stats().hits).toBe(1)
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  })

  /**
   * Starts a render and waits until its request reaches the origin, which holds
   * it. Wrapped, since an async function returning the render would await it.
   */
  async function inFlight(server: Origin, url: string, cache: ImageCache): Promise<{ rendering: ReturnType<typeof render> }> {
    const rendering = render(url, cache)
    while (server.requests === 0) await new Promise(resolve => setTimeout(resolve, 5))
    return { rendering }
  }

  it('keeps a fetch in flight during clear out of the store', async () => {
    const server = await origin(() => ({ ...ok({ 'cache-control': 'max-age=60' }), delay: 300 }))
    const cache = memory()
    const { rendering } = await inFlight(server, server.url('/a.png'), cache)
    await cache.clear()
    expect((await rendering).warnings).toEqual([])
    expect(cache.stats().entries).toBe(0)
  })

  it('keeps a fetch in flight during delete out of the store', async () => {
    const server = await origin(() => ({ ...ok({ 'cache-control': 'max-age=60' }), delay: 300 }))
    const cache = memory()
    const url = server.url('/a.png')
    const { rendering } = await inFlight(server, url, cache)
    await cache.delete(url)
    expect((await rendering).warnings).toEqual([])
    expect(cache.stats().entries).toBe(0)
  })

  it('revalidates an entry stored in the future rather than serving it fresh', async () => {
    const server = await origin(() => ({ status: 304, headers: { 'cache-control': 'max-age=60' } }))
    const store = memoryImageStore({ maxBytes: 1 << 20 })
    const url = server.url('/a.png')
    const future = Date.now() + 60 * 60 * 1000
    await store.set(ImageCache.key(url), {
      bytes: DOT,
      etag: '"v1"',
      storedAt: future,
      expiresAt: future + 60 * 60 * 1000,
      staleIfErrorUntil: future + 60 * 60 * 1000,
      revalidateAlways: false,
    })
    const cache = createImageCache({ store })
    expect((await render(url, cache)).warnings).toEqual([])
    expect(server.requests).toBe(1)
    expect(cache.stats().revalidated).toBe(1)
  })

  it('keeps each cache its own counts over one shared store', async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    const store = memoryImageStore({ maxBytes: 1 << 20 })
    const first = createImageCache({ store })
    const second = createImageCache({ store, fallbackTtl: 5000 })
    const url = server.url('/a.png')
    await render(url, first)
    await render(url, second)
    expect(server.requests).toBe(1)
    const [a, b] = [first.stats(), second.stats()]
    expect([a.misses, a.hits, b.misses, b.hits]).toEqual([1, 0, 0, 1])
    expect([a.entries, a.bytes]).toEqual([b.entries, b.bytes])
  })

  it('returns the full bytes from the store after a render', async () => {
    // The addon copies what it is given rather than detaching it, so the store
    // shares the array with the render; this fails the day it detaches.
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    const store = memoryImageStore({ maxBytes: 1 << 20 })
    const url = server.url('/a.png')
    await render(url, createImageCache({ store }))
    const held = await store.get(ImageCache.key(url))
    await render(url, createImageCache({ store }))
    expect(held?.bytes.byteLength).toBe(DOT.byteLength)
    expect([...(held?.bytes ?? [])]).toEqual([...DOT])
  })

  it('does not cache a data: URL', async () => {
    const cache = memory()
    const url = `data:image/png;base64,${Buffer.from(DOT).toString('base64')}`
    expect((await render(url, cache)).warnings).toEqual([])
    expect(cache.stats().entries).toBe(0)
  })

  /** A store that fails as it is told to, and records what it deleted. */
  function failing(corrupt: boolean): ImageStore & { deleted: string[] } {
    const deleted: string[] = []
    return {
      deleted,
      get: () => Promise.reject(corrupt ? new ImageStoreError('bad magic', { corrupt: true }) : new Error('permission denied')),
      set: () => Promise.reject(new Error('no space left on device')),
      delete: async key => void deleted.push(key),
      clear: async () => undefined,
    }
  }

  it('treats a failing store as a miss with a diagnostic and never fails the render', async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    const url = server.url('/a.png')
    const found = await render(url, createImageCache({ store: failing(false) }))
    expect(found.warnings).toEqual([])
    expect(found.diagnostics).toEqual([
      `${url} the image cache could not read its entry (permission denied), so the image is fetched`,
      `${url} the image cache could not store the image (no space left on device); the render draws the fetched bytes`,
    ])
  })

  it('deletes and reports a corrupt entry', async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    const store = failing(true)
    const url = server.url('/a.png')
    const found = await render(url, createImageCache({ store }))
    expect(found.diagnostics[0]).toBe(`${url} the image cache's entry is corrupt (bad magic), so it is deleted and the image fetched`)
    expect(store.deleted).toEqual([ImageCache.key(url)])
  })

  it("reports after the renderer's own diagnostics", async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    const url = server.url('/a.png')
    const found = await render(url, createImageCache({ store: failing(false) }), undefined, { filter: 'bogus(1)' })
    expect(found.diagnostics.map(line => line.split(' ')[0])).toEqual(['filter', url, url])
  })

  it('replaces a corrupt disk file', async () => {
    const server = await origin(() => ok({ 'cache-control': 'max-age=60' }))
    const dir = mkdtempSync(join(tmpdir(), 'meo-canvas-cache-'))
    try {
      const cache = createImageCache({ store: diskImageStore({ dir, maxBytes: 1 << 24 }) })
      const url = server.url('/a.png')
      await render(url, cache)
      for (const name of readdirSync(dir)) writeFileSync(join(dir, name), 'garbage')
      const found = await render(url, cache)
      expect(found.warnings).toEqual([])
      expect(found.diagnostics).toHaveLength(1)
      expect(server.requests).toBe(2)
      await render(url, cache)
      expect(server.requests).toBe(2)
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  })
})

describe('createImageCache', () => {
  it('refuses a missing store', () => {
    expect(() => createImageCache({} as never)).toThrow(TypeError)
  })

  it.each([
    ['fallbackTtl', -1],
    ['fallbackTtl', Number.NaN],
    ['staleIfError', Number.POSITIVE_INFINITY],
    ['staleIfError', -5],
  ] as const)('refuses %s %d', (name, value) => {
    const store = memoryImageStore({ maxBytes: 1 })
    expect(() => createImageCache({ store, [name]: value })).toThrow(RangeError)
  })

  it('has an entry cost that matches its documentation', () => {
    expect(ENTRY_OVERHEAD_BYTES).toBe(256)
  })
})
