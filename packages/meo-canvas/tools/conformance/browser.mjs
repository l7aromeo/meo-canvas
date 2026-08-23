// What every conformance measurement shares: a browser, a page, and a font
// that is provably the one we asked for.
//
// Chrome is the reference this project is written against, and until now every
// number taken from it came from a page written by hand for one question. That
// produced four tables and at least four measurement defects — a ruler that
// held a line box open, corners that could not tell a circle from an ellipse,
// a probe key that overwrote its own axis, an ink scan that computed its right
// edge twice. A harness a command drives can be re-run when a row is doubted;
// a page written once cannot.
//
// **The rows that agree are what make the rows that disagree trustworthy.** The
// first table this harness produced had all six rows fail, and the cause was
// the harness: a reference render with no width, wrapping into the column it
// was handed. A suite that is uniformly wrong looks exactly like a renderer
// with one defect per row. Read the agreeing rows first; if there are none,
// the instrument is the suspect.
//
// Nothing here compares pixels. Chrome's rasteriser is not ours and never will
// be, so a pixel diff would fail on antialiasing and say nothing: what crosses
// is geometry, colour at points derived from that geometry, ink spans against
// a stated threshold, and the contents of laid-out lines.

import { readFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { chromium } from 'playwright'

const HERE = dirname(fileURLToPath(import.meta.url))

/** The repository's own face, which every text measurement uses. */
export const FONT = {
  family: 'Fixture',
  path: resolve(HERE, '../../../../crates/meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf'),
}

/**
 * Opens a page with the font embedded and **proved** to have loaded.
 *
 * The proof is the point. `document.fonts.load` resolves whether or not the
 * face arrived, and a page that quietly fell back to the platform's default
 * measures a different font while reporting a number that looks fine — which
 * is what happened to every text measurement taken before the face was
 * inlined. This throws instead, naming the family, so a run either measures
 * the right face or does not finish.
 */
export async function open() {
  const font = await readFile(FONT.path)
  const browser = await chromium.launch()
  const page = await browser.newPage()

  await page.setContent(`<!doctype html>
<meta charset="utf-8">
<style>
  @font-face {
    font-family: '${FONT.family}';
    src: url(data:font/ttf;base64,${font.toString('base64')}) format('truetype');
  }
  html, body { margin: 0; padding: 0; }
</style>
<body></body>`)

  await page.evaluate(async family => {
    // 100px so the load is unambiguous, then the check, which is the assertion
    // this page exists for.
    await document.fonts.load(`100px "${family}"`)
    if (!document.fonts.check(`100px "${family}"`)) {
      throw new Error(`the face "${family}" did not load; every measurement from this page would be the fallback's`)
    }
  }, FONT.family)

  return {
    page,
    /** Closes the browser. Always, even where a measurement threw. */
    async close() {
      await browser.close()
    },
  }
}

/**
 * Writes a table beside the ones the walkers already read.
 *
 * A `.tsv` with a commented header rather than JSON: these are read by eye as
 * often as by a test, and a table of short rows reads better as columns.
 */
export function table(lines) {
  return `${lines.join('\n')}\n`
}
