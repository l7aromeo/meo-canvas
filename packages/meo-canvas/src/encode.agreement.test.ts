import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Root } from './root.js'
import { Box } from './node.js'
import type { Format } from './index.js'

/**
 * One scene, encoded in every format, compared against the Rust surface.
 *
 * # What this is for
 *
 * The two surfaces agree about the wire — `arena.test.ts` reads the same
 * `arena-cases.json` the Rust tables generate, and `chart.agreement.test.ts`
 * asserts the same bytes `chart_agreement.rs` does. Nothing asserted anything
 * about what happens **after** the wire. Measured rather than assumed: a
 * JavaScript-side encode default of `quality: 0.5` changed 27 of the 76 files
 * `just example` compares — every `jpg`, `webp` and `avif` — and left
 * `typecheck`, `lint-check`, `arena-cases-check`, `test-js` and the whole Rust
 * suite green. **The encoders had exactly one arm and it was a recipe that
 * installs a package and compiles a binary per scene.**
 *
 * # What it measures, and it is not correctness
 *
 * **An encode-agreement arm measures agreement, not correctness.** Both
 * surfaces calling one encoder with one wrong option agree perfectly, and this
 * file passes. What holds correctness is the fixtures — and they hold it for
 * PNG. **Nothing in this repository establishes that the JPEG this library
 * writes is a correct JPEG**; this establishes that two callers of the same
 * encoder produce the same bytes, which is a different and smaller claim.
 *
 * Read it beside `chart.agreement.test.ts`, which says the same thing about
 * geometry: three checks, three questions, none a substitute for another.
 *
 * # Why a refusal is a failure rather than a skip
 *
 * A format that will not encode a scene is a result. `just example` already
 * treats it that way and it earned that: a scale change once failed there in
 * nine seconds with no file compared at all, because ICO refuses anything over
 * 256x256 — a red run that was not a disagreement. An arm that caught the
 * refusal and skipped the format would have reported agreement on a format
 * neither surface wrote.
 *
 * # Why this scene has no curve, gradient, blend or glyph
 *
 * `fixtures.rs` measured the dividing line: on `linux-x86_64`, 15 of 23
 * fixtures are byte-identical to the macOS reference and the 8 that are not
 * are the ones with a curve, a gradient, a blend or a glyph. A committed hash
 * is a claim about every platform that runs this suite, so the scene is flat
 * rectangles — the half of that measurement that does not move.
 *
 * **This is the part I could not verify.** Run-to-run determinism was measured
 * here, on this machine, for all eight formats. Whether these hashes hold on
 * Linux and Windows is what CI answers on the first run, and if a format
 * proves platform-dependent the answer is a per-platform variant beside it —
 * the way the fixtures already do it — rather than a tolerance.
 *
 *
 * # Shown to go red from both sides, not from one
 *
 * Construction says the pair is symmetric; construction is not evidence, and
 * this file is an argument against taking it as such. So the same divergence
 * was introduced on each surface in turn — `quality: 0.5` defaulted in
 * `toBuffer` here, and `quality: Some(0.5)` defaulted in `Root::to_buffer`
 * there — and each turned **three of eight** formats red, `jpg`, `webp` and
 * `avif`, leaving `png`, `bmp`, `tiff`, `svg` and `raw` byte-identical.
 *
 * **Both provocations produced the same `jpg` hash**, `5e0afffe9d6b74ad`. Two
 * surfaces given one option arriving at one byte string is evidence they reach
 * the same encoder — so what this arm measures is the option and not the
 * language, which is the claim it has to be able to make.
 *
 * **That number was written down before the second measurement**, in the
 * report of this side's provocation, when no Rust provocation existed. It is a
 * prediction that held rather than an agreement noticed once both were in
 * hand, and the two read identically afterwards unless somebody says which
 * happened.
 *
 *
 * # One platform moves, and which
 *
 * `windows-x86_64` writes a different `tiff` and nothing else. Both surfaces
 * there produced `2d53a12a013360c6` — the Rust half in one CI run and this one
 * in another — so they agree with each other and differ from the macOS
 * reference, which makes it a platform row rather than the cross-surface
 * disagreement this arm exists to catch. `ubuntu-latest` moves nothing, so the
 * axis is Windows and not "every platform except the reference". The evidence
 * that it is the container rather than the picture, and the limit of the
 * claim, are in `flat-hashes.windows-x86_64.txt`.
 *
 * **To measure a platform this machine is not**, dispatch
 * `.github/workflows/encode-hashes.yml`. It prints the rows that differ and
 * uploads them; it writes nothing to the branch, because a row is a claim
 * about a platform and a bare hash is unreviewable.
 *
 * # Regenerating
 *
 * `UPDATE_ENCODE_HASHES=1 npx vitest run encode.agreement`, and the Rust side
 * asserts against the same file, so a regeneration that was not legitimate
 * fails there. The asset is committed rather than written on every run for the
 * reason `chart.agreement.test.ts` gives: `ci` runs the Rust tests first, so a
 * file written here would leave that side comparing against the previous run.
 */
const assets = (name: string) => fileURLToPath(new URL(`../../../crates/meo-canvas/tests/assets/encode/${name}`, import.meta.url))

/**
 * This host's variant suffix, `windows-x86_64` and so on.
 *
 * **Spelled the way Rust spells it**, because the two sides address one file
 * and a disagreement about its name is a disagreement about which asset each
 * is reading. Node says `win32` and `x64` where Rust says `windows` and
 * `x86_64`, so the mapping lives here and is the only place either side
 * translates. If it were wrong, one surface would read an overlay the other
 * did not and the arm would fail loudly rather than quietly agree — which is
 * why the mapping needs no test of its own.
 */
const hostVariant = (): string => {
  const os: Record<string, string> = { win32: 'windows', darwin: 'macos', linux: 'linux' }
  const arch: Record<string, string> = { x64: 'x86_64', arm64: 'aarch64' }
  return `${os[process.platform] ?? process.platform}-${arch[process.arch] ?? process.arch}`
}

/** The platform the base asset was written on, as `fixtures.rs` names it. */
const REFERENCE = 'macos-aarch64'

/** The `<format> <hash>` rows of an asset, with comments and blanks dropped. */
const rows = (text: string): Map<string, string> =>
  new Map(
    text
      .split('\n')
      .map(line => line.trim())
      .filter(line => line !== '' && !line.startsWith('#'))
      .map(line => {
        const at = line.indexOf(' ')
        if (at < 0) throw new Error(`\`${line}\` is not \`<format> <hash>\``)
        return [line.slice(0, at), line.slice(at + 1)] as const
      }),
  )

/**
 * The base rows with this platform's overlay applied.
 *
 * # Why an overlay rather than a whole file per platform
 *
 * `fixtures.rs` keeps a whole `expected.<os>-<arch>.png` because a PNG is one
 * indivisible artefact — there is no way to say "this image, but one pixel
 * differs". Eight independent rows are not like that, and a whole-file variant
 * would store seven values twice. **That duplication goes stale in one
 * direction and says nothing about it**: regenerate the base, forget the
 * variant, and seven rows disagree for a reason nobody intended while the one
 * that was supposed to differ looks untouched.
 *
 * # A row that has stopped differing is an error
 *
 * `fixtures.rs` states the rule — a variant exists **only where a platform is
 * measurably different** — and checks half of it: an absent variant means this
 * platform agrees, and the run finds out. A *present* variant that has become
 * identical to what it replaces is checked nowhere, and is indistinguishable
 * from one still doing work. So each overlay row is asserted to differ from
 * the row it replaces, and an equal one says to delete the line.
 */
const expectedRows = (): Map<string, string> => {
  const expected = rows(readFileSync(assets('flat-hashes.txt'), 'utf8'))
  const overlay = assets(`flat-hashes.${hostVariant()}.txt`)
  if (!existsSync(overlay)) return expected

  for (const [format, hash] of rows(readFileSync(overlay, 'utf8'))) {
    const base = expected.get(format)
    if (base === undefined) {
      throw new Error(`${overlay}: names \`${format}\`, which the base asset does not`)
    }
    if (base === hash) {
      throw new Error(
        `${overlay}: \`${format}\` no longer differs from the base asset. Delete the line — ` +
          'an override that agrees with what it overrides cannot be told from one still doing work.',
      )
    }
    expected.set(format, hash)
  }

  return expected
}

/** The formats `just example` writes for every scene. */
const FORMATS: readonly Format[] = ['png', 'jpg', 'webp', 'avif', 'bmp', 'tiff', 'svg', 'raw']

/**
 * FNV-1a, 64-bit, over the encoded bytes.
 *
 * Hand-written on both sides rather than taken from a dependency, because the
 * Rust workspace has no hasher and adding one to compare two byte strings is a
 * dependency bought for a test. Both implementations are checked against the
 * published vector below, so an arm that agreed because both hashers were
 * broken the same way cannot pass.
 */
const fnv1a = (bytes: Uint8Array): string => {
  let hash = 0xcbf2_9ce4_8422_2325n
  for (const byte of bytes) {
    hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100_0000_01b3n)
  }
  return hash.toString(16).padStart(16, '0')
}

/** The scene both surfaces build. Flat rectangles, for the reason above. */
const scene = () =>
  Root({
    width: 120,
    height: 80,
    backgroundColor: '#101014',
    children: [Box({ width: 40, height: 20, backgroundColor: '#ff0000' }), Box({ width: 30, height: 30, backgroundColor: '#00ff00' })],
  })

describe('the two surfaces encode the same bytes', () => {
  it('hashes the way the Rust side hashes', () => {
    // The published FNV-1a 64 vector for "a". A hasher that agrees with the
    // other side because both are wrong cannot survive this line.
    expect(fnv1a(new TextEncoder().encode('a'))).toBe('af63dc4c8601ec8c')
    expect(fnv1a(new Uint8Array())).toBe('cbf29ce484222325')
  })

  it('produces the hashes the Rust side checks against', async () => {
    const canvas = await scene()
    const measured = new Map<string, string>()

    try {
      for (const format of FORMATS) {
        let bytes: Buffer
        try {
          bytes = await canvas.toBuffer(format)
        } catch (cause) {
          // Not a skip. See the header.
          throw new Error(`${format}: the surface refused to encode this scene`, { cause })
        }
        measured.set(format, fnv1a(bytes))
      }
    } finally {
      canvas.release()
    }

    const expected = expectedRows()

    // **Regeneration writes the base only on the platform the base describes.**
    // Anywhere else it prints the rows that differ and writes nothing, because
    // writing here would overwrite a macOS asset with this platform's values
    // and every other platform would then be measured against whichever runner
    // regenerated last. What a non-reference platform produces is an overlay,
    // and an overlay is prose as much as it is a hash — which platform, and the
    // evidence that the difference is real — so a person writes it from these
    // lines rather than a process writing it for them. `fixtures.yml` makes the
    // same choice for the same reason: a workflow that commits an accepted
    // value turns a regression into a commit nobody reviewed.
    if (process.env['UPDATE_ENCODE_HASHES'] === '1') {
      const variant = hostVariant()
      if (variant === REFERENCE) {
        writeFileSync(assets('flat-hashes.txt'), `${[...measured].map(([format, hash]) => `${format} ${hash}`).join('\n')}\n`)
      } else {
        const moved = [...measured].filter(([format, hash]) => expected.get(format) !== hash)
        process.stdout.write(
          `\n${variant}: ${moved.length} of ${measured.size} formats differ from what this ` +
            `platform is checked against.\nPut these in flat-hashes.${variant}.txt with the ` +
            `evidence that the difference is real:\n` +
            `${moved.map(([format, hash]) => `${format} ${hash}`).join('\n')}\n`,
        )
      }
    }

    const lines = [...measured].map(([format, hash]) => `${format} ${hash}`)
    const against = [...expected].map(([format, hash]) => `${format} ${hash}`)
    expect(lines.join('\n')).toBe(against.join('\n'))
  })
})
