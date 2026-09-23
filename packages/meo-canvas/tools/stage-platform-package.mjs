// Stages one platform package: the compiled addon and a manifest naming its host.
// One package per target, so an install downloads only the 51 MB binary it can
// run; `os`, `cpu` and `libc` let a package manager skip the rest, and
// `optionalDependencies` makes skipping succeed. Run by `just pack` and the release.

import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const PACKAGE = resolve(HERE, '../package.json')

/**
 * Every target a release carries, keyed by package-name suffix. `addon.test.ts`
 * asserts these, `PLATFORM_PACKAGES` in `src/addon.ts` and `optionalDependencies`
 * agree, so a target named in one place and not the others fails there.
 */
export const TARGETS = {
  'darwin-arm64': { os: ['darwin'], cpu: ['arm64'], rust: 'aarch64-apple-darwin', runner: 'macos-latest' },
  'linux-x64-gnu': {
    os: ['linux'],
    cpu: ['x64'],
    libc: ['glibc'],
    rust: 'x86_64-unknown-linux-gnu',
    runner: 'ubuntu-latest',
    // Measured on the artefact the `manylinux_2_28` image produces. A floor
    // declared above what the artefact demands fails nothing and tells a
    // consumer to expect a newer machine than they need.
    floors: { glibc: '2.28', glibcxx: '3.4.21' },
  },
  'linux-arm64-gnu': {
    os: ['linux'],
    cpu: ['arm64'],
    libc: ['glibc'],
    rust: 'aarch64-unknown-linux-gnu',
    runner: 'ubuntu-24.04-arm',
    // Measured on an `aarch64-unknown-linux-gnu` artefact built from `2b5f580` in
    // `Dockerfile.glibc`: `GLIBC_2.28` and `GLIBCXX_3.4.21`, 5 September 2026.
    floors: { glibc: '2.28', glibcxx: '3.4.21' },
  },
  // No `floors` on the musl pair: a musl binary links no glibc, so that floor does
  // not exist, and a GLIBCXX floor is unmeasured. `tools/acceptance.mjs` decides
  // whether these load; a floor guessed here would be confirmed against itself.
  'linux-x64-musl': { os: ['linux'], cpu: ['x64'], libc: ['musl'], rust: 'x86_64-unknown-linux-musl', runner: 'ubuntu-latest' },
  'linux-arm64-musl': { os: ['linux'], cpu: ['arm64'], libc: ['musl'], rust: 'aarch64-unknown-linux-musl', runner: 'ubuntu-24.04-arm' },
  // No `floors`: the floors are ELF symbol versions, and a PE binary has none.
  // Windows links DirectWrite rather than fontconfig and freetype, so the
  // acceptance harness loads it on the runner rather than in a container.
  'win32-x64': { os: ['win32'], cpu: ['x64'], rust: 'x86_64-pc-windows-msvc', runner: 'windows-latest' },
  // No `floors`, as for x64. rust-skia ships a prebuilt for this triple, so no Skia
  // compile runs -- d7b5417's message says otherwise, reading
  // `windows.rs::specific_target`, which is about the `win7` vendor; the prebuilt
  // key is built in `binary_cache/binaries.rs`.
  'win32-arm64': { os: ['win32'], cpu: ['arm64'], rust: 'aarch64-pc-windows-msvc', runner: 'windows-11-arm' },
}

/**
 * Which C library this process runs against, on Linux. The same check as
 * `src/addon.ts`, duplicated because this runs under plain `node` and cannot
 * import the shipped surface.
 */
function hostLibc() {
  if (process.platform !== 'linux') return undefined
  return process.report?.getReport()?.header?.glibcVersionRuntime === undefined ? 'musl' : 'glibc'
}

/**
 * The target suffix for this machine, derived by matching the host against
 * `TARGETS` on OS, architecture and libc. Refuses when nothing matches, since a
 * package named for a different machine would install where it cannot load.
 */
export function hostSuffix() {
  const arch = process.arch
  const libc = hostLibc()
  const found = Object.entries(TARGETS).find(
    ([, spec]) => spec.os.includes(process.platform) && spec.cpu.includes(arch) && (spec.libc === undefined || spec.libc.includes(libc)),
  )
  if (found === undefined) {
    throw new Error(
      `no target matches this host (${process.platform}-${arch}${libc === undefined ? '' : `-${libc}`}); known: ${Object.keys(TARGETS).join(', ')}`,
    )
  }
  return found[0]
}

/**
 * The ELF symbol floors a target's artefact has today. The release asserts the
 * binary stays under them, a diagnostic naming the symbol that moved; only a load
 * decides, which is `tools/acceptance.mjs`. Absent on targets with no ELF floors.
 */

/**
 * The package a target's binary ships in: `@<main>/<suffix>`. Scoped because
 * npm refused the unscoped names with `E403 Package name triggered spam detection`;
 * the main package stays unscoped. It lives beside `TARGETS` so a target has one name.
 */
export function packageName(main, suffix) {
  // A scoped main package would compose `@@scope/name/suffix`, which npm will
  // not accept. Refuse rather than emit it: this reaches a manifest, and a
  // malformed name there fails at publish time with nothing pointing back here.
  if (main.startsWith('@')) {
    throw new Error(`the main package is scoped (${main}); packageName() composes a scope from it and cannot`)
  }
  return `@${main}/${suffix}`
}

/** The manifest a platform package ships, derived from the main one. */
export function manifest(suffix, version) {
  const main = JSON.parse(readFileSync(PACKAGE, 'utf8'))
  const target = TARGETS[suffix]
  if (target === undefined) throw new Error(`no target named ${suffix}; known: ${Object.keys(TARGETS).join(', ')}`)
  return {
    name: packageName(main.name, suffix),
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
    // `require('@meo-canvas/darwin-arm64')` resolve to it rather than to a
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

// The command-line half runs only when this file is the command: `TARGETS`,
// `hostSuffix` and `manifest` are imported by `src/addon.test.ts`, and an
// unguarded body would end that test run with a usage message.
if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [suffix, binary, outDir] = process.argv.slice(2)
  // The host's own suffix, so `just pack` asks rather than deciding.
  if (suffix === '--host') {
    process.stdout.write(`${hostSuffix()}\n`)
    process.exit(0)
  }
  // The same list as the release workflow's job matrix, so the workflow is not a
  // fourth place a target is named. One line of JSON, which `$GITHUB_OUTPUT` takes.
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
