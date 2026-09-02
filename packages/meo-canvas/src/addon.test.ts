// The three lists that describe which platforms a release carries, asserted
// against each other.
//
// A target is named in three places and each has a different job: `TARGETS` in
// `tools/stage-platform-package.mjs` is what a release *builds*,
// `optionalDependencies` is what an install *fetches*, and `PLATFORM_PACKAGES`
// in `addon.ts` is what a running process *resolves*. Any two agreeing while
// the third does not is a failure nothing else here can see:
//
// - built and pinned but not resolved — the binary installs and `require`
//   never looks for it, so every render fails with the addon on disk
// - pinned and resolved but not built — `npm install` fails on a package that
//   was never published, and a version bump is what surfaces it
// - built and resolved but not pinned — nothing installs it, and it works only
//   in a checkout, which is exactly where it would be tested
//
// Both directions, because a list that only reports additions goes stale
// silently when a target is dropped.

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { PLATFORM_PACKAGES, target } from './addon.js'
import { TARGETS } from '../tools/stage-platform-package.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const PACKAGE = JSON.parse(readFileSync(resolve(HERE, '../package.json'), 'utf8')) as {
  name: string
  version: string
  optionalDependencies?: Record<string, string>
}

/** The package names a release builds, derived from the suffixes. */
const built = Object.keys(TARGETS)
  .map(suffix => `${PACKAGE.name}-${suffix}`)
  .sort()

describe('the platform target lists', () => {
  it('builds exactly what the manifest pins', () => {
    expect(Object.keys(PACKAGE.optionalDependencies ?? {}).sort()).toEqual(built)
  })

  it('pins every platform package at the main package version', () => {
    // An exact pin rather than a range: the binary and the JavaScript that
    // calls it are one artefact cut at one commit, and a range would let a
    // package manager pair a new surface with an older addon.
    for (const [name, range] of Object.entries(PACKAGE.optionalDependencies ?? {})) {
      expect(range, `${name} is not pinned to the main package version`).toBe(PACKAGE.version)
    }
  })

  it('resolves exactly what it builds', () => {
    expect([...new Set(Object.values(PLATFORM_PACKAGES))].sort()).toEqual(built)
  })

  it('maps each host onto a package for its own platform', () => {
    // `darwin-arm64` must not resolve a linux binary. The host key is
    // `platform-arch` and the package name ends in the target suffix, so the
    // platform word has to appear in both.
    for (const [host, name] of Object.entries(PLATFORM_PACKAGES)) {
      const platform = host.split('-')[0] as string
      expect(name.startsWith(`${PACKAGE.name}-${platform}`), `${host} resolves ${name}`).toBe(true)
    }
  })

  it('names this host, so the suite runs where a release is built', () => {
    // Guards the case where every list agrees and none of them covers the
    // machine running the tests: CI would build, pin and resolve a set that
    // its own runner is not in, and nothing above would notice.
    expect(Object.keys(PLATFORM_PACKAGES)).toContain(target())
  })
})
