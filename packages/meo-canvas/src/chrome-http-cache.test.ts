// The image cache against Chrome's HTTP cache (`http-cache.tsv`): every row's
// responses replayed through an `ImageCache`, one render per load, comparing
// what the origin counted and what each request carried with what Chrome sent.
// The crate's `chrome_http_cache.rs` replays the same rows.

import { readFileSync } from 'node:fs'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { createImageCache, memoryImageStore } from './image-cache.js'
import { Image } from './node.js'
import { Root } from './root.js'

const TABLE = fileURLToPath(new URL('../../../crates/meo-canvas/tests/assets/chrome/http-cache.tsv', import.meta.url))

/**
 * The rows this cache answers differently from Chrome. None: the dates are read
 * as Chrome reads them and a 304's headers merge as Chrome merges them. Date
 * forms beyond the table are not measured, and a difference there would
 * show as a row added to `http-cache.tsv` failing here.
 */
const APART: string[] = []

/** Each zone's offset from GMT in minutes, as `httpcache.mjs` writes it. */
const ZONE_MINUTES: Readonly<Record<string, number>> = {
  EST: -300,
  PST: -480,
  EDT: -240,
  JST: 540,
  MET: 60,
  BST: 60,
  '+0130': 90,
  '-0800': -480,
  'GMT+0300': 180,
}

/** A 1x1 PNG, the body Chrome was served. */
const DOT = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==', 'base64')

const rows = readFileSync(TABLE, 'utf8')
  .split('\n')
  .filter(line => line !== '' && !line.startsWith('#'))
  .map(line => {
    const [name = '', responses = '', loads = '', requests = '', , sent = ''] = line.split('\t')
    return { name, responses: responses.split(' || '), loads: Number(loads), requests: Number(requests), sent }
  })

/** What this cache's origin counted and was sent, replaying one row. */
async function replay(responses: readonly string[], loads: number): Promise<{ requests: number; sent: string }> {
  const sent: string[] = []
  const server = createServer((request, response) => {
    const now = Date.now()
    const inm = request.headers['if-none-match']
    const since = request.headers['if-modified-since']
    // As an offset from now to the nearest 100 s, as `httpcache.mjs` records.
    const ims = since === undefined ? undefined : `if-modified-since @${Math.round((Date.parse(since) - now) / 100_000) * 100}`
    sent.push(inm === undefined ? (ims ?? 'plain') : `if-none-match ${inm}${ims === undefined ? '' : ` + ${ims}`}`)
    const [head = '404', ...headers] = (responses[Math.min(sent.length - 1, responses.length - 1)] ?? '404').split(' | ')
    const [status = '404', flag] = head.split(' ')
    const body = status === '200' ? DOT : Buffer.alloc(0)
    response.sendDate = flag !== 'nodate'
    response.writeHead(Number(status), {
      'content-type': 'image/png',
      'content-length': String(body.length),
      connection: 'close',
      ...Object.fromEntries(
        headers.map(header => {
          const at = header.indexOf(': ')
          const value = header.slice(at + 2).replace(/@([+-]?\d+)(?:~(\S+))?/, (_, offset: string, zone: string | undefined) => {
            const instant = now + Number(offset) * 1000
            if (zone === undefined) return new Date(instant).toUTCString()
            if (zone === 'none') return new Date(instant).toUTCString().replace(' GMT', '')
            return new Date(instant + (ZONE_MINUTES[zone] ?? 0) * 60_000).toUTCString().replace('GMT', zone)
          })
          return [header.slice(0, at), value]
        }),
      ),
    })
    response.end(body)
  })
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve))
  try {
    const url = `http://127.0.0.1:${(server.address() as AddressInfo).port}/image.png`
    const imageCache = createImageCache({ store: memoryImageStore({ maxBytes: 1 << 20 }) })
    for (let load = 0; load < loads; load += 1) {
      const canvas = await Root({ width: 4, height: 4, imageCache, onImageError: 'throw', children: Image({ src: { url } }) })
      canvas.release()
    }
    return { requests: sent.length, sent: sent.join(' ') }
  } finally {
    server.closeAllConnections()
    await new Promise(resolve => server.close(resolve))
  }
}

describe('http-cache.tsv', () => {
  it('has every row', () => {
    expect(rows).toHaveLength(132)
  })

  it('agrees with Chrome on every row not listed apart, and differs on each one listed', async () => {
    const apart: string[] = []
    for (const row of rows) {
      const found = await replay(row.responses, row.loads)
      if (found.requests !== row.requests || found.sent !== row.sent) {
        apart.push(row.name)
        expect(APART, `${row.name}: Chrome made ${row.requests} requests (${row.sent}), this cache ${found.requests} (${found.sent})`).toContain(row.name)
      }
    }
    expect(apart).toEqual(APART)
  })
})
