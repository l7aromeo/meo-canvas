// `ci-steps` runs exactly the two lists, and neither list is empty.
//
// **The thing this defends is a check that cannot fail, one level up from the
// code.** `ci-steps` used to name all twenty-three recipes; it now names
// `portable` and `native`, and CI runs those two halves in separate jobs rather
// than running `ci-steps` at all. So the composition is exercised nowhere but a
// developer's machine, and an edit that drops a recipe from one of the lists --
// or rewrites `ci-steps` to run one half -- makes the local gate faster, quieter
// and still green. **A gate that stops running ten recipes looks exactly like a
// fast gate.**
//
// **It reads the justfile through `just` rather than parsing it.** `just
// --dump --dump-format json` is the same resolver a recipe runs under, so this
// cannot disagree with what the gate actually does about what a dependency is,
// how a name resolves, or which attributes hide a recipe. A regular expression
// over the file would be a second implementation of `just`, and the interesting
// failures are the ones where the two implementations differ.
//
// **Three assertions, and each catches a different edit. Written down because
// the first one is weaker than it looks and would otherwise be read as cover:**
//
// - *Identity* fires when `ci-steps` stops running one of the halves, or is
//   spelled out as recipe names again and has lost one. It does **not** fire
//   when a recipe is deleted from `portable` or `native` while `ci-steps` still
//   reads `portable native`: the union and the expansion move together, so the
//   comparison is true before and after. Measured, not reasoned -- removing
//   `layout-check` from `portable` passes this check.
// - *Disjointness* fires when a recipe is added to the second list rather than
//   moved, which is what a hurried recategorisation looks like.
// - *The floors* are what actually catch a deletion, and only after three of
//   them. They also catch the case identity is blind to for the opposite reason:
//   all three lists empty satisfies identity exactly as a correct tree does.
//
// So the deletion of a single recipe from a list is **not** guarded here, and
// the honest reason is that guarding it needs an inventory -- every recipe in
// the justfile is in a list or deliberately outside the gate, and `audit`,
// `net-check`, `conformance` and the generators are all deliberately outside.
// That is a maintained exclusion list, which is a different check with a
// different cost, and it is not this one.
//
// **A fourth assertion, about the workflow rather than the justfile.** The three
// above stay true if the `portable` job is deleted from
// `.github/workflows/ci.yml` and the matrix left on `just native`: eleven
// recipes stop running in CI, this check among them, and nothing goes red. That
// is a two-line edit a reviewer reads as tidying. So: **both `just portable` and
// `just native` must be invoked somewhere in that workflow.**
//
// `ci.yml` is in the tree and the gate checks the tree, which is the same
// argument that put a check on prose and a check on a directory's shape in it.
//
// **It reads `run:` lines and ignores comments, and that is not fussiness.** The
// workflow's own prose names `just portable` and `just native` while explaining
// the split, so a substring search over the file would pass with both `run:`
// lines deleted -- a check that cannot fail, in the file added to stop one.
// Measured: deleting the `- run: just portable` line and keeping the paragraph
// above it fails this, and passed the version that searched the whole text.
//
// **What it does not catch, and these are real:** a job that invokes both and
// then skips itself with an `if:`; `on:` narrowed so the workflow stops
// triggering. Both leave the invocation in place, which is all that assertion
// looks at. Deleting the workflow outright is caught, but by the read failing
// rather than by an assertion.
//
// **A fifth assertion, for the one of those that turned out to be reachable in
// two lines.** Three recipes run on exactly one platform each, each guarded by
// an `if: runner.os` in `ci.yml`:
//
//     just audit           Linux     the only thing that reads the advisory database
//     just net-check       Linux     the only thing that compiles the `net` feature
//     just threads-probe   Windows   the only thing that loads the addon under workers
//
// **Drop a platform from the matrix and its guarded step stops running with
// nothing red** -- the step is still in the file, the job is still green, and the
// run is faster, which reads as an improvement. That is the same defect the
// fourth assertion exists for, one level down: there the job was deleted, here
// the platform it needed is.
//
// **So this asserts the coupling rather than either side.** Checking only that
// the recipes are invoked passes the matrix edit, because the `run:` line is
// untouched; checking only that the matrix holds three platforms passes the
// deletion of a step. What has to hold is that **each guarded recipe's platform
// is in the matrix** -- and the guards are read out of the workflow rather than
// written down here, so a fourth guarded step is covered by existing.
//
// **The three are named rather than counted.** `FLOORS` is right for the lists,
// where the recipes are interchangeable in kind and the harm is attrition; it is
// wrong here, where each of the three is the only thing that covers what it
// covers. A floor of three would pass a tree that had swapped `net-check` for a
// second Linux step.
//
// **And the list has to be complete, which is a second assertion rather than a
// property of the first.** Reading each guard's platform out of the workflow is
// not the same as noticing a guard the map has never heard of: the loop is over
// the map, so a fourth guarded step added tomorrow would be exactly as droppable
// as these three and this would stay green. **That is the defect being fixed,
// wearing the check's own clothes.** So every `if: runner.os` step in the
// workflow that runs a `just` recipe must have an entry here, and a new one
// fails until someone adds it.
//
// **A guarded step that runs no `just` recipe is exempt by construction**, which
// is the two libaom installs: they are setup for the platform they run on rather
// than coverage that platform is the only source of, so dropping the platform
// drops the need for them at the same time. Written as a property rather than a
// list of exempt names, because a list would have to be maintained by the same
// person who forgot to add the entry.
//
// **Past this it is a YAML validator, which is a different tool and should be
// one.** The line is that this reads what a step says about itself -- its `run:`
// and its `if:` -- and never what GitHub would do with the file.
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..')

/**
 * How short each list may get before this asks whether it was meant.
 *
 * Two below today's thirteen and thirteen: removing a recipe deliberately
 * should not require editing a number here, and a list emptied by an edit must
 * not pass. This is the only assertion that sees a deletion at all, which is
 * why it is close rather than generous.
 */
const FLOORS = { portable: 11, native: 11 }

const recipes = JSON.parse(execFileSync('just', ['--dump', '--dump-format', 'json'], { cwd: ROOT, encoding: 'utf8' })).recipes

/** The recipes one list names, in order, or `undefined` if the list is gone. */
const namesOf = list => recipes[list]?.dependencies.map(one => one.recipe)

const lists = {}
for (const list of ['ci-steps', 'portable', 'native']) {
  const names = namesOf(list)
  if (names === undefined) {
    process.stderr.write(
      `\nThe justfile has no \`${list}\` recipe. The gate is composed of \`ci-steps: portable native\`, and this ` +
        'check is written against that shape -- if the shape changed deliberately, change this with it rather ' +
        'than deleting it.\n',
    )
    process.exit(1)
  }
  lists[list] = names
}

// **One level of expansion, so both spellings are checked.** Today `ci-steps`
// names the two groups, and substituting their contents is the identity. The
// check exists for the day it names recipes directly again: an explicit list
// that has quietly lost one is the failure, and it is invisible to a comparison
// that only asks whether `ci-steps` still says `portable native`.
const runs = lists['ci-steps'].flatMap(name => (name === 'portable' || name === 'native' ? lists[name] : [name]))
const union = [...lists['portable'], ...lists['native']]

const problems = []

const repeated = union.filter((name, at) => union.indexOf(name) !== at)
if (repeated.length > 0) problems.push(`both lists name: ${[...new Set(repeated)].sort().join(', ')}`)

const missing = union.filter(name => !runs.includes(name))
if (missing.length > 0) problems.push(`in a list but not run by ci-steps: ${missing.sort().join(', ')}`)

const extra = runs.filter(name => !union.includes(name))
if (extra.length > 0) problems.push(`run by ci-steps but in neither list: ${extra.sort().join(', ')}`)

for (const [list, floor] of Object.entries(FLOORS)) {
  if (lists[list].length < floor) problems.push(`\`${list}\` has ${lists[list].length} recipes and the floor is ${floor}`)
}

// **Order, for the two places in `native` where it carries meaning.**
// Everything else in these lists is independent and the order is taste; these
// two are not, and until now nothing would have noticed either being undone.
//
// `test-js` before `test`: `just` stops at the first failure, so with `test`
// first a red Rust suite means the JavaScript suite never runs and a reader
// sees one surface and no information about the other. `addon` before
// `test-js`: `test-js` loads the compiled `.node`.
//
// Asserted as a pair because they pull opposite ways — satisfy the first by
// moving `test-js` to the front and the second breaks — so a future edit is
// told both constraints rather than discovering the second by failing it.
const ORDERED = [
  ['addon', 'test-js', '`test-js` loads the compiled addon'],
  ['test-js', 'test', 'a red Rust suite must not stop the JavaScript suite from reporting'],
]
for (const [first, second, why] of ORDERED) {
  const at = lists['native'].indexOf(first)
  const then = lists['native'].indexOf(second)
  if (at === -1 || then === -1) {
    problems.push(`\`native\` names neither or only one of \`${first}\` and \`${second}\`, so their order cannot be checked`)
    continue
  }
  if (at > then) problems.push(`\`native\` runs \`${second}\` before \`${first}\`: ${why}`)
}

// The workflow's `run:` lines, with comments dropped. A YAML parser is not on
// hand and would be more than this needs: the question is only whether a command
// is invoked, and the shapes it can take here are `- run: just x` and a `run: |`
// block with the command on its own line.
const WORKFLOW = join(ROOT, '.github', 'workflows', 'ci.yml')
const invocations = readFileSync(WORKFLOW, 'utf8')
  .split('\n')
  .filter(line => !line.trimStart().startsWith('#'))
  .map(line =>
    line
      .trim()
      .replace(/^-\s*/, '')
      .replace(/^run:\s*/, ''),
  )

for (const list of ['portable', 'native']) {
  const invoked = invocations.some(line => new RegExp(`(?:^|[\\s;&|])just\\s+${list}(?:[\\s;&|]|$)`).test(line))
  if (!invoked) problems.push(`\`.github/workflows/ci.yml\` never runs \`just ${list}\``)
}

// The platform-guarded recipes, and the platform each one needs in the matrix.
//
// `runner.os` is what a step's `if:` spells; the matrix spells runner labels. The
// two vocabularies are joined here rather than in either file, which is the only
// place that knows both.
const GUARDED = { audit: 'Linux', 'net-check': 'Linux', 'threads-probe': 'Windows' }
const LABEL = { Linux: 'ubuntu', Windows: 'windows', macOS: 'macos' }

const workflowLines = readFileSync(WORKFLOW, 'utf8')
  .split('\n')
  .filter(line => !line.trimStart().startsWith('#'))

// The matrix's `os:` list, as labels. One line, read as text for the same reason
// the invocations are: the question is which platforms are named, not what YAML
// means by them.
const matrixLine = workflowLines.find(line => /^\s*os:\s*\[/.test(line)) ?? ''
const platforms = [...matrixLine.matchAll(/[a-z]+(?=-latest|-[0-9])/g)].map(found => found[0])

// **Every guarded step that runs a recipe is in the map.** Without this the loop
// below only knows the three recipes written above, and a fourth guarded step
// would be unprotected while the check reported success.
for (const [index, line] of workflowLines.entries()) {
  if (!/if:\s*runner\.os\s*==/.test(line)) continue
  // The step's own `run:` lines: from the guard to the start of the next step.
  const end = workflowLines.findIndex((later, at) => at > index && /^\s{6}- /.test(later))
  const body = workflowLines.slice(index, end < 0 ? undefined : end)
  for (const command of body) {
    const invoked = /(?:^|run:\s*|\s)just\s+([a-z][a-z0-9-]*)/.exec(command.trim())
    if (invoked && !(invoked[1] in GUARDED)) {
      problems.push(
        `\`just ${invoked[1]}\` is guarded by \`runner.os\` in \`.github/workflows/ci.yml\` and is not in ` +
          `this tool's list, so nothing checks that its platform is in the matrix`,
      )
    }
  }
}

for (const [recipe, os] of Object.entries(GUARDED)) {
  // **The command, in either shape it takes here**: `run: just x`, or its own
  // line inside a `run: |` block. Written for both because `audit` is the second
  // and a version that matched only the first reported it missing -- a false
  // positive that would have been read as the check working.
  const at = workflowLines.findIndex(line => new RegExp(`(?:^|run:\\s*)\\s*just\\s+${recipe}(?:[\\s;&|]|$)`).test(line.trim()))
  if (at < 0) {
    problems.push(`\`.github/workflows/ci.yml\` never runs \`just ${recipe}\``)
    continue
  }
  // **The step's own guard, found by walking back to the step it belongs to.**
  // A fixed window would read a neighbour's `if:` when the command sits deep in
  // a block, and would miss its own when the step is long. `- ` at that
  // indentation starts a step, so it is where the search stops.
  let guard
  for (let line = at; line >= 0; line -= 1) {
    const text = workflowLines[line]
    if (/if:\s*runner\.os\s*==/.test(text)) {
      guard = text
      break
    }
    if (/^\s{6}- /.test(text) && line !== at) break
  }
  // A step that carries no guard runs everywhere, which is not this assertion's
  // business.
  if (guard === undefined) continue
  const needs = /runner\.os\s*==\s*'([A-Za-z]+)'/.exec(guard)?.[1] ?? os
  if (!platforms.includes(LABEL[needs] ?? needs)) {
    problems.push(`\`just ${recipe}\` runs only on ${needs}, and the matrix does not name it: it would stop ` + `running with nothing red`)
  }
}

if (problems.length > 0) {
  for (const one of problems) process.stdout.write(`  ${one}\n`)
  process.stderr.write(
    '\n`ci-steps` must run exactly the recipes `portable` and `native` name between them, and neither list may ' +
      'go empty. CI runs the two halves as separate jobs and never runs `ci-steps`, so nothing else notices a ' +
      'recipe leaving: the gate gets faster and stays green. Put the recipe back, or move it between the lists ' +
      'by the test written beside them -- does it, or anything it depends on, name `cargo`?\n\n' +
      'And both halves must be invoked in `.github/workflows/ci.yml`. Deleting a job there leaves every other ' +
      'assertion above it true while its recipes stop running in CI -- including this check, which is in ' +
      '`portable`.\n\n' +
      'The platform-guarded recipes are the same failure one level down: `audit`, `net-check` and ' +
      '`threads-probe` each run on one platform, so removing that platform from the matrix stops the step ' +
      'without failing anything. Put the platform back, or move the recipe to one the matrix still has.\n',
  )
  process.exit(1)
}

process.stdout.write(
  `ci-steps runs all ${runs.length} recipes the two lists name: ${lists['portable'].length} portable, ${lists['native'].length} native, ` +
    'no overlap, the workflow runs both halves, and every platform-guarded recipe has its platform in the ' +
    'matrix.\n',
)
