/**
 * Finding the compiled addon, in the three places it can be.
 *
 * The binary is ~51 MB per platform, so it does not ship inside this package:
 * one package per target carries one binary, listed in `optionalDependencies`
 * with its own `os` and `cpu`, and a package manager installs only the one that
 * matches. The alternative — a postinstall script that downloads — needs the
 * network at install time and breaks every offline and locked-down install.
 *
 * **Only targets that CI actually builds appear here.** A key in
 * {@link PLATFORM_PACKAGES} is a promise that `npm install` can keep, so the
 * table and the build matrix in `.github/workflows/build.yml` are one list
 * written twice; `just platform-targets` prints it, and the workflow reads it.
 * A platform absent from the table gets a message naming its own triple rather
 * than a module-not-found for a package that was never published.
 */

import { createRequire } from 'node:module'

/**
 * The platform packages, keyed by the triple this module derives.
 *
 * `linux-x64` maps to a glibc build and says so in the package name, because a
 * musl host resolving it would load a binary it cannot run. {@link libc} is
 * what keeps that from being a link error at first render.
 */
export const PLATFORM_PACKAGES: Readonly<Record<string, string>> = {
  'darwin-arm64': '@l7aromeo/meo-canvas-darwin-arm64',
  'linux-x64': '@l7aromeo/meo-canvas-linux-x64-gnu',
}

/** The environment variable that overrides every other path. */
const OVERRIDE = 'MEO_CANVAS_ADDON'

/**
 * The addon built in this working tree, beside the package's own files.
 *
 * `just addon` writes it here, and it wins over an installed platform package
 * so that a checkout tests what it just built rather than what npm resolved.
 */
const IN_TREE = '../meo-canvas.node'

/**
 * Which C library this Node runs against, on Linux.
 *
 * `glibcVersionRuntime` is present in the process report only on a glibc host,
 * which is the check Node's own tooling uses. It answers `undefined` off Linux,
 * where the question does not arise.
 */
function libc(): 'glibc' | 'musl' | undefined {
  if (process.platform !== 'linux') return undefined
  const header = process.report?.getReport() as { header?: { glibcVersionRuntime?: string } } | undefined
  return header?.header?.glibcVersionRuntime === undefined ? 'musl' : 'glibc'
}

/** The triple used to look a package up, and to name the host in an error. */
export function target(): string {
  return `${process.platform}-${process.arch}`
}

/** Every attempt made, so a failure can say what was tried rather than what was last tried. */
interface Attempt {
  readonly where: string
  readonly cause: unknown
}

/**
 * The addon, from the override, the working tree, or the platform package.
 *
 * Ordered by how specific the intent is: an explicit path beats a local build,
 * and a local build beats whatever a package manager resolved. Each failure is
 * kept rather than replaced -- a "cannot find module" naming only the last
 * candidate sends a reader to the wrong one of three questions.
 */
export function resolveAddon<T>(): T {
  const require = createRequire(import.meta.url)
  const attempts: Attempt[] = []

  const override = process.env[OVERRIDE]
  if (override !== undefined && override !== '') {
    try {
      return require(override) as T
    } catch (cause) {
      // An override that does not load is an error rather than a fallback: it
      // was set deliberately, and silently loading a different binary than the
      // one named is how a test reports on code nobody asked it about.
      throw new Error(`${OVERRIDE} is set to ${override}, and no addon loaded from there`, { cause })
    }
  }

  try {
    return require(IN_TREE) as T
  } catch (cause) {
    attempts.push({ where: `this working tree (${IN_TREE}, written by \`just addon\`)`, cause })
  }

  const host = target()
  const platformPackage = PLATFORM_PACKAGES[host]
  if (platformPackage === undefined) {
    throw new Error(
      `no prebuilt addon is published for ${host}${libc() === 'musl' ? ' (musl)' : ''}. ` +
        `The published targets are ${Object.keys(PLATFORM_PACKAGES).join(', ')}. ` +
        `Build one with \`just addon\`, or point ${OVERRIDE} at a binary you built.`,
    )
  }

  if (libc() === 'musl') {
    throw new Error(
      `${platformPackage} is a glibc build and this host is musl. ` + `Build one with \`just addon\`, or point ${OVERRIDE} at a binary you built.`,
    )
  }

  try {
    return require(platformPackage) as T
  } catch (cause) {
    attempts.push({ where: `the platform package ${platformPackage}`, cause })
  }

  throw new Error(
    `the addon for ${host} was not found in ${attempts.length} places: ` +
      attempts.map(attempt => attempt.where).join('; ') +
      `. Install the package with its optional dependencies, or run \`just addon\` in a checkout.`,
    { cause: attempts[attempts.length - 1]?.cause },
  )
}
