// Which Actions cache entries are superseded, for `cache-prune.mjs` to delete and
// `cache-budget.mjs` to leave out of the working set it checks. Entries sharing
// rust-cache's key prefix are one job at different moments; the one kept is the one
// read most recently, since the hashes are content and an older entry can be current.

/**
 * The field the rule orders on. The prose a caller prints is generated from it,
 * so changing the sort changes every sentence describing it. `last_accessed_at`
 * counts a stale branch's read too: a proxy, which the floor keeps non-destructive.
 */
export const ORDER_BY = 'last_accessed_at'

/** How the ordering reads in a heading, derived rather than restated. */
export const ORDER_READS = 'read less recently than a sibling'

/** Strips rust-cache's two trailing hashes, so a key without them is its own group. */
export const prefixOf = key => key.replace(/-[0-9a-f]{8}-[0-9a-f]{8}$/, '')

/**
 * The entries nothing will restore: same prefix, read less recently than a sibling.
 * A group's most recent entry always survives, so the last entry for a prefix is
 * never selected -- the floor, kept here so no caller can forget it.
 */
export const supersededOf = entries => {
  const byPrefix = new Map()
  for (const entry of entries) {
    const prefix = prefixOf(entry.key)
    const group = byPrefix.get(prefix) ?? []
    group.push(entry)
    byPrefix.set(prefix, group)
  }

  const superseded = []
  for (const group of byPrefix.values()) {
    if (group.length < 2) continue
    group.sort((a, b) => Date.parse(b[ORDER_BY]) - Date.parse(a[ORDER_BY]))
    superseded.push(...group.slice(1))
  }
  return superseded
}

/**
 * Delete or dry run, from `argv` and `DELETE_INPUT` -- `''` on `workflow_run`,
 * `'true'` or `'false'` on a dispatch, absent by hand. Decided here rather than in
 * a workflow `a && b || c` expression, which is unsound when `b` can be empty. An
 * unrecognised value is refused rather than defaulted.
 */
export const modeFrom = ({ argv = [], env = {} } = {}) => {
  if (argv.includes('--delete')) return 'delete'

  const event = env['EVENT'] ?? ''
  const input = env['DELETE_INPUT'] ?? ''

  // No event: a person at a terminal, who gets a dry run unless they passed
  // the flag above.
  if (event === '') return 'dry-run'

  // Only `workflow_run` deletes; any other event is refused, so a new trigger --
  // `schedule:`, a tag `push:`, `workflow_call` -- fails in `verifyMode` before the
  // network instead of deleting.
  if (event === 'workflow_run') return 'delete'
  if (event !== 'workflow_dispatch') {
    throw new Error(
      `EVENT is ${JSON.stringify(event)}, which is neither 'workflow_run' nor 'workflow_dispatch'. ` +
        'A trigger was added and nothing here says what it should do. Decide it above, with the case ' +
        'in `verifyMode`, rather than letting an unnamed event inherit the deleting path.',
    )
  }

  if (input === 'true') return 'delete'
  if (input === 'false' || input === '') return 'dry-run'

  throw new Error(
    `DELETE_INPUT is ${JSON.stringify(input)}, which is neither 'true' nor 'false'. ` +
      'Refusing rather than guessing: a value nobody predicted is not evidence of an intention.',
  )
}

/**
 * `modeFrom` over every state that reaches it, called rather than restated, before
 * it is trusted.
 */
export const verifyMode = () => {
  const cases = [
    [{ env: { EVENT: 'workflow_run', DELETE_INPUT: '' } }, 'delete'],
    [{ env: { EVENT: 'workflow_dispatch', DELETE_INPUT: 'false' } }, 'dry-run'],
    [{ env: { EVENT: 'workflow_dispatch', DELETE_INPUT: 'true' } }, 'delete'],
    [{}, 'dry-run'],
    [{ argv: ['--delete'] }, 'delete'],
  ]
  for (const [input, want] of cases) {
    const got = modeFrom(input)
    if (got !== want) {
      throw new Error(`modeFrom(${JSON.stringify(input)}) is ${got} and should be ${want}`)
    }
  }

  // **Both refusals executed rather than asserted**, because a refusal that is
  // only described is a branch nobody has run.
  for (const [input, what] of [
    [{ env: { EVENT: 'workflow_dispatch', DELETE_INPUT: 'yes' } }, 'an unrecognised DELETE_INPUT'],
    [{ env: { EVENT: 'schedule', DELETE_INPUT: '' } }, 'an event nobody named'],
  ]) {
    let refused = false
    try {
      modeFrom(input)
    } catch {
      refused = true
    }
    if (!refused) throw new Error(`${what} was accepted; it has to be refused`)
  }
}

/**
 * The rule against the shape of 2026-09-10's cache: two platforms whose creation
 * and access orders disagree, so sorting by `created_at` fails here. Throws on
 * failure rather than returning, so a caller cannot ignore it.
 */
export const verifySelection = () => {
  const fixture = [
    // macOS: the survivor was created first and read last.
    {
      id: 1,
      key: 'v0-rust-macos-latest-ci-Darwin-arm64-ad2795f7-1111aaaa',
      size_in_bytes: 1_200_000_000,
      created_at: '2026-09-09T10:00:00Z',
      last_accessed_at: '2026-09-10T04:00:00Z',
    },
    {
      id: 7535197054,
      key: 'v0-rust-macos-latest-ci-Darwin-arm64-cccc2222-3333bbbb',
      size_in_bytes: 1_200_000_000,
      created_at: '2026-09-10T01:00:00Z',
      last_accessed_at: '2026-09-10T01:00:00Z',
    },
    // Windows: the same shape, and the entry a `main` run logged a full match on.
    {
      id: 2,
      key: 'v0-rust-windows-latest-ci-Windows-x86_64-3b76fde0-8983e69d',
      size_in_bytes: 2_200_000_000,
      created_at: '2026-09-09T11:00:00Z',
      last_accessed_at: '2026-09-10T05:00:00Z',
    },
    {
      id: 7543949648,
      key: 'v0-rust-windows-latest-ci-Windows-x86_64-dddd4444-5555eeee',
      size_in_bytes: 2_200_000_000,
      created_at: '2026-09-10T02:00:00Z',
      last_accessed_at: '2026-09-10T02:00:00Z',
    },
    // A key with no trailing hashes: its own group of one, never superseded.
    { id: 3, key: 'bun-ubuntu-latest', size_in_bytes: 40_000_000, created_at: '2026-09-01T00:00:00Z', last_accessed_at: '2026-09-01T00:00:00Z' },
  ]

  const chosen = supersededOf(fixture)
  const ids = chosen.map(entry => entry.id).sort((a, b) => a - b)
  const want = [7535197054, 7543949648]

  if (ids.join(',') !== want.join(',')) {
    throw new Error(
      `the superseded rule selected [${ids.join(', ')}] and the recorded answer is [${want.join(', ')}].\n` +
        'These are the entries from 2026-09-10: the two that were deleted by hand, and the two that had to\n' +
        'survive because `main` restores them -- `ad2795f7` on macOS and `3b76fde0` on Windows, both of which\n' +
        'were created BEFORE the siblings that were removed. A rule sorted by `created_at` inverts this.',
    )
  }

  for (const entry of chosen) {
    if (entry.key.includes('ad2795f7') || entry.key.includes('3b76fde0')) {
      throw new Error(`the rule selected ${entry.key}, which a \`main\` run logged a full match on`)
    }
  }

  // A group of one is never selected, whatever else changes.
  if (supersededOf([fixture[4]]).length !== 0) {
    throw new Error('the rule selected the only entry for a prefix; the floor is gone')
  }
}

/**
 * Superseded entries the latest successful prune should already have removed: last
 * read before their newer sibling was created, which was before `prunedAt`, so that
 * prune listed both and ranked this one behind. Their presence means it did not delete.
 */
export const staleOf = (entries, prunedAt) => {
  const newest = new Map()
  for (const entry of entries) {
    const prefix = prefixOf(entry.key)
    const held = newest.get(prefix)
    if (held === undefined || Date.parse(entry[ORDER_BY]) > Date.parse(held[ORDER_BY])) newest.set(prefix, entry)
  }
  const cutoff = Date.parse(prunedAt)
  return supersededOf(entries).filter(entry => {
    const sibling = Date.parse(newest.get(prefixOf(entry.key)).created_at)
    return Date.parse(entry[ORDER_BY]) < sibling && sibling < cutoff
  })
}

/**
 * The budget as the check reads it: `working` is the total less what `supersededOf`
 * selects, which the prune after each `ci` on `main` deletes; `over` compares the
 * working set with `floor`. `stale` is empty when no successful prune is known.
 */
export const budgetOf = (entries, { floor, prunedAt }) => {
  const total = entries.reduce((sum, entry) => sum + entry.size_in_bytes, 0)
  const superseded = supersededOf(entries)
  const pending = superseded.reduce((sum, entry) => sum + entry.size_in_bytes, 0)
  const stale = prunedAt === undefined ? [] : staleOf(entries, prunedAt)
  return { total, working: total - pending, pending, superseded, stale, over: total - pending > floor }
}

/**
 * {@link budgetOf} against the cache as it stood mid-`ci` on 2026-09-23: ubuntu and
 * macOS had saved new keys, Windows had not, and the previous prune was 09-21's. The
 * same listing with that day's prune moved after the saves must report both old entries.
 */
export const verifyBudget = () => {
  const MIB = 1024 ** 2
  const entry = (id, key, mib, created_at, last_accessed_at) => ({ id, key, size_in_bytes: mib * MIB, created_at, last_accessed_at })
  const fixture = [
    entry(7931640841, 'v0-rust-ubuntu-latest-ci-Linux-x64-b566bed7-7eb6fe4c', 2712, '2026-09-21T12:38:00Z', '2026-09-23T13:27:40Z'),
    entry(8024965760, 'v0-rust-ubuntu-latest-ci-Linux-x64-b566bed7-8dec0dfc', 2712, '2026-09-23T13:41:00Z', '2026-09-23T13:41:00Z'),
    entry(7931529645, 'v0-rust-macos-latest-ci-Darwin-arm64-d1842c12-7eb6fe4c', 1307, '2026-09-21T12:30:00Z', '2026-09-23T13:27:45Z'),
    entry(8024958370, 'v0-rust-macos-latest-ci-Darwin-arm64-d1842c12-8dec0dfc', 1307, '2026-09-23T13:40:00Z', '2026-09-23T13:40:00Z'),
    entry(7932007922, 'v0-rust-windows-latest-ci-Windows_NT-x64-3b76fde0-7eb6fe4c', 1362, '2026-09-21T12:39:00Z', '2026-09-23T13:27:50Z'),
    entry(7609041463, 'bun-+CxZWfwTi+/AufZaQ1DSt6rSdDY=', 34, '2026-09-12T01:06:15Z', '2026-09-23T13:27:30Z'),
    entry(7609154682, 'bun-Hh9NYlpnO5g9QEGjuhbNWJevxFw=', 37, '2026-09-12T01:13:19Z', '2026-09-23T13:27:30Z'),
    entry(7744736921, 'bun-rG58rdMUwFIpDovjdVSDwkde0NA=', 22, '2026-09-16T04:16:34Z', '2026-09-23T13:27:30Z'),
  ]
  const floor = 7.5 * 1024 * MIB
  const ids = list =>
    list
      .map(one => one.id)
      .sort((a, b) => a - b)
      .join(',')
  const stuck = '7931529645,7931640841'

  const window = budgetOf(fixture, { floor, prunedAt: '2026-09-21T12:39:36Z' })
  if (window.total !== 9493 * MIB || window.working !== 5474 * MIB || window.over || window.stale.length !== 0) {
    throw new Error(
      `mid-run, the budget read total ${window.total / MIB} MiB, working ${window.working / MIB} MiB, over ${window.over}, ` +
        `stale [${ids(window.stale)}]; the recorded answer is 9493, 5474, false, []. Old and new keys coexist until ` +
        'the prune after `ci`, and that is not a working set past the floor.',
    )
  }
  const missed = budgetOf(fixture, { floor, prunedAt: '2026-09-23T14:01:58Z' })
  if (ids(missed.stale) !== stuck) {
    throw new Error(`with a prune after the saves, the stale entries read [${ids(missed.stale)}]; the recorded answer is [${stuck}]`)
  }
}
