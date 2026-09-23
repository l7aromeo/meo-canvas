import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Root } from './root.js'
import { Box } from './node.js'
import type { Format } from './index.js'

/**
 * One flat scene encoded in every format on both surfaces and compared by hash:
 * agreement that two callers of one encoder write the same bytes, not that the
 * bytes are correct. Flat, since curves, gradients, blends and glyphs move across
 * platforms; a refused format fails. Regenerate with `UPDATE_ENCODE_HASHES=1`.
 */
const assets = (name: string) => fileURLToPath(new URL(`../../../crates/meo-canvas/tests/assets/encode/${name}`, import.meta.url))

/**
 * This host's variant suffix, spelled as Rust spells it (`windows-x86_64`, not
 * `win32-x64`), since both sides address one file; a wrong mapping fails loudly.
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
 * The base rows with this platform's overlay applied: eight independent rows need
 * no whole-file copy per platform. An overlay row equal to the row it replaces is
 * an error, since a variant exists only where a platform measurably differs.
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
 * FNV-1a, 64-bit, hand-written on both sides since the Rust workspace has no
 * hasher; both are checked against the published vector below.
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

    // Regeneration writes the base only on the platform it describes; elsewhere it
    // prints the differing rows and writes nothing, since an overlay is a claim about
    // a platform that a person writes with its evidence, as `fixtures.yml` does.
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
