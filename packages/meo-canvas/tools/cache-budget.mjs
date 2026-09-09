// The Actions cache budget, and where it is going.
//
// **GitHub emits no signal when it evicts.** The only symptom is a restore
// taking one to six seconds instead of thirty to sixty, followed by a cold
// build -- and nothing looks at that number. On 2026-09-09 this repository sat
// over the limit for an unknown period, a different leg cold-built on each run,
// and it read as "Windows is slow" for long enough to reach a task description
// as a diagnosis. Three runs, from the job step timings:
//
//     run           leg      restore  install   just native
//     34337356783   windows      58s       1s         883s
//     34339254035   ubuntu        1s      90s        2090s
//     34343357328   windows       6s     185s        2784s
//
// **Per ref, because the total is the autopsy and the breakdown is the
// warning.** What went wrong was a merged pull request's caches outliving it:
// 3.53 GiB under `refs/pull/59/merge`, unreadable by anything, sitting beside a
// working set. A total says the budget is tight; a ref that should not be there
// says which change to make.
//
// **Every call it makes is bounded, because a gate step that hangs is worse
// than one that fails.** This reads the network and shells out to `gh`, and
// neither had a bound when it was written: on 2026-09-10 two `just ci` runs
// died at this recipe with `terminated on line 1541 by signal 15` after five
// lines of output, which is a fifteen-minute hang and then whatever was
// watching giving up. A check that can stop the gate indefinitely is a worse
// failure than the eviction it exists to catch, and it is the same fault as an
// unbounded fetch anywhere else -- which this repository already bounds, at
// sixty seconds, for image sources.
//
// **What this cannot see, stated so nobody reads more into a pass.** It takes
// one reading at one moment. Eviction happens between runs, so a pass means the
// budget was fine when it looked -- not that nothing was evicted since the last
// look, and not that nothing will be before the next. Catching an eviction as it
// happens would mean reading restore durations out of job logs, which is a
// different tool. This one is a smoke alarm, not a flight recorder.
import { execFileSync } from 'node:child_process'

const GIB = 1024 ** 3
const MIB = 1024 ** 2

/**
 * The size at which this asks for attention, in bytes.
 *
 * **Derived rather than picked, and the derivation is the point.** GitHub
 * documents the limit as "10 GB" without saying which unit; on either reading
 * the headroom argument is the same. An alarm is only useful while there is
 * still room for the next save, so the floor is the limit less the largest
 * single entry: the biggest cache here is the ubuntu set at 2215 MiB, so an
 * alarm at 8 GiB would leave 2 GiB -- less than one of it -- and the next save
 * would evict something before anyone read the warning.
 *
 * 7.5 GiB leaves 2.5 GiB, which is one largest-entry plus a margin. For scale:
 * a clean set is ubuntu 2215 + windows 1398 + macos 1242 + bun 94 = 4949 MiB,
 * and the reading on 2026-09-10 was 6.04 GiB because a windows key had rotated
 * and both copies were still live. So this fires on roughly one more rotation,
 * or on anything that should not be here at all.
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
const auth = token()

// **The skip is refused where it would matter.** A check that quietly does
// nothing without credentials is the shape this repository has been burned by
// repeatedly, so it is allowed exactly where it cannot hide: on a developer's
// machine, out loud, naming what would make it run. In CI there is always a
// token and `actions: read` is granted on this job, so an absence there is a
// broken workflow rather than a laptop without `gh`.
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
    // **Unreachable is not the same as fine, and not the same as broken.** In
    // CI the runner is already talking to this host, so a failure here says the
    // job's access is wrong and the run is compromised either way. On a machine
    // it says the network is having a moment, which is not a reason to stop
    // someone's gate -- the same split the missing-token arm above makes, for
    // the same reason.
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

if (total > FLOOR_BYTES) {
  const largest = entries.reduce((big, entry) => (entry.size_in_bytes > big.size_in_bytes ? entry : big))
  process.stderr.write(
    `\nThe cache is ${(total / GIB).toFixed(2)} GiB, past the ${(FLOOR_BYTES / GIB).toFixed(1)} GiB this asks about, and ` +
      `the largest single entry is ${(largest.size_in_bytes / MIB).toFixed(0)} MiB. Read the per-ref lines above: a ref ` +
      'that is not the default branch is a pull request or a tag whose caches outlive it and can never be restored by ' +
      'anything, and deleting those is the cheapest fix. `gh api repos/OWNER/REPO/actions/caches` lists them and ' +
      '`gh api -X DELETE .../actions/caches?key=KEY` removes one. If every ref here is legitimate, the working set has ' +
      'grown and the floor is what needs the argument.\n',
  )
  process.exit(1)
}
