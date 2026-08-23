/**
 * Compares v1's prop surface against v2's, and prints the comparison.
 *
 * # Why a recipe and not a document
 *
 * This was a checked-in Markdown table whose own header told the reader to
 * regenerate it rather than edit it. **A document cannot enforce that**: a
 * transcribed list is a copy, and a copy is only correct at the moment it is
 * made. A recipe cannot be stale because it is not a copy.
 *
 * # What it does when it breaks
 *
 * The comparison rests on a brace-depth scan of TypeScript, which is fragile —
 * a reformatting could silently drop an interface and the report would show a
 * smaller surface with nothing saying so. **A count floor would not help**: it
 * cannot tell a shrinking surface from a broken parser, because both move the
 * number the same way, and the first person to hit a legitimate removal will
 * tune it down until it checks nothing.
 *
 * So the guards name what must be there rather than how much:
 *
 * - **anchors** — interfaces that must be found, by name, each with a
 *   non-empty prop list. A missing anchor is a sentence about the instrument:
 *   *the scan found no `TextProps`*. Anchors present with fewer props is a
 *   sentence about v1.
 * - **balance** — the scan must end at depth zero. Losing track means
 *   everything after the first mistake is wrong and nothing downstream knows.
 * - **completeness** — the scan must reach the end of the file. The same
 *   failure as losing track, with no symptom at all.
 * - **provenance** — v1's tag, commit, the file and its size are printed, so a
 *   reader who sees `61 KB, 4 interfaces` knows something is wrong without
 *   knowing what.
 *
 * **The guards are shown to fire every time this runs**, not once when it was
 * written: `proveTheScanWorks` hands the scanner an interface it must find and
 * an unclosed one it must reject, before the real file is opened. A reader
 * cannot tell a live detector from a dead one, and neither can its author a
 * month later — so the proof travels with the guard rather than living in a
 * message.
 */

import { execFileSync } from 'node:child_process'
import { readFileSync, statSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const V1 = resolve(HERE, '../../../../meo-canvas-old')
const V1_TYPES = join(V1, 'src/canvas/canvas.type.ts')

/** v2's prop surface, in the files that declare it. */
const V2_FILES = ['style.ts', 'node.ts', 'root.ts', 'canvas.ts'].map(name => resolve(HERE, '../src', name))

/**
 * Interfaces that must be found, or the scan is broken rather than the surface
 * smaller. Chosen as the ones a caller cannot avoid: every scene has a box, a
 * root and something drawn in it.
 */
const V1_ANCHORS = ['BaseProps', 'BoxProps', 'TextProps', 'ImageProps', 'RootProps']

/** The interfaces in one TypeScript source, as name to prop names. */
export function interfaces(source, label) {
  const found = new Map()
  const opener = /export interface ([A-Za-z][A-Za-z0-9]*)[^{]*\{/g
  let match
  while ((match = opener.exec(source)) !== null) {
    const name = match[1]
    let depth = 1
    let index = match.index + match[0].length
    const body = []
    while (index < source.length && depth > 0) {
      const character = source[index]
      if (character === '{') depth += 1
      else if (character === '}') depth -= 1
      if (depth > 0) body.push(character)
      index += 1
    }
    if (depth !== 0) {
      throw new Error(`${label}: interface ${name} never closes — the scan lost its place, and everything after it is wrong`)
    }
    const props = [...body.join('').matchAll(/^\s{2}(?:readonly\s+)?([A-Za-z][A-Za-z0-9]*)\??\s*:/gm)].map(m => m[1])
    found.set(name, props)
  }
  return found
}

/** v1's tag and commit, so the report can be regenerated comparably. */
function provenance() {
  const git = args => execFileSync('git', ['-C', V1, ...args], { encoding: 'utf8' }).trim()
  try {
    return `${git(['describe', '--tags', '--always'])} (${git(['rev-parse', '--short', 'HEAD'])})`
  } catch {
    return 'unknown — not a git checkout'
  }
}

/**
 * Hands the scanner two inputs whose answers are known, before it is trusted
 * with a real file.
 *
 * A guard that cannot fail is worth nothing, and nothing about reading this
 * file would tell you which kind these are. **Cheap enough to run on every
 * invocation**, which is what makes it a property of the tool rather than a
 * thing someone once checked.
 */
function proveTheScanWorks() {
  const closed = interfaces('export interface TextProps {\n  color?: string\n}\n', 'self-test')
  if (!closed.has('TextProps') || closed.get('TextProps').length !== 1) {
    throw new Error('the scan cannot read an interface it was handed, so its answers about v1 mean nothing')
  }
  let rejected = false
  try {
    interfaces('export interface TextProps {\n  color?: string\n', 'self-test')
  } catch {
    rejected = true
  }
  if (!rejected) {
    throw new Error('the scan accepted an interface that never closes, so the balance guard is dead')
  }
}

function main() {
  proveTheScanWorks()

  let v1Source
  try {
    v1Source = readFileSync(V1_TYPES, 'utf8')
  } catch {
    console.error(`v1 is not where this expects it: ${V1_TYPES}`)
    console.error('Clone meo-canvas-old beside this repository, or pass its path in.')
    process.exit(1)
  }

  const v1 = interfaces(v1Source, 'v1')
  const missing = V1_ANCHORS.filter(name => !v1.has(name) || v1.get(name).length === 0)
  if (missing.length > 0) {
    console.error(`the scan found no ${missing.join(', ')} in ${V1_TYPES}`)
    console.error('That is this script failing to read v1, not v1 having fewer props.')
    process.exit(1)
  }

  const v2 = new Map()
  for (const file of V2_FILES) {
    for (const [name, props] of interfaces(readFileSync(file, 'utf8'), file)) {
      v2.set(name, props)
    }
  }

  const size = statSync(V1_TYPES).size
  console.log(`v1 ${provenance()}`)
  console.log(`   ${V1_TYPES}`)
  console.log(`   ${(size / 1024).toFixed(1)} KB, ${v1.size} interfaces`)
  console.log(`v2 ${V2_FILES.length} files, ${v2.size} interfaces\n`)

  const v2Props = new Set([...v2.values()].flat())
  for (const name of [...v1.keys()].sort()) {
    const props = v1.get(name)
    const absent = props.filter(prop => !v2Props.has(prop))
    const mark = absent.length === 0 ? 'all' : `${props.length - absent.length}/${props.length}`
    console.log(`${name.padEnd(28)} ${mark}`)
    if (absent.length > 0) console.log(`${' '.repeat(30)}absent: ${absent.join(', ')}`)
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main()
