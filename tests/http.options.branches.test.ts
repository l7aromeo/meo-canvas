import { hashHttpOptions } from '@/util/http.options.js'

/** Two options objects that differ only in the value under test must hash apart. */
const differs = (a: RequestInit, b: RequestInit) => hashHttpOptions(a) !== hashHttpOptions(b)

describe('hashHttpOptions — body shapes', () => {
  it('folds URLSearchParams down to sorted entries', () => {
    const one = new URLSearchParams([
      ['b', '2'],
      ['a', '1'],
    ])
    const two = new URLSearchParams([
      ['a', '1'],
      ['b', '2'],
    ])
    expect(hashHttpOptions({ body: one })).toBe(hashHttpOptions({ body: two }))
    expect(differs({ body: one }, { body: new URLSearchParams([['a', '9']]) })).toBe(true)
  })

  it('folds Headers down to sorted entries', () => {
    const one = new Headers([
      ['x-b', '2'],
      ['x-a', '1'],
    ])
    const two = new Headers([
      ['x-a', '1'],
      ['x-b', '2'],
    ])
    expect(hashHttpOptions({ headers: one })).toBe(hashHttpOptions({ headers: two }))
  })

  it('describes a Blob by size and type', () => {
    const blob = new Blob(['hello'], { type: 'text/plain' })
    expect(differs({ body: blob }, { body: new Blob(['hello there'], { type: 'text/plain' }) })).toBe(true)
    expect(differs({ body: blob }, { body: new Blob(['hello'], { type: 'text/html' }) })).toBe(true)
  })

  it('includes a File name in its description', () => {
    const file = new File(['x'], 'one.txt', { type: 'text/plain' })
    const other = new File(['x'], 'two.txt', { type: 'text/plain' })
    expect(differs({ body: file }, { body: other })).toBe(true)
  })

  it('folds FormData, describing any Blob parts inside it', () => {
    const form = new FormData()
    form.append('name', 'ada')
    form.append('file', new Blob(['abc'], { type: 'text/plain' }), 'a.txt')
    const same = new FormData()
    same.append('file', new Blob(['abc'], { type: 'text/plain' }), 'a.txt')
    same.append('name', 'ada')
    expect(hashHttpOptions({ body: form })).toBe(hashHttpOptions({ body: same }))

    const different = new FormData()
    different.append('name', 'grace')
    expect(differs({ body: form }, { body: different })).toBe(true)
  })

  it('reduces a ReadableStream to a bare marker', () => {
    const streamOf = (chunk: string) =>
      new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode(chunk))
          controller.close()
        },
      })
    expect(hashHttpOptions({ body: streamOf('a') })).toBe(hashHttpOptions({ body: streamOf('b') }))
  })

  it('hashes an ArrayBuffer by its bytes', () => {
    const buffer = new Uint8Array([1, 2, 3]).buffer
    const same = new Uint8Array([1, 2, 3]).buffer
    const other = new Uint8Array([1, 2, 4]).buffer
    expect(hashHttpOptions({ body: buffer })).toBe(hashHttpOptions({ body: same }))
    expect(differs({ body: buffer }, { body: other })).toBe(true)
  })

  it('hashes a typed-array view by the bytes it spans, not the whole buffer', () => {
    const backing = new Uint8Array([9, 1, 2, 3, 9])
    const view = new Uint8Array(backing.buffer, 1, 3)
    expect(hashHttpOptions({ body: view })).toBe(hashHttpOptions({ body: new Uint8Array([1, 2, 3]) }))
  })
})

describe('hashHttpOptions — structure', () => {
  it('drops an AbortSignal, which is not part of the resource identity', () => {
    const controller = new AbortController()
    expect(hashHttpOptions({ method: 'GET', signal: controller.signal })).toBe(hashHttpOptions({ method: 'GET' }))
  })

  it('returns a stable hash whatever order the keys arrive in', () => {
    expect(hashHttpOptions({ method: 'POST', cache: 'no-store' })).toBe(hashHttpOptions({ cache: 'no-store', method: 'POST' }))
  })

  it('survives a circular reference', () => {
    const circular: Record<string, unknown> = { a: 1 }
    circular.self = circular
    expect(() => hashHttpOptions({ ...(circular as RequestInit) })).not.toThrow()
  })

  it('normalises arrays element by element', () => {
    expect(differs({ headers: [['a', '1']] }, { headers: [['a', '2']] })).toBe(true)
  })

  it('orders entries sharing a key by their value', () => {
    const one = new URLSearchParams([
      ['k', 'b'],
      ['k', 'a'],
    ])
    const two = new URLSearchParams([
      ['k', 'a'],
      ['k', 'b'],
    ])
    expect(hashHttpOptions({ body: one })).toBe(hashHttpOptions({ body: two }))
  })

  it('hashes absent options to a stable empty value', () => {
    expect(hashHttpOptions()).toBe(hashHttpOptions(undefined))
  })

  it('ignores a key whose value normalises away', () => {
    const controller = new AbortController()
    expect(hashHttpOptions({ signal: controller.signal })).toBe(hashHttpOptions({}))
  })

  it('passes primitives through untouched', () => {
    expect(differs({ method: 'GET' }, { method: 'POST' })).toBe(true)
    expect(differs({ keepalive: true }, { keepalive: false })).toBe(true)
  })

  it('treats null as a value rather than an object', () => {
    expect(() => hashHttpOptions({ body: null })).not.toThrow()
  })

  it('leaves two entries alike in both key and value in the order they came', () => {
    const one = new URLSearchParams([
      ['k', 'same'],
      ['k', 'same'],
    ])
    const two = new URLSearchParams([
      ['k', 'same'],
      ['k', 'same'],
    ])
    expect(hashHttpOptions({ body: one })).toBe(hashHttpOptions({ body: two }))
  })

  it('keeps a key whose value survives normalisation', () => {
    expect(hashHttpOptions({ method: 'GET', redirect: 'follow' })).not.toBe(hashHttpOptions({ method: 'GET' }))
  })

  it('normalises the elements of a nested array', () => {
    expect(
      differs(
        {
          headers: [
            ['a', '1'],
            ['b', '2'],
          ],
        },
        {
          headers: [
            ['a', '1'],
            ['b', '3'],
          ],
        },
      ),
    ).toBe(true)
  })
})
