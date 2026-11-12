// What Chrome's HTTP cache does with an image's response headers: each row loads
// one <img> several times, each time in a new page of one browser context, so the
// HTTP cache answers and Blink's per-document memory cache cannot. A local origin
// counts the requests and replays a scripted response for each.

import { writeFile } from 'node:fs/promises'
import { createServer } from 'node:http'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/http-cache.tsv')

/** A 1x1 PNG, so every load decodes. */
const DOT = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==', 'base64')

/**
 * Each row: a name, the responses to its successive requests, and how many
 * times the image is loaded. A response is a status, `nodate` to send no `Date`,
 * and headers. In a value, `@-90` is the HTTP-date ninety seconds before the
 * request arrives, `@0` the moment it arrives, and `@+7200~EST` that instant as a
 * wall clock in the named zone, so a row's ages hold whenever it runs.
 */
const BASE = [
  ['fresh max-age, the control', ['200 | cache-control: max-age=600'], 2],
  ['no-store, the control', ['200 | cache-control: no-store'], 2],
  ['Expires: 0 with Last-Modified', ['200 | date: @0 | expires: 0 | last-modified: @-864000 | etag: "a"', '304'], 2],
  ['Expires: 0 without Last-Modified', ['200 | date: @0 | expires: 0 | etag: "a"', '304'], 2],
]

/** Expires values that are not dates: beside an old Last-Modified, the heuristic answers. */
const NOT_DATES = ['soon', '-1', '1', '123456789', 'now', '""', '0 GMT']

/**
 * Date forms, each written past and future. A past Expires beside a ten-day-old
 * Last-Modified revalidates only if it was read; a future one with no
 * Last-Modified is a hit only if it was read; so each form is checked both ways.
 */
const FORMS = [
  ['IMF-fixdate', 'Sun, 06 Nov 1994 08:49:37 GMT', 'Fri, 01 Jan 2100 00:00:00 GMT'],
  ['RFC 850', 'Sunday, 06-Nov-94 08:49:37 GMT', 'Wednesday, 01-Jan-48 00:00:00 GMT'],
  ['asctime', 'Sun Nov  6 08:49:37 1994', 'Fri Jan  1 00:00:00 2100'],
  ['no time', 'Thu, 01 Dec 1994', 'Fri, 01 Jan 2100'],
  ['no zone', 'Sun, 06 Nov 1994 08:49:37', 'Fri, 01 Jan 2100 00:00:00'],
  ['ISO date', '1994-11-06', '2099-01-01'],
  ['ISO with Z', '1994-11-06T08:49:37Z', '2099-01-01T00:00:00Z'],
  ['ISO without zone', '1994-11-06T08:49:37', '2099-01-01T00:00:00'],
  ['ISO with fraction', '1994-11-06T08:49:37.342156Z', '2099-01-01T00:00:00.5Z'],
  ['UTC', 'Sun, 06 Nov 1994 08:49:37 UTC', 'Fri, 01 Jan 2100 00:00:00 UTC'],
  ['UT', 'Sun, 06 Nov 1994 08:49:37 UT', 'Fri, 01 Jan 2100 00:00:00 UT'],
  ['+0000', 'Sun, 06 Nov 1994 08:49:37 +0000', 'Fri, 01 Jan 2100 00:00:00 +0000'],
  ['EST', 'Sun, 06 Nov 1994 08:49:37 EST', 'Fri, 01 Jan 2100 00:00:00 EST'],
  ['no weekday', '06 Nov 1994 08:49:37 GMT', '01 Jan 2100 00:00:00 GMT'],
  ['lower case', 'sun, 06 nov 1994 08:49:37 gmt', 'fri, 01 jan 2100 00:00:00 gmt'],
  ['single digits', 'Sun, 6 Nov 1994 8:49:7 GMT', 'Fri, 1 Jan 2100 0:0:0 GMT'],
  ['no space after comma', 'Sun,06 Nov 1994 08:49:37 GMT', 'Fri,01 Jan 2100 00:00:00 GMT'],
  ['trailing text', 'Sun, 06 Nov 1994 08:49:37 GMT junk', 'Fri, 01 Jan 2100 00:00:00 GMT junk'],
  ['dashes and fraction', '22-AUG-1993 10:59:12.82', '22-AUG-2093 10:59:12.82'],
  ['US slashes with PM', '06/21/95 04:24:34 PM', '06/21/2095 04:24:34 PM'],
  ['day-first slashes', '20/06/95 21:07', '20/06/2095 21:07'],
  ['year-first dashes', '95-06-08 19:32:48 EDT', '2095-06-08 19:32:48 EDT'],
  ['long names', 'Friday, August 04, 1995 3:54 PM', 'Thursday, August 04, 2095 3:54 PM'],
  ['zone glued to time', '6 May 1992 16:41-JST (Wednesday)', '6 May 2092 16:41-JST (Tuesday)'],
  ['offset before year', 'Mon Jan 16 16:12 +0130 1989', 'Sun Jan 16 16:12 +0130 2089'],
  ['12 am', '22-AUG-1993 12:59am', '22-AUG-2093 12:59am'],
  ['hour 24', 'Sun, 06 Nov 1994 24:00:00 GMT', 'Fri, 01 Jan 2100 24:00:00 GMT'],
  ['31 February', 'Mon, 31 Feb 1994 00:00:00 GMT', 'Wed, 31 Feb 2100 00:00:00 GMT'],
]

/**
 * Zones, read near the present so that misreading the offset flips the answer:
 * the instant is two hours away on the side where reading the wall clock as GMT
 * lands on the other side of now.
 */
const ZONES = [
  ['EST', 7200],
  ['PST', 7200],
  ['EDT', 7200],
  ['JST', -7200],
  ['MET', -1800],
  ['BST', -1800],
  ['+0130', -3600],
  ['-0800', 7200],
  ['GMT+0300', -3600],
]

/**
 * How Chrome reads a value where two readings disagree, with no Last-Modified and
 * a `Date` in the future: the apparent age is then zero, so the second load is a
 * hit exactly when Expires was read as a date after that `Date`. Each value puts
 * its readings on opposite sides of it, so the rows hold whenever they run.
 */
const READ_AGAINST = 'Wed, 15 Jun 2095 12:00:00 GMT'
const READINGS = [
  ['an hour after, the control', 'Wed, 15 Jun 2095 13:00:00 GMT'],
  ['an hour before, the control', 'Wed, 15 Jun 2095 11:00:00 GMT'],
  ['the first month', 'Jun 16 2095 May'],
  ['Z after an offset', '+0100 15 Jun 2095 12:30:00Z'],
  ['a second colon ending the time', '15 Jun 2095 12:30: GMT'],
  ['a letter after a numeric date', '2095-06-16x'],
  ['a three-digit first number', '999/06/16 Jun 2095'],
  ['a first number of 0', '0-06-16 Jun 15 2095 13:00'],
  ['a three-digit number', '15 Jun 123 2095 13:00'],
  ['a number before a dot', 'Jun 16.5 2095'],
  ['PM after Z', '15 Jun 2095 11:30:00Z PM'],
  ['AM past noon', '15 Jun 2095 13:00 AM'],
  ['six digits as a small year', '06/14/002095 Jun 16 2095'],
  ['an offset after an ignored zone', '15 Jun 2095 12:30:00 GMT EST +0100'],
  ['an hour with no minutes', 'Jun 2095 16:'],
  ['three-digit minutes', 'Jun 2095 16:4937'],
  ['three-digit seconds', 'Jun 2095 16:30:999'],
  ['T and a digit inside a word', '15 Jun 2095 06:00:00 PST08'],
  ['a lower-case t', '2095-06-16t01:00'],
  ['a lower-case z', '15 Jun 2095 EST 09:00:00z'],
  ['a zone glued to the time', '15 Jun 2095 08:00:00EST'],
  ['a month after a numeric date', '2095-06-14 Jul'],
  ['Z on a later time', '15 Jun 2095 11:00 EST 09:00Z'],
  ['PM on a later time', '15 Jun 2095 11:00 01:00 PM'],
  ['a sign inside a word', '15 Jun 2095 12:30 abc+0100'],
  ['a sign inside junk', '15 Jun 2095 12:30 #+0100'],
  ['a letter outside ASCII', '15 Jun 2095 08:00:00 \u00c9ST'],
]

const ROWS = [
  ...BASE,
  ...NOT_DATES.map(value => [`Expires: ${value} with Last-Modified`, [`200 | date: @0 | expires: ${value} | last-modified: @-864000 | etag: "a"`, '304'], 2]),
  ...NOT_DATES.map(value => [`Expires: ${value} without Last-Modified`, [`200 | date: @0 | expires: ${value} | etag: "a"`, '304'], 2]),
  ...FORMS.flatMap(([form, past, future]) => [
    [`${form}, past`, [`200 | date: @0 | expires: ${past} | last-modified: @-864000 | etag: "a"`, '304'], 2],
    [`${form}, future`, [`200 | date: @0 | expires: ${future} | etag: "a"`, '304'], 2],
  ]),
  ...ZONES.map(([zone, offset]) => [`zone ${zone}`, [`200 | date: @0 | expires: @${offset > 0 ? '+' : ''}${offset}~${zone} | etag: "a"`, '304'], 2]),
  ...READINGS.map(([reading, value]) => [`reading ${reading}`, [`200 | date: ${READ_AGAINST} | expires: ${value} | etag: "a"`, '304'], 2]),
  // A lifetime of 100 s under a tenth: fresh at an age of 90, stale at 110. A
  // twentieth would make the first stale, a fifth the second fresh.
  ['heuristic, age 90 of 100', ['200 | date: @-90 | last-modified: @-1090 | etag: "a"', '304'], 2],
  ['heuristic, age 110 of 100', ['200 | date: @-110 | last-modified: @-1110 | etag: "a"', '304'], 2],
  ['heuristic from a lenient Last-Modified', ['200 | date: @0 | last-modified: Sun, 6 Nov 1994 8:49:37 | etag: "a"', '304'], 2],
  // Rules the cache states without RFC 9111 deciding them: each against Chrome.
  ['Pragma: no-cache, no Cache-Control', ['200 | pragma: no-cache | expires: @+600 | etag: "a"', '304'], 2],
  ['no-cache="set-cookie" with max-age', ['200 | cache-control: no-cache="set-cookie", max-age=600 | etag: "a"', '304'], 2],
  ['heuristic under must-revalidate', ['200 | date: @0 | cache-control: must-revalidate | last-modified: @-864000 | etag: "a"', '304'], 2],
  ['heuristic with Last-Modified after Date', ['200 | date: @0 | last-modified: @+86400 | etag: "a"', '304'], 2],
  // Chrome runs in Tokyo below, nine hours from GMT: a zoneless instant two
  // hours ahead is a hit read as GMT and stale read as local time.
  ['zone none near now', ['200 | date: @0 | expires: @+7200~none | etag: "a"', '304'], 2],
  ['no-cache with max-age', ['200 | cache-control: no-cache, max-age=600 | etag: "a"', '304', '304'], 3],
  ['Age beyond max-age', ['200 | cache-control: max-age=60 | age: 120 | etag: "a"', '304'], 2],
  ['Age within max-age, the control', ['200 | cache-control: max-age=60 | age: 30 | etag: "a"', '304'], 2],
  // A 304's headers update the stored ones, header by header; what it omits is kept.
  ['304 with a max-age renews', ['200 | cache-control: max-age=0 | etag: "a"', '304 | cache-control: max-age=600', '304'], 3],
  ['304 with max-age=0 does not', ['200 | cache-control: max-age=0 | etag: "a"', '304 | cache-control: max-age=0', '304'], 3],
  ['304 with no Cache-Control', ['200 | date: @0 | cache-control: max-age=0 | last-modified: @-864000 | etag: "a"', '304 | etag: "a"', '304'], 3],
  ['304 with only Expires over a stored max-age', ['200 | cache-control: max-age=0 | etag: "a"', '304 | expires: @+600', '304'], 3],
  ['304 with only Cache-Control over a stored Expires', ['200 | date: @0 | expires: @-10 | etag: "a"', '304 | cache-control: max-age=600', '304'], 3],
  ['304 with only Pragma over a stored Pragma', ['200 | pragma: no-cache | cache-control: max-age=600 | etag: "a"', '304 | pragma: x', '304'], 3],
  [
    '304 with only Pragma: no-cache',
    ['200 | date: @0 | expires: @+600 | cache-control: no-cache | etag: "a"', '304 | pragma: no-cache | cache-control: max-age=600', '304'],
    3,
  ],
  [
    '304 with no Date over a stored old Date',
    ['200 | date: @-1000 | cache-control: max-age=600 | etag: "a"', '304 nodate | cache-control: max-age=600', '304'],
    3,
  ],
  ['304 with no Age over a stored Age', ['200 | cache-control: max-age=600 | age: 700 | etag: "a"', '304 | cache-control: max-age=600', '304'], 3],
  ['304 with a new ETag', ['200 | cache-control: max-age=0 | etag: "a"', '304 | etag: "b" | cache-control: max-age=0', '304'], 3],
  [
    '304 with a new Last-Modified',
    ['200 | cache-control: max-age=0 | last-modified: @-864000', '304 | last-modified: @-3600 | cache-control: max-age=0', '304'],
    3,
  ],
]

/** Each zone's offset from GMT in minutes, as Chrome reads it, daylight time included. */
const ZONE_MINUTES = { EST: -300, PST: -480, EDT: -240, JST: 540, MET: 60, BST: 60, '+0130': 90, '-0800': -480, 'GMT+0300': 180 }

/** `instant` as an HTTP-date, or as a wall clock in `zone` followed by the zone. */
function written(instant, zone) {
  if (zone === undefined) return new Date(instant).toUTCString()
  if (zone === 'none') return new Date(instant).toUTCString().replace(' GMT', '')
  const wall = new Date(instant + ZONE_MINUTES[zone] * 60_000).toUTCString()
  return wall.replace('GMT', zone)
}

/** A scripted response's status, whether it sends a `Date`, and its headers, dates resolved against `now`. */
function scripted(spec, now) {
  const [head, ...headers] = spec.split(' | ')
  const [status, flag] = head.split(' ')
  return {
    status: Number(status),
    dated: flag !== 'nodate',
    headers: Object.fromEntries(
      headers.map(header => {
        const at = header.indexOf(': ')
        const value = header.slice(at + 2).replace(/@([+-]?\d+)(?:~(\S+))?/, (_, offset, zone) => written(now + Number(offset) * 1000, zone))
        return [header.slice(0, at), value]
      }),
    ),
  }
}

const requests = new Map()
const server = createServer((request, response) => {
  const url = new URL(request.url ?? '/', 'http://127.0.0.1')
  const row = Number(url.searchParams.get('row'))
  if (url.pathname === '/page') {
    response.writeHead(200, { 'content-type': 'text/html', 'cache-control': 'no-store' })
    response.end(`<!doctype html><img src="/image.png?row=${row}">`)
    return
  }
  const seen = requests.get(row) ?? []
  requests.set(row, seen)
  const script = ROWS[row]?.[1] ?? []
  const now = Date.now()
  const { status, dated, headers } = scripted(script[Math.min(seen.length, script.length - 1)] ?? '404', now)
  const ims = request.headers['if-modified-since']
  // The date a revalidation sends, as an offset from now to the nearest 100 s,
  // so a row written with `@-3600` can be told from one with `@-864000`.
  seen.push({ status, inm: request.headers['if-none-match'], ims: ims === undefined ? undefined : `@${Math.round((Date.parse(ims) - now) / 100_000) * 100}` })
  response.sendDate = dated
  const body = status === 200 ? DOT : Buffer.alloc(0)
  response.writeHead(status, { 'content-type': 'image/png', 'content-length': String(body.length), ...headers })
  response.end(body)
})
await new Promise(listening => server.listen(0, '127.0.0.1', listening))
const origin = `http://127.0.0.1:${server.address().port}`

// Chrome reads a date without a zone in some zone; running it nine hours from
// GMT, wherever this runs, is what lets `zone none near now` tell GMT from local.
process.env['TZ'] = 'Asia/Tokyo'
const { page: first, close } = await open()
const browser = first.context().browser()
try {
  const rows = []
  for (const [index, [name, script, loads]] of ROWS.entries()) {
    // A context per row, so no row reads another's cache.
    const context = await browser.newContext()
    const sources = []
    for (let load = 0; load < loads; load += 1) {
      const page = await context.newPage()
      const cdp = await context.newCDPSession(page)
      await cdp.send('Network.enable')
      let source = 'none'
      cdp.on('Network.requestServedFromCache', () => {
        source = 'memory'
      })
      cdp.on('Network.responseReceived', ({ response, type }) => {
        if (type !== 'Image' || source === 'memory') return
        source = response.fromDiskCache ? 'disk' : `network-${response.status}`
      })
      await page.goto(`${origin}/page?row=${index}`)
      await page.waitForFunction(() => document.images[0]?.complete === true)
      const decoded = await page.evaluate(() => document.images[0]?.naturalWidth ?? 0)
      if (decoded !== 1) throw new Error(`${name}: load ${load + 1} drew no image`)
      sources.push(source)
      await page.close()
    }
    await context.close()
    const seen = requests.get(index) ?? []
    const sent = seen.map(
      ({ inm, ims }) =>
        [inm === undefined ? '' : `if-none-match ${inm}`, ims === undefined ? '' : `if-modified-since ${ims}`].filter(Boolean).join(' + ') || 'plain',
    )
    rows.push([name, script.join(' || '), loads, seen.length, sources.join(' '), sent.join(' ')].join('\t'))
  }

  const header = [
    "# Chrome, through `just conformance`: what its HTTP cache does with an image's response headers.",
    '# Each row loads one <img> `loads` times, each in a new page of one context, so the HTTP cache answers rather than',
    "# a document's memory cache. `responses` are the origin's answers to successive requests, ` || ` between them and",
    '# ` | ` between a status and its headers; `@-90` is the HTTP-date 90 s before the request arrived, `@0` its arrival,',
    '# `@+7200~EST` that instant as a wall clock in EST, `~none` with no zone written; `nodate` after a status means',
    '# no `Date` was sent. Chrome runs with TZ=Asia/Tokyo.',
    '# `requests` is what the origin counted; `sources` is how each load was answered, from CDP: `disk` is the HTTP cache',
    '# with no request, `network-200` and `network-304` went to the origin, `memory` would mean the measurement is void.',
    '# `sent` is what each request carried: `plain`, or its conditional headers, If-Modified-Since as an offset from the',
    '# request to the nearest 100 s.',
    '# Written by packages/meo-canvas/tools/conformance/httpcache.mjs.',
    '#',
    '# case\tresponses\tloads\trequests\tsources\tsent',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`http cache: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await close()
  server.close()
}
