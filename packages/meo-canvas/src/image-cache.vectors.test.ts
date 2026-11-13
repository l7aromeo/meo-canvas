// The image cache's shared tables, read here and by the crate's tests: the
// freshness each response gets, the key and file each request maps to, and
// one entry file both surfaces write byte for byte.

import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { decode, diskImageStore, encode, fileName } from './image-cache-disk.js'
import { DEFAULT_STALE_IF_ERROR, freshness, httpDate, ImageCache, isFresh, staleAllowed, type ImageCacheEntry } from './image-cache.js'

const TABLES = fileURLToPath(new URL('../../../crates/meo-canvas-core/tests/assets/image-cache/', import.meta.url))

/** The rows of a table, header dropped, each split into cells. */
function rows(name: string, width: number): string[][] {
  const lines = readFileSync(join(TABLES, name), 'utf8')
    .split('\n')
    .filter(line => line !== '' && !line.startsWith('#'))
  const body = lines.slice(1).map(line => line.split('\t'))
  for (const row of body) expect(row, row.join(' | ')).toHaveLength(width)
  return body
}

/** The table's zero: Sun, 01 Nov 2026 00:00:00 GMT. */
const EPOCH = 1_793_491_200_000

const cell = (value: string | undefined): string | undefined => (value === '-' ? undefined : value)
const yes = (value: string | undefined): boolean => {
  expect(['yes', 'no']).toContain(value)
  return value === 'yes'
}

describe('freshness.tsv', () => {
  const table = rows('freshness.tsv', 15)

  it('has every row', () => {
    expect(table).toHaveLength(44)
  })

  it.each(table.map(row => [row[0], row] as const))('%s', (_, c) => {
    const headers = new Headers()
    for (const item of cell(c[6])?.split(' | ') ?? []) {
      const at = item.indexOf(': ')
      headers.append(item.slice(0, at), item.slice(at + 2))
    }
    const settings = {
      fallbackTtl: cell(c[4]) === undefined ? undefined : Number(c[4]),
      staleIfError: cell(c[5]) === undefined ? DEFAULT_STALE_IF_ERROR : Number(c[5]),
    }
    const [request, response, now] = [EPOCH + Number(c[1]), EPOCH + Number(c[2]), EPOCH + Number(c[3])]
    const found = freshness(headers, request, response, settings)
    expect(found.storable).toBe(yes(c[7]))
    if (!found.storable) return
    expect(found.expiresAt - EPOCH).toBe(Number(c[8]))
    expect(found.staleIfErrorUntil - EPOCH).toBe(Number(c[9]))
    expect(found.revalidateAlways).toBe(yes(c[10]))
    expect(isFresh(response, found.expiresAt, found.revalidateAlways, now)).toBe(yes(c[11]))
    expect(staleAllowed(found.staleIfErrorUntil, settings.staleIfError, now)).toBe(yes(c[12]))
    expect(headers.get('etag') ?? undefined).toBe(cell(c[13]))
    expect(headers.get('last-modified') ?? undefined).toBe(cell(c[14]))
  })
})

describe('dates.tsv', () => {
  const table = rows('dates.tsv', 3)

  it('has every row', () => {
    expect(table).toHaveLength(141)
  })

  it.each(table.map(row => [row[0], row] as const))('%s', (_, c) => {
    expect(httpDate(c[1] ?? '')).toBe(c[2] === '-' ? undefined : Number(c[2]))
  })
})

describe('keys.tsv', () => {
  const table = rows('keys.tsv', 5)

  it('has every row', () => {
    expect(table).toHaveLength(8)
  })

  it.each(table.map(row => [row[0], row] as const))('%s', (_, c) => {
    const headers: [string, string][] = []
    for (const item of cell(c[2])?.split(' | ') ?? []) {
      const at = item.indexOf(': ')
      headers.push([item.slice(0, at), item.slice(at + 2).replaceAll('\\t', '\t')])
    }
    const key = ImageCache.key(c[1] ?? '', { headers })
    expect(key).toBe(c[3]?.replaceAll('\\n', '\n'))
    expect(fileName(key)).toBe(c[4])
  })
})

describe('entry.tsv', () => {
  const [c = []] = rows('entry.tsv', 14)
  const key = (c[0] ?? '').replaceAll('\\n', '\n')
  const fields = ['etag', 'lastModified', 'cacheControl', 'expires', 'pragma', 'date', 'age'] as const
  const wanted: ImageCacheEntry = {
    bytes: Uint8Array.from(Buffer.from(c[12] ?? '', 'hex')),
    ...Object.fromEntries(fields.flatMap((field, at) => (c[at + 1] === undefined || c[at + 1] === '-' ? [] : [[field, c[at + 1]]]))),
    storedAt: Number(c[8]),
    expiresAt: Number(c[9]),
    staleIfErrorUntil: Number(c[10]),
    revalidateAlways: c[11] === 'yes',
  }
  const committed = readFileSync(join(TABLES, c[13] ?? ''))

  it('is written byte for byte as the committed file', () => {
    expect(fileName(key)).toBe(c[13])
    expect(Buffer.from(encode(key, wanted)).equals(committed)).toBe(true)
  })

  it('is read back whole from the committed file', () => {
    const read = decode(new Uint8Array(committed), key)
    expect({ ...read, bytes: [...(read?.bytes ?? [])] }).toEqual({ ...wanted, bytes: [...wanted.bytes] })
  })

  it('is found by a disk store in a directory holding only that file', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'meo-canvas-entry-'))
    try {
      mkdirSync(dir, { recursive: true })
      copyFileSync(join(TABLES, c[13] ?? ''), join(dir, c[13] ?? ''))
      const store = diskImageStore({ dir, maxBytes: 1 << 20 })
      const read = await store.get(key)
      expect(store.stats?.().entries).toBe(1)
      expect([...(read?.bytes ?? [])]).toEqual([...wanted.bytes])
      expect(read?.etag).toBe(wanted.etag)
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  })
})
