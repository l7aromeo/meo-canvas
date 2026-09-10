// A reference in a comment names the repository it lives in.
//
// **A bare `#N` resolves, and that is what makes it worse than a dead link.**
// Three in this tree pointed at merged pull requests about other things: a
// comment on corner marks sent a reader to a release, and one on probe values
// to a backdrop-filter fix. A broken link announces itself; a real page about
// something else does not, and nothing in it says it is the wrong one.
//
// So a reference is written `owner/repo#N`, or as words. Two comments already
// used the qualified form before this existed, and they are the only two that
// could not go wrong.
//
// **It reads comments, not files.** Short hex colours in `color.rs`'s tables,
// the arena's byte-string inputs, and the malformed colours `unit.rs` requires a
// parser to reject all look like references and are none of them: every one sits
// in a string literal. Excluding them by their spelling would be a rule about how
// a colour looks, which is the fault this file exists to avoid — the property is
// *a reader could follow this*, and only comment text poses it.
//
// **The literals are named there and not here, and that is not squeamishness.**
// This file is read by the check it defines, so an example written in this
// comment is a violation of the rule it illustrates. The first version of this
// header held four, and they were invisible until the file was committed: the
// scan lists files from git, so while it was untracked it did not read itself.
//
// **What it does not do.** It lexes rather than parses the Rust: line comments,
// block comments, ordinary strings and raw strings, which is every form this
// tree uses. It does not see a reference built at runtime, or one inside a
// generated file — `target/`, `dist/` and `node_modules/` are never read,
// because a generated reference is the generator's to fix.
import { readFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'

/**
 * One entry per glob below, and each must match at least one file.
 *
 * **A total was the wrong shape and the magnitude is what gave it away.** The
 * first attempt floored the file count at 200 against 226, which catches every
 * glob collapsing at once and catches nothing else: dropping `*.toml` alone
 * takes 226 to about 221, the tree passes, the reference list still looks long
 * and healthy, and the manifest gap this tool was widened to close is silently
 * back. The same objection retired the count it replaced -- twenty-three
 * notches of slack, then twenty-six.
 *
 * Per glob, the number justifies itself: **one, because zero means that kind is
 * not being read.** No slack to drift, nothing to re-tune as the tree grows,
 * and a category disappearing is caught by the guard for that category rather
 * than by a total having to fall past a threshold.
 *
 * The predicates mirror the globs one for one rather than by convenience --
 * `*.ts` and `*.mts` are two globs and two entries, because a pathspec of
 * `*.ts` does not match a `.mts` file and grouping them would let either
 * vanish behind the other.
 */
const SCOPE = [
  ['*.rs', file => file.endsWith('.rs')],
  ['*.ts', file => file.endsWith('.ts')],
  ['*.mjs', file => file.endsWith('.mjs')],
  ['*.mts', file => file.endsWith('.mts')],
  ['*.toml', file => file.endsWith('.toml')],
  ['justfile', file => file === 'justfile' || file.endsWith('/justfile')],
  ['.github/workflows/*.yml', file => file.startsWith('.github/workflows/')],
]

/**
 * The scan reading no comments at all, caught without reference to the tree.
 *
 * **A file count cannot see this and a reference count sees it too late.** If
 * `commentsOf` returns nothing -- a lexer edit, a state machine that never
 * leaves a string -- the file list is still long and the tree still passes with
 * nothing examined. So both lexers are run over a source written here, whose
 * answer cannot change when the tree does, and which contains exactly what the
 * scan is looking for.
 *
 * The fixtures use `n/n` as the repository, so the qualified example is a
 * reference to nothing rather than a reference this file would then have to
 * live up to -- this tool reads itself, which is why the header carries no
 * examples either.
 */
function probeLexers() {
  const rust = 'fn a() {\n    // see n/n#1 and #2\n    let s = "n/n#3";\n}\n'
  const hash = 'a = 1 # see n/n#4 and #5\nb = "n/n#6"\n'
  const seen = kind => {
    const comments = kind === 'rust' ? commentsOf(rust) : hashCommentsOf(hash)
    const numbers = []
    for (const [, text] of comments) {
      for (const match of text.matchAll(REFERENCE)) {
        numbers.push(Number(match.groups?.['number']))
      }
    }
    return numbers.sort((one, two) => one - two)
  }
  const want = { rust: [1, 2], hash: [4, 5] }
  for (const kind of ['rust', 'hash']) {
    const got = seen(kind)
    const same = got.length === want[kind].length && got.every((one, at) => one === want[kind][at])
    if (!same) {
      process.stderr.write(
        `\nThe ${kind} lexer found [${got.join(', ')}] in a source written to contain ` +
          `[${want[kind].join(', ')}]. The scan below would report a clean tree while reading ` +
          'nothing. Fix the lexer rather than this fixture.\n',
      )
      process.exit(1)
    }
  }
}

/** A reference someone could follow: a hash, then one to five decimal digits. */
const REFERENCE = /(?<!\w)(?<qualified>[\w.-]+\/[\w.-]+)?#(?<number>\d{1,5})(?!\w)/gu

/** The comment text of a JavaScript-family or Rust source, as `[line, text]`. */
function commentsOf(source) {
  const found = []
  let line = 1
  let at = 0
  const state = { block: false, lineComment: false, string: null, raw: 0 }
  let start = 0
  const flush = end => {
    if (end > start) found.push([line, source.slice(start, end)])
  }
  while (at < source.length) {
    const two = source.slice(at, at + 2)
    if (state.lineComment) {
      if (source[at] === '\n') {
        flush(at)
        state.lineComment = false
      }
    } else if (state.block) {
      if (two === '*/') {
        flush(at)
        state.block = false
        at += 1
      }
    } else if (state.string !== null) {
      if (state.raw > 0) {
        if (source[at] === '"' && source.slice(at + 1, at + 1 + state.raw) === '#'.repeat(state.raw)) {
          state.string = null
          state.raw = 0
        }
      } else if (source[at] === '\\') at += 1
      else if (source[at] === state.string) state.string = null
    } else if (two === '//') {
      state.lineComment = true
      start = at
      at += 1
    } else if (two === '/*') {
      state.block = true
      start = at
      at += 1
    } else if (source[at] === 'r' && /[^\w]/u.test(source[at - 1] ?? ' ')) {
      // A Rust raw string: `r"…"`, `r#"…"#`, `r##"…"##`.
      let hashes = 0
      while (source[at + 1 + hashes] === '#') hashes += 1
      if (source[at + 1 + hashes] === '"') {
        state.string = '"'
        state.raw = hashes
        at += 1 + hashes
      }
    } else if (source[at] === '"' || source[at] === "'" || source[at] === '`') {
      state.string = source[at]
    }
    if (source[at] === '\n') line += 1
    at += 1
  }
  if (state.lineComment || state.block) flush(source.length)
  return found
}

/**
 * `#` comments, for a file whose whole comment syntax is one character.
 *
 * **The marker and the reference are the same character**, so what is returned
 * is the text *after* the first `#` rather than the line. Otherwise a comment
 * whose words begin with a number matches as a bare reference, and the tool
 * reports a violation in prose that names no issue at all. The cost is stated
 * rather than hidden: a reference cannot be the marker itself, so a line whose
 * entire content is the marker followed immediately by digits is invisible
 * here. Nothing writes one -- a comment
 * with no words is not a comment -- and the alternative is a false positive on
 * every numbered remark in the tree.
 *
 * **Quote-aware, and that is what a trailing comment needs.** `edition = 2024
 * # owner/repo#1` is a comment and `name = "a#1"` is not, and the difference is
 * whether the `#` is inside a string. TOML, YAML and `just` all agree on that
 * rule and on both quote characters, which is why one function serves the
 * three. A quote opened and never closed on a line ends at the newline, which
 * is what those languages do with an unterminated string anyway.
 */
function hashCommentsOf(source) {
  const found = []
  const lines = source.split('\n')
  for (const [index, text] of lines.entries()) {
    let quote = null
    for (let at = 0; at < text.length; at += 1) {
      const here = text[at]
      if (quote !== null) {
        if (here === quote) quote = null
        continue
      }
      if (here === '"' || here === "'") {
        quote = here
        continue
      }
      if (here === '#') {
        found.push([index + 1, text.slice(at + 1)])
        break
      }
    }
  }
  return found
}

// `--others --exclude-standard` as well as the tracked set: a file added and not
// yet committed is exactly the file most likely to carry a new reference, and
// listing only what git already knows about is how this check first passed on a
// tree containing its own violations.
// **Manifests and workflows are in, and they were the gap.** A bare reference
// in a `Cargo.toml` comment ships exactly as silently as one in a source file,
// and `.github/workflows/ci.yml` carried one. Both are `#`-comment languages,
// so they cost a glob rather than a lexer.
//
// **What is deliberately out.** `package.json` and every other JSON manifest,
// because JSON has no comments and a `#N` in one is data. `*.md`, because prose
// is where a reference belongs and a Markdown link carries its own repository.
// `.mts`, `.ts`, `.rs` and `justfile` were already in.
const files = execFileSync(
  'git',
  ['ls-files', '--cached', '--others', '--exclude-standard', '*.rs', '*.ts', '*.mjs', '*.mts', '*.toml', 'justfile', '.github/workflows/*.yml'],
  { encoding: 'utf8' },
)
  .split('\n')
  .filter(one => one !== '')

// Before anything is read from the tree: a scan that cannot see a reference
// reports a clean tree, and it reports it in exactly the words a clean tree
// earns.
probeLexers()

const bare = []
let qualified = 0
for (const file of files) {
  const source = readFileSync(file, 'utf8')
  const hash = file.endsWith('justfile') || file.endsWith('.toml') || file.endsWith('.yml')
  const comments = hash ? hashCommentsOf(source) : commentsOf(source)
  for (const [line, text] of comments) {
    for (const match of text.matchAll(REFERENCE)) {
      if (match.groups?.['qualified'] !== undefined) qualified += 1
      else bare.push(`${file}:${line} #${match.groups?.['number']}`)
    }
  }
}

if (bare.length > 0) {
  for (const one of bare) process.stdout.write(`  names no repository: ${one}\n`)
  process.stderr.write(
    `\n${bare.length} reference${bare.length === 1 ? '' : 's'} a reader would follow into this ` +
      'repository, whichever repository they meant. Write `owner/repo#N`, or say it in words if the ' +
      'number is not recoverable.\n',
  )
  process.exit(1)
}

const empty = SCOPE.filter(([, matches]) => !files.some(file => matches(file)))
if (empty.length > 0) {
  process.stderr.write(
    `\n${empty.map(([glob]) => glob).join(', ')} matched no file. A kind that is not read is a ` +
      'kind whose references are not checked, and the tree still passes with a long list from the ' +
      'globs that survived. Restore the glob, or delete its entry from SCOPE deliberately.\n',
  )
  process.exit(1)
}

process.stdout.write(`${qualified} references, every one naming its repository, across ${files.length} files.\n`)
