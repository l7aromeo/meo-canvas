/**
 * Compares v9's prop surface and exported functions against this renderer's, and
 * prints the comparison: a recipe rather than a checked-in table, which would be a
 * copy. Its guards name what must be found (anchors, a balanced and complete scan,
 * provenance), and `proveTheScanWorks` shows on every run that they fire.
 */

import { execFileSync } from 'node:child_process'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const V1 = resolve(HERE, '../../../../meo-canvas-old')
const V1_TYPES = join(V1, 'src/canvas/canvas.type.ts')

/** This renderer's prop surface, in the files that declare it. */
const V2_SRC = resolve(HERE, '../src')

/**
 * Where v9 keeps the helpers it exports as functions, and this renderer's built
 * entry point: `canvas.type.ts` holds interfaces and no functions, and a re-export
 * chain is what a caller resolves, so the entry point is read rather than parsed.
 */
const V1_INDEX = join(V1, 'src/index.ts')
const V1_ANIMATE = join(V1, 'src/animate')
const V2_DIST = resolve(HERE, '../dist/index.js')

/**
 * Sources deliberately outside the comparison, each with its reason. The list read
 * is derived from the directory, so anything left out has to say why.
 */
const V2_SKIPPED = new Map([
  ['index.ts', 're-exports only; declares no props of its own'],
  ['arena.ts', 'the wire encoder, not a caller-facing surface'],
])

/** Every non-test source under `src`, minus what is deliberately skipped. */
function v2Sources() {
  const all = readdirSync(V2_SRC).filter(name => name.endsWith('.ts') && !name.endsWith('.test.ts'))
  return { all, read: all.filter(name => !V2_SKIPPED.has(name)) }
}

/**
 * Interfaces that must be found, or the scan is broken rather than the surface
 * smaller. Chosen as the ones a caller cannot avoid: every scene has a box, a
 * root and something drawn in it.
 */
const V1_ANCHORS = ['BaseProps', 'BoxProps', 'TextProps', 'ImageProps', 'RootProps']

/**
 * The interfaces this renderer's scan must find, by name: a shortfall on either
 * side hides a difference, and a name fails when the scan stops reading a form,
 * where a count would be lowered by whoever it first inconvenienced.
 */
const V2_ANCHORS = ['Style', 'TextProps', 'ImageProps', 'PathProps', 'RootProps']

/** The interfaces in one TypeScript source, as name to prop names. */
export function interfaces(source, label) {
  const found = new Map()
  // Three declaration forms: v9 writes `export interface X {` and this renderer
  // `export type X = Style & {`, whose brace may follow a wrapped line. The span
  // before `{` may cross newlines but not a blank line or another `export`, or a
  // brace-less alias would swallow the block after it.
  const opener = /export (?:interface|type) ([A-Za-z][A-Za-z0-9]*)(?:(?!\n\s*\n|export )[^{])*(?<!\$)\{/g
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
      // Only the outermost level: a nested object literal's fields are not
      // props of this type, and collecting them made indentation the thing
      // that decided, which is how `TextProps` reported zero.
      if (depth === 1) body.push(character)
      index += 1
    }
    if (depth !== 0) {
      throw new Error(`${label}: interface ${name} never closes — the scan lost its place, and everything after it is wrong`)
    }
    // The `?` is captured, so `required` below can report a prop that v9 leaves
    // optional and this renderer requires.
    const matched = [...body.join('').matchAll(/^\s*(?:readonly\s+)?([A-Za-z][A-Za-z0-9]*)(\??)\s*:/gm)]
    const props = matched.map(match => match[1])
    props.required = new Set(matched.filter(match => match[2] === '').map(match => match[1]))
    found.set(name, props)
  }
  return found
}

/**
 * The functions each package exports, compared by name: v9's harvested from its
 * sources, this renderer's read from the built entry point. A missing `dist` is
 * said rather than counted as an empty surface.
 */
async function reportExports() {
  console.log('')
  const harvest = file => [...readFileSync(file, 'utf8').matchAll(/^export (?:function|const) ([A-Za-z_]\w*)/gm)].map(match => match[1])

  const theirs = new Set()
  for (const file of [
    V1_INDEX,
    ...readdirSync(V1_ANIMATE)
      .filter(name => name.endsWith('.ts'))
      .map(name => join(V1_ANIMATE, name)),
  ]) {
    try {
      for (const name of harvest(file)) theirs.add(name)
    } catch {
      // A file v9 no longer has is not this report's problem to raise.
    }
  }
  // v9 re-exports these from `index.ts` without declaring them there.
  for (const match of readFileSync(V1_INDEX, 'utf8').matchAll(/^export \{([^}]*)\}/gm)) {
    for (const name of match[1].split(',')) {
      const bare = name
        .trim()
        .split(/\s+as\s+/)
        .pop()
        ?.trim()
      if (bare !== undefined && bare !== '' && !bare.startsWith('type ')) theirs.add(bare)
    }
  }

  let ours
  try {
    ours = new Set(Object.keys(await import(pathToFileURL(V2_DIST).href)))
  } catch {
    console.log('exported functions      NOT COMPARED -- no dist; run `just build-js` first')
    return
  }

  const absent = [...theirs].filter(name => !ours.has(name)).sort()
  console.log(`exported names          v9 ${theirs.size}, this renderer ${ours.size}`)
  if (absent.length > 0) console.log(`${' '.repeat(24)}absent from this renderer: ${absent.join(', ')}`)
}

/** v9's tag and commit, so the report can be regenerated comparably. */
function provenance() {
  const git = args => execFileSync('git', ['-C', V1, ...args], { encoding: 'utf8' }).trim()
  try {
    return `${git(['describe', '--tags', '--always'])} (${git(['rev-parse', '--short', 'HEAD'])})`
  } catch {
    return 'unknown — not a git checkout'
  }
}

/**
 * Hands the scanner inputs whose answers are known before it is trusted with a real
 * file: every declaration form it must read, a union alias it must not, and an
 * unclosed interface it must reject. Cheap enough to run every time.
 */
function proveTheScanWorks() {
  // Every declaration form, since the one it is never given is the one it cannot
  // read.
  const forms = [
    ['interface', 'export interface TextProps {\n  color?: string\n}\n'],
    ['extends', 'export interface TextProps extends Base {\n  color?: string\n}\n'],
    ['intersection', 'export type TextProps = Style & {\n  readonly color?: string\n}\n'],
  ]
  for (const [form, source] of forms) {
    const parsed = interfaces(source, 'self-test')
    if (!parsed.has('TextProps') || parsed.get('TextProps').length !== 1) {
      throw new Error(`the scan cannot read a ${form} declaration, so whatever it reports about that form is silence rather than absence`)
    }
  }

  // The other direction, since over-reading looks healthier than under-reading: a
  // union alias has no props, and a template literal's `{` is not a body.
  const union = interfaces('export type Length = number | `${number}%`\n', 'self-test')
  if (union.size !== 0) {
    throw new Error(`the scan invented ${[...union.keys()].join(', ')} out of a union alias, so its interface count is noise`)
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

async function main() {
  proveTheScanWorks()

  let v1Source
  try {
    v1Source = readFileSync(V1_TYPES, 'utf8')
  } catch {
    console.error(`v9 is not where this expects it: ${V1_TYPES}`)
    console.error('Clone meo-canvas-old beside this repository, or pass its path in.')
    process.exit(1)
  }

  const v1 = interfaces(v1Source, 'v9')
  const missing = V1_ANCHORS.filter(name => !v1.has(name) || v1.get(name).length === 0)
  if (missing.length > 0) {
    console.error(`the scan found no ${missing.join(', ')} in ${V1_TYPES}`)
    console.error('That is this script failing to read v9, not v9 having fewer props.')
    process.exit(1)
  }

  const v2 = new Map()
  const sources = v2Sources()
  for (const file of sources.read) {
    const path = join(V2_SRC, file)
    for (const [name, props] of interfaces(readFileSync(path, 'utf8'), path)) {
      v2.set(name, props)
    }
  }
  const unseen = V2_ANCHORS.filter(name => !v2.has(name))
  if (unseen.length > 0) {
    console.error(`the scan found no ${unseen.join(', ')} in this renderer's own sources`)
    console.error('That is this script failing to read this renderer, not this renderer having fewer props.')
    process.exit(1)
  }

  const size = statSync(V1_TYPES).size
  console.log(`v9 ${provenance()}`)
  console.log(`   ${V1_TYPES}`)
  console.log(`   ${(size / 1024).toFixed(1)} KB, ${v1.size} interfaces`)
  console.log(`this renderer ${sources.read.length} of ${sources.all.length} sources ` + `(${V2_SKIPPED.size} skipped), ${v2.size} interfaces\n`)

  const v2Props = new Set([...v2.values()].flat())
  for (const name of [...v1.keys()].sort()) {
    const props = v1.get(name)
    const absent = props.filter(prop => !v2Props.has(prop))
    const mark = absent.length === 0 ? 'all' : `${props.length - absent.length}/${props.length}`
    console.log(`${name.padEnd(28)} ${mark}`)
    if (absent.length > 0) console.log(`${' '.repeat(30)}absent: ${absent.join(', ')}`)
  }

  await reportExports()

  // Present on both surfaces and required on only one. A caller feels this the
  // way they feel a missing prop -- their code does not compile -- and the
  // section above cannot report it, because the prop is right there.
  console.log('')
  let stricter = 0
  for (const [name, props] of [...v2.entries()].sort()) {
    const theirs = v1.get(name)
    if (theirs === undefined) continue
    const newlyRequired = [...props.required].filter(prop => theirs.includes(prop) && !theirs.required.has(prop))
    if (newlyRequired.length === 0) continue
    stricter += 1
    console.log(`${name.padEnd(28)} required here, optional in v9: ${newlyRequired.join(', ')}`)
  }
  if (stricter === 0) console.log('nothing v9 leaves optional is required here')
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main()
