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
// **And it says nothing about the workflow.** Deleting the `portable` job from
// `.github/workflows/ci.yml` leaves every assertion here true and stops those
// recipes running in CI entirely -- and this check is itself in `portable`, so
// it would be among the ones that stopped.
import { execFileSync } from 'node:child_process'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..')

/**
 * How short each list may get before this asks whether it was meant.
 *
 * Two below today's eleven and thirteen: removing a recipe deliberately should
 * not require editing a number here, and a list emptied by an edit must not
 * pass. This is the only assertion that sees a deletion at all, which is why it
 * is close rather than generous.
 */
const FLOORS = { portable: 9, native: 11 }

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

if (problems.length > 0) {
  for (const one of problems) process.stdout.write(`  ${one}\n`)
  process.stderr.write(
    '\n`ci-steps` must run exactly the recipes `portable` and `native` name between them, and neither list may ' +
      'go empty. CI runs the two halves as separate jobs and never runs `ci-steps`, so nothing else notices a ' +
      'recipe leaving: the gate gets faster and stays green. Put the recipe back, or move it between the lists ' +
      'by the test written beside them -- does it, or anything it depends on, name `cargo`?\n',
  )
  process.exit(1)
}

process.stdout.write(
  `ci-steps runs all ${runs.length} recipes the two lists name: ${lists['portable'].length} portable, ${lists['native'].length} native, no overlap.\n`,
)
