// The comments of this tree's source, and the references inside them -- one lexer
// shared by every caller, since a regex over the same globs reads string literals
// as references. It lexes line, block, ordinary and raw strings; it does not see a
// reference built at runtime or in a generated file. Examples live in the fixtures.
import { execFileSync } from 'node:child_process'

/**
 * One entry per glob below, and each must match at least one file: zero means that
 * kind is not being read, where a total hides one glob collapsing. `*.ts` and
 * `*.mts` are separate entries, since a `*.ts` pathspec does not match `.mts`.
 */
export const SCOPE = [
  ['*.rs', file => file.endsWith('.rs')],
  ['*.ts', file => file.endsWith('.ts')],
  ['*.mjs', file => file.endsWith('.mjs')],
  ['*.mts', file => file.endsWith('.mts')],
  ['*.toml', file => file.endsWith('.toml')],
  ['justfile', file => file === 'justfile' || file.endsWith('/justfile')],
  ['.github/workflows/*.yml', file => file.startsWith('.github/workflows/')],
]

/**
 * The scan reading no comments at all, caught by running both lexers over sources
 * written here, whose answers cannot change with the tree. They use `n/n` as the
 * repository, so the qualified example refers to nothing.
 */
export function verifyLexers() {
  const rust = 'fn a(x: &\'static str) {\n    let c = \'"\';\n    // see n/n#1 and #2\n    let s = "n/n#3";\n}\n'
  const hash = 'a = 1 # see n/n#4 and #5\nb = "n/n#6"\n'
  const seen = kind => {
    const comments = kind === 'rust' ? commentsOf(rust, { rust: true }) : hashCommentsOf(hash)
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
      throw new Error(
        `the ${kind} lexer found [${got.join(', ')}] in a source written to contain ` +
          `[${want[kind].join(', ')}]. A scan built on it reports a clean tree while reading ` +
          'nothing. Fix the lexer rather than this fixture.',
      )
    }
  }
}

/** A reference someone could follow: a hash, then one to five decimal digits. */
export const REFERENCE = /(?<!\w)(?<qualified>[\w.-]+\/[\w.-]+)?#(?<number>\d{1,5})(?!\w)/gu

/**
 * The same reference written as a link, which {@link REFERENCE} cannot see --
 * taffy's 1151 and 1163 appear here only in this form. Kept separate: the two fail
 * differently. `issue-refs` enforces nothing about it (l7aromeo/meo-canvas#119).
 */
export const REFERENCE_URL = /https:\/\/github\.com\/(?<qualified>[\w.-]+\/[\w.-]+)\/(?:issues|pull)\/(?<number>\d{1,5})\b/gu

/**
 * The comments of one file, through whichever lexer its kind takes -- decided here
 * once, so two callers cannot choose differently.
 */
export function commentsIn(path, source) {
  const hash = path.endsWith('justfile') || path.endsWith('.toml') || path.endsWith('.yml')
  return hash ? hashCommentsOf(source) : commentsOf(source, { rust: path.endsWith('.rs') })
}

/**
 * Every file in {@link SCOPE}, tracked or not yet committed, since a new file is
 * the one most likely to carry a new reference. The pathspecs come from `SCOPE`.
 */
export function trackedFiles() {
  return execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', ...SCOPE.map(([glob]) => glob)], {
    encoding: 'utf8',
  })
    .split('\n')
    .filter(one => one !== '')
}

/**
 * The comment text of a JavaScript-family or Rust source, as `[line, text]`: the
 * line a comment ends on, and its text without the closing delimiter or newline. Pass
 * `rust: true` for a `.rs` file, where `'` opens a string only as a char literal.
 */
export function commentsOf(source, { rust = false } = {}) {
  return scan(source, rust).comments
}

/**
 * Where each comment and string literal in a source starts and ends, as
 * `{ kind, start, end }` with `end` past the closing delimiter: what a parser needs
 * to read code while skipping the braces and text inside both.
 */
export function spansOf(source, { rust = false } = {}) {
  return scan(source, rust).spans
}

/** One pass that both {@link commentsOf} and {@link spansOf} read. */
function scan(source, rust) {
  const comments = []
  const spans = []
  let line = 1
  let at = 0
  const state = { block: false, lineComment: false, string: null, raw: 0 }
  let start = 0
  const flush = (end, close) => {
    if (end > start) comments.push([line, source.slice(start, end)])
    spans.push({ kind: 'comment', start, end: close })
  }
  const closeString = end => {
    spans.push({ kind: 'string', start, end })
    state.string = null
    state.raw = 0
  }
  while (at < source.length) {
    const two = source.slice(at, at + 2)
    if (state.lineComment) {
      if (source[at] === '\n') {
        flush(at, at)
        state.lineComment = false
      }
    } else if (state.block) {
      if (two === '*/') {
        flush(at, at + 2)
        state.block = false
        at += 1
      }
    } else if (state.string !== null) {
      if (state.raw > 0) {
        if (source[at] === '"' && source.slice(at + 1, at + 1 + state.raw) === '#'.repeat(state.raw)) {
          closeString(at + 1 + state.raw)
        }
      } else if (source[at] === '\\') at += 1
      else if (source[at] === state.string) closeString(at + 1)
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
        start = at
        at += 1 + hashes
      }
    } else if (source[at] === "'" && rust && source[at + 1] !== '\\' && source[at + 2] !== "'") {
      // A Rust lifetime such as `'a` or `'static`, not a char literal.
    } else if (source[at] === '"' || source[at] === "'" || source[at] === '`') {
      state.string = source[at]
      start = at
    }
    if (source[at] === '\n') line += 1
    at += 1
  }
  if (state.lineComment || state.block) flush(source.length, source.length)
  else if (state.string !== null) closeString(source.length)
  return { comments, spans }
}

/**
 * `#` comments, returning the text after the first `#` so a comment opening with a
 * number is not a bare reference; a comment that is only `#` and digits is unseen.
 * Quote-aware, so a `#` inside a string is not a comment. TOML, YAML and `just`
 * share that rule; an unclosed quote ends at the newline.
 */
export function hashCommentsOf(source) {
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
