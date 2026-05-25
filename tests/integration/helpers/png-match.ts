import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import pixelmatch from 'pixelmatch'
import { PNG } from 'pngjs'

const FIXTURES_DIR = join(dirname(fileURLToPath(import.meta.url)), '../../fixtures/renders')
const UPDATE_FIXTURES = process.env.UPDATE_FIXTURES === '1'

/** Per-channel tolerance for anti-aliasing drift across skia builds (0–1 scale). */
const PIXEL_THRESHOLD = 0.1

/** Fail if more than this share of pixels differ after thresholding. */
const DEFAULT_MAX_DIFF_RATIO = 0.01

/** Charts draw bars, axes, and grid lines — more cross-platform skia variance than text-only scenes. */
const FIXTURE_MAX_DIFF_RATIO: Record<string, number> = {
  'bar-chart-minimal': 0.05,
}

function decodePng(buffer: Buffer): PNG {
  return PNG.sync.read(buffer)
}

export async function expectPngMatch(name: string, buffer: Buffer): Promise<void> {
  const fixturePath = join(FIXTURES_DIR, `${name}.png`)
  const maxDiffRatio = FIXTURE_MAX_DIFF_RATIO[name] ?? DEFAULT_MAX_DIFF_RATIO

  if (UPDATE_FIXTURES || !existsSync(fixturePath)) {
    mkdirSync(FIXTURES_DIR, { recursive: true })
    writeFileSync(fixturePath, buffer)
  }

  const actual = decodePng(buffer)
  const expected = decodePng(readFileSync(fixturePath))

  expect(actual.width).toBe(expected.width)
  expect(actual.height).toBe(expected.height)

  const diffPixels = pixelmatch(actual.data, expected.data, undefined, actual.width, actual.height, {
    threshold: PIXEL_THRESHOLD,
  })

  const totalPixels = actual.width * actual.height
  const diffRatio = diffPixels / totalPixels

  expect(diffRatio).toBeLessThanOrEqual(maxDiffRatio)
}
