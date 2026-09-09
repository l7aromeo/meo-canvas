// The two release channels agree about which tag belongs to which, and
// `docs.yml`'s filter is run rather than read.
//
// **The failure this exists for is silent.** `docs.yml` decides whether a tag
// has a JavaScript reference by matching it against a regular expression, and
// on a `release` event a tag that does not match prints a notice and publishes
// nothing: no error, no red job, no deploy. So a prefix changed in
// `release.yml` and not here does not break a release, it removes the
// documentation from one and says so in a line nobody reads. That is the
// failure mode this file is about, and it is why the check is here rather than
// in a comment saying "keep these in step".
//
// **It extracts the prefixes rather than restating them.** A table of tag
// shapes written down here would be a second opinion about what the workflows
// do, and the interesting failure is exactly the one where the two disagree.
// So the npm prefix comes out of `release.yml`'s own `git_tag=` line, the Rust
// prefix out of `crates-io.yml`'s, and the pattern out of `docs.yml`'s `grep`.
// Rename a prefix in one file and this fails, because the tag it now builds is
// run through the filter that did not change.
//
// **The pattern is run through `grep -E`, not through `RegExp`.** They are
// different languages -- POSIX ERE has no lazy quantifiers, no `\d`, and
// different escaping inside a bracket expression -- and a JavaScript engine
// agreeing with a pattern the shell will reject is the shape of a check that
// tests the wrong thing. This shells out to the same `grep -Eq` the workflow
// runs, on the same string.
//
// **What it does not check:** that the tag is ever pushed, that `docs.yml` is
// triggered at all, or that the site directory is named correctly. The first
// two are workflow wiring; the third is checked by the deploy failing to
// appear in the index, which is the failure that made the directory strip the
// prefix in the first place and is not mechanically testable from here.
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

  // **The tag-to-version derivation, run rather than restated.** This is the
  // step that was wrong: `${VERSION#v}` was correct while the tag was
  // `v10.0.0` and silently wrong once it was `npm-v10.0.0`, because `#v` does
  // not strip a prefix the string does not begin with. The two lines are
  // lifted out of `docs.yml` and executed in `bash`, so a change to them is a
  // change to what this asserts.
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

      // **And the directory it then writes has to be one the index can read.**
      // `docs.yml` writes `site/v${VERSION}`; `site-index.mjs` parses the
      // directory names it finds. A name that does not parse is not an error
      // anywhere -- the reference deploys, the index never lists it, and
      // `latest/` never advances past the last release named the old way.
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

  // The notes both channels now require, and the directory each reads from.
  //
  // **Including whether the file could be committed at all.** `.gitignore`
  // here denies everything and re-includes by name, so a notes directory
  // nobody named is refused by `git add` -- and the release would then stop on
  // an error naming a path that cannot be created, permanently, however many
  // times someone writes the note. That is a two-line omission in a file
  // nobody reads during a release, and it is checked here because there is no
  // other moment at which anyone would look.
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
