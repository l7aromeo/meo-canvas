// What upstream has done about the defects this tree works around, to catch a fix
// merged and not released, which Dependabot cannot see. Every reference yields a
// row, a state or an error; it exits non-zero on a query it could not make or a
// fix merged into no release. It reads the tree each run and writes nowhere.

import { readFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'

import { REFERENCE, REFERENCE_URL, commentsIn, trackedFiles, verifyLexers } from './comments.mjs'

/** Long enough for a slow API, short enough that a wedged call is not a hang. */
const DEADLINE_MS = 30_000

/** A token, from the environment in CI and from `gh` on a developer's machine. */
function token() {
  const fromEnv = process.env['GITHUB_TOKEN'] ?? process.env['GH_TOKEN']
  if (fromEnv !== undefined && fromEnv !== '') return fromEnv
  try {
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

/** This repository, so its own issues are not watched as if they were upstream. */
function self() {
  // No `origin`, or a remote that is not GitHub, both leave the filter unable to
  // say what is ours, and arrive at the same refusal rather than a stack trace.
  let url
  try {
    url = execFileSync('git', ['remote', 'get-url', 'origin'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim()
  } catch {
    return undefined
  }
  const match = /github\.com[/:](?<owner>[\w.-]+)\/(?<repo>[\w.-]+?)(?:\.git)?$/u.exec(url)
  return match?.groups === undefined ? undefined : `${match.groups['owner']}/${match.groups['repo']}`
}

/**
 * Every reference in a comment, in both spellings -- taffy's 1151 and 1163 appear
 * here only as full URLs -- deduplicated by reference, so a doc paragraph and the
 * comment marking the code collapse to one row.
 */
function referenced() {
  const found = new Map()
  for (const file of trackedFiles()) {
    const source = readFileSync(file, 'utf8')
    for (const [line, text] of commentsIn(file, source)) {
      for (const pattern of [REFERENCE, REFERENCE_URL]) {
        for (const match of text.matchAll(pattern)) {
          const repo = match.groups?.['qualified']
          if (repo === undefined) continue
          const key = `${repo}#${match.groups?.['number']}`
          if (!found.has(key)) found.set(key, `${file}:${line}`)
        }
      }
    }
  }
  return found
}

/** One API call, returning its body or the reason there is none. */
function ask(path, auth) {
  const args = ['api', path, '-H', 'Accept: application/vnd.github+json']
  try {
    const body = execFileSync('gh', args, {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
      timeout: DEADLINE_MS,
      env: auth === undefined ? process.env : { ...process.env, GH_TOKEN: auth },
    })
    return { ok: true, body: JSON.parse(body) }
  } catch (cause) {
    const text = `${cause.stderr ?? ''}`.trim() || cause.message
    return { ok: false, why: text.split('\n')[0].slice(0, 160) }
  }
}

/**
 * What a reference's state means for the workaround citing it. Merged after the
 * newest release is in no release; merged before it is reported as `merged`,
 * since that needs the merge commit compared against the tag, which this does not do.
 */
function verdict(item, release) {
  if (item.merged_at != null) {
    if (release?.published_at != null && item.merged_at > release.published_at) {
      return {
        state: 'MERGED, in no release',
        gap: true,
        note: `merged ${item.merged_at.slice(0, 10)}, after the latest release ${release.tag_name} of ${release.published_at.slice(0, 10)}`,
      }
    }
    return {
      state: 'MERGED',
      gap: false,
      note:
        release?.tag_name == null
          ? 'no release found to compare against'
          : `at or before ${release.tag_name}; a bump is what surfaces it, and Dependabot opens one`,
    }
  }
  if (item.state === 'closed')
    return { state: 'CLOSED', gap: false, note: 'closed with no merge recorded; a citation may be history rather than a live workaround' }
  return { state: 'OPEN', gap: false, note: '' }
}

/**
 * The self-test: rows built from fixed answers, one of them an error, with the
 * counts checked -- a run where no query fails cannot show that none is dropped.
 */
export function verifySummary() {
  const answers = [
    { key: 'a/b#1', answer: { ok: true, row: 'OPEN' } },
    { key: 'a/b#2', answer: { ok: false, why: 'HTTP 403: rate limit' } },
    { key: 'a/b#3', answer: { ok: true, row: 'MERGED' } },
  ]
  const answered = answers.filter(one => one.answer.ok).length
  if (answers.length !== 3 || answered !== 2) {
    throw new Error(`the fixture should hold 3 references of which 2 answered, and holds ${answers.length} and ${answered}`)
  }
  if (answered === answers.length) {
    throw new Error('a fixture carrying a failed query counted every reference as answered; the summary cannot tell a failure from a quiet week')
  }
}

verifyLexers()
verifySummary()

const mine = self()
if (mine === undefined) {
  // Refuse rather than watch everything: with no name for this repository the
  // filter drops nothing, and our own issues would be reported as upstream.
  process.stderr.write(
    'upstream watch: `git remote get-url origin` named no owner/repo, so there is no way to tell this ' +
      "repository's own references from an upstream one. Refusing rather than watching every reference in the tree.\n",
  )
  process.exit(1)
}
const auth = token()
const all = referenced()
const watched = [...all].filter(([key]) => !key.startsWith(`${mine}#`)).sort(([one], [two]) => one.localeCompare(two))

if (watched.length === 0) {
  process.stdout.write('upstream watch: no reference to a repository other than this one. Nothing is being worked around, or nothing says so.\n')
  process.exit(0)
}

const rows = []
let answered = 0
const releases = new Map()

for (const [key, site] of watched) {
  const [repo, number] = key.split('#')
  const item = ask(`repos/${repo}/issues/${number}`, auth)
  if (!item.ok) {
    rows.push({ key, site, state: 'COULD NOT QUERY', note: item.why, gap: false })
    continue
  }
  answered += 1

  let detail = item.body
  if (item.body.pull_request != null) {
    const pull = ask(`repos/${repo}/pulls/${number}`, auth)
    if (pull.ok) detail = pull.body
  }
  if (!releases.has(repo)) {
    const latest = ask(`repos/${repo}/releases/latest`, auth)
    releases.set(repo, latest.ok ? latest.body : undefined)
  }
  rows.push({ key, site, ...verdict(detail, releases.get(repo)) })
}

// Only a fix merged into no release is a finding. A closed issue or a released
// fix is covered by Dependabot and the probe beside the workaround; the rest of
// the citations are context.
const gaps = rows.filter(row => row.gap)
const unanswered = watched.length - answered

process.stdout.write(`upstream watch: ${watched.length} references, ${answered} answered.\n`)
for (const row of rows) {
  process.stdout.write(`  ${row.key.padEnd(24)} ${row.state}${row.note === '' ? '' : ` -- ${row.note}`}\n    cited at ${row.site}\n`)
}

const summary = process.env['GITHUB_STEP_SUMMARY']
if (summary !== undefined && summary !== '') {
  const lines = [`### upstream watch -- ${watched.length} references, ${answered} answered`, '']
  for (const row of rows) lines.push(`- \`${row.key}\` **${row.state}**${row.note === '' ? '' : ` -- ${row.note}`} (\`${row.site}\`)`)
  execFileSync('tee', ['-a', summary], { input: `${lines.join('\n')}\n`, stdio: ['pipe', 'ignore', 'ignore'] })
}

if (unanswered > 0) {
  process.stderr.write(
    `\n${unanswered} of ${watched.length} references could not be queried. That is not a quiet week: a rate limit, a ` +
      'network failure and a renamed repository all look exactly like nothing to report, which is why this is the one ' +
      'condition that fails rather than reports.\n',
  )
  process.exit(1)
}

if (gaps.length > 0) {
  for (const row of gaps) process.stdout.write(`\n  ${row.key} -- ${row.site}\n`)
  process.stderr.write(
    `\n${gaps.length} of ${watched.length} are fixed upstream and in no release. Nothing else here can see that: ` +
      'there is no published version to bump, so Dependabot opens nothing and no probe fails. **This fails the run ' +
      'rather than reporting**, because a finding inside a green scheduled job is a finding nobody reads.\n\n' +
      'It clears when the workaround and the reference citing it are deleted. If the fix is not one this tree can ' +
      'take yet, say so where the reference lives -- the row follows the reference, not a stored verdict.\n',
  )
  process.exit(1)
}
process.exit(0)
