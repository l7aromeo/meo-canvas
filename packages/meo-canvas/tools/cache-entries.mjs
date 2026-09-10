// Which Actions cache entries are superseded, and the proof that the rule is
// the one it looks like.
//
// **Extracted so that one implementation answers two questions.**
// `cache-budget.mjs` reports what would be removed and `cache-prune.mjs`
// removes it; a second copy of this rule would be two answers about one cache,
// and the failure would be a deletion rather than a disagreement.
//
// # The rule, and the thing about it that reads as a bug
//
// rust-cache's key is a prefix, then a hash of the toolchain and the
// `CARGO`/`RUST`-shaped environment, then a hash of the manifests. Two entries
// sharing a prefix and differing in either hash are the same job at two
// different moments.
//
// **The one kept is the one read most recently, not the one created most
// recently.** Those come apart, and the difference is not academic: the
// trailing hashes are content, not a clock, so a *newer* entry can be a cache
// of a lockfile state `main` has moved past while an *older* one is exactly
// current. Measured on 2026-09-10 from a `main` CI run's own log:
//
//     Restored from "...3b76fde0-8983e69d" full match: true
//
// -- created 09-09, and the older of its pair. A rule sorted by creation would
// have called that dead and deleted the Windows cache `main` actually
// restores, which is an hour of cold Skia build.
//
// **A worker did read the comment that used to sit here, conclude the code was
// the bug, and produce a delete list containing exactly that entry.** The
// comment said created; the code said last accessed; the code was right. That
// is why `verifySelection` exists and why `cache-prune.mjs` refuses to run
// when it fails.
//
// # What the rule cannot see
//
// `last_accessed_at` answers *something read this*, not *`main` will read
// this*. A pull request restoring a cache marks it read, so a superseded entry
// a stale branch touched looks current for as long as that branch is alive.
// The rule is a good proxy and it is a proxy; the floor below is what keeps a
// wrong proxy from being a destructive one.

/**
 * The field the rule orders on, named once and used everywhere it is described.
 *
 * **The defect this file exists for was not a wrong rule; it was two rules in
 * one file with nothing forcing them to agree** -- a comment saying `created`
 * beside a sort on `last_accessed_at`. Fixing the words would leave the same
 * arrangement in place for the next edit. So the words are generated from the
 * field: a caller printing what it selected reads this, and a change to the
 * sort below changes every sentence that describes it.
 *
 * The prose that cannot be generated -- why access rather than creation -- is
 * above, and it is the part a reader has to be able to disagree with.
 */
export const ORDER_BY = 'last_accessed_at'

/** How the ordering reads in a heading, derived rather than restated. */
export const ORDER_READS = 'read less recently than a sibling'

/** Strips rust-cache's two trailing hashes, so a key without them is its own group. */
export const prefixOf = key => key.replace(/-[0-9a-f]{8}-[0-9a-f]{8}$/, '')

/**
 * The entries nothing will restore: same prefix, read less recently than a sibling.
 *
 * The most recently read entry of every group survives, so a group of one is
 * never superseded and **the last entry for a prefix can never be selected**.
 * That is the floor, and it is here rather than in the caller because a caller
 * that forgot it would delete the working set.
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
 * Delete or dry run, decided once, from values that are strings.
 *
 * # Why this is not a workflow expression
 *
 * It was one, and the expression was wrong twice.
 *
 * The first said `inputs.delete == false && 'dry run' || 'delete'`. On
 * `workflow_run` there are no inputs, so `inputs.delete` is null; GitHub
 * coerces a mismatched comparison to a number, null casts to 0 and false casts
 * to 0, so `null == false` is **true** and the run that deletes was named a dry
 * run.
 *
 * The second guarded the input behind the event -- and moved the defect onto
 * the safe path. `EVENT == 'workflow_dispatch' && (input && '--delete' || '')
 * || '--delete'`: on a manual dry run the inner branch yields `''`, **the empty
 * string is falsy in this language**, so the outer `||` fires and a run a
 * person chose because they wanted to be safe would have deleted. Both facts
 * are in the same table on GitHub's expressions reference, which neither
 * reading of mine had opened.
 *
 * **So the pattern is the defect, not either instance.** An `a && b || c`
 * chain whose `b` can be empty is unsound here and unsound invisibly. This
 * takes the raw values through the environment and decides in a language whose
 * semantics can be executed on the machine writing it.
 *
 * `DELETE_INPUT` is `''` on `workflow_run`, `'true'` or `'false'` on a
 * dispatch, and absent entirely when a person runs the tool by hand.
 *
 * **An unrecognised value is refused rather than defaulted.** Defaulting it to
 * a dry run would be safe and silent; defaulting to delete would be neither.
 * Refusing says the environment is not what this expects, which is the only
 * honest answer to a value nobody predicted.
 */
export const modeFrom = ({ argv = [], env = {} } = {}) => {
  if (argv.includes('--delete')) return 'delete'

  const event = env['EVENT'] ?? ''
  const input = env['DELETE_INPUT'] ?? ''

  // No event: a person at a terminal, who gets a dry run unless they passed
  // the flag above.
  if (event === '') return 'dry-run'

  // **The deleting event is named, and every other one is refused.** This read
  // `event !== 'workflow_dispatch'` and returned `delete`, which is correct for
  // the two events that exist today and is the same reasoning that made
  // `DELETE_INPUT: 'yes'` look safe to default -- an event nobody predicted
  // treated as evidence of the most consequential intention.
  //
  // **`workflow_run` and no other**, because the ways that branch goes wrong
  // are ordinary rather than exotic: a `schedule:` trigger, a `push:` on a tag,
  // a `workflow_call` so another workflow can prune after a release. All three
  // land here, all three would have deleted, and none is a change whose author
  // would think to check a mode that lives in a `.mjs` file two directories
  // from the YAML they edited. Named, adding a trigger fails in `verifyMode`
  // before the network instead of arming itself.
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
 * `modeFrom` over every state that reaches it, before it is trusted.
 *
 * **Executed rather than read.** The expression this replaces was checked by
 * enumerating its rows by eye, and by restating it in another language and
 * running that -- which agreed with the intention and not with the artefact.
 * These call the function itself.
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
 * The rule, run against the night it was established, before it is trusted.
 *
 * The fixture is the shape of 2026-09-10's cache: two platforms, each with a
 * pair whose creation order and access order disagree. **Sorting by
 * `created_at` passes every other check in this file and fails here**, which
 * is the whole point -- it is the edit a reader of the old comment would have
 * made, and it would have deleted the two entries `main` restores.
 *
 * Returns nothing and throws on failure, so a caller cannot proceed past it by
 * ignoring a return value.
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
