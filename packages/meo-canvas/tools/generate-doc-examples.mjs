// Lifts the fenced examples out of the doc comments into one compiled file.
//
// The Rust half runs its examples: `just docs` fails the build on a doctest
// naming a field that no longer exists. TypeScript compiles nothing inside a
// comment, so a `.ts` doc example is prose — it can name a removed property and
// every gate stays green. Renaming a style property leaves every example that
// used the old name compiling, because none of them is compiled at all.
//
// So the examples are lifted into `src/generated/doc-examples.ts`, which the
// existing `just typecheck` already covers because it covers `src`. That reuses
// a gate rather than adding one, and it is the same generated-and-diffed shape
// as the arena tables.
//
// Each example becomes a function, so one example's `const card` cannot collide
// with another's. Imports cannot live in a function, so they are hoisted and
// deduplicated, and `'meo-canvas'` is rewritten to the package's own entry —
// the examples name the package as a reader would, and the generated file sits
// inside it.

import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const SOURCE_DIR = resolve(HERE, '../src')
const CHECKED_IN = resolve(HERE, '../src/generated/doc-examples.ts')

// An explicit destination lets the drift check emit somewhere disposable and
// diff the result, rather than asking git what changed — git's answer depends
// on whether a file is untracked, written or staged.
const TARGET = process.argv[2] ? resolve(process.argv[2]) : CHECKED_IN

/** How an example spells this package when importing from it. */
const PACKAGE_SPECIFIER = 'meo-canvas'

/** Where the generated file must import from instead. */
const LOCAL_SPECIFIER = '../index.js'

/** Fails with a message naming the file and line the parse gave up on. */
function fail(where, message) {
  throw new Error(`${where}: ${message}`)
}

/** The `.ts` files whose comments carry examples. */
async function sources() {
  const entries = await readdir(SOURCE_DIR, { withFileTypes: true })
  return entries
    .filter(entry => entry.isFile() && entry.name.endsWith('.ts'))
    .filter(entry => !entry.name.endsWith('.test.ts'))
    .map(entry => join(SOURCE_DIR, entry.name))
    .sort()
}

/**
 * Every ```ts block in `text`, with its leading comment asterisks stripped.
 *
 * Line-based rather than one expression over the whole file: a doc comment's
 * every line begins with ` * `, and a regular expression that also had to
 * survive nested backticks in prose would be the harder thing to trust.
 */
function examples(path, text) {
  const found = []
  const lines = text.split('\n')

  let open = -1
  lines.forEach((line, index) => {
    const stripped = line.replace(/^\s*\* ?/, '')
    if (stripped.trim() !== '```ts' && stripped.trim() !== '```') return

    if (stripped.trim() === '```ts') {
      if (open !== -1) fail(path, `line ${index + 1}: a \`\`\`ts block opens inside another`)
      open = index
      return
    }
    if (open === -1) return

    const body = lines.slice(open + 1, index).map(inner => inner.replace(/^\s*\* ?/, ''))
    found.push({ anchor: anchorAfter(lines, index), body })
    open = -1
  })

  if (open !== -1) fail(path, `line ${open + 1}: a \`\`\`ts block is never closed`)
  return found
}

/** What a declaration looks like, close enough to take its name from. */
const DECLARATION = /^export (?:async )?(?:function|const|class|interface|type|enum) (\w+)/

/**
 * The name of the item a block documents, found by reading forward from it.
 *
 * The name rather than the block's line number, which would go stale whenever
 * anything above the comment moved and force a regeneration for a change to no
 * example. A line number is where a thing is; the name is what it is, and only
 * the second is what this gate checks.
 *
 * `undefined` for a block in a module-level comment, which documents no item.
 */
function anchorAfter(lines, closedAt) {
  // The comment first: a block sits inside one, and the item is what follows
  // its close. Reading on past that would walk through imports and other
  // declarations and attach a module-level example to whatever came first,
  // which is a name that says the wrong thing rather than no name.
  let index = closedAt
  while (index < lines.length && !lines[index].includes('*/')) index += 1
  index += 1

  while (index < lines.length && lines[index].trim() === '') index += 1
  return DECLARATION.exec(lines[index] ?? '')?.[1]
}

/** Splits an example into its import lines and everything else. */
function split(body) {
  const imports = []
  const rest = []
  for (const line of body) {
    if (/^import\s/.test(line)) imports.push(line)
    else rest.push(line)
  }
  return { imports, rest }
}

/**
 * One import per source, with the named bindings merged.
 *
 * Deduplicating whole lines is not enough: two examples importing `Text` and
 * `{ Column, Row, Text }` from the same module would emit both and TypeScript
 * would refuse the duplicate binding. The names are merged instead, so an
 * example importing what another already did costs nothing.
 */
function mergeImports(collected) {
  /** `source` -> `{ value: Set<string>, type: Set<string> }`. */
  const bySource = new Map()

  for (const example of collected) {
    for (const line of example.imports) {
      const rewritten = line.replaceAll(`'${PACKAGE_SPECIFIER}'`, `'${LOCAL_SPECIFIER}'`)
      const parsed = /^import\s+(type\s+)?\{([^}]*)\}\s+from\s+'([^']+)'/.exec(rewritten)
      if (!parsed) {
        fail(example.file, `${example.anchor ?? 'a module-level comment'}: this generator reads only \`import { .. } from '..'\`, not ${rewritten.trim()}`)
      }
      const [, isType, names, source] = parsed
      const entry = bySource.get(source) ?? { value: new Set(), type: new Set() }
      const target = isType ? entry.type : entry.value
      for (const name of names.split(',')) {
        const trimmed = name.trim()
        if (trimmed !== '') target.add(trimmed)
      }
      bySource.set(source, entry)
    }
  }

  const lines = []
  for (const [source, entry] of [...bySource].sort()) {
    if (entry.value.size > 0) {
      lines.push(`import { ${[...entry.value].sort().join(', ')} } from '${source}'`)
    }
    if (entry.type.size > 0) {
      lines.push(`import type { ${[...entry.type].sort().join(', ')} } from '${source}'`)
    }
  }
  return lines
}

/** The generated TypeScript. */
function emit(collected) {
  const imports = mergeImports(collected)

  // `async`, so an example may `await`. A caller's example is written the way a
  // caller writes it, and half of this package's surface returns a Promise.
  // Named and labelled by the item each example documents rather than by
  // position or line, so an edit elsewhere in the file does not move them. Two
  // blocks on one item are numbered against each other, which is the only
  // ordering left that a reader could be surprised by.
  const seen = new Map()
  const bodies = collected.map(example => {
    const base = example.anchor ?? example.file.replace(/\.ts$/, '').replaceAll(/[^A-Za-z0-9]/g, '_')
    const count = (seen.get(base) ?? 0) + 1
    seen.set(base, count)
    const name = `example_${base}${count > 1 ? `_${count}` : ''}`
    const documents = example.anchor === undefined ? example.file : `${example.file}, ${example.anchor}`
    const indented = example.rest.map(line => (line === '' ? '' : `  ${line}`))
    return [`/** \`${documents}\`. */`, `export async function ${name}(): Promise<void> {`, ...indented, '}', ''].join('\n')
  })

  return [
    '// Generated by `just doc-examples` from the fenced blocks in this',
    "// package's doc comments. Do not edit: `just ci` regenerates this file and",
    '// fails on a difference, so an edit here is a build failure rather than a',
    '// change.',
    '//',
    '// It exists so an example that does not compile fails a gate. TypeScript',
    '// compiles nothing inside a comment, so without this an example may name a',
    '// property that no longer exists and every check stays green.',
    '',
    ...imports,
    '',
    ...bodies,
  ].join('\n')
}

const collected = []
for (const path of await sources()) {
  const text = await readFile(path, 'utf8')
  const relative = path.slice(SOURCE_DIR.length + 1)
  for (const example of examples(path, text)) {
    const { imports, rest } = split(example.body)
    collected.push({ file: relative, anchor: example.anchor, imports, rest })
  }
}

if (collected.length === 0) {
  fail(SOURCE_DIR, 'no ```ts examples found; the generator would emit a file guarding nothing')
}

await mkdir(dirname(TARGET), { recursive: true })
await writeFile(TARGET, emit(collected), 'utf8')

process.stderr.write(`doc examples: ${collected.length}\n`)
