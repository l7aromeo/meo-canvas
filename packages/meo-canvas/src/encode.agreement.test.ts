import { readFileSync, writeFileSync } from 'node:fs'
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
 * # Regenerating
 *
 * `UPDATE_ENCODE_HASHES=1 npx vitest run encode.agreement`, and the Rust side
 * asserts against the same file, so a regeneration that was not legitimate
 * fails there. The asset is committed rather than written on every run for the
 * reason `chart.agreement.test.ts` gives: `ci` runs the Rust tests first, so a
 * file written here would leave that side comparing against the previous run.
 */
const asset = fileURLToPath(new URL('../../../crates/meo-canvas/tests/assets/encode/flat-hashes.txt', import.meta.url))

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

    const lines = [...measured].map(([format, hash]) => `${format} ${hash}`)
    if (process.env['UPDATE_ENCODE_HASHES'] === '1') {
      writeFileSync(asset, `${lines.join('\n')}\n`)
    }

    expect(lines.join('\n')).toBe(readFileSync(asset, 'utf8').trim())
  })
})
