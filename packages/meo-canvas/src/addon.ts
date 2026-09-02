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
 * {@link PLATFORM_PACKAGES} is a promise that `npm install` can keep, and it is
 * kept by construction rather than by agreement: `TARGETS` in
 * `tools/stage-platform-package.mjs` is the one declaration, and both this list
 * and `optionalDependencies` are generated from it by `just platform-packages`.
 * `.github/workflows/release.yml` reads its matrix from the same place. A
 * platform absent from the table gets a message naming its own triple rather
 * than a module-not-found for a package that was never published.
 *
 * It was three hand-maintained lists with a test asserting they agreed, which
 * reported drift rather than preventing it -- adding two targets meant editing
 * three files and being told about the two you forgot.
 */

import { createRequire } from 'node:module'

/**
 * The platform packages, keyed by the triple {@link target} derives.
 *
 * Generated from `TARGETS` rather than written here -- see
 * `generated/platform-packages.ts` for why the list arrives as a committed
 * file instead of an import, and `tools/generate-platform-packages.mjs` for
 * what writes it. Re-exported so this module stays the one place a caller asks
 * about platforms.
 */
import { PLATFORM_PACKAGES } from './generated/platform-packages.js'

export { PLATFORM_PACKAGES }

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
 * Which C library this Node runs against, and which version of it.
 *
 * `glibcVersionRuntime` is present in the process report only on a glibc host,
 * which is the check Node's own tooling uses. It answers `undefined` off Linux,
 * where the question does not arise.
 *
 * **The version is kept rather than discarded**, because it is the number that
 * decides whether the addon loads at all: a binary built against a newer glibc
 * fails at `dlopen` with `version GLIBC_2.xx not found`, and a host that cannot
 * be told its own version has no way to act on that. Reading the string and
 * returning only the family threw away the one fact the diagnosis needs.
 */
function libc(): { readonly family: 'glibc' | 'musl'; readonly version?: string } | undefined {
  if (process.platform !== 'linux') return undefined
  const header = process.report?.getReport() as { header?: { glibcVersionRuntime?: string } } | undefined
  const version = header?.header?.glibcVersionRuntime
  return version === undefined ? { family: 'musl' } : { family: 'glibc', version }
}

/**
 * The triple used to look a package up, and to name the host in an error.
 *
 * `platform-arch` everywhere except Linux, where the C library is part of the
 * identity: a glibc binary and a musl one are different artefacts for the same
 * `linux-x64`, and a host that cannot say which it is cannot be given the right
 * one. Off Linux the question does not arise and the suffix stays two parts,
 * matching the names in {@link PLATFORM_PACKAGES}.
 */
export function target(): string {
  const host = `${process.platform}-${process.arch}`
  const family = libc()?.family
  return family === undefined ? host : `${host}-${family}`
}

/**
 * The floors a platform package declares, or `undefined` where it declares none.
 *
 * Read from the package's own `package.json` rather than from anything in this
 * package, and that is the load-bearing part: **a manifest stays readable when
 * the binary beside it will not load**, which is exactly the moment the numbers
 * are needed. A target with no ELF floors -- darwin, win32 -- carries no
 * `floors` key at all, so the absence is structural rather than a null someone
 * has to interpret.
 */
function declaredFloors(platformPackage: string): { glibc?: string; glibcxx?: string } | undefined {
  const require = createRequire(import.meta.url)
  try {
    const manifest = require(`${platformPackage}/package.json`) as {
      meoCanvas?: { floors?: { glibc?: string; glibcxx?: string } }
    }
    return manifest.meoCanvas?.floors
  } catch {
    // An older platform package predating the declaration, or one whose
    // manifest is unreadable. Neither is worth failing over: the diagnosis is
    // an improvement on the loader's message, not a precondition for it.
    return undefined
  }
}

/** Whether `version` is older than `floor`, both dotted numbers like `2.35`. */
function older(version: string, floor: string): boolean {
  const parts = (value: string) => value.split('.').map(part => Number.parseInt(part, 10))
  const [left, right] = [parts(version), parts(floor)]
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const [a, b] = [left[index] ?? 0, right[index] ?? 0]
    // A non-numeric component makes the comparison meaningless rather than
    // false. Saying nothing beats asserting an ordering that was never read.
    if (Number.isNaN(a) || Number.isNaN(b)) return false
    if (a !== b) return a < b
  }
  return false
}

/**
 * Why an installed platform package would not load, in terms a reader can act on.
 *
 * The loader's own message names a symbol or a file and nothing else -- `version
 * GLIBC_2.35 not found`, `libfontconfig.so.1: cannot open shared object file` --
 * which says what the kernel refused and not what the reader should do. Each
 * branch here names the host's own number, what the binary wants, and the fix.
 *
 * **This diagnoses a failure that has already happened; it does not predict
 * one.** A binary can satisfy every declared floor and still fail to load on an
 * unversioned symbol -- `_M_replace_cold`, a GCC 12 symbol carrying no version
 * tag for any check to compare -- so nothing here, and nothing in the build's
 * ceiling assertion, establishes that an artefact loads. Only loading it does.
 */
function loadFailure(platformPackage: string, cause: unknown): string {
  const detail = cause instanceof Error ? cause.message : String(cause)
  const fix = `Build one with \`just addon\`, or point ${OVERRIDE} at a binary you built.`

  // A shared object the binary needs and the host does not have. First because
  // it is the failure a consumer meets first: a stock `node:22-slim` has
  // neither `libfontconfig.so.1` nor `libfreetype.so.6`.
  const missing = /([\w.+-]+\.so[\w.]*): cannot open shared object file/.exec(detail)
  if (missing !== null) {
    return (
      `${platformPackage} is installed, and loading it needs ${missing[1]}, which this host does not have. ` +
      `Install it -- \`libfontconfig1\` and \`libfreetype6\` on Debian and Ubuntu, \`fontconfig\` and \`freetype\` on RHEL-family images. ` +
      fix
    )
  }

  // A glibc older than the binary was built against, named against the floor
  // the package declares so the message carries both numbers rather than one.
  const host = libc()
  const floors = declaredFloors(platformPackage)
  if (host?.family === 'glibc' && host.version !== undefined && floors?.glibc !== undefined && older(host.version, floors.glibc)) {
    return (
      `${platformPackage} is installed, and it needs glibc ${floors.glibc} or newer; this host has ${host.version}. ` +
      `No package installs around that -- it is the C library the system is built on. ` +
      `Run on a newer image, or ${fix.charAt(0).toLowerCase()}${fix.slice(1)}`
    )
  }

  // Everything else, including the unversioned-symbol case the floors cannot
  // see. The loader's text is passed through rather than paraphrased: it names
  // the symbol, and a guess about the cause would be worse than the fact.
  return `${platformPackage} is installed and its binary would not load: ${detail}. ${fix}`
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
      // `host` already carries the libc on Linux, so a musl host with no build
      // reads as `linux-x64-musl` rather than needing the family bolted on.
      // It used to say `(musl)` here and then refuse every musl host a few
      // lines below, which was correct only while no musl build existed.
      `no prebuilt addon is published for ${host}. ` +
        `The published targets are ${Object.keys(PLATFORM_PACKAGES).join(', ')}. ` +
        `Build one with \`just addon\`, or point ${OVERRIDE} at a binary you built.`,
    )
  }

  // **Resolved and failed to load is not the same problem as not installed**,
  // and the two have different fixes: one is `npm install`, the other is the
  // host. Asked with `require.resolve` rather than by reading the message off
  // the `require` failure, because a dynamic-linker error is a string from the
  // platform's loader and matching on it would make this depend on wording no
  // one here controls.
  let resolved: string | undefined
  try {
    resolved = require.resolve(platformPackage)
  } catch {
    attempts.push({ where: `the platform package ${platformPackage}`, cause: undefined })
  }

  if (resolved !== undefined) {
    try {
      return require(platformPackage) as T
    } catch (cause) {
      // The package is installed and its binary would not load. Everything
      // below is about saying why, because the loader's own message names a
      // symbol and nothing a reader can act on.
      throw new Error(loadFailure(platformPackage, cause), { cause })
    }
  }

  throw new Error(
    `the addon for ${host} was not found in ${attempts.length} places: ` +
      attempts.map(attempt => attempt.where).join('; ') +
      `. Install the package with its optional dependencies, or run \`just addon\` in a checkout.`,
    { cause: attempts[attempts.length - 1]?.cause },
  )
}
