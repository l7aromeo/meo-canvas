// Puts `/// <reference types="node" />` into the declarations that need it.
//
// **`Buffer` is the only Node global this package names in a type position, and
// a consumer's compiler cannot resolve it.** TypeScript 6 does not auto-include
// `node_modules/@types`, so `Buffer` is an unresolved name for anyone who has
// not written `"types": ["node"]` -- it becomes `any`, and `skipLibCheck`,
// which `tsc --init` writes as `true`, hides the error that would have said so.
// A `Promise<Buffer>` that arrives as `Promise<any>` is worse than a wrong type
// because it is a type the consumer's own compiler will not argue with.
//
// A reference is followed transitively and fixes it. It cannot be written in
// the source: `tsc` elides a triple-slash type reference from declaration emit
// -- it reaches `dist/canvas.js` and never `dist/canvas.d.ts` -- and
// `import type { Buffer } from 'node:buffer'`, which does survive emit, does
// not resolve either, because a bare `node:` specifier needs `@types/node`
// already loaded. So it is added here, after `tsc` and before anything is
// packed. `src/canvas.ts` carries the same argument next to the type it is
// about.
//
// Run by `just build-js`. `verify-package.mjs` is what proves it worked: it
// compiles a consumer with a default tsconfig, and its control -- assigning
// `await canvas.toBuffer('png')` to a `string` -- has to fail. If this file
// stops running, or runs and changes nothing, that control compiles clean and
// the gate says so.

import { readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/** The emitted declarations, beside this package rather than the caller's cwd. */
const DIST = resolve(dirname(fileURLToPath(import.meta.url)), '../dist')

/** The directive, exactly as TypeScript writes it. */
const REFERENCE = '/// <reference types="node" />'

/**
 * Whether a declaration names a type that only `@types/node` supplies.
 *
 * **Type positions only.** `Buffer` appears in prose in several files -- the
 * paragraph explaining why `toBuffer` answers one is in `canvas.ts` and travels
 * into `canvas.d.ts` -- and prepending a reference to a file because of a
 * comment would put it where nothing needs it. `RequestInit` is deliberately
 * absent from this list: it looks like the same problem and is not, because it
 * comes from the DOM library a consumer's `target` already pulls in.
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
