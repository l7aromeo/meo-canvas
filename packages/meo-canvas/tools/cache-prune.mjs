// Deletes the Actions cache entries nothing will restore.
//
// **The budget check says what is dead and stops there**, so every push to
// `main` that moves `Cargo.lock` mints three new keys at 1.2-2.2 GiB and the
// superseded ones stay until a person removes them. Three merges on 2026-09-10
// produced nine entries and 8.11 GiB against a 7.5 GiB floor, and the floor was
// crossed twice in one evening with a human deleting by hand both times.
//
// GitHub evicts at its own limit and never says so. Our floor sits below it on
// purpose, so the gate goes red before GitHub acts -- which is the right order
// and is why the remedy has to be ours.
//
// # Dry run is the default, and `--delete` is the whole opt-in
//
// A wrong deletion costs an hour of cold Skia build on Windows. So this prints
// what it would remove and exits; nothing is deleted without the flag, and the
// flag appears in exactly one place, the workflow that carries
// `actions: write`.
//
// # It refuses to run when its own rule fails
//
// `verifySelection` replays the night this rule was established -- two
// platforms whose creation order and access order disagree -- and throws if the
// selection is not the one that was correct then. **That check runs before the
// network call, not after**, so a broken rule cannot reach the API at all.
//
// # Why the fetch here is not the budget check's fetch
//
// `cache-budget.mjs` skips quietly without a token on a developer's machine and
// fails in CI: right for a report, wrong for this. A pruner that skipped would
// be a pruner that never prunes and nobody would notice. So this refuses
// without credentials wherever it runs, and the paging loop is short enough
// that saying so costs less than sharing a function whose behaviour must
// differ. The part that must not be duplicated -- which entries are dead -- is
// imported.
import { execFileSync } from 'node:child_process'
import { appendFileSync } from 'node:fs'

import { ORDER_BY, ORDER_READS, modeFrom, prefixOf, supersededOf, verifyMode, verifySelection } from './cache-entries.mjs'

const MIB = 1024 ** 2
const GIB = 1024 ** 3
const DEADLINE_MS = 20_000

// **One decision, from strings, in a language whose semantics run here.** The
// workflow passes `EVENT` and `DELETE_INPUT` through and does not decide; the
// mode below is what both the flag and every line printed about it come from,
// so a name that disagrees with the action is not expressible. Twice a workflow
// expression got this wrong in opposite directions -- see `modeFrom`.
const mode = modeFrom({ argv: process.argv, env: process.env })
const remove = mode === 'delete'

/** `owner/repo`, from the environment in CI and from the remote otherwise. */
const repository = () => {
  const fromEnv = process.env['GITHUB_REPOSITORY']
  if (fromEnv !== undefined && fromEnv !== '') return fromEnv
  const url = execFileSync('git', ['remote', 'get-url', 'origin'], { encoding: 'utf8' }).trim()
  const match = /[:/]([^/:]+\/[^/]+?)(?:\.git)?$/.exec(url)
  if (match?.[1] === undefined) throw new Error(`cannot read owner/repo from ${url}`)
  return match[1]
}

const token = () => {
  const fromEnv = process.env['GITHUB_TOKEN'] ?? process.env['GH_TOKEN']
  if (fromEnv !== undefined && fromEnv !== '') return fromEnv
  try {
    const fromGh = execFileSync('gh', ['auth', 'token'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], timeout: DEADLINE_MS }).trim()
    return fromGh === '' ? undefined : fromGh
  } catch {
    return undefined
  }
}

// Before anything else, and before the network.
verifyMode()
verifySelection()

// **The mode, first, and into the run's own summary.** A record of what this
// run decided, written by the run, rather than a prediction made from the
// inputs before it started -- which is what the two wrong expressions were.
process.stdout.write(`mode: ${mode}${remove ? ' -- entries will be deleted' : ' -- nothing will be deleted'}\n`)
if (process.env['GITHUB_STEP_SUMMARY'] !== undefined) {
  appendFileSync(process.env['GITHUB_STEP_SUMMARY'], `cache-prune: **${mode}**\n`)
}

const auth = token()
if (auth === undefined) {
  process.stderr.write('\nNo token. This needs `actions: write` to delete and `actions: read` to list; it will not run without one.\n')
  process.exit(1)
}

const repo = repository()
const headers = { authorization: `Bearer ${auth}`, accept: 'application/vnd.github+json', 'x-github-api-version': '2022-11-28' }

const entries = []
for (let page = 1; ; page += 1) {
  const response = await fetch(`https://api.github.com/repos/${repo}/actions/caches?per_page=100&page=${page}`, {
    headers,
    signal: AbortSignal.timeout(DEADLINE_MS),
  })
  if (!response.ok) {
    process.stderr.write(`\nGitHub answered ${response.status} listing ${repo}'s caches. This call needs \`actions: read\`.\n`)
    process.exit(1)
  }
  const body = await response.json()
  const page_entries = body.actions_caches ?? []
  entries.push(...page_entries)
  if (page_entries.length < 100) break
}

const total = entries.reduce((sum, entry) => sum + entry.size_in_bytes, 0)
const superseded = supersededOf(entries)
const freed = superseded.reduce((sum, entry) => sum + entry.size_in_bytes, 0)

process.stdout.write(
  `cache: ${(total / GIB).toFixed(2)} GiB across ${entries.length} entries; ${superseded.length} superseded, ` +
    `${(freed / GIB).toFixed(2)} GiB -- same key prefix, ${ORDER_READS}\n`,
)

// **The floor, asserted here as well as guaranteed there.** `supersededOf`
// keeps the most recently read entry of every group, so the last entry for a
// prefix can never be selected. This says so again at the point of deletion,
// because the cost of the two being out of step is measured in cold builds and
// the cost of checking is a loop over a handful of rows.
const survivors = new Map()
for (const entry of entries) survivors.set(prefixOf(entry.key), (survivors.get(prefixOf(entry.key)) ?? 0) + 1)
for (const entry of superseded) {
  const prefix = prefixOf(entry.key)
  const remaining = survivors.get(prefix) - superseded.filter(one => prefixOf(one.key) === prefix).length
  if (remaining < 1) {
    process.stderr.write(
      `\nRefusing: deleting the selection would leave nothing under \`${prefix}\`. The rule is not supposed to be ` +
        'able to do this.\n\nA wrongly deleted cache is not a slow run, it is a cold build: no prebuilt Skia exists ' +
        'for this feature set on Windows, so a miss compiles it from source. Measured twice, on different days and ' +
        'different trees: 61 and 60.6 minutes. That hour is what this floor is for.\n',
    )
    process.exit(1)
  }
}

if (superseded.length === 0) {
  process.stdout.write('nothing to prune\n')
  process.exit(0)
}

for (const entry of superseded) {
  const size = `${(entry.size_in_bytes / MIB).toFixed(0)} MiB`
  if (!remove) {
    process.stdout.write(`  would delete  ${entry.id}  ${size.padStart(9)}  ${entry.key}  (${ORDER_BY} ${entry[ORDER_BY]})\n`)
    continue
  }
  const response = await fetch(`https://api.github.com/repos/${repo}/actions/caches/${entry.id}`, {
    method: 'DELETE',
    headers,
    signal: AbortSignal.timeout(DEADLINE_MS),
  })
  if (!response.ok) {
    process.stderr.write(`\nGitHub answered ${response.status} deleting ${entry.id}. This call needs \`actions: write\`.\n`)
    process.exit(1)
  }
  process.stdout.write(`  deleted  ${entry.id}  ${size.padStart(9)}  ${entry.key}\n`)
}

process.stdout.write(
  remove
    ? `pruned ${superseded.length} entries, ${(freed / GIB).toFixed(2)} GiB freed; ${((total - freed) / GIB).toFixed(2)} GiB remains\n`
    : `dry run -- nothing was deleted. ${(freed / GIB).toFixed(2)} GiB would be freed, leaving ${((total - freed) / GIB).toFixed(2)} GiB. Pass --delete to act.\n`,
)
