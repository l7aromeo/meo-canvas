// The Actions cache budget, per ref and per superseded key, since GitHub evicts
// silently and the only symptom is a cold build. It names what to remove and the
// command that removes it. Every call is bounded so the gate cannot hang, and a
// pass is one reading at one moment, not proof nothing was evicted since.
import { execFileSync } from 'node:child_process'

import { ORDER_READS, supersededOf } from './cache-entries.mjs'

const GIB = 1024 ** 3
const MIB = 1024 ** 2

/**
 * The size at which this asks for attention: the 10 GB limit less the largest
 * single entry, so the alarm fires while the next save still fits. The ubuntu set
 * is 2215 MiB, so 7.5 GiB leaves one largest entry plus a margin.
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

// **Counted, so an empty list cannot pass for a healthy one.** Zero entries is
// a real state -- a fresh repository, or every cache evicted -- and it is also
// what a wrong query returns. Under the floor either way, which is why the
// number is printed rather than only compared.
const total = entries.reduce((sum, entry) => sum + entry.size_in_bytes, 0)
const byRef = new Map()
for (const entry of entries) byRef.set(entry.ref, (byRef.get(entry.ref) ?? 0) + entry.size_in_bytes)

const refs = [...byRef].sort((a, b) => b[1] - a[1])
process.stdout.write(`cache budget: ${(total / GIB).toFixed(2)} GiB across ${entries.length} entries, floor ${(FLOOR_BYTES / GIB).toFixed(1)} GiB\n`)
for (const [ref, size] of refs) process.stdout.write(`  ${(size / GIB).toFixed(2).padStart(6)} GiB  ${ref}\n`)

// Superseded: same key prefix, read less recently than a sibling. The rule is
// `cache-entries.mjs`'s, imported so this report and `cache-prune.mjs` agree.
const superseded = supersededOf(entries)
const supersededBytes = superseded.reduce((sum, entry) => sum + entry.size_in_bytes, 0)
if (superseded.length > 0) {
  process.stdout.write(`  ${superseded.length} superseded, ${(supersededBytes / GIB).toFixed(2)} GiB -- same key prefix, ${ORDER_READS}:\n`)
  for (const entry of superseded)
    process.stdout.write(`    gh api -X DELETE repos/${repo}/actions/caches/${entry.id}  # ${(entry.size_in_bytes / MIB).toFixed(0)} MiB ${entry.key}\n`)
}

if (total > FLOOR_BYTES && beforeMain) {
  process.stdout.write(
    `cache budget: ${(total / GIB).toFixed(2)} GiB is over the ${(FLOOR_BYTES / GIB).toFixed(1)} GiB floor -- ` +
      `reported, not enforced, because this is a ${process.env['GITHUB_EVENT_NAME']} and the cache is a ` +
      'property of the repository rather than of this change. The next run on `main` fails on it.\n',
  )
  process.exit(0)
}

if (total > FLOOR_BYTES) {
  const largest = entries.reduce((big, entry) => (entry.size_in_bytes > big.size_in_bytes ? entry : big))
  const remainder = (total - supersededBytes) / GIB
  process.stderr.write(
    `\nThe cache is ${(total / GIB).toFixed(2)} GiB, past the ${(FLOOR_BYTES / GIB).toFixed(1)} GiB this asks about, and ` +
      `the largest single entry is ${(largest.size_in_bytes / MIB).toFixed(0)} MiB.\n\n` +
      (superseded.length > 0
        ? `Start with the ${superseded.length} superseded above: the commands are printed and removing them leaves ` +
          `${remainder.toFixed(2)} GiB, which is ${remainder > FLOOR_BYTES / GIB ? 'still over the floor -- so that is a start and not the fix' : 'under the floor'}.\n`
        : 'Nothing here is superseded, so there is no dead weight to remove.\n') +
      'Then read the per-ref lines: a ref that is not the default branch is a pull request or a tag whose caches ' +
      'outlive it and can never be restored by anything. If every ref is the default branch and nothing is ' +
      'superseded, the working set itself has grown -- look for a workflow saving into this budget that does not ' +
      'need to, before arguing about the floor.\n',
  )
  process.exit(1)
}
