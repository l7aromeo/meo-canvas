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
// **It reads comments, not files.** `#0008` and `#808080` are colours in test
// tables, `"#1122 33 44"` is arena input, and `["#12", "#12345"]` are strings a
// parser must reject — none is a reference, and none is in a comment. Excluding
// them by their spelling would be a rule about how a colour looks, which is the
// fault this file exists to avoid: the property is *a reader could follow this*,
// and only comment text is read.
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

/** A reference someone could follow: `#12`, but not `#1a2b` and not `#123456`. */
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

const files = execFileSync('git', ['ls-files', '*.rs', '*.ts', '*.mjs', '*.mts', 'justfile'], {
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
