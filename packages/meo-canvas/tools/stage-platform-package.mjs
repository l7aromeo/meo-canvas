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
  'linux-x64-gnu': {
    os: ['linux'],
    cpu: ['x64'],
    libc: ['glibc'],
    rust: 'x86_64-unknown-linux-gnu',
    runner: 'ubuntu-latest',
    floors: { glibc: '2.35', glibcxx: '3.4.30' },
  },
  'linux-arm64-gnu': {
    os: ['linux'],
    cpu: ['arm64'],
    libc: ['glibc'],
    rust: 'aarch64-unknown-linux-gnu',
    runner: 'ubuntu-24.04-arm',
    // **Inherited from the x64 build and not yet measured on this one.** The
    // floors are a property of an artefact, and no arm64 artefact exists yet.
    // Declaring the sibling's numbers rather than omitting the key is the
    // honest option of the two available: an absent `floors` means "no ELF
    // floor to check here", which is true of darwin and win32 and false of
    // this. If the arm64 artefact's real floors are higher, the release's
    // assertion fails and names the symbol, which is the correction arriving
    // through the mechanism built for it.
    floors: { glibc: '2.35', glibcxx: '3.4.30' },
  },
  // No `floors`: the floors are ELF symbol versions, and a PE binary has none.
  // Windows links DirectWrite rather than fontconfig and freetype, so the
  // acceptance harness loads it on the runner rather than in a container.
  'win32-x64': { os: ['win32'], cpu: ['x64'], rust: 'x86_64-pc-windows-msvc', runner: 'windows-latest' },
}

/**
 * The ELF symbol floors a target's artefact currently has.
 *
 * **This is a diagnostic, not a gate, and the difference is structural.** The
 * release workflow asserts the built binary does not exceed these, which
 * catches versioned drift early and names the symbol that moved -- worth having,
 * and much better than discovering a floor rose and hunting for why. What it
 * cannot do is establish that the artefact loads: an *unversioned* symbol has no
 * tag to compare, and a binary reporting `GLIBCXX_3.4.21`, under every ceiling,
 * still failed to load on `undefined symbol: _M_replace_cold`. Only loading it
 * decides that, which is what `tools/acceptance.mjs` is for.
 *
 * These are the numbers the artefact has **today**, not the ones we want. They
 * are declared so a rise is noticed, and they are expected to be edited down as
 * the build moves to an older base image -- a gate that is expected to be red
 * teaches people to ignore it, so the declaration tracks reality and tightens
 * behind it.
 *
 * A target with no ELF floors carries no `floors` key at all rather than a null:
 * darwin has none, and neither will win32. Absent means "nothing here to
 * check", which the asserting step reports rather than skipping silently, since
 * an unchecked target that prints nothing is indistinguishable from one that
 * passed.
 */

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
    // Carried into the platform package so `resolveAddon` can read it when the
    // binary beside it will not load: **a manifest stays readable when the
    // `.node` does not**, which is exactly the moment the numbers are wanted.
    ...(target.floors === undefined ? {} : { meoCanvas: { floors: target.floors } }),
    // The binary is the whole package, and `main` is what makes
    // `require('meo-canvas-darwin-arm64')` resolve to it rather than to a
    // directory with no entry point.
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
