// Asserts the set of module-private declarations with no doc comment against the
// names in `private-docs-baseline.txt`. One that loses its doc fails, so a doc moved
// from one declaration to another fails too; one that gains a doc updates the
// baseline. Only top-level statements of non-test sources are read.
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import ts from 'typescript'

const HERE = dirname(fileURLToPath(import.meta.url))
const SOURCE = join(HERE, '..', 'src')
const BASELINE = join(HERE, 'private-docs-baseline.txt')

/** The entries a baseline file holds: one `file name` per line, `#` for a note. */
function entriesOf(text) {
  return text
    .split('\n')
    .map(line => line.trim())
    .filter(line => line !== '' && !line.startsWith('#'))
}

/** Whether a statement is exported, and so already the other tool's business. */
function exported(statement) {
  return (ts.getCombinedModifierFlags(statement) & ts.ModifierFlags.Export) !== 0
}

/** The names a top-level statement declares, or none if it declares nothing. */
function declared(statement) {
  if (ts.isVariableStatement(statement)) {
    return exported(statement.declarationList.declarations[0]) ? [] : statement.declarationList.declarations.map(one => one.name.getText())
  }
  const named =
    ts.isFunctionDeclaration(statement) ||
    ts.isClassDeclaration(statement) ||
    ts.isInterfaceDeclaration(statement) ||
    ts.isTypeAliasDeclaration(statement) ||
    ts.isEnumDeclaration(statement)
  if (!named || exported(statement)) return []
  return statement.name === undefined ? [] : [statement.name.getText()]
}

const undocumented = []
for (const file of readdirSync(SOURCE).sort()) {
  if (!file.endsWith('.ts') || file.endsWith('.test.ts') || file.endsWith('.d.ts')) continue
  const path = join(SOURCE, file)
  const source = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true)
  for (const statement of source.statements) {
    const names = declared(statement)
    if (names.length === 0) continue
    // `getLeadingCommentRanges` sees every comment attached to the statement,
    // which is what an orphaned doc stops being: the insertion takes the
    // position, and the comment above it belongs to the insertion instead.
    const comments = ts.getLeadingCommentRanges(source.text, statement.pos) ?? []
    const documented = comments.some(range => source.text.slice(range.pos, range.end).startsWith('/**'))
    if (!documented) {
      for (const name of names) undocumented.push(`${file} ${name}`)
    }
  }
}

const found = [...new Set(undocumented)].sort()

// **A missing baseline is written rather than treated as empty.** Empty would
// report every existing exception as a regression on a fresh clone, which is
// the shape of a check people delete.
if (!existsSync(BASELINE)) {
  writeFileSync(BASELINE, `${found.join('\n')}\n`)
  process.stdout.write(`No baseline; wrote ${found.length} existing exceptions to the baseline file.\n`)
  process.exit(0)
}

const allowed = new Set(entriesOf(readFileSync(BASELINE, 'utf8')))

const entered = found.filter(one => !allowed.has(one))
const left = [...allowed].filter(one => !found.includes(one)).sort()

process.stdout.write(`Module-private declarations without a doc comment: ${found.length}.\n`)

if (entered.length > 0) {
  for (const one of entered) process.stdout.write(`  lost its doc comment: ${one}\n`)
  for (const one of left) process.stdout.write(`  gained one: ${one}\n`)
  process.stderr.write(
    `\n${entered.length} declaration${entered.length === 1 ? '' : 's'} carried a doc comment and no longer ${entered.length === 1 ? 'does' : 'do'}. ` +
      'A doc separated from what it describes reads as a regression here, because the declaration below it ' +
      'is the one that lost its documentation. If the change is deliberate, edit ' +
      'tools/private-docs-baseline.txt.\n',
  )
  process.exit(1)
}

if (left.length > 0) {
  writeFileSync(BASELINE, `${found.join('\n')}\n`)
  process.stdout.write(`${left.length} now documented -- commit tools/private-docs-baseline.txt with the change:\n` + left.map(one => `  ${one}\n`).join(''))
}
