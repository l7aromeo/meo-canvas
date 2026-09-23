// What `resolveAddon` says when it cannot produce an addon: the error paths nothing
// else reaches, where a message can name the wrong host or offer the wrong fix.
// `node:module` is mocked, since `resolveAddon` takes its `require` from
// `createRequire`, and the host triple is stubbed on `process` per test.

import { afterEach, describe, expect, it, vi } from 'vitest'

/** The `require` the module under test will be handed, swapped per test. */
const injected = vi.hoisted(() => ({
  current: undefined as unknown as ((id: string) => unknown) & { resolve: (id: string) => string },
}))

vi.mock('node:module', () => ({ createRequire: () => injected.current }))

const { resolveAddon, PLATFORM_PACKAGES } = await import('./addon.js')

/**
 * A `require` that answers from a table and throws for anything absent. `resolve`
 * succeeds for an id the table knows even when requiring it throws, which is
 * installed-and-broken as opposed to not installed.
 */
function requiring(table: Record<string, unknown>) {
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
    // **`just addon` on its own is an instruction to someone with a checkout**,
    // and this reader ran `npm install`. The repository is named so the advice
    // is a step rather than a command they do not have.
    expect(() => resolveAddon()).toThrow(/checkout of https:\/\/github\.com\/l7aromeo\/meo-canvas with `just addon`/)
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
    // `target()` spells a glibc host's suffix `gnu`: npm's `libc` field says
    // `glibc`, and a key built from that would match nothing. musl is spelled the
    // same both ways, so only a glibc Linux host shows it.
    host({ platform: 'linux', arch: 'x64', glibc: '2.39' })
    const addon = { rendered: true }
    injected.current = requiring({ '@meo-canvas/linux-x64-gnu': addon })

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
    injected.current = requiring({ '@meo-canvas/linux-x64-musl': addon })

    expect(resolveAddon()).toBe(addon)
  })

  it('is not given the glibc build', () => {
    // The collision the old `platform-arch` key would have had. A musl host
    // loading a glibc binary fails at first render rather than at install.
    host({ platform: 'linux', arch: 'x64' })
    injected.current = requiring({ '@meo-canvas/linux-x64-gnu': { wrong: true } })

    expect(() => resolveAddon()).toThrow(/not found/)
  })

  it('and a glibc host on the same architecture are given different packages', () => {
    expect(PLATFORM_PACKAGES['linux-x64-musl']).not.toBe(PLATFORM_PACKAGES['linux-x64-gnu'])
  })
})

describe('a platform package that does not resolve', () => {
  it('names all three ways it goes missing, because it cannot tell them apart', () => {
    host({ platform: 'darwin', arch: 'arm64' })
    injected.current = requiring({})

    // Skipped at install, bundled away from its `node_modules`, or never installed:
    // the three arrive identically, so all three are named, the bundler case
    // because a bundle works beside its tree and fails once copied out alone.
    expect(() => resolveAddon()).toThrow(/was not found in 2 places/)
    expect(() => resolveAddon()).toThrow(/--omit=optional/)
    expect(() => resolveAddon()).toThrow(/mark meo-canvas external/)
    expect(() => resolveAddon()).toThrow(/checkout of https:\/\/github\.com\/l7aromeo\/meo-canvas/)
  })
})

describe('a platform package that resolves and will not load', () => {
  const dlopen = (message: string) => () => {
    throw new Error(message)
  }

  it('says it is installed rather than that it was not found', () => {
    host({ platform: 'linux', arch: 'x64', glibc: '2.39' })
    injected.current = requiring({
      '@meo-canvas/linux-x64-gnu': dlopen('some loader complaint'),
      '@meo-canvas/linux-x64-gnu/package.json': {},
    })

    // The distinction `require.resolve` exists to draw: `npm install` is the
    // fix for one of these and the host is the fix for the other.
    expect(() => resolveAddon()).toThrow(/is installed/)
    expect(() => resolveAddon()).not.toThrow(/was not found/)
  })

  it('names the missing shared object and what installs it', () => {
    // No published binary reaches this, since fontconfig and freetype are linked
    // statically; the branch is kept for a dynamically linked target, a
    // `MEO_CANVAS_ADDON` binary and musl's `libstdc++`, and this keeps it honest.
    host({ platform: 'linux', arch: 'x64', glibc: '2.39' })
    injected.current = requiring({
      '@meo-canvas/linux-x64-gnu': dlopen('libfontconfig.so.1: cannot open shared object file: No such file or directory'),
      '@meo-canvas/linux-x64-gnu/package.json': {},
    })

    expect(() => resolveAddon()).toThrow(/libfontconfig\.so\.1/)
    expect(() => resolveAddon()).toThrow(/libfontconfig1/)
    expect(() => resolveAddon()).toThrow(/fontconfig/)
  })

  it("recognises musl's wording, which is not glibc's", () => {
    // musl words this differently from glibc, and is the one target where a missing
    // library is expected: its artefact needs a `libstdc++` a bare Alpine image
    // lacks. Both strings were read off the loaders, on `node:22-slim` and
    // `node:22-alpine`, loading an object with a `DT_NEEDED` nothing provides.
    host({ platform: 'linux', arch: 'x64' })
    injected.current = requiring({
      '@meo-canvas/linux-x64-musl': dlopen('Error loading shared library libstdc++.so.6: No such file or directory (needed by /app/meo-canvas.node)'),
      '@meo-canvas/linux-x64-musl/package.json': {},
    })

    expect(() => resolveAddon()).toThrow(/loading it needs libstdc\+\+\.so\.6/)
    // And the remedy is for the library it found. This said `libfontconfig1`
    // whatever was missing, which is a fix for something else and reads as a
    // package that does not know what it needs.
    expect(() => resolveAddon()).toThrow(/libstdc\+\+6/)
    expect(() => resolveAddon()).not.toThrow(/libfontconfig1/)
  })

  it('does not read a wrong-architecture binary as a missing dependency', () => {
    // musl opens both sentences with `Error loading shared library`, and this one is
    // a binary for another architecture, with a different fix. It is what
    // `node:22-alpine` says when handed the glibc build.
    host({ platform: 'linux', arch: 'x64' })
    injected.current = requiring({
      '@meo-canvas/linux-x64-musl': dlopen('Error loading shared library /app/meo-canvas.node: Exec format error'),
      '@meo-canvas/linux-x64-musl/package.json': {},
    })

    expect(() => resolveAddon()).not.toThrow(/loading it needs/)
    expect(() => resolveAddon()).toThrow(/would not load: Error loading shared library/)
  })

  it('says which command asks, for a library it has no package name for', () => {
    host({ platform: 'linux', arch: 'x64', glibc: '2.39' })
    injected.current = requiring({
      '@meo-canvas/linux-x64-gnu': dlopen('libmeo-nosuch.so.1: cannot open shared object file: No such file or directory'),
      '@meo-canvas/linux-x64-gnu/package.json': {},
    })

    expect(() => resolveAddon()).toThrow(/apt-file search/)
    expect(() => resolveAddon()).not.toThrow(/libfontconfig1/)
  })

  it('names both glibc versions when the host is below the declared floor', () => {
    // The number the host has and the number the binary wants, from the
    // manifest of the package that would not load — readable precisely because
    // a manifest loads when the `.node` beside it does not.
    host({ platform: 'linux', arch: 'x64', glibc: '2.28' })
    injected.current = requiring({
      '@meo-canvas/linux-x64-gnu': dlopen('version GLIBC_2.35 not found'),
      '@meo-canvas/linux-x64-gnu/package.json': { meoCanvas: { floors: { glibc: '2.35', glibcxx: '3.4.30' } } },
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
      '@meo-canvas/linux-x64-gnu': dlopen('undefined symbol: _M_replace_cold'),
      '@meo-canvas/linux-x64-gnu/package.json': { meoCanvas: { floors: { glibc: '2.35' } } },
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
    injected.current = requiring({ '@meo-canvas/linux-x64-gnu': { wouldHaveWorked: true } })

    expect(() => resolveAddon()).toThrow(/MEO_CANVAS_ADDON is set to \/nowhere\/addon\.node/)
  })
})
