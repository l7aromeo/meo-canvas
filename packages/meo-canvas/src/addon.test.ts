// What the platform lists must *be*, now that they can no longer disagree.
//
// `optionalDependencies` and `PLATFORM_PACKAGES` are generated from `TARGETS`
// by `just platform-packages`, and `just platform-packages-check` fails on a
// difference. So the assertions this file used to carry — that the three lists
// agreed — are gone, replaced by that check: **a generated file plus an
// equality test against its source is one mechanism written twice**, and a
// reader cannot tell which is authoritative.
//
// What remains is the half generation does not answer. A generator can produce
// a list faithfully derived from `TARGETS` and still wrong: keys that drop the
// libc, a caret range where an exact pin is required, a package named for one
// platform under a host key for another. Those are properties of the content
// rather than of how it is maintained, and no amount of regenerating settles
// them.

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { PLATFORM_PACKAGES, target } from './addon.js'
import { TARGETS, hostSuffix } from '../tools/stage-platform-package.mjs'

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
    // `platform-arch` and the package name ends in the target suffix, so the
    // platform word has to appear in both.
    for (const [host, name] of Object.entries(PLATFORM_PACKAGES)) {
      const platform = host.split('-')[0] as string
      expect(name.startsWith(`${PACKAGE.name}-${platform}`), `${host} resolves ${name}`).toBe(true)
    }
  })

  it('keys the resolver by the same suffix the builder uses', () => {
    // The invariant publishing musl introduced. `linux-x64` named one build
    // unambiguously while there was one; with a glibc and a musl build it names
    // two, so the host key carries the libc and these keys are exactly the
    // suffixes `TARGETS` builds. Comparing the values alone would not catch a
    // key that drifted from its suffix, and a key that no `target()` can ever
    // return resolves nothing while looking correct.
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
