// `ci-steps` runs exactly the two lists, neither list is empty, and CI runs both.
// CI runs `portable` and `native` as separate jobs and never `ci-steps`, so a recipe
// leaving a list makes the gate faster and still green. The justfile is read
// through `just --dump`, the resolver the gate itself runs under.
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..')

/**
 * How short each list may get: two below each length, and the only assertion that
 * sees a deletion. Not derived from the lists, which could not notice one; a firing
 * floor prints the length it measured, so the lengths are not restated here.
 */
const FLOORS = { portable: 13, native: 11 }

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

// One level of expansion, so `ci-steps` is checked whether it names the two groups
// or spells recipes out, where a lost recipe would pass a `portable native` check.
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

// Order where it carries meaning in `native`: `addon` before `test-js`, which loads
// the `.node`, and `test-js` before `test`, since `just` stops at the first failure.
// Asserted as a pair: moving `test-js` first satisfies one and breaks the other.
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

// Both halves must be invoked from a `run:` line: the workflow's prose names them
// too, so a search of the whole text could not fail. A job skipped by `if:`, or an
// `on:` narrowed so the workflow never runs, still passes.
for (const list of ['portable', 'native']) {
  const invoked = invocations.some(line => new RegExp(`(?:^|[\\s;&|])just\\s+${list}(?:[\\s;&|]|$)`).test(line))
  if (!invoked) problems.push(`\`.github/workflows/ci.yml\` never runs \`just ${list}\``)
}

// The platform-guarded recipes, named rather than counted since each is the only
// coverage of what it covers, and the platform each needs in the matrix. `LABEL`
// joins `runner.os`, which an `if:` spells, to the matrix's runner labels.
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

// Every guarded step that runs a recipe must be in the map, or the loop below
// would leave a new one unchecked. A guarded step running no recipe -- the libaom
// installs -- is exempt: dropping its platform drops the need for it too.
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
