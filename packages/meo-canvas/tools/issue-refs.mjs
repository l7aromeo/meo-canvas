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
// **It reads comments, not files**, through the lexer in `comments.mjs`, which
// carries the reasoning for that and the fixtures proving it. A scheduled
// watcher needs the same references for a different question, and two
// extractors over the same globs is the arrangement whose failure mode is that
// they differ — demonstrably, since a regex written for the second returned
// this file's own self-test literals as references to follow.
//
// **The literals are named in the fixtures and not here, and that is not
// squeamishness.** This file is read by the check it defines, so an example
// written in this comment is a violation of the rule it illustrates. The first
// version of this header held four, and they were invisible until the file was
// committed: the scan lists files from git, so while it was untracked it did
// not read itself.
//
// **What it does not do.** It lexes rather than parses the Rust: line comments,
// block comments, ordinary strings and raw strings, which is every form this
// tree uses. It does not see a reference built at runtime, or one inside a
// generated file — `target/`, `dist/` and `node_modules/` are never read,
import { readFileSync } from 'node:fs'

import { REFERENCE, SCOPE, commentsIn, trackedFiles, verifyLexers } from './comments.mjs'

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
const files = trackedFiles()

// Before anything is read from the tree: a scan that cannot see a reference
// reports a clean tree, and it reports it in exactly the words a clean tree
// earns.
verifyLexers()

const bare = []
let qualified = 0
for (const file of files) {
  const source = readFileSync(file, 'utf8')
  const comments = commentsIn(file, source)
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
