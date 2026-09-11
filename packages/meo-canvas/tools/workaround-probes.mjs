// Every `[WORKAROUND]` marks code that still has a live probe behind it.
//
// A compensation for an upstream defect is two halves: the tag, which makes it
// greppable, and a probe pinning what the dependency does today, so the release
// that fixes it turns the probe red and the red is the notification. AGENTS.md's
// "Working around an upstream defect" is the convention; this is the check it
// was written for.
//
// **A probe rots in exactly one direction and nothing reports it.** Rename the
// test file and the tag points at a path that is not there. Mark a test
// `#[ignore]` and it is still a target, still compiles, and asserts nothing.
// Delete the row that pins the property the compensation *depends* on and the
// file still fails the day the dependency is fixed, so it still looks like a
// probe -- while the thing that would break the compensation quietly went
// unpinned. Each of those leaves a green gate and a workaround nobody can retire
// without re-deriving it.
//
// So, for each marked site: it names a probe, the probe is a real test target,
// the target has a test that actually runs, and the target carries at least one
// test marked `[FOUNDATION]`.
//
// **`[FOUNDATION]` is a marker, and this sees the marker.** It cannot read
// whether the test beneath it pins a property the compensation depends on, or
// whether it pins anything at all -- a `[FOUNDATION]` above an assertion that
// two plus two is four passes here exactly as the real one does. What the check
// buys is that the question was answered once, deliberately, in a place a
// reader lands: a probe with no marked test is one where nobody has said which
// row is the foundation, and that is the half of the convention that is easy to
// leave out. `taffy_negative_margin.rs` carries no marker and is not required
// to: it compensates nothing, so nothing rests on it.
//
// **And it sees marks rather than compensations: a site that stops being one
// is invisible here.** Measured by deleting the tag from a site that named a
// probe, leaving the code and the other sites alone -- four marked sites became
// three, three naming a probe became two, and the run exited 0. `SITES_FLOOR`
// is 1, so the count can fall that far before anything speaks, and the only
// witness is a summary line nobody diffs. The compensation is still there and
// is no longer greppable, no longer examined here, and free to rot afterwards
// through any of the three failures above with this green throughout. The same
// three breaks, measured on the same tree, each exit 1: a probe naming no
// `[FOUNDATION]` row, a path that does not exist, and a test marked
// `#[ignore]`. A reader who has watched those go red would reasonably expect
// the fourth to, and it does not.
//
// **Marks and mentions are told apart by position, not presence.** The tag
// appears nine times in this tree and marks code three times; the others are
// the convention being described in prose, in a `//!` header and inside an
// assertion string. A mark is a comment whose *first* words are the tag.
//
// **Every kind `comments.mjs` reads is read here, not only Rust.** A
// compensation is not a Rust-only thing -- a tool working around a formatter's
// defect or a workflow working around a runner's is the same object with the
// same retirement problem -- and scanning `*.rs` alone would report a clean
// tree while the tag sat unread in a `justfile`. `trackedFiles` and
// `commentsIn` come from `comments.mjs` so the file list and the choice of
// lexer are one decision rather than two, which is the arrangement where one
// side quietly stops reading a kind.
//
// **It reads itself, and that is why the tag is spelled in one place.**
// `WORKAROUND` below is a string, so no comment here opens with the tag and
// this file marks nothing. Writing an example of a marked comment in this
// header would make this tool report itself, which is a fault `issue-refs`
// found in its own first version.
import { readFileSync, existsSync } from 'node:fs'
import { dirname, join, posix } from 'node:path'
import { commentsIn, trackedFiles } from './comments.mjs'

/** The tag on a compensation, at the head of the comment that marks it. */
const WORKAROUND = '[WORKAROUND]'

/** The tag on the row a compensation rests on, inside its probe. */
const FOUNDATION = '[FOUNDATION]'

/**
 * How few marked sites may be found before this asks whether it was meant.
 *
 * **One, and the magnitude is the argument.** Three sites carry the tag today,
 * and a floor near three would be a pinned list wearing a threshold: a
 * compensation reaching one site fewer after a refactor is ordinary, and having
 * to edit a constant for it trains the next person to edit the constant. Zero
 * is the number that means something else -- either the scan reads nothing, or
 * this repository has no workarounds at all, and the second is a fact worth one
 * sentence in a commit message.
 *
 * That second state is a claim about a future rather than an imminent one:
 * `DioxusLabs/taffy#804`, which both of today's compensations name, is open.
 *
 * The blindness this number guards against is guarded better above it:
 * [`probeScan`] runs the mark-finder over sources written here, so a lexer that
 * stopped seeing comments fails against fixtures whose answers cannot change
 * when the tree does. The floor is what is left after that -- the case where
 * the lexer works and the tree is empty.
 */
const SITES_FLOOR = 1

/**
 * The mark-finder, over sources whose answers cannot change with the tree.
 *
 * **Two fixtures, because two lexers reach this.** The Rust one holds three
 * occurrences of the tag and one mark -- a `//!` header describing the
 * convention, the mark, and one inside a string literal -- which is this tree's
 * own ratio in miniature. The `#`-comment one holds two and one mark, since a
 * `justfile` or a manifest can carry a compensation and takes the other lexer
 * entirely.
 *
 * A finder that returns every occurrence has stopped telling marks from
 * mentions; one that returns none has stopped reading comments and would report
 * every site sound by finding no sites at all.
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
 * The marked comment blocks of one source, as `{ line, text }`.
 *
 * **Blocks, because a mark is one comment and its site is several.** Both
 * lexers return a line at a time, and the reference naming the probe is three
 * or four lines below the tag every time it is written. So consecutive comments
 * are joined, and a block is marked when the block's first words are the tag.
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
 * The words of a comment, with its marker and any doc marker removed.
 *
 * **This is what tells a mark from a mention**, and it lives here rather than
 * in `comments.mjs` because it is the tag convention's predicate and not
 * lexing: that module answers *what is a comment*, and this answers *does this
 * comment open with the tag*. `hashCommentsOf` has already dropped the `#`, so
 * what is left to strip is Rust's markers and the leading space both leave.
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
 * Whether a test file has a test that will actually run.
 *
 * **The attribute, never the word.** `#[ignore]` is what makes a test compile
 * and assert nothing; the word appears in this tree's prose constantly --
 * `.gitignore`, "ignores", a comment about what a stage ignores -- and matching
 * it would refuse a sound probe for its documentation. `cfg_attr` reaches the
 * same place by a longer road and is matched too.
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
