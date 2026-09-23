// Checks every workaround tag marks code with a live probe: the probe exists, is a
// test target with a test that runs, and has a test tagged as its foundation. It
// sees marks, not compensations -- a site whose tag is deleted is invisible. The
// tag is spelled only in `WORKAROUND` below, since this file is scanned too.
import { readFileSync, existsSync } from 'node:fs'
import { dirname, join, posix } from 'node:path'
import { commentsIn, trackedFiles } from './comments.mjs'

/** The tag on a compensation, at the head of the comment that marks it. */
const WORKAROUND = '[WORKAROUND]'

/** The tag on the row a compensation rests on, inside its probe. */
const FOUNDATION = '[FOUNDATION]'

/**
 * How few marked sites may be found before this asks whether it was meant. One:
 * zero means the scan reads nothing or there are no workarounds left, while a
 * higher floor would make an ordinary refactor edit this constant.
 */
const SITES_FLOOR = 1

/**
 * The mark-finder over fixtures, one per lexer, each holding mentions and one
 * mark. A finder returning every occurrence no longer tells marks from mentions;
 * one returning none has stopped reading comments.
 */
function probeScan() {
  const cases = [
    {
      path: 'probe.rs',
      source: [
        '//! The convention is written `[WORKAROUND]` and this is not one.',
        '',
        '// [WORKAROUND] the dependency does x. `owner/repo#1`. Probed by',
        '// `crates/n/tests/n.rs`.',
        'fn compensate() {',
        '    let message = "a `[WORKAROUND]` in a string is data";',
        '}',
      ].join('\n'),
    },
    {
      path: 'justfile',
      source: [
        '# [WORKAROUND] the runner does y. `owner/repo#2`. Probed by',
        '# `crates/n/tests/n.rs`.',
        'recipe:',
        '    echo "a [WORKAROUND] in a string is data"',
      ].join('\n'),
    },
  ]
  for (const { path, source } of cases) {
    const found = marksIn(path, source)
    if (found.length !== 1) {
      process.stderr.write(
        `\nThe mark-finder found ${found.length} marked sites in a ${path} written to contain one. ` +
          'The scan below would report every site sound while reading nothing, or would report the ' +
          'convention itself as a site. Fix the finder rather than this fixture.\n',
      )
      process.exit(1)
    }
    if (!found[0].text.includes('crates/n/tests/n.rs')) {
      process.stderr.write(
        `\nThe mark-finder found the mark in ${path} but not the rest of its block, so a site would ` +
          'be reported as naming no probe whenever the path sits on a later line -- which is where ' +
          'it always sits. Fix the grouping rather than this fixture.\n',
      )
      process.exit(1)
    }
  }
}

/**
 * The marked comment blocks of one source, as `{ line, text }`. Consecutive comment
 * lines are joined, since the reference naming the probe sits lines below the tag;
 * a block is marked when its first words are the tag.
 */
function marksIn(path, source) {
  const blocks = []
  for (const [line, text] of commentsIn(path, source)) {
    const last = blocks.at(-1)
    const lines = text.split('\n').length
    if (last !== undefined && last.line + last.lines === line) {
      last.text += `\n${text}`
      last.lines += lines
      continue
    }
    blocks.push({ line, text, lines })
  }
  return blocks.filter(block => commentBody(block.text).startsWith(WORKAROUND))
}

/**
 * The words of a comment, with its marker and any doc marker removed: the tag
 * convention's predicate, kept here rather than in `comments.mjs`, which only lexes.
 */
function commentBody(text) {
  return text
    .replace(/^\/\*+/u, '')
    .replace(/\*+\/$/u, '')
    .replace(/^\/\/[/!]?/u, '')
    .replace(/^[\s*]+/u, '')
}

/** Paths into `crates/` naming a Rust file, as written inside a comment. */
const PROBE_PATH = /crates\/[A-Za-z0-9_./-]+\.rs/gu

/**
 * Whether a test file has a test that will run. Matches the `#[ignore]` attribute,
 * directly or through `cfg_attr`, never the word, which prose uses constantly.
 */
function runnableTests(source) {
  const lines = source.split('\n').map(one => one.trim())
  const tests = lines.filter(one => /^#\[\s*test\s*\]/u.test(one)).length
  const ignored = lines.filter(one => /^#\[\s*ignore\b/u.test(one) || /^#\[\s*cfg_attr\b.*\bignore\s*[),]/u.test(one)).length
  return { tests, ignored }
}

/** Whether the crate owning a `tests/` file still discovers it automatically. */
function autoDiscovered(path) {
  const manifest = join(dirname(dirname(path)), 'Cargo.toml')
  if (!existsSync(manifest)) return `has no Cargo.toml at ${manifest}`
  const source = readFileSync(manifest, 'utf8')
  if (/^\s*autotests\s*=\s*false/mu.test(source)) return 'is in a crate whose Cargo.toml sets `autotests = false`'
  if (/^\s*\[\[test\]\]/mu.test(source)) return 'is in a crate whose Cargo.toml declares `[[test]]` targets'
  return null
}

probeScan()

const sources = trackedFiles()

const faults = []
let sites = 0
let probes = 0
for (const file of sources) {
  const marks = marksIn(file, readFileSync(file, 'utf8'))
  sites += marks.length
  const named = marks.filter(mark => mark.text.match(PROBE_PATH) !== null).length
  for (const mark of marks) {
    const paths = [...new Set(mark.text.match(PROBE_PATH) ?? [])]
    const where = `${file}:${mark.line}`
    if (paths.length === 0) {
      // A site that defers: one compensation reaches several places, and the
      // one carrying the explanation is the one that carries the reference.
      // Deferring to a site in the same file is the shape that already exists;
      // deferring to nothing is a site whose probe nobody can find.
      if (named === 0) faults.push(`${where} names no probe, and no site in this file names one`)
      continue
    }
    for (const path of paths) {
      probes += 1
      if (!existsSync(path)) {
        faults.push(`${where} names \`${path}\`, which does not exist`)
        continue
      }
      // `posix` rather than the platform separator: the path is written in a
      // comment, where it is always spelled with forward slashes.
      const parts = path.split(posix.sep)
      if (parts.length !== 4 || parts[0] !== 'crates' || parts[2] !== 'tests') {
        faults.push(`${where} names \`${path}\`, which is not a \`crates/<crate>/tests/<name>.rs\` target`)
        continue
      }
      const undiscovered = autoDiscovered(path)
      if (undiscovered !== null) {
        faults.push(`${where} names \`${path}\`, which ${undiscovered}`)
        continue
      }
      const probe = readFileSync(path, 'utf8')
      const { tests, ignored } = runnableTests(probe)
      if (tests === 0) faults.push(`${where} names \`${path}\`, which has no \`#[test]\``)
      if (ignored > 0) {
        faults.push(`${where} names \`${path}\`, where ${ignored} test${ignored === 1 ? ' is' : 's are'} \`#[ignore]\`d and assert nothing`)
      }
      const foundations = commentsIn(path, probe).filter(([, text]) => commentBody(text).startsWith(FOUNDATION)).length
      if (foundations === 0) {
        faults.push(
          `${where} names \`${path}\`, which marks no test \`${FOUNDATION}\` -- nothing there says ` + 'which row pins the property this compensation rests on',
        )
      }
    }
  }
}

if (faults.length > 0) {
  for (const fault of faults) process.stdout.write(`  ${fault}\n`)
  process.stderr.write(
    `\n${faults.length} marked site${faults.length === 1 ? '' : 's'} without a live probe behind ` +
      'it. A compensation whose probe has gone is one that cannot be retired without re-deriving ' +
      'it, and the day the dependency is fixed nothing goes red. AGENTS.md, "Working around an ' +
      'upstream defect".\n',
  )
  process.exit(1)
}

if (sites < SITES_FLOOR) {
  process.stderr.write(
    `\nNo \`${WORKAROUND}\` anywhere in ${sources.length} files. Either the last compensation was ` +
      'retired, which is worth a sentence in the commit that did it and a lower floor here, or this ' +
      'is reading the wrong set of files and every site went unexamined.\n',
  )
  process.exit(1)
}

process.stdout.write(
  `${sites} marked sites across ${sources.length} files, ${probes} naming a probe that runs ` +
    `and marks its ${FOUNDATION} row. Whether that row pins the right property is not checked here.\n`,
)
