// Fails once an `overrides` entry in the root package.json can go: every package
// in bun.lock depending on the overridden one accepts the forced major itself. It
// also fails on an override nothing depends on, or one the lockfile does not obey.
// Why each override exists is in docs/design.md, under Dependencies.
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..')

/** The leading major of a version or comparator, `undefined` for a wildcard. */
function majorOf(text) {
  const match = /^(?<major>\d+)/u.exec(text)
  return match?.groups === undefined ? undefined : Number(match.groups['major'])
}

/**
 * Whether a semver range admits some version of `major`. Reads the forms npm
 * manifests use -- `^`, `~`, exact, `x`, `*`, comparators and `||` -- at the
 * major only, which is all the question needs.
 */
function admitsMajor(range, major) {
  return range.split('||').some(set => {
    const comparators = set
      .trim()
      .split(/\s+/u)
      .filter(one => one !== '')
    return comparators.every(comparator => {
      const { op, version } = /^(?<op>[<>]=?|[\^~=]?)v?(?<version>.*)$/u.exec(comparator)?.groups ?? { op: '', version: comparator }
      const at = majorOf(version)
      if (at === undefined) return true
      if (op === '>=' || op === '>') return at <= major
      if (op === '<=') return major <= at
      if (op === '<') return major < at || (major === at && !/^\d+(\.0)*(-.*)?$/u.test(version))
      return at === major
    })
  })
}

/** The parser over ranges whose answer is known, so a parser saying yes or no to everything is caught. */
function probeRanges() {
  const cases = [
    ['^10.2.4', 11, false],
    ['^11.0.1', 11, true],
    ['10.2.6', 11, false],
    ['11.x', 11, true],
    ['^10.0.0 || ^11.0.0', 11, true],
    ['>=10 <11', 11, false],
    ['>=10 <12', 11, true],
    ['>=10.2 <11.0.0', 11, false],
    ['*', 11, true],
  ]
  const wrong = cases.filter(([range, major, admits]) => admitsMajor(range, major) !== admits)
  if (wrong.length > 0) {
    for (const [range, major, admits] of wrong) process.stderr.write(`  ${range} admits ${major}: expected ${admits}\n`)
    process.stderr.write('\nThe range reader is wrong about a range whose answer is known. Fix the reader rather than the case.\n')
    process.exit(1)
  }
}

/** bun.lock is JSON with trailing commas; its strings hold no `,]` or `,}`. */
function readLock(path) {
  return JSON.parse(readFileSync(path, 'utf8').replace(/,(\s*[\]}])/gu, '$1'))
}

probeRanges()

const manifest = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'))
const overrides = Object.entries(manifest.overrides ?? {})
const packages = Object.values(readLock(join(ROOT, 'bun.lock')).packages)

const faults = []
for (const [name, forced] of overrides) {
  const major = majorOf(forced.replace(/^[\^~=v]+/u, ''))
  if (major === undefined) {
    faults.push(`${name} is forced to \`${forced}\`, which names no major for this check to compare against`)
    continue
  }
  const resolved = packages.map(([ident]) => ident).filter(ident => ident.startsWith(`${name}@`))
  const disobeyed = resolved.filter(ident => majorOf(ident.slice(name.length + 1)) !== major)
  if (resolved.length === 0) faults.push(`${name} is forced to \`${forced}\` and bun.lock resolves no ${name} at all`)
  if (disobeyed.length > 0) faults.push(`${name} is forced to \`${forced}\` and bun.lock still resolves ${disobeyed.join(', ')}`)

  const dependents = packages
    .filter(([ident, , meta]) => !ident.startsWith(`${name}@`) && meta !== undefined)
    .map(([ident, , meta]) => [ident, meta.dependencies?.[name] ?? meta.optionalDependencies?.[name]])
    .filter(([, range]) => range !== undefined)
  const holding = dependents.filter(([, range]) => !admitsMajor(range, major))
  if (dependents.length === 0) {
    faults.push(`${name} is forced to \`${forced}\` and nothing in bun.lock depends on it: remove the override`)
  } else if (holding.length === 0) {
    faults.push(
      `${name} is forced to \`${forced}\` and every dependent now accepts ${major} itself ` +
        `(${dependents.map(([ident, range]) => `${ident} wants ${range}`).join(', ')}): remove the override and its note in docs/design.md`,
    )
  } else {
    process.stdout.write(`${name} forced to ${forced}; held back by ${holding.map(([ident, range]) => `${ident} (${range})`).join(', ')}\n`)
  }
}

if (faults.length > 0) {
  for (const fault of faults) process.stdout.write(`  ${fault}\n`)
  process.stderr.write(`\n${faults.length} override${faults.length === 1 ? '' : 's'} in package.json no longer doing what it is there for.\n`)
  process.exit(1)
}
process.stdout.write(`override probes: ${overrides.length} override${overrides.length === 1 ? '' : 's'} checked against bun.lock\n`)
