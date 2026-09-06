// Every conformance tool's write sits behind `WRITE`, checked statically.
//
// **The check runs where the thing it checks cannot.** `conformance` is
// deliberately outside `ci`: it drives a browser and produces a diff a person
// reads. So nothing in the gate ever executes those tools, and a tool added
// later could write a tracked fixture on any invocation with nothing to notice
// — unassigned rather than overlooked. This is static, so it has none of that
// constraint and can sit in `ci-steps` even though its subject cannot.
//
// **The bindings are resolved rather than a name being matched.** The first
// version compared the callee's text against `'writeFile'`, which is a grep
// with an AST in front of it: `writeFileSync`, `fs.writeFile`, `appendFile` and
// `import { writeFile as save }` all walk past it. A reviewer aliased the import
// and deleted the guard, and this file reported that every write was guarded.
// The miss landed on `writeFileSync` — the exact spelling an earlier survey of
// this directory grepped for and drew the opposite conclusion from — so the
// check was blind to the one shape someone here demonstrably reaches for.
//
// **And it counts what it found.** A check over a set it never matches passes
// for the same reason it passes when everything is correct, so the number of
// guarded writes is floored: rename every helper in the directory and this fails
// for finding nothing rather than succeeding for finding nothing wrong.
import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import ts from 'typescript'

const HERE = dirname(fileURLToPath(import.meta.url))
const TOOLS = join(HERE, 'conformance')

/** How many guarded writes the directory holds today. */
const FLOOR = 14

/** The `node:fs` exports that put bytes somewhere. */
const MUTATORS = new Set([
  'appendFile',
  'appendFileSync',
  'copyFile',
  'copyFileSync',
  'createWriteStream',
  'cp',
  'cpSync',
  'open',
  'openSync',
  'rename',
  'renameSync',
  'truncate',
  'truncateSync',
  'writeFile',
  'writeFileSync',
])

const FS_MODULES = new Set(['fs', 'node:fs', 'fs/promises', 'node:fs/promises'])

/**
 * The local names in one file that reach a mutating `node:fs` export.
 *
 * Returns the bare identifiers a `{ writeFile }` or `{ writeFile as save }`
 * import binds, and separately the namespaces a `* as fs` import binds, whose
 * member calls have to be matched by property instead.
 */
function mutatorsOf(source) {
  const direct = new Set()
  const namespaces = new Set()
  for (const statement of source.statements) {
    if (!ts.isImportDeclaration(statement)) continue
    if (!ts.isStringLiteral(statement.moduleSpecifier)) continue
    if (!FS_MODULES.has(statement.moduleSpecifier.text)) continue
    const bindings = statement.importClause?.namedBindings
    if (bindings === undefined) continue
    if (ts.isNamespaceImport(bindings)) {
      namespaces.add(bindings.name.text)
      continue
    }
    for (const element of bindings.elements) {
      const imported = (element.propertyName ?? element.name).text
      if (MUTATORS.has(imported)) direct.add(element.name.text)
    }
  }
  return { direct, namespaces }
}

/** Whether any enclosing `if` tests `WRITE`, which is what the recipe sets. */
function guarded(node) {
  for (let above = node.parent; above !== undefined; above = above.parent) {
    if (ts.isIfStatement(above) && above.expression.getText().includes('WRITE')) return true
  }
  return false
}

const unguarded = []
let guardedWrites = 0
let scanned = 0
for (const file of readdirSync(TOOLS).sort()) {
  if (!file.endsWith('.mjs')) continue
  scanned += 1
  const path = join(TOOLS, file)
  const source = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true)
  const { direct, namespaces } = mutatorsOf(source)
  const visit = node => {
    if (ts.isCallExpression(node)) {
      const callee = node.expression
      const writes = ts.isIdentifier(callee)
        ? direct.has(callee.text)
        : ts.isPropertyAccessExpression(callee) &&
          ts.isIdentifier(callee.expression) &&
          namespaces.has(callee.expression.text) &&
          MUTATORS.has(callee.name.text)
      if (writes) {
        if (guarded(node)) guardedWrites += 1
        else {
          const { line } = source.getLineAndCharacterOfPosition(node.getStart(source))
          unguarded.push(`${file}:${line + 1}`)
        }
      }
    }
    ts.forEachChild(node, visit)
  }
  visit(source)
}

if (unguarded.length > 0) {
  for (const one of unguarded) process.stdout.write(`  writes without a WRITE test: ${one}\n`)
  process.stderr.write(
    '\nA conformance tool writes a tracked fixture on any invocation, so reading what Chrome says ' +
      'and replacing what the gate compares against are the same act. Put the write inside ' +
      "`if (process.env['WRITE'] === '1')` and print the table otherwise, as the others do.\n",
  )
  process.exit(1)
}

if (guardedWrites < FLOOR) {
  process.stderr.write(
    `\nFound ${guardedWrites} guarded writes across ${scanned} files, and expected at least ${FLOOR}. ` +
      'Either a tool stopped writing, or this check stopped recognising how it writes -- and the ' +
      'second is what a green with nothing matched looks like. Raise or lower FLOOR deliberately.\n',
  )
  process.exit(1)
}

process.stdout.write(`${guardedWrites} conformance writes, every one behind WRITE, across ${scanned} files.\n`)
