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

/**
 * How many guarded writes the conformance directory holds.
 *
 * **Compared as an equality, and that is the whole of what this constant is
 * for.** A bound cannot see an addition: a tool added without winding the
 * number leaves a green check and a wrong number, and the number is what the
 * check exists to state. It drifted twice that way -- wound to 22 by a commit
 * whose body said twenty-one, and to 20 by one where the tree already held 20.
 * Neither was a forgotten wind. Both were a wrong one, and a floor is silent
 * about both.
 *
 * **Equality is not extra work.** A commit that adds a tool or a table winds
 * one of these numbers as part of adding it; winding is the ritual already.
 * What equality adds is a failure that names the number to write, rather than
 * a new obligation.
 *
 * **It still catches the scan matching nothing**, which is what the bound was
 * for and the only thing it caught: emptying `MUTATORS` so no writer is
 * recognised gives 0, and 0 is not equal to this either.
 *
 * No history here. The block this replaces listed which tool joined when, went
 * two winds stale, and read as coherent while the value was wrong -- which is
 * what makes a stale narration worse than a bare number. git records when a
 * tool arrived; prose does not maintain it.
 */
const GUARDED_WRITES = 22

/**
 * How many stamped `.tsv` tables the assets directory holds.
 *
 * Equality for the reason above, and this one had the sharper version of the
 * argument already written against it: stamping one table alone would leave
 * this a table short and still pass, because the comparison was a bound. The
 * same sentence applies to the constant lagging the directory, which is what
 * happened.
 *
 * **Not the same quantity as [`GUARDED_WRITES`]**, and equal to it by
 * coincidence rather than construction: this counts stamped tables, that counts
 * guarded writes across the whole tool directory. They diverge the first time a
 * tool writes something that is not a table, or a table is produced by a tool
 * that already existed.
 */
const STAMPED_TABLES = 22

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

// **The two directions do not take the same advice, which is why they are two
// branches.** Above the constant, the measured number is the answer and writing
// it is the whole repair. Below it, the measured number may be the symptom --
// writing `0` because the scan matched nothing would make a broken check permanent
// and green.
// **At zero the advice is withheld, because following it kills the check.**
// The message below hands over the measured number, and at zero that number is
// `0` -- a constant of zero passes against a scan that matches nothing, which
// is a permanently green check with a success line saying every one of no
// writes is guarded. A conditional clause asking the reader not to is not an
// obstacle. So the number is named where either cause is live, and not here:
// a directory still holding tool files has not had every tool stop writing.
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

// **Every recorded table says which browser produced it.**
//
// CI launches no browser: the tables are measured by hand and committed, so the
// suite checks this renderer against a *recording* of Chrome. That is
// deliberate. What was missing is which Chrome — earlier tables came from
// whatever was installed whenever someone last ran `just conformance`, so a row
// disagreeing with an older row could be a renderer change or a browser change
// and nothing said which.
//
// `ensure-browser` proves the executable exists; its own comment records that an
// earlier version exited 0 whether or not anything was there. What neither it
// nor anything else can see is a browser that exists and is stale.
//
// The stamp is a measurement, not a back-fill: `just conformance` was run end to
// end on 153.0.8010.12 and no committed table moved, so every number here is
// reproduced by that build.
//
// **The two `.json` tables are not stamped and that is a gap, not a decision.**
// JSON carries no comment, and `chrome_tables.rs` reads them with a `read_rows`
// that expects a bare array — stamping them means changing that parser, which is
// a Rust file and another lane. Named here rather than left for someone to
// notice the check only ever looked at fourteen of sixteen.
const TABLES = join(HERE, '..', '..', '..', 'crates', 'meo-canvas', 'tests', 'assets', 'chrome')
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

// **Zero withheld for the reason above, and the cause named is the one that is
// live.** A stamp this no longer recognises is caught before here, by the
// unstamped list -- every table lands in it and that guard fires first, which
// was measured by replacing the pattern. What reaches this is no `.tsv` in the
// directory at all: a path that stopped resolving, or a directory that moved.
// Naming the recogniser here would send a reader to look at a pattern that is
// working.
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
