// Installs the packed tarballs into a throwaway project and renders with them.
//
// **Packing is not publishing and publishing is not installing.** `npm pack`
// reports a file list, which says a file is in the tarball and nothing about
// whether a consumer can reach it: `exports` can point at a path the allowlist
// dropped, `main` in a platform package can name a binary that is not there,
// and `optionalDependencies` can pin a version nobody built. Every one of those
// packs cleanly and fails at the first `import`.
//
// So this does what a consumer does — a directory that is not this repository,
// `npm install` from the tarballs, `import` by package name, render, check the
// bytes are a PNG. The addon is found the way it will be found in the wild,
// through the platform package rather than through the copy sitting in this
// working tree, because the in-tree path does not exist inside `node_modules`.
//
// **Then it asks the two things a render cannot.** A render reaches the package
// one way and erases every type on the way in, so it is blind to a consumer who
// writes `require` and blind to a declaration that does not compile. Two
// defects reached a release through exactly those two blind spots, and each is
// one call away from being impossible: `require` by package name, and `tsc`
// from the consumer directory with the tsconfig a consumer actually has.
//
// Run by `just verify-pack` and by the release workflow, before anything is
// published.

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
 * Running npm, which takes two Windows-only accommodations rather than one.
 *
 * Both failed the `win32-x64` release job at this step, after the addon had
 * built and packed cleanly -- the artefact was fine and the tool checking it
 * was not, twice over.
 *
 * **The name.** npm on Windows is `npm.cmd`, and `execFileSync` does not
 * consult PATHEXT: `spawnSync npm ENOENT` on a machine where `npm` works in
 * any shell.
 *
 * **The shell.** Naming it correctly then fails differently --
 * `spawnSync npm.cmd EINVAL` -- because Node refuses to spawn a `.cmd` or
 * `.bat` without an explicit shell, which is the fix for CVE-2024-27980: a
 * batch file re-parses its command line, so arguments could escape into it.
 * Passing `shell: true` is the supported way, **and it hands the quoting back
 * to us** -- which is exactly the hazard that CVE describes, so the paths are
 * quoted here rather than trusted to contain nothing a shell reads. They are
 * `mkdtemp` and `readdir` output, not user input, but a temporary directory
 * under `C:\Users\Given Name\` needs no malice to break this.
 */
const WINDOWS = process.platform === 'win32'
const NPM = WINDOWS ? 'npm.cmd' : 'npm'
const TSC = WINDOWS ? 'tsc.cmd' : 'tsc'

/** One argument, safe to hand a shell on the platform that needs one. */
function arg(value) {
  return WINDOWS ? `"${value}"` : value
}

/**
 * The TypeScript the consumer project compiles with, taken from this
 * repository's own pin.
 *
 * **The axis under test is the `tsconfig` and the `node_modules` layout, not
 * the compiler version.** Resolving `typescript` afresh would make a gate that
 * can go red because a release happened, which is a different fact from the one
 * it is here to report; reading the pin keeps the two versions from drifting
 * apart without anyone choosing to.
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
  //
  // A render proves the runtime works for the one caller shape it uses. It says
  // nothing about a caller who reaches the package by `require`, and nothing at
  // all about types, which are erased before anything runs. Both gaps let a
  // defect through to a release: `exports` carried no `require` condition, so
  // every CommonJS consumer met `ERR_PACKAGE_PATH_NOT_EXPORTED`; and the
  // declarations named the ambient `Buffer`, which resolves to `any` in a
  // consumer that has not put `node` in `types`. Neither is visible from here
  // without asking, and nothing else in the repository asks: every in-tree
  // typecheck runs where `@types/node` is a direct development dependency.

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

  // **The control, which has to fail before the probe passing means anything.**
  // `toBuffer` answers a `Buffer`, so assigning it to a `string` is an error --
  // unless `Buffer` did not resolve, in which case it is `any`, the assignment
  // is accepted, and `skipLibCheck` swallows the reason. A clean compile here
  // is the defect, not the absence of one, which is why it is asserted in the
  // direction that reads backwards.
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
