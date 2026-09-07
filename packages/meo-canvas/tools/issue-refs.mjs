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

/** How many qualified references the tree holds today. */
const FLOOR = 2

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

/** `#` comments, for a file whose whole comment syntax is one character. */
function hashCommentsOf(source) {
  return source
    .split('\n')
    .map((text, index) => [index + 1, text])
    .filter(([, text]) => /^\s*#/u.test(text))
}

// `--others --exclude-standard` as well as the tracked set: a file added and not
// yet committed is exactly the file most likely to carry a new reference, and
// listing only what git already knows about is how this check first passed on a
// tree containing its own violations.
const files = execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', '*.rs', '*.ts', '*.mjs', '*.mts', 'justfile'], {
  encoding: 'utf8',
})
  .split('\n')
  .filter(one => one !== '')

const bare = []
let qualified = 0
for (const file of files) {
  const source = readFileSync(file, 'utf8')
  const comments = file.endsWith('justfile') ? hashCommentsOf(source) : commentsOf(source)
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

if (qualified < FLOOR) {
  process.stderr.write(
    `\nFound ${qualified} qualified references and expected at least ${FLOOR}. Either they were ` +
      'reworded, or this stopped recognising them -- and the second is what a green with nothing ' +
      'matched looks like. Change FLOOR deliberately.\n',
  )
  process.exit(1)
}

process.stdout.write(`${qualified} references, every one naming its repository, across ${files.length} files.\n`)
