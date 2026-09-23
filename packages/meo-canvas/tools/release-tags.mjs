// The two release channels agree on which tag is whose, and `docs.yml`'s filter is
// run rather than read: a tag it refuses publishes no reference, silently. The
// prefixes come from `release.yml`'s and `crates-io.yml`'s own lines, and the pattern
// runs through `grep -E`, as the workflow runs it, rather than `RegExp`.
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..')
const workflow = name => readFileSync(join(ROOT, '.github', 'workflows', name), 'utf8')

const problems = []
const fail = message => problems.push(message)

/** The one thing read out of `docs.yml`: the extended regular expression it filters tags with. */
const pattern = (() => {
  const found = workflow('docs.yml').match(/grep -Eq '(\^[^']+)'/)
  if (!found) {
    fail("docs.yml no longer contains a `grep -Eq '^...'` tag filter; this check reads that line")
    return undefined
  }
  return found[1]
})()

/** The prefix a workflow builds its tag from, e.g. `npm-v` out of `git_tag="npm-v${VERSION}"`. */
const prefixOf = (file, source) => {
  const found = source.match(/git_tag="([A-Za-z-]*)v\$\{/)
  if (!found) {
    fail(`${file} no longer assigns git_tag="<prefix>v\${...}"; this check reads that line`)
    return undefined
  }
  return `${found[1]}v`
}

let checked = 0
const npm = prefixOf('release.yml', workflow('release.yml'))
const rust = prefixOf('crates-io.yml', workflow('crates-io.yml'))

/** Does `site-index.mjs`'s own directory pattern accept this name? */
const indexParses = (() => {
  const source = readFileSync(join(ROOT, 'packages', 'meo-canvas', 'tools', 'typedoc', 'site-index.mjs'), 'utf8')
  const found = source.match(/const m = \/(.+)\/\.exec\(name\)/)
  if (!found) {
    fail('site-index.mjs no longer matches directory names with a `^v...` pattern; this check reads that line')
    return () => true
  }
  const pattern = new RegExp(found[1])
  return name => pattern.test(name)
})()

/** Does the workflow's own filter accept this tag? */
const accepts = tag => {
  try {
    execFileSync('grep', ['-Eq', pattern], { input: tag })
    return true
  } catch {
    return false
  }
}

if (pattern && npm && rust) {
  if (npm === rust) fail(`both channels build a ${npm} tag; the namespaces have to differ`)

  // Derived from the prefixes above, so a rename moves these with it.
  const expected = [
    [`${npm}10.0.0`, true, 'an npm release'],
    [`${npm}10.0.0-alpha.6`, true, 'an npm prerelease'],
    ['v10.0.0-alpha.5', true, 'the pre-rename npm spelling, which workflow_dispatch backfills'],
    [`${rust}0.1.0`, false, 'a crate release, which has no JavaScript reference'],
    [`${rust}0.1.0-rc.1`, false, 'a crate prerelease'],
    ['crates-v0.1.0', false, 'the pre-rename crate spelling'],
    [`${npm}10`, false, 'not a version'],
    [`${npm}10.0.0 `, false, 'trailing space'],
    [`x${npm}10.0.0`, false, 'not anchored at the start'],
  ]

  checked = expected.length
  for (const [tag, want, why] of expected) {
    const got = accepts(tag)
    if (got !== want) {
      fail(`docs.yml ${got ? 'accepts' : 'refuses'} ${JSON.stringify(tag)} and should ${want ? 'accept' : 'refuse'} it: ${why}`)
    }
  }

  // The tag-to-version derivation, lifted out of `docs.yml` and executed in `bash`:
  // `${VERSION#v}` alone does not strip `npm-v`, since `#v` removes only a prefix
  // the string begins with.
  const derivation = (() => {
    const found = workflow('docs.yml').match(/^\s*VERSION="\$\{TAG#npm-\}"\n\s*VERSION="\$\{VERSION#v\}"$/m)
    if (!found) {
      fail('docs.yml no longer derives VERSION from TAG in the two lines this check runs')
      return undefined
    }
    return found[0]
  })()

  /** The line that names the site directory, read rather than assumed. */
  const dirLine = (() => {
    const found = workflow('docs.yml').match(/^\s*(dir=.*)$/m)
    if (!found) {
      fail('docs.yml no longer assigns `dir=`; this check runs that line')
      return undefined
    }
    return found[1]
  })()

  /** What `docs.yml` would compute for this tag: the version, and the directory. */
  const computed = tag =>
    execFileSync('bash', ['-c', `set -eu; TAG="$1"; ${derivation}; ${dirLine ?? 'dir='}; printf '%s\\n%s' "$VERSION" "$dir"`, 'sh', tag], {
      encoding: 'utf8',
    }).split('\n')

  if (derivation && dirLine) {
    for (const [tag, want] of [
      [`${npm}10.0.0-alpha.6`, '10.0.0-alpha.6'],
      [`${npm}10.0.0`, '10.0.0'],
      ['v10.0.0-alpha.5', '10.0.0-alpha.5'],
    ]) {
      const [got, dir] = computed(tag)
      if (got !== want) fail(`docs.yml turns ${tag} into ${JSON.stringify(got)}, not ${JSON.stringify(want)}`)

      // The directory `docs.yml` writes must parse in `site-index.mjs`; one that
      // does not deploys unlisted and `latest/` stops advancing, with no error.
      if (!indexParses(dir)) {
        fail(`site-index.mjs would not list the directory docs.yml writes for ${tag}: ${JSON.stringify(dir)}`)
      }
    }

    // A crate tag never reaches the derivation, because the filter refuses it
    // first. Asserted as a pair so widening the filter fails here rather than
    // publishing a JavaScript reference for a crate version.
    if (accepts(`${rust}0.1.0-alpha.1`)) {
      fail(`docs.yml accepts ${rust}0.1.0-alpha.1, which would derive a version and deploy a reference for a crate release`)
    }
  }

  // The notes both channels require, and whether each directory can be committed:
  // `.gitignore` denies everything and re-includes by name, so an unnamed notes
  // directory is refused by `git add` and the release cannot proceed.
  for (const [file, dir] of [
    ['release.yml', 'docs/releases/npm/'],
    ['crates-io.yml', 'docs/releases/rust/'],
  ]) {
    const source = workflow(file)
    if (!source.includes(dir)) fail(`${file} no longer reads its notes from ${dir}`)
    if (/git log --no-merges/.test(source)) {
      fail(`${file} generates release notes from the log again; notes are written by hand and a missing file must stop the release`)
    }
    const example = `${dir}1.2.3.md`
    try {
      execFileSync('git', ['check-ignore', '-q', example], { cwd: ROOT })
      fail(`${example} is ignored by .gitignore, so the note ${file} demands could never be committed`)
    } catch {
      // A non-zero exit is git saying the path is not ignored, which is what
      // this wants. The read above already failed loudly if the tree is gone.
    }
  }
}

if (problems.length > 0) {
  for (const one of problems) process.stderr.write(`${one}\n`)
  process.exit(1)
}

process.stdout.write(`release tags: ${npm}* has a reference, ${rust}* does not; ${checked} shapes checked\n`)
