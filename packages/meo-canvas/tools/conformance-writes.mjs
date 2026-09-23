// Every conformance tool's write sits behind `WRITE`, checked statically, so it can
// run in `ci-steps` though the tools themselves drive a browser and never do.
// Imports are resolved to bindings rather than matched by name; a tool may import
// only a sibling or a bare specifier; a local alias of a writer is not chased.
import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

import ts from 'typescript'

const HERE = dirname(fileURLToPath(import.meta.url))
const TOOLS = join(HERE, 'conformance')

/**
 * How many guarded writes the conformance directory holds, compared as an
 * equality: a bound cannot see a tool added without winding it, and still a scan
 * matching nothing gives 0, which fails too.
 */
const GUARDED_WRITES = 24

/**
 * How many stamped `.tsv` tables the assets directory holds, as an equality for
 * the same reason. Equal to [`GUARDED_WRITES`] by coincidence: the two diverge
 * once a tool writes something that is not a table.
 */
const STAMPED_TABLES = 24

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
 * The local names in one file that reach a mutating `node:fs` export: the bare
 * identifiers a named import binds, and separately the `* as fs` namespaces,
 * whose member calls are matched by property.
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
    // Resolved, not spelled: `'./../fixture-writer.mjs'` leaves the directory as
    // surely as `'../fixture-writer.mjs'`, so anything relative that resolves
    // outside `TOOLS` is out.
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

// At zero no number is offered: the measured count is `0`, and winding the
// constant to it would make a check that matches nothing permanently green. A
// directory still holding tool files has not had every tool stop writing.
if (guardedWrites === 0 && scanned > 0) {
  process.stderr.write(
    `\nFound no guarded writes at all across ${scanned} files. Every tool would have had to stop writing at ` +
      'once, so the live cause is that this check no longer recognises how a tool writes. No number is offered ' +
      'here on purpose: winding GUARDED_WRITES to nought is what a dead check looks like from the outside.\n',
  )
  process.exit(1)
}

if (guardedWrites > GUARDED_WRITES) {
  process.stderr.write(
    `\nFound ${guardedWrites} guarded writes across ${scanned} files where GUARDED_WRITES says ${GUARDED_WRITES}. ` +
      `A tool was added and the constant was not wound: write ${guardedWrites}.\n`,
  )
  process.exit(1)
}

if (guardedWrites < GUARDED_WRITES) {
  process.stderr.write(
    `\nFound ${guardedWrites} guarded writes across ${scanned} files where GUARDED_WRITES says ${GUARDED_WRITES}. ` +
      'Either a tool stopped writing, or this check stopped recognising how it writes -- and the second is what a ' +
      `green with nothing matched looks like. Write ${guardedWrites} only once you know which.\n`,
  )
  process.exit(1)
}

// Every recorded table says which browser produced it. CI launches no browser,
// so the suite checks against a recording, and a row disagreeing with an older
// one is a renderer change or a browser change only the stamp can tell apart.
// `ensure-browser` proves a browser exists; nothing else sees a stale one.
const TABLES = join(HERE, '..', '..', '..', 'crates', 'meo-canvas', 'tests', 'assets', 'chrome')
// A table measured on several engines passes on one line: only Chrome's version
// in `ratio-stretch-main.tsv` has four parts. Widening the pattern would admit
// any dotted pair in a note column.
const STAMP = /^#.*\b\d+\.\d+\.\d+\.\d+\b/
const unstamped = []
let stamped = 0
let tables = 0
for (const file of readdirSync(TABLES).sort()) {
  if (!file.endsWith('.tsv')) continue
  tables += 1
  const head = readFileSync(join(TABLES, file), 'utf8').split('\n').slice(0, 6)
  if (head.some(line => STAMP.test(line))) stamped += 1
  else unstamped.push(file)
}

if (unstamped.length > 0) {
  for (const one of unstamped) process.stdout.write(`  no browser version in the first six lines: ${one}\n`)
  process.stderr.write(
    '\nA recorded conformance table has to say which browser produced it, in a comment among its first six ' +
      'lines, as a four-part version. Without it a row that disagrees with an older row could be a renderer ' +
      'change or a browser change and nobody can tell which. `just conformance` writes these; run it and record ' +
      'the version it used.\n',
  )
  process.exit(1)
}

// Zero withheld for the reason above. A stamp this no longer recognises lands
// every table in the unstamped list first, so what reaches here is no `.tsv` at
// all: a path that stopped resolving, or a directory that moved.
if (stamped === 0) {
  process.stderr.write(
    `\nFound no stamped tables, from ${tables} \`.tsv\` files in ${TABLES}. A stamp this no longer recognises ` +
      'is caught above, by every table landing in the unstamped list -- so what is left is that there are no ' +
      'tables to read, and the directory is the thing to look at. No number is offered here on purpose: winding ' +
      'STAMPED_TABLES to nought is what a dead check looks like from the outside.\n',
  )
  process.exit(1)
}

if (stamped > STAMPED_TABLES) {
  process.stderr.write(
    `\nFound ${stamped} stamped tables where STAMPED_TABLES says ${STAMPED_TABLES}. A table was added and the ` + `constant was not wound: write ${stamped}.\n`,
  )
  process.exit(1)
}

if (stamped < STAMPED_TABLES) {
  process.stderr.write(
    `\nFound ${stamped} stamped tables where STAMPED_TABLES says ${STAMPED_TABLES}. Either tables were removed, or ` +
      'this check stopped recognising the stamp -- and the second is what a green with nothing matched looks like. ' +
      `Write ${stamped} only once you know which.\n`,
  )
  process.exit(1)
}

process.stdout.write(`${guardedWrites} conformance writes, every one behind WRITE, across ${scanned} files that reach no further than a sibling.\n`)
process.stdout.write(`${stamped} recorded tables, every one naming the browser that produced it.\n`)
