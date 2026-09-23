// Does the built binary demand more symbol versions than its target's `floors` in
// `TARGETS` declare? It names the symbol that drifted, and prints the measured
// numbers either way, so a floor declared too high shows. It cannot prove the binary
// loads -- an unversioned symbol has no tag -- which `acceptance.mjs` decides.

import { execFileSync } from 'node:child_process'
import { existsSync } from 'node:fs'

import { TARGETS } from './stage-platform-package.mjs'

/** The symbol families a floor is expressed in. */
const FAMILIES = { glibc: 'GLIBC', glibcxx: 'GLIBCXX' }

/** Compares two dotted versions, newest last. */
function newer(left, right) {
  const a = left.split('.').map(Number)
  const b = right.split('.').map(Number)
  for (let at = 0; at < Math.max(a.length, b.length); at += 1) {
    const diff = (a[at] ?? 0) - (b[at] ?? 0)
    if (diff !== 0) return diff > 0
  }
  return false
}

/**
 * Every versioned symbol the binary imports, grouped by family: undefined symbols
 * only, since a defined one is provided rather than demanded.
 */
function required(binary) {
  const dump = execFileSync('objdump', ['-T', binary], { encoding: 'utf8', maxBuffer: 64 << 20 })
  const found = new Map()
  for (const line of dump.split('\n')) {
    if (!line.includes('*UND*')) continue
    const version = /\b(GLIBC|GLIBCXX|CXXABI)_([0-9][0-9.]*)/.exec(line)
    if (version === null) continue
    const [, family, number] = version
    const symbol = line.trim().split(/\s+/).pop()
    const seen = found.get(family)
    if (seen === undefined || newer(number, seen.version)) {
      found.set(family, { version: number, symbols: [symbol] })
    } else if (number === seen.version && seen.symbols.length < 5) {
      seen.symbols.push(symbol)
    }
  }
  return found
}

const [suffix, binary] = process.argv.slice(2)
if (suffix === undefined || binary === undefined) {
  process.stderr.write('usage: check-abi-floor.mjs <target suffix> <path to the built binary>\n')
  process.exit(2)
}

const target = TARGETS[suffix]
if (target === undefined) {
  process.stderr.write(`no target named ${suffix}; known: ${Object.keys(TARGETS).join(', ')}\n`)
  process.exit(2)
}
if (!existsSync(binary)) {
  process.stderr.write(`no binary at ${binary}\n`)
  process.exit(2)
}

// Said out loud rather than skipped. An unchecked target that prints nothing is
// indistinguishable from one that passed.
if (target.floors === undefined) {
  process.stdout.write(`${suffix} declares no ELF floors — nothing here to check.\n`)
  process.stdout.write('That is correct for darwin, win32 and musl, and a defect for any other target.\n')
  process.exit(0)
}

const measured = required(binary)

// An empty read is not portability. "No versions found" and "no versions
// needed" print the same way, and only one of them is good news.
if (measured.size === 0) {
  process.stderr.write(`no version references were read from ${binary} at all.\n`)
  process.stderr.write('This check is no longer reading the binary. Do not read a pass into it.\n')
  process.exit(1)
}

let failed = false
for (const [key, family] of Object.entries(FAMILIES)) {
  const ceiling = target.floors[key]
  const seen = measured.get(family)
  if (ceiling === undefined) continue
  if (seen === undefined) {
    process.stdout.write(`  ${family}: nothing required (declared ${ceiling})\n`)
    continue
  }
  const over = newer(seen.version, ceiling)
  process.stdout.write(`  ${family}: requires ${seen.version}, declared ${ceiling}${over ? '  EXCEEDED' : ''}\n`)
  if (over) {
    failed = true
    process.stderr.write(`::error::${suffix} requires ${family}_${seen.version}, above the ${ceiling} declared in TARGETS.\n`)
    process.stderr.write(`Pinned by: ${seen.symbols.join(', ')}\n`)
    process.stderr.write(
      'Either the build base moved, or a dependency started using a newer symbol. Lower the base or raise the declaration deliberately -- and remember the declaration is what consumers are promised.\n',
    )
  }
}

// Reported whether or not it exceeds, because the failure this cannot catch is
// a declaration that is too high, and the only way to see one is to read the
// measured number beside it.
const extra = [...measured].filter(([family]) => !Object.values(FAMILIES).includes(family))
for (const [family, seen] of extra) process.stdout.write(`  ${family}: requires ${seen.version} (not declared)\n`)

process.exit(failed ? 1 : 0)
