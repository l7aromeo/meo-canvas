// Stages one platform package: a directory holding the compiled addon and a
// manifest saying which host it is for.
//
// A package per target rather than one package holding every binary, because a
// caller downloads what it can run and nothing else — the addon is 51 MB, so
// two targets in one package is 51 MB wasted on every install and seven is
// six. `os`, `cpu` and `libc` are what let a package manager skip the ones it
// cannot use, and being listed in `optionalDependencies` is what makes skipping
// them succeed rather than fail.
//
// The alternative — a postinstall script that downloads the right binary — needs
// the network at install time and breaks offline installs, locked-down CI and
// any environment with `--ignore-scripts`.
//
// Run for the host by `just pack`, and once per target by the release workflow,
// which passes the binary it just built.

import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const PACKAGE = resolve(HERE, '../package.json')

/**
 * Every target a release carries, and what a package manager needs to pick one.
 *
 * The keys are the package-name suffixes. `PLATFORM_PACKAGES` in `src/addon.ts`
 * maps a running host onto them and `optionalDependencies` pins them, and
 * `addon.test.ts` asserts all three agree — a target added in one place and not
 * the others fails that test rather than shipping a package nothing resolves or
 * naming one nothing builds.
 */
export const TARGETS = {
  'darwin-arm64': { os: ['darwin'], cpu: ['arm64'], rust: 'aarch64-apple-darwin', runner: 'macos-latest' },
  'linux-x64-gnu': { os: ['linux'], cpu: ['x64'], libc: ['glibc'], rust: 'x86_64-unknown-linux-gnu', runner: 'ubuntu-latest' },
}

/** The manifest a platform package ships, derived from the main one. */
export function manifest(suffix, version) {
  const main = JSON.parse(readFileSync(PACKAGE, 'utf8'))
  const target = TARGETS[suffix]
  if (target === undefined) throw new Error(`no target named ${suffix}; known: ${Object.keys(TARGETS).join(', ')}`)
  return {
    name: `${main.name}-${suffix}`,
    version,
    description: `The ${suffix} binary for ${main.name}.`,
    license: main.license,
    repository: main.repository,
    engines: main.engines,
    os: target.os,
    cpu: target.cpu,
    ...(target.libc === undefined ? {} : { libc: target.libc }),
    // The binary is the whole package, and `main` is what makes
    // `require('@l7aromeo/meo-canvas-darwin-arm64')` resolve to it rather than
    // to a directory with no entry point.
    main: 'meo-canvas.node',
    files: ['meo-canvas.node'],
  }
}

/** Writes one staged package, and reports where it went. */
export function stage(suffix, binary, outDir) {
  const { version } = JSON.parse(readFileSync(PACKAGE, 'utf8'))
  const staged = resolve(outDir, suffix)
  mkdirSync(staged, { recursive: true })
  writeFileSync(resolve(staged, 'package.json'), `${JSON.stringify(manifest(suffix, version), null, 2)}\n`)
  copyFileSync(resolve(binary), resolve(staged, 'meo-canvas.node'))
  return { name: manifest(suffix, version).name, version, staged }
}

// The command-line half runs only when this file *is* the command. `TARGETS`
// and `manifest` are imported by `src/addon.test.ts`, which asserts them
// against the two other places a target is named, and an unguarded body would
// exit that test run with a usage message instead.
if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [suffix, binary, outDir] = process.argv.slice(2)
  // The release workflow needs the same list as a job matrix, and deriving it
  // here is what keeps the workflow from being a fourth place a target is
  // named. One line of JSON on stdout, which is what `$GITHUB_OUTPUT` takes.
  if (suffix === '--matrix') {
    process.stdout.write(
      `${JSON.stringify({
        include: Object.entries(TARGETS).map(([name, spec]) => ({ suffix: name, rust: spec.rust, runner: spec.runner })),
      })}\n`,
    )
    process.exit(0)
  }
  if (suffix === undefined || binary === undefined || outDir === undefined) {
    process.stderr.write('usage: stage-platform-package.mjs <suffix> <path to .node> <output directory>\n')
    process.exit(2)
  }
  const written = stage(suffix, binary, outDir)
  process.stderr.write(`staged ${written.name}@${written.version} in ${written.staged}\n`)
}
