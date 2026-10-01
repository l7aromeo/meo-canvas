// The Actions cache budget: the working set -- every entry less those superseded,
// which the prune after each `ci` on `main` deletes -- against a floor, and a failure
// for any superseded entry that prune should already have removed. Every call is
// bounded, and a pass is one reading at one moment.
import { execFileSync } from 'node:child_process'

import { ORDER_READS, budgetOf, verifyBudget } from './cache-entries.mjs'

const GIB = 1024 ** 3
const MIB = 1024 ** 2

/**
 * The working set at which this asks for attention: GitHub's 10 GB limit less the
 * largest entry when it was set, 2215 MiB for ubuntu. That entry is now 2712 MiB,
 * so the margin is narrower than this derivation assumed.
 */
const FLOOR_BYTES = 7.5 * GIB

/** `owner/repo`, from the environment in CI and from the remote otherwise. */
function repository() {
  const fromEnv = process.env['GITHUB_REPOSITORY']
  if (fromEnv !== undefined && fromEnv !== '') return fromEnv
  const url = execFileSync('git', ['remote', 'get-url', 'origin'], { encoding: 'utf8' }).trim()
  const match = /[:/]([^/:]+\/[^/]+?)(?:\.git)?$/.exec(url)
  if (match?.[1] === undefined) throw new Error(`cannot read owner/repo from ${url}`)
  return match[1]
}

/** How long any one call here may take before it is abandoned, in milliseconds. */
const DEADLINE_MS = 20_000

/** A token, from the environment in CI and from `gh` on a developer's machine. */
function token() {
  const fromEnv = process.env['GITHUB_TOKEN'] ?? process.env['GH_TOKEN']
  if (fromEnv !== undefined && fromEnv !== '') return fromEnv
  try {
    // `timeout` because this is a subprocess in the middle of the gate. Without
    // it a slow or wedged `gh` stops `just ci` for as long as it likes, and the
    // symptom is a recipe that prints nothing rather than an error anyone can
    // read.
    const fromGh = execFileSync('gh', ['auth', 'token'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
      timeout: DEADLINE_MS,
    }).trim()
    return fromGh === '' ? undefined : fromGh
  } catch {
    return undefined
  }
}

const inCi = process.env['GITHUB_ACTIONS'] === 'true'

verifyBudget()

// The total is a fact about the repository: it fails `main` and is reported on a
// change not yet there -- a pull request, `pull_request_target`, or a merge queue a
// failure would jam with no `main` run to prune it. The set is named, so an unknown
// event enforces; reporting prints the entries it would have failed on.
const REPORT_ONLY_EVENTS = new Set(['pull_request', 'pull_request_target', 'merge_group'])
const beforeMain = REPORT_ONLY_EVENTS.has(process.env['GITHUB_EVENT_NAME'] ?? '')
const auth = token()

// Without credentials this skips only off CI, out loud, naming what would make it
// run. CI always has a token with `actions: read`, so an absence there is a broken
// workflow.
if (auth === undefined) {
  if (inCi) {
    process.stderr.write(
      '\nNo token, and this is CI. The `portable` job grants `actions: read` and passes `GITHUB_TOKEN` to this ' +
        'step; if either was removed, this check cannot read the cache list and is not allowed to pass quietly.\n',
    )
    process.exit(1)
  }
  process.stdout.write('cache budget: not checked -- no GITHUB_TOKEN and `gh auth token` gave nothing. Run `gh auth login` to include it.\n')
  process.exit(0)
}

const repo = repository()
const entries = []
for (let page = 1; ; page += 1) {
  let response
  try {
    response = await fetch(`https://api.github.com/repos/${repo}/actions/caches?per_page=100&page=${page}`, {
      headers: { authorization: `Bearer ${auth}`, accept: 'application/vnd.github+json', 'x-github-api-version': '2022-11-28' },
      signal: AbortSignal.timeout(DEADLINE_MS),
    })
  } catch (cause) {
    // Unreachable fails in CI, where the runner already reaches this host and the
    // job's access is wrong; on a machine it warns, like the missing-token arm.
    const detail = cause instanceof Error ? cause.message : String(cause)
    if (inCi) {
      process.stderr.write(`\nCould not reach the cache list for ${repo} within ${DEADLINE_MS / 1000}s: ${detail}\n`)
      process.exit(1)
    }
    process.stdout.write(`cache budget: not checked -- could not reach the API within ${DEADLINE_MS / 1000}s (${detail}).\n`)
    process.exit(0)
  }
  if (!response.ok) {
    process.stderr.write(`\nGitHub answered ${response.status} for ${repo}'s cache list. With \`actions: read\` this call succeeds; without it, it does not.\n`)
    process.exit(1)
  }
  const body = await response.json()
  const page_entries = body.actions_caches ?? []
  entries.push(...page_entries)
  if (page_entries.length < 100) break
}

// The latest successful prune on `workflow_run`, the one event that deletes, so a
// dry-run dispatch cannot date the stale-entry guard. Unreachable is handled as for
// the cache list: a failure in CI, a warning on a machine.
let runs
try {
  runs = await fetch(`https://api.github.com/repos/${repo}/actions/workflows/cache-prune.yml/runs?status=success&event=workflow_run&per_page=1`, {
    headers: { authorization: `Bearer ${auth}`, accept: 'application/vnd.github+json', 'x-github-api-version': '2022-11-28' },
    signal: AbortSignal.timeout(DEADLINE_MS),
  })
} catch (cause) {
  const detail = cause instanceof Error ? cause.message : String(cause)
  if (inCi) {
    process.stderr.write(`\nCould not reach ${repo}'s cache-prune runs within ${DEADLINE_MS / 1000}s: ${detail}\n`)
    process.exit(1)
  }
  process.stdout.write(`cache budget: not checked -- could not reach the API within ${DEADLINE_MS / 1000}s (${detail}).\n`)
  process.exit(0)
}
if (!runs.ok) {
  process.stderr.write(`\nGitHub answered ${runs.status} listing ${repo}'s cache-prune runs. With \`actions: read\` this call succeeds.\n`)
  process.exit(1)
}
const pruned = (await runs.json()).workflow_runs?.[0]

// **Counted, so an empty list cannot pass for a healthy one.** Zero entries is a real
// state and also what a wrong query returns, so the numbers are printed, not only compared.
const budget = budgetOf(entries, { floor: FLOOR_BYTES, prunedAt: pruned?.run_started_at })
const gib = bytes => `${(bytes / GIB).toFixed(2)} GiB`
const byRef = new Map()
for (const entry of entries) byRef.set(entry.ref, (byRef.get(entry.ref) ?? 0) + entry.size_in_bytes)

const refs = [...byRef].sort((a, b) => b[1] - a[1])
process.stdout.write(
  `cache budget: working set ${gib(budget.working)} against a ${(FLOOR_BYTES / GIB).toFixed(1)} GiB floor; ` +
    `${gib(budget.pending)} superseded, pending prune; ${gib(budget.total)} across ${entries.length} entries\n`,
)
for (const [ref, size] of refs) process.stdout.write(`  ${(size / GIB).toFixed(2).padStart(6)} GiB  ${ref}\n`)

// Superseded: same key prefix, read less recently than a sibling. The rule is
// `cache-entries.mjs`'s, imported so this report and `cache-prune.mjs` agree.
if (budget.superseded.length > 0) {
  process.stdout.write(`  ${budget.superseded.length} superseded, ${gib(budget.pending)} -- same key prefix, ${ORDER_READS}:\n`)
  for (const entry of budget.superseded)
    process.stdout.write(`    gh api -X DELETE repos/${repo}/actions/caches/${entry.id}  # ${(entry.size_in_bytes / MIB).toFixed(0)} MiB ${entry.key}\n`)
}
process.stdout.write(
  pruned === undefined
    ? '  no successful cache-prune run found, so nothing is checked for a prune that missed an entry\n'
    : `  last successful prune: run ${pruned.id}, started ${pruned.run_started_at}\n`,
)

const problems = []
if (budget.stale.length > 0) {
  problems.push(
    `cache-prune run ${pruned.id} (${pruned.html_url}) started after the newer sibling of ` +
      `${budget.stale.map(entry => `${entry.id} ${entry.key}`).join(', ')} was saved, and the entry is still here. ` +
      'That prune listed both and should have deleted it: read its log for what it selected.',
  )
}
if (budget.over) {
  const largest = entries.reduce((big, entry) => (entry.size_in_bytes > big.size_in_bytes ? entry : big))
  problems.push(
    `The working set is ${gib(budget.working)}, past the ${(FLOOR_BYTES / GIB).toFixed(1)} GiB floor, and the largest ` +
      `single entry is ${(largest.size_in_bytes / MIB).toFixed(0)} MiB. Superseded entries are already excluded, so ` +
      'pruning will not bring it down. Read the per-ref lines: a ref that is not the default branch holds caches ' +
      'nothing can restore. If every ref is the default branch, the working set itself has grown -- look for a ' +
      'workflow saving into this budget that does not need to, before arguing about the floor.',
  )
}

if (problems.length > 0 && beforeMain) {
  process.stdout.write(
    `cache budget: ${problems.length} problem(s) -- reported, not enforced, because this is a ` +
      `${process.env['GITHUB_EVENT_NAME']} and the cache is a property of the repository rather than of this change. ` +
      `The next run on \`main\` fails on it.\n${problems.map(one => `  ${one}\n`).join('')}`,
  )
  process.exit(0)
}

if (problems.length > 0) {
  process.stderr.write(`\n${problems.join('\n\n')}\n`)
  process.exit(1)
}
