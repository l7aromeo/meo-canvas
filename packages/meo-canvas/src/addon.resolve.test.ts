// What `resolveAddon` says when it cannot produce an addon.
//
// **These are the paths with no coverage and the highest cost of being wrong.**
// A bare `export { PLATFORM_PACKAGES } from ...` — which does not bind the name
// locally — passed the whole suite and would have thrown a `ReferenceError` on
// the "no prebuilt addon is published" message, because both internal reads sit
// on error paths nothing reached. `tsc` caught that one. It will not catch a
// message that names the wrong host, offers the wrong fix, or says a package
// was not found when it was found and would not load.
//
// # Injected rather than contrived
//
// None of this builds a broken host. `resolveAddon` takes its `require` from
// `createRequire`, imported at module scope, so mocking `node:module` replaces
// every resolution it performs — and the host triple comes from `process`,
// which is stubbed per test. That seam already existed: nothing here asked for
// a change to `resolveAddon` to make it reachable, which is the trade worth
// refusing.

import { afterEach, describe, expect, it, vi } from 'vitest'

/** The `require` the module under test will be handed, swapped per test. */
const injected = vi.hoisted(() => ({
  current: undefined as unknown as ((id: string) => unknown) & { resolve: (id: string) => string },
}))

vi.mock('node:module', () => ({ createRequire: () => injected.current }))

const { resolveAddon, PLATFORM_PACKAGES } = await import('./addon.js')

/**
 * A `require` that answers from a table and throws for anything absent.
 *
 * `resolve` succeeds for an id the table knows even when requiring it throws,
 * which is the whole distinction under test: installed-and-broken is a
 * different problem from not-installed, with a different fix.
 */
function requiring(table: Record<string, unknown | (() => never)>) {
  const load = (id: string) => {
    if (!(id in table)) throw Object.assign(new Error(`Cannot find module '${id}'`), { code: 'MODULE_NOT_FOUND' })
    const entry = table[id]
    if (typeof entry === 'function') return (entry as () => never)()
    return entry
  }
  return Object.assign(load, {
    resolve: (id: string) => {
      if (!(id in table)) throw Object.assign(new Error(`Cannot find module '${id}'`), { code: 'MODULE_NOT_FOUND' })
      return `/fake/${id}`
    },
  })
}

/** Pretends this process is running on `platform`/`arch`, with `glibc` or musl. */
function host({ platform, arch, glibc }: { platform: string; arch: string; glibc?: string }) {
  for (const [key, value] of Object.entries({ platform, arch })) {
    Object.defineProperty(process, key, { value, configurable: true })
  }
  vi.spyOn(process, 'report', 'get').mockReturnValue({
    getReport: () => (glibc === undefined ? { header: {} } : { header: { glibcVersionRuntime: glibc } }),
  } as unknown as typeof process.report)
}

const REAL = { platform: process.platform, arch: process.arch }

afterEach(() => {
  for (const [key, value] of Object.entries(REAL)) {
    Object.defineProperty(process, key, { value, configurable: true })
  }
  vi.restoreAllMocks()
  vi.unstubAllEnvs()
})

describe('a platform nothing is published for', () => {
  it('names the host triple and what is published', () => {
    host({ platform: 'sunos', arch: 'x64' })
    injected.current = requiring({})

    // The message a user on an unsupported platform gets, which is the one
    // that would have been a `ReferenceError`.
    expect(() => resolveAddon()).toThrow(/no prebuilt addon is published for sunos-x64/)
    // The published list is host triples rather than package names, which is
    // what a reader compares their own host against.
    expect(() => resolveAddon()).toThrow(/darwin-arm64, linux-arm64-gnu/)
    expect(() => resolveAddon()).toThrow(/just addon/)
  })

  it('names a musl host as musl, on an architecture with no musl build', () => {
    // The host key carries the libc, so the triple reported is the one a
    // reader would look for in the published list rather than `linux-riscv64`
    // with the deciding half missing.
    host({ platform: 'linux', arch: 'riscv64' })
    injected.current = requiring({})

    expect(() => resolveAddon()).toThrow(/no prebuilt addon is published for linux-riscv64-musl/)
  })
})

describe('a glibc host', () => {
  it('is given the gnu build, because `glibc` and `gnu` are different words', () => {
    // **The regression.** `target()` returned the C library's name, so a glibc
    // Linux host derived `linux-x64-glibc`, which matches no key, and every
    // such host was told no addon is published for it -- the whole primary
    // Linux target. npm's `libc` field takes `glibc`; every triple naming the
    // same thing spells it `gnu`.
    //
    // It survived because musl is spelled identically in both, so the musl
    // half of the keying worked and the tests that could see it run on darwin.
    // Only a Linux CI run, or this, catches it.
    host({ platform: 'linux', arch: 'x64', glibc: '2.39' })
    const addon = { rendered: true }
    injected.current = requiring({ 'meo-canvas-linux-x64-gnu': addon })

    expect(resolveAddon()).toBe(addon)
  })

  it('derives a triple the resolver actually has a key for', () => {
    // The property behind it, asserted directly rather than through a load:
    // whatever `target()` builds must be findable in the published list, on
    // every host shape rather than on the one running the suite.
    for (const shape of [
      { platform: 'linux', arch: 'x64', glibc: '2.39' },
      { platform: 'linux', arch: 'arm64', glibc: '2.39' },
      { platform: 'linux', arch: 'x64' },
      { platform: 'linux', arch: 'arm64' },
      { platform: 'darwin', arch: 'arm64' },
      { platform: 'win32', arch: 'x64' },
    ]) {
      host(shape)
      injected.current = requiring({})
      expect(() => resolveAddon()).not.toThrow(/no prebuilt addon is published/)
    }
  })
})

describe('a musl host', () => {
  it('is given the musl build rather than refused', () => {
    // `resolveAddon` used to throw unconditionally on any musl host — correct
    // until the day musl publishes, and then a refusal issued *after* the
    // resolver had the right package to hand. Nothing reached it, and no
    // target expansion would have.
    host({ platform: 'linux', arch: 'x64' })
    const addon = { rendered: true }
    injected.current = requiring({ 'meo-canvas-linux-x64-musl': addon })

    expect(resolveAddon()).toBe(addon)
  })

  it('is not given the glibc build', () => {
    // The collision the old `platform-arch` key would have had. A musl host
    // loading a glibc binary fails at first render rather than at install.
    host({ platform: 'linux', arch: 'x64' })
    injected.current = requiring({ 'meo-canvas-linux-x64-gnu': { wrong: true } })

    expect(() => resolveAddon()).toThrow(/not found/)
  })

  it('and a glibc host on the same architecture are given different packages', () => {
    expect(PLATFORM_PACKAGES['linux-x64-musl']).not.toBe(PLATFORM_PACKAGES['linux-x64-gnu'])
  })
})

describe('a platform package that resolves and will not load', () => {
  const dlopen = (message: string) => () => {
    throw new Error(message)
  }

  it('says it is installed rather than that it was not found', () => {
    host({ platform: 'linux', arch: 'x64', glibc: '2.39' })
    injected.current = requiring({
      'meo-canvas-linux-x64-gnu': dlopen('some loader complaint'),
      'meo-canvas-linux-x64-gnu/package.json': {},
    })

    // The distinction `require.resolve` exists to draw: `npm install` is the
    // fix for one of these and the host is the fix for the other.
    expect(() => resolveAddon()).toThrow(/is installed/)
    expect(() => resolveAddon()).not.toThrow(/was not found/)
  })

  it('names the missing shared object and what installs it', () => {
    // The failure a consumer meets first: a stock `node:22-slim` has neither
    // `libfontconfig.so.1` nor `libfreetype.so.6`.
    host({ platform: 'linux', arch: 'x64', glibc: '2.39' })
    injected.current = requiring({
      'meo-canvas-linux-x64-gnu': dlopen('libfontconfig.so.1: cannot open shared object file: No such file or directory'),
      'meo-canvas-linux-x64-gnu/package.json': {},
    })

    expect(() => resolveAddon()).toThrow(/libfontconfig\.so\.1/)
    expect(() => resolveAddon()).toThrow(/libfontconfig1/)
    expect(() => resolveAddon()).toThrow(/fontconfig/)
  })

  it('names both glibc versions when the host is below the declared floor', () => {
    // The number the host has and the number the binary wants, from the
    // manifest of the package that would not load — readable precisely because
    // a manifest loads when the `.node` beside it does not.
    host({ platform: 'linux', arch: 'x64', glibc: '2.28' })
    injected.current = requiring({
      'meo-canvas-linux-x64-gnu': dlopen('version GLIBC_2.35 not found'),
      'meo-canvas-linux-x64-gnu/package.json': { meoCanvas: { floors: { glibc: '2.35', glibcxx: '3.4.30' } } },
    })

    expect(() => resolveAddon()).toThrow(/needs glibc 2\.35 or newer/)
    expect(() => resolveAddon()).toThrow(/this host has 2\.28/)
  })

  it('passes the loader through when no floor explains it', () => {
    // The unversioned-symbol case, which no declared floor can see: a binary
    // under every ceiling still failing on `_M_replace_cold`. Guessing a cause
    // would be worse than the fact.
    host({ platform: 'linux', arch: 'x64', glibc: '2.39' })
    injected.current = requiring({
      'meo-canvas-linux-x64-gnu': dlopen('undefined symbol: _M_replace_cold'),
      'meo-canvas-linux-x64-gnu/package.json': { meoCanvas: { floors: { glibc: '2.35' } } },
    })

    expect(() => resolveAddon()).toThrow(/_M_replace_cold/)
  })
})

describe('the override', () => {
  it('is an error when it does not load, never a fallback', () => {
    // Set deliberately, so silently loading a different binary than the one
    // named is how a test reports on code nobody asked it about.
    host({ platform: 'linux', arch: 'x64', glibc: '2.39' })
    vi.stubEnv('MEO_CANVAS_ADDON', '/nowhere/addon.node')
    injected.current = requiring({ 'meo-canvas-linux-x64-gnu': { wouldHaveWorked: true } })

    expect(() => resolveAddon()).toThrow(/MEO_CANVAS_ADDON is set to \/nowhere\/addon\.node/)
  })
})
