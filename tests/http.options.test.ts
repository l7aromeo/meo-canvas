import { hashHttpOptions } from '@/util/http.options.js'

describe('hashHttpOptions', () => {
  it('returns an empty string for undefined options', () => {
    expect(hashHttpOptions(undefined)).toBe('')
  })

  it('returns an empty string for an empty object', () => {
    expect(hashHttpOptions({})).toBe('')
  })

  it('produces the same hash for identical options', () => {
    const a = hashHttpOptions({ headers: { Authorization: 'Bearer x' }, method: 'GET' })
    const b = hashHttpOptions({ headers: { Authorization: 'Bearer x' }, method: 'GET' })
    expect(a).toBe(b)
    expect(a).not.toBe('')
  })

  it('produces different hashes for different header values', () => {
    const a = hashHttpOptions({ headers: { Authorization: 'Bearer A' } })
    const b = hashHttpOptions({ headers: { Authorization: 'Bearer B' } })
    expect(a).not.toBe(b)
  })

  it('is independent of key ordering', () => {
    const a = hashHttpOptions({ method: 'GET', headers: { A: '1', B: '2' } })
    const b = hashHttpOptions({ headers: { B: '2', A: '1' }, method: 'GET' })
    expect(a).toBe(b)
  })

  it('treats a Headers instance the same as an equivalent plain object', () => {
    const a = hashHttpOptions({ headers: new Headers({ Authorization: 'Bearer x' }) })
    // Headers lowercases keys, so the plain-object equivalent must also be lowercased
    const b = hashHttpOptions({ headers: { authorization: 'Bearer x' } })
    expect(a).toBe(b)
  })

  it('treats a URLSearchParams body deterministically', () => {
    const a = hashHttpOptions({ body: new URLSearchParams({ a: '1', b: '2' }) })
    const b = hashHttpOptions({ body: new URLSearchParams({ b: '2', a: '1' }) })
    expect(a).toBe(b)
  })

  it('does not throw on circular references and returns a stable non-empty hash', () => {
    const circular: any = { headers: { 'X-Test': '1' } }
    circular.self = circular
    let result = ''
    expect(() => {
      result = hashHttpOptions(circular)
    }).not.toThrow()
    expect(result).not.toBe('')
    // deterministic across calls
    const circular2: any = { headers: { 'X-Test': '1' } }
    circular2.self = circular2
    expect(hashHttpOptions(circular2)).toBe(result)
  })

  it('ignores an AbortSignal (identity-irrelevant for caching)', () => {
    const withSignal = hashHttpOptions({ headers: { A: '1' }, signal: new AbortController().signal })
    const without = hashHttpOptions({ headers: { A: '1' } })
    expect(withSignal).toBe(without)
  })

  it('distinguishes different methods', () => {
    expect(hashHttpOptions({ method: 'GET' })).not.toBe(hashHttpOptions({ method: 'POST' }))
  })
})
