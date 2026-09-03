// Builds the JavaScript API reference and gates on what it found.
//
// TypeDoc reports one severity for every validation, and the two kinds of
// finding here do not deserve the same treatment. A broken link, or a type
// that escapes into a signature without being exported, is a defect in the
// declarations and the build stops. A member with no doc comment is a gap,
// and failing on every gap the day this arrives would teach everyone to pass
// the flag that turns the whole check off -- which is how a gate dies.
//
// So: structural findings fail immediately, and the undocumented count
// ratchets. It may fall and it may hold. It may not rise.
//
// Adapted from `meo-skia-canvas/scripts/typedoc/build.mjs`, with the input
// changed: that project checks its declarations in, this one builds them, so
// `just docs-js` runs `build-js` first and this reads what it emitted.

import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const BASELINE = join(HERE, 'undocumented-baseline.txt')
const ENTRY = resolve(HERE, '../../dist/index.d.ts')

// TypeDoc's own wording, matched rather than parsed: the message is the only
// thing distinguishing a coverage warning from a structural one.
const UNDOCUMENTED = 'does not have any documentation'

const typedoc = join(HERE, 'node_modules', '.bin', 'typedoc')
if (!existsSync(typedoc)) {
  process.stderr.write(
    'The reference tool is not installed. Run `just docs-js`, which installs it, or `bun install --cwd packages/meo-canvas/tools/typedoc`.\n',
  )
  process.exit(1)
}

// The declarations are built, not committed. A stale or missing `dist/` would
// document a surface nobody ships, or nothing -- and TypeDoc reports the
// latter as "no entry points", which reads like a config error.
if (!existsSync(ENTRY)) {
  process.stderr.write(`No declarations at ${ENTRY}. Run \`just build-js\` first; \`just docs-js\` does.\n`)
  process.exit(1)
}

// Both streams, because TypeDoc writes its findings to stderr and its
// progress to stdout -- reading only what a command returns would have found
// nothing to report and called that a clean build. The sibling project did,
// once.
const run = spawnSync(typedoc, ['--options', join(HERE, 'typedoc.json')], { cwd: HERE, encoding: 'utf8' })
const output = `${run.stdout ?? ''}${run.stderr ?? ''}`
process.stdout.write(output)

// Strip the colour codes so the matching below reads the words, not the
// escape sequences around them. The control character is the point.
// eslint-disable-next-line no-control-regex
const lines = output.replace(/\[[0-9;]*m/g, '').split('\n')

const structural = lines.filter(
  line => (line.includes('[warning]') || line.includes('[error]')) && !line.includes(UNDOCUMENTED) && !/Found \d+ errors? and \d+ warnings?/.test(line),
)
const undocumented = lines.filter(line => line.includes(UNDOCUMENTED)).length

for (const line of structural) process.stderr.write(`${line}\n`)

if (run.status !== 0 || structural.length > 0) {
  process.stderr.write(
    `\nThe reference did not build cleanly: ${structural.length} structural finding(s) above. ` +
      'These are defects in the declarations -- a link that resolves to nothing, or a type used in a signature ' +
      'without being exported -- and a reader hits them as dead ends.\n',
  )
  process.exit(1)
}

const baseline = existsSync(BASELINE) ? Number.parseInt(readFileSync(BASELINE, 'utf8').trim(), 10) : Number.POSITIVE_INFINITY

process.stdout.write(`\nReference built. Undocumented members: ${undocumented}.\n`)

if (undocumented > baseline) {
  process.stderr.write(
    `\nThat is ${undocumented - baseline} more than the baseline of ${baseline}. ` +
      'Document what you added, or say here why it needs no documentation.\nThe list is above, one line per member.\n',
  )
  process.exit(1)
}

if (undocumented < baseline) {
  writeFileSync(BASELINE, `${undocumented}\n`)
  process.stdout.write(`Down from ${baseline}. Baseline lowered -- commit tools/typedoc/undocumented-baseline.txt with the change.\n`)
}
