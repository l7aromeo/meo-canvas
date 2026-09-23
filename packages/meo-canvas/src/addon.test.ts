// What the platform lists must be. Generation from `TARGETS` keeps them agreeing,
// which `just platform-packages-check` enforces; this file checks their content,
// which a faithful generator cannot settle: keys carrying the libc, exact pins,
// and each package named for its own platform.

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { PLATFORM_PACKAGES, target } from './addon.js'
import { TARGETS, hostSuffix, manifest } from '../tools/stage-platform-package.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const PACKAGE = JSON.parse(readFileSync(resolve(HERE, '../package.json'), 'utf8')) as {
  name: string
  version: string
  optionalDependencies?: Record<string, string>
}

describe('the platform target lists', () => {
  it('pins every platform package at the main package version', () => {
    // An exact pin rather than a range: the binary and the JavaScript that
    // calls it are one artefact cut at one commit, and a range would let a
    // package manager pair a new surface with an older addon.
    for (const [name, range] of Object.entries(PACKAGE.optionalDependencies ?? {})) {
      expect(range, `${name} is not pinned to the main package version`).toBe(PACKAGE.version)
    }
  })

  it('maps each host onto a package for its own platform', () => {
    // `darwin-arm64` must not resolve a linux binary. The host key is
    // `platform-arch` and the package name is the target suffix under the
    // main package's own scope, so the platform word has to appear in both.
    for (const [host, name] of Object.entries(PLATFORM_PACKAGES)) {
      const platform = host.split('-')[0] as string
      expect(name.startsWith(`@${PACKAGE.name}/${platform}`), `${host} resolves ${name}`).toBe(true)
    }
  })

  it('names the staged package what the resolver will ask for', () => {
    // The staged package's own manifest, not a third list from the same function:
    // `PLATFORM_PACKAGES` and `optionalDependencies` both come from `packageName`
    // and agree whatever it returns, while the manifest decides whether an install
    // works.
    for (const [host, name] of Object.entries(PLATFORM_PACKAGES)) {
      expect(manifest(host, PACKAGE.version).name, `the staged package for ${host}`).toBe(name)
    }
  })

  it('scopes every platform package under the main package, which is not scoped', () => {
    // npm's spam heuristic refuses unscoped platform names, so the main package is
    // unscoped and the platform packages scoped. Pinned both ways: a scoped main
    // package would make `@${PACKAGE.name}/...` read `@@scope/name/...`.
    expect(PACKAGE.name.startsWith('@'), 'the main package is scoped').toBe(false)
    for (const name of Object.keys(PACKAGE.optionalDependencies ?? {})) {
      expect(name.startsWith(`@${PACKAGE.name}/`), `${name} is not under the scope`).toBe(true)
    }
  })

  it('keys the resolver by the same suffix the builder uses', () => {
    // With a glibc and a musl build, a host key carries the libc, so the keys are
    // exactly the suffixes `TARGETS` builds; comparing values alone would miss a key
    // no `target()` can return.
    expect(Object.keys(PLATFORM_PACKAGES).sort()).toEqual(Object.keys(TARGETS).sort())
  })

  it('distinguishes the two Linux C libraries, which is why the key grew', () => {
    // The pair that would collide under the old `platform-arch` key. If these
    // ever resolve to the same package, a musl host loads a glibc binary and
    // fails at first render rather than at install.
    expect(PLATFORM_PACKAGES['linux-x64-gnu']).not.toBe(PLATFORM_PACKAGES['linux-x64-musl'])
    expect(PLATFORM_PACKAGES['linux-arm64-gnu']).not.toBe(PLATFORM_PACKAGES['linux-arm64-musl'])
  })

  it('derives this host a suffix that the release actually builds', () => {
    // `just pack` asks for this rather than deciding with a ternary, so a
    // suffix it cannot find in `TARGETS` would stage a package under a name
    // nothing publishes.
    expect(Object.keys(TARGETS)).toContain(hostSuffix())
  })

  it('names this host, so the suite runs where a release is built', () => {
    // Guards the case where every list agrees and none of them covers the
    // machine running the tests: CI would build, pin and resolve a set that
    // its own runner is not in, and nothing above would notice.
    expect(Object.keys(PLATFORM_PACKAGES)).toContain(target())
  })
})
