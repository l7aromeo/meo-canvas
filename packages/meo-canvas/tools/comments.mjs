// The comments of this tree's source, and the references inside them.
//
// **One lexer, because the failure mode of two is that they differ.** This was
// `issue-refs.mjs`'s alone until a scheduled watcher needed the same
// references for a different question -- what upstream has done about them --
// and a second extractor written to answer it returned the fixture literals
// below as if they were references to follow, because a regex over the same
// globs is not the same instrument as a lexer that knows a string from a
// comment. The two disagree exactly where it matters, and the fixtures are
// where that disagreement is visible.
//
// **It reads comments, not files.** Short hex colours in `color.rs`'s tables,
// the arena's byte-string inputs, and the malformed colours `unit.rs` requires
// a parser to reject all look like references and are none of them: every one
// sits in a string literal. Excluding them by their spelling would be a rule
// about how a colour looks, which is the fault this exists to avoid -- the
// property is *a reader could follow this*, and only comment text poses it.
//
// **The literals are named in the fixtures and not in this header.** The scan
// these feed reads this file like any other, so an example written in prose
// here is a violation of the rule it illustrates.
//
// **What it does not do.** It lexes rather than parses: line comments, block
// comments, ordinary strings and raw strings, which is every form this tree
// uses. It does not see a reference built at runtime, or one inside a
// generated file -- `target/`, `dist/` and `node_modules/` are never listed,
// because a generated reference is the generator's to fix.
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
export function verifyLexers() {
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
 * The same reference written as a link, which the pattern above cannot see.
 *
 * A tracker with no `owner/repo#n` form takes a full URL, and GitHub's own
 * issues are written that way here where a sentence wanted a link rather than
 * a token. `taffy`'s 1151 and 1163 appear in this tree **only** in this form,
 * so a consumer reading the qualified pattern alone watches neither and
 * reports a clean week for two it never asked about.
 *
 * Kept separate from {@link REFERENCE} rather than merged into one pattern:
 * they fail in different ways, and a merged regex is hard to show correct.
 *
 * `issue-refs` does not enforce anything about this form -- that is
 * l7aromeo/meo-canvas#119. It is carried here because the watcher needs it as
 * input.
 */
export const REFERENCE_URL = /https:\/\/github\.com\/(?<qualified>[\w.-]+\/[\w.-]+)\/(?:issues|pull)\/(?<number>\d{1,5})\b/gu

/**
 * The comments of one file, through whichever lexer its kind takes.
 *
 * **The choice lives here and not in each caller.** Which lexer a `.toml`
 * takes is exactly the kind of decision two callers make differently, and two
 * callers disagreeing about it is a category silently unread on one side.
 */
export function commentsIn(path, source) {
  const hash = path.endsWith('justfile') || path.endsWith('.toml') || path.endsWith('.yml')
  return hash ? hashCommentsOf(source) : commentsOf(source)
}

/**
 * Every file in {@link SCOPE}, tracked or not yet committed.
 *
 * `--others --exclude-standard` as well as the tracked set: a file added and
 * not yet committed is exactly the file most likely to carry a new reference,
 * and listing only what git already knows about is how the check that reads
 * this first passed on a tree containing its own violations.
 *
 * **The pathspecs come from `SCOPE` rather than beside it.** They were a second
 * copy of the same seven globs, which is the arrangement where one list gains a
 * kind and the other does not -- and the half that drifts is whichever one the
 * next person does not have open.
 */
export function trackedFiles() {
  return execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', ...SCOPE.map(([glob]) => glob)], {
    encoding: 'utf8',
  })
    .split('\n')
    .filter(one => one !== '')
}

/** The comment text of a JavaScript-family or Rust source, as `[line, text]`. */
export function commentsOf(source) {
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
 * **Quote-aware, and that is what a trailing comment needs.** A qualified
 * reference in a trailing `#` comment is found and `name = "a#1"` is not, and
 * the difference is whether the `#` is inside a string. The example is
 * described rather than spelled: a followable reference written in prose here
 * is one a consumer of this module will go and query, and a fixture repository
 * answers 404 -- which is the shape of a renamed repository and has to stay
 * loud. TOML, YAML and `just` all agree on that
 * rule and on both quote characters, which is why one function serves the
 * three. A quote opened and never closed on a line ends at the newline, which
 * is what those languages do with an unterminated string anyway.
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
