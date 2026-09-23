// A reference in a comment names its repository: `owner/repo#N`, or words. A bare
// `#N` resolves to whatever this repository numbered N, a real page about something
// else. Comments are read through `comments.mjs`'s lexer, whose fixtures hold the
// examples, since this file is scanned too.
import { readFileSync } from 'node:fs'

import { REFERENCE, SCOPE, commentsIn, trackedFiles, verifyLexers } from './comments.mjs'

// Tracked and untracked-but-not-ignored files, since a new file is the likeliest to
// carry a new reference. Manifests and workflows are `#`-comment files and are in;
// JSON has no comments and Markdown links carry their repository, so both are out.
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
