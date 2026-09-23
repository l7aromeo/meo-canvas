// Which Actions cache entries are superseded, for `cache-budget.mjs` to report and
// `cache-prune.mjs` to delete. Entries sharing rust-cache's key prefix are one job
// at different moments; the one kept is the one read most recently, not created
// -- the hashes are content, so an older entry can be the one `main` restores.

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
