// Installs the packed tarballs into a throwaway project and uses them as a consumer
// does: `import` by name through the platform package, render, check for a PNG --
// then `require` by name and `tsc` with a consumer's tsconfig, which a render
// cannot see. Run by `just verify-pack` and the release, before publishing.

import { execFileSync } from 'node:child_process'
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/** This file's own directory, which is how the repository root is reached. */
const HERE = dirname(fileURLToPath(import.meta.url))

/** The first eight bytes of every PNG, which is what proves a render happened. */
const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])

/**
 * Running npm, which on Windows is `npm.cmd`: `execFileSync` skips PATHEXT, and Node
 * spawns a `.cmd` only with a shell since CVE-2024-27980. A shell hands the quoting
 * back to us, so the paths are quoted -- a `C:\Users\Given Name\` breaks them unquoted.
 */
const WINDOWS = process.platform === 'win32'
const NPM = WINDOWS ? 'npm.cmd' : 'npm'
const TSC = WINDOWS ? 'tsc.cmd' : 'tsc'

/** One argument, safe to hand a shell on the platform that needs one. */
function arg(value) {
  return WINDOWS ? `"${value}"` : value
}

/**
 * The TypeScript the consumer project compiles with, from this repository's pin:
 * the axis under test is the tsconfig and the `node_modules` layout, and a fresh
 * resolve would turn a TypeScript release red here.
 */
const TYPESCRIPT = JSON.parse(readFileSync(resolve(HERE, '../../../package.json'), 'utf8')).devDependencies.typescript

/** A command whose failure is an answer rather than a crash. */
function attempt(file, args, options) {
  try {
    return { status: 0, output: execFileSync(file, args, { ...options, encoding: 'utf8', stdio: 'pipe' }) }
  } catch (cause) {
    return { status: cause.status ?? 1, output: `${cause.stdout ?? ''}${cause.stderr ?? ''}` }
  }
}

const releaseDir = resolve(process.argv[2] ?? 'release')
const tarballs = readdirSync(releaseDir)
  .filter(name => name.endsWith('.tgz'))
  .map(name => join(releaseDir, name))

if (tarballs.length < 2) {
  process.stderr.write(`expected the main tarball and at least one platform tarball in ${releaseDir}, found ${tarballs.length}\n`)
  process.exit(1)
}

const project = mkdtempSync(join(tmpdir(), 'meo-canvas-verify-'))
process.stderr.write(`verifying ${tarballs.length} tarballs in ${project}\n`)

try {
  writeFileSync(join(project, 'package.json'), `${JSON.stringify({ name: 'verify', private: true, type: 'module', version: '0.0.0' }, null, 2)}\n`)

  // Both at once, so npm resolves the main package's `optionalDependencies`
  // against the platform tarball rather than reaching the registry for a
  // version that is not published yet.
  execFileSync(NPM, ['install', '--silent', '--no-audit', '--no-fund', ...tarballs.map(arg), arg(`typescript@${TYPESCRIPT}`)], {
    cwd: project,
    stdio: 'inherit',
    shell: WINDOWS,
  })

  const script = `
import { Box, Root } from 'meo-canvas'
const canvas = await Root({
  width: 120,
  height: 60,
  backgroundColor: '#101820',
  padding: 12,
  children: [Box({ width: 40, height: 20, backgroundColor: '#f2aa4c' })],
})
const bytes = await canvas.toBuffer('png')
canvas.release()
process.stdout.write(String(bytes.length))
`
  writeFileSync(join(project, 'render.mjs'), script)
  const size = Number(execFileSync(process.execPath, ['render.mjs'], { cwd: project, encoding: 'utf8' }))

  // The length alone would pass on an empty buffer or an error string, so the
  // magic is checked too: this asserts a PNG, not a truthy value.
  const bytes = execFileSync(
    process.execPath,
    [
      '--input-type=module',
      '-e',
      `${script.replace('process.stdout.write(String(bytes.length))', 'process.stdout.write(bytes.subarray(0, 8).toString("base64"))')}`,
    ],
    { cwd: project, encoding: 'utf8' },
  )
  if (!Buffer.from(bytes, 'base64').equals(PNG_MAGIC)) {
    throw new Error(`the render produced ${size} bytes that are not a PNG`)
  }

  process.stderr.write(`rendered ${size} bytes of PNG through the installed package\n`)

  // ── The two questions a render cannot ask ────────────────────────────────
  // `require` by package name, since `exports` needs a `require` condition, and
  // types compiled where `@types/node` is not a direct dependency, where an ambient
  // `Buffer` resolves to `any` -- nothing in the tree typechecks from there.

  /** The export names one module system sees, sorted so the two can be compared. */
  function names(inputType, source) {
    const run = attempt(process.execPath, [`--input-type=${inputType}`, '-e', source], { cwd: project })
    if (run.status !== 0) throw new Error(`the package could not be loaded with ${inputType}:\n${run.output}`)
    return run.output.trim()
  }

  const esm = names('module', `process.stdout.write(Object.keys(await import('meo-canvas')).sort().join(','))`)
  const cjs = names('commonjs', `process.stdout.write(Object.keys(require('meo-canvas')).sort().join(','))`)

  // **Compared rather than counted, and asserted non-empty before compared.**
  // Two empty lists are equal, so equality alone would pass a package that
  // exports nothing through either door -- the shape of check this file exists
  // to stop being satisfied with.
  if (esm === '') throw new Error('importing the installed package produced no exports at all')
  if (cjs !== esm) {
    throw new Error(`\`require\` and \`import\` disagree about what the package exports.\n  import:  ${esm}\n  require: ${cjs}`)
  }
  process.stderr.write(`the same ${esm.split(',').length} exports arrive through \`import\` and through \`require\`\n`)

  // A consumer's tsconfig, which is the one no check in this repository uses:
  // `skipLibCheck` on, as `tsc --init` writes it, and no `types` field, because
  // a consumer who has not needed one has not written one.
  const tsconfig = {
    compilerOptions: {
      module: 'nodenext',
      moduleResolution: 'nodenext',
      noEmit: true,
      skipLibCheck: true,
      strict: true,
      target: 'es2023',
    },
  }
  for (const file of ['probe', 'control']) {
    writeFileSync(join(project, `tsconfig.${file}.json`), `${JSON.stringify({ ...tsconfig, files: [`${file}.ts`] }, null, 2)}\n`)
  }

  // Ordinary correct use, reaching each module the package publishes rather
  // than the two names the render happens to need. A dropped export or an
  // `exports` entry pointing at a file the allowlist never packed is a
  // compile error here.
  writeFileSync(
    join(project, 'probe.ts'),
    `import { Box, Canvas, Chart, Root, Text, ease, isColor, track } from 'meo-canvas'
import type { RootProps, TrackConfig } from 'meo-canvas'

const motion: TrackConfig<number> = { duration: 1, ease: 'outCubic', from: 0, to: 40 }
const props: RootProps = {
  backgroundColor: '#101820',
  children: [
    Box({ height: 20, width: track(motion).at({ time: 1 }) }),
    Text('x'),
    Chart({ data: { datasets: [{ data: [1, 2] }], labels: ['a', 'b'] }, type: 'bar' }),
  ],
  height: 60,
  width: 120,
}
const canvas: Canvas = await Root(props)
const bytes = await canvas.toBuffer('png')
canvas.release()
void [bytes.length, ease('outCubic', 0.5), isColor('#fff')]
`,
  )

  // The control, asserted to fail: assigning `toBuffer`'s `Buffer` to a `string` is
  // an error unless `Buffer` resolved to `any` and `skipLibCheck` hid why -- so a
  // clean compile here is the defect.
  writeFileSync(
    join(project, 'control.ts'),
    `import { Box, Root } from 'meo-canvas'

const canvas = await Root({ height: 10, width: 10, children: [Box({})] })
const wrong: string = await canvas.toBuffer('png')
canvas.release()
void wrong
`,
  )

  const tsc = join(project, 'node_modules', '.bin', TSC)
  const probe = attempt(tsc, [arg('-p'), arg(join(project, 'tsconfig.probe.json'))], { cwd: project, shell: WINDOWS })
  if (probe.status !== 0) {
    throw new Error(`the shipped types do not compile in a consumer with a default tsconfig:\n${probe.output}`)
  }

  const control = attempt(tsc, [arg('-p'), arg(join(project, 'tsconfig.control.json'))], { cwd: project, shell: WINDOWS })
  if (control.status === 0) {
    throw new Error(
      "the control compiled. `const wrong: string = await canvas.toBuffer('png')` was accepted, " +
        'which means `Buffer` reached the consumer as `any` and the check above proved nothing. ' +
        'The declarations need a `/// <reference types="node" />` for a consumer who has not put `node` in `types`.',
    )
  }

  process.stderr.write(`the shipped types compile under a consumer's default tsconfig, and the control fails as it must\n`)
} finally {
  rmSync(project, { recursive: true, force: true })
}
