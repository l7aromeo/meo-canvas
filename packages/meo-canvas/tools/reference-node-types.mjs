// Puts `/// <reference types="node" />` into the declarations that name `Buffer`,
// which a consumer without `"types": ["node"]` would otherwise see as `any`, hidden
// by `skipLibCheck`. `tsc` drops the reference from declaration emit, so it is added
// after `tsc`; `verify-package.mjs`'s control fails to compile if this stops working.

import { readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/** The emitted declarations, beside this package rather than the caller's cwd. */
const DIST = resolve(dirname(fileURLToPath(import.meta.url)), '../dist')

/** The directive, exactly as TypeScript writes it. */
const REFERENCE = '/// <reference types="node" />'

/**
 * Whether a declaration names a type only `@types/node` supplies, in a type position
 * -- `Buffer` also appears in prose. `RequestInit` is absent: the DOM library a
 * consumer's `target` pulls in supplies it.
 */
function needsNodeTypes(source) {
  return /(?:^|[^\w$.])Buffer\s*(?:[|)>,;\]]|$)/m.test(source.replaceAll(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/g, ''))
}

const declarations = readdirSync(DIST).filter(name => name.endsWith('.d.ts'))
const carrying = []

for (const name of declarations) {
  const path = join(DIST, name)
  const source = readFileSync(path, 'utf8')
  if (source.startsWith(REFERENCE)) {
    carrying.push(name)
    continue
  }
  if (!needsNodeTypes(source)) continue
  writeFileSync(path, `${REFERENCE}\n${source}`)
  carrying.push(name)
}

// **The invariant is that a declaration carries the reference, not that this
// file wrote one.** A future TypeScript that stops eliding the directive would
// leave nothing to do here, and a silent no-op is indistinguishable from a
// rename that made the search miss. Asserting the finished state covers both.
if (carrying.length === 0) {
  process.stderr.write(
    `no emitted declaration in ${DIST} names a Node global, so none carries ${REFERENCE}.\n` +
      `Either the type was removed -- in which case delete this tool and the control in verify-package.mjs -- ` +
      `or the search in \`needsNodeTypes\` no longer matches how it is written.\n`,
  )
  process.exit(1)
}

process.stderr.write(`${carrying.join(', ')} ${carrying.length === 1 ? 'carries' : 'carry'} ${REFERENCE}\n`)
