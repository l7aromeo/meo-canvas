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
// **The frame is closed rather than the value chased.** A write factored into
// `tools/fixture-writer.mjs` and imported from a conformance tool leaves this
// scan entirely, and the floor cannot see it go: the fourteen guarded writes
// still count, so the floor holds *while a new unguarded path exists*. It
// protects against every write vanishing, not against one migrating out. And the
// migration is what the directory's own convention invites -- `browser.mjs` and
// `png.mjs` are already helpers, and `tools/` proper is where the other shared
// ones live, so a third would naturally be pulled up one level.
//
// So a conformance tool may import a sibling or a bare specifier and nothing
// else -- and *sibling* is decided by resolving the specifier rather than by
// reading its first characters, because `'./../x.mjs'` and `'../x.mjs'` reach
// the same file and only one of them looks like it does. Widening the scan to `tools/**` instead would drown it: `acceptance.mjs`
// and `stage-platform-package.mjs` write legitimately and have no `WRITE` to
// test. Following imports transitively would put a module resolver here. Closing
// the door needs neither.
//
// **What it does not do**, stated so nobody mistakes it for cover: it does not
// chase a local alias. `const save = writeFile` then `save(...)` is invisible
// here, and chasing it means chasing arbitrary assignment, which is a type
// checker's work. Someone who writes that line is working around the check, and
// a check cannot be built against its own author.
//
// **And it counts what it found.** A check over a set it never matches passes
// for the same reason it passes when everything is correct, so the number of
// guarded writes is floored: rename every helper in the directory and this fails
// for finding nothing rather than succeeding for finding nothing wrong.
import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
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

/** The module specifiers a file imports, static and dynamic. */
function importsOf(source) {
  const specifiers = []
  const visit = node => {
    if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier !== undefined && ts.isStringLiteral(node.moduleSpecifier)) {
      specifiers.push(node.moduleSpecifier.text)
    }
    if (
      ts.isCallExpression(node) &&
      node.expression.kind === ts.SyntaxKind.ImportKeyword &&
      node.arguments[0] !== undefined &&
      ts.isStringLiteral(node.arguments[0])
    ) {
      specifiers.push(node.arguments[0].text)
    }
    ts.forEachChild(node, visit)
  }
  visit(source)
  return specifiers
}

/** Whether any enclosing `if` tests `WRITE`, which is what the recipe sets. */
function guarded(node) {
  for (let above = node.parent; above !== undefined; above = above.parent) {
    if (ts.isIfStatement(above) && above.expression.getText().includes('WRITE')) return true
  }
  return false
}

const unguarded = []
const escaping = []
let guardedWrites = 0
let scanned = 0
for (const file of readdirSync(TOOLS).sort()) {
  if (!file.endsWith('.mjs')) continue
  scanned += 1
  const path = join(TOOLS, file)
  const source = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true)
  const { direct, namespaces } = mutatorsOf(source)
  for (const specifier of importsOf(source)) {
    // **Resolved, not spelled.** `startsWith('../')` is a test of how the path
    // was written, and `'./../fixture-writer.mjs'` is the same reach with one
    // more segment in front -- Node resolves it, and a check on the prefix
    // reports that nothing left the directory. The property is where the
    // specifier lands, so land it: anything relative that resolves outside
    // `TOOLS` is out, however it was typed.
    if (!specifier.startsWith('.')) continue
    const landed = relative(TOOLS, join(TOOLS, specifier))
    if (landed.startsWith('..')) escaping.push(`${file} imports ${specifier}`)
  }
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

if (escaping.length > 0) {
  for (const one of escaping) process.stdout.write(`  reaches outside the directory: ${one}\n`)
  process.stderr.write(
    '\nA conformance tool may import a sibling or a bare specifier and nothing else. A write ' +
      'factored into a module above this directory leaves the check entirely, and the guarded-write ' +
      'floor cannot see it go -- the writes that remain still satisfy it. Keep the helper beside its ' +
      'callers.\n',
  )
  process.exit(1)
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

process.stdout.write(`${guardedWrites} conformance writes, every one behind WRITE, across ${scanned} files that reach no further than a sibling.\n`)
