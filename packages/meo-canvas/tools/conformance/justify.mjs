// Where Chrome puts a justified line's slack, character by character: which
// characters take a share after them, around Latin words and spaces, beside
// narrowed punctuation, on a last line and before a newline; and, swept over
// code points, which characters take a share at all. `chrome_justify.rs`
// reads the table.

import { readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/justify.tsv')
const FONTS = resolve(HERE, '../../../../crates/meo-canvas-core/tests/assets/fonts')

/**
 * Noto Sans CJK SC 2.004 cut to the text-spacing-trim table's characters, with
 * printable ASCII, and Noto Sans SC cut for the line-break table, which holds
 * the kana, Hangul and other scripts the classes below need.
 */
const FACES = {
  Trim: resolve(FONTS, 'NotoSansCJKsc-Trim.otf'),
  Wide: resolve(FONTS, 'NotoSansSC-Subset.ttf'),
}

/**
 * One character of each class whose share is in question. Each is set four
 * times and between Latin letters, on one line justified as a last line is
 * under `text-align-last: justify`, so a share can only come from it.
 */
const CLASSES = [...'漢かカｶｱ한ＡＺ１ㄅ、。，「」（）・ー～！　①㈱…—‥éก']

/** Justifies a line that is the only one. */
const LAST = 'text-align-last:justify'

/** `[text, css, face]`, each laid out justified, 120px wide unless `css` says otherwise. */
const ROWS = [
  ...CLASSES.flatMap(character => [
    [character.repeat(4), LAST, 'Wide'],
    [`a${character}a${character}a`, LAST, 'Wide'],
  ]),
  ['aaaaa', LAST, 'Wide'],
  ['漢字漢字漢字漢字漢字', '', 'Trim'],
  ['漢字漢字漢字漢字漢字', 'width:100px', 'Trim'],
  ['かなかなかなかなかな', '', 'Wide'],
  ['漢字abc漢字漢字漢字漢字', '', 'Trim'],
  ['abc漢字漢字漢字漢字漢字', '', 'Trim'],
  ['漢字 漢字 漢字漢字漢字漢字', '', 'Trim'],
  ['ab cd 漢字漢字漢字漢字漢字', '', 'Trim'],
  ['Flower of Paradise in bloom', '', 'Trim'],
  ['漢字、漢字。漢字漢字漢字', '', 'Trim'],
  ['漢「漢字」漢字漢字漢字漢字', '', 'Trim'],
  ['漢字　漢字漢字漢字漢字', '', 'Trim'],
  // Narrowed by `text-spacing-trim`, then spread.
  ['漢字、。漢字漢字漢字漢字', 'width:128px', 'Trim'],
  ['漢字」「漢字漢字漢字漢字', 'width:128px', 'Trim'],
  // Lines a justification leaves alone, and one it spreads with letter-spacing.
  ['漢字漢字漢字漢字漢字', 'text-align-last:auto', 'Trim'],
  ['漢字漢字漢字漢\n漢字', 'white-space:pre-line', 'Trim'],
  ['漢字 漢字\n漢字漢字漢字漢字漢字', 'white-space:pre-line', 'Trim'],
  ['漢字漢字漢字漢字漢字', 'letter-spacing:2px', 'Trim'],
  // A Latin word that runs on into ideographs across a space and without one.
  ['Tokyo東京Tokyo東京Tokyo東京', '', 'Wide'],
  ['abcdefgh漢字漢字漢字', '', 'Trim'],
]

/**
 * The code points swept for the share class, `[from, to, step]`: every one
 * where classes mix, and a sample of the large blocks of one script.
 */
const SWEPT = [
  [0x20, 0x33ff, 1],
  [0x3400, 0x4dbf, 97],
  [0x4e00, 0x9fff, 61],
  [0xa000, 0xabff, 1],
  [0xac00, 0xd7ff, 127],
  [0xf900, 0xfffd, 1],
  [0x1f000, 0x1faff, 1],
  [0x20000, 0x2ffff, 1021],
  [0x30000, 0x3134f, 997],
]

/**
 * How much wider than set alone a character's rect must be in `aXaXa`, a
 * 400px line justified, to have taken a share: a share there is about 70px,
 * and shaping in context moves a width by a pixel or two.
 */
const SHARED = 5

const browser = await open()
try {
  const page = browser.page
  await page.setViewportSize({ width: 1200, height: 800 })
  for (const [family, path] of Object.entries(FACES)) {
    const data = (await readFile(path)).toString('base64')
    await page.evaluate(
      async ({ family, data }) => {
        document.fonts.add(await new FontFace(family, `url(data:font/otf;base64,${data})`).load())
      },
      { family, data },
    )
    if (!(await page.evaluate(family => document.fonts.check(`16px "${family}"`), family))) {
      throw new Error(`${family} did not load`)
    }
  }
  await settle(page)

  /** Each character as `line:x:width` against the block, or `-` for a newline or one with no box. */
  const measured = await page.evaluate(
    rows =>
      rows.map(([text, css, family]) => {
        const div = document.createElement('div')
        div.style.cssText = `position:absolute;top:0;left:0;font:16px "${family}";line-height:20px;width:120px;text-align:justify;${css}`
        div.textContent = text
        document.body.append(div)
        const origin = div.getBoundingClientRect()
        const range = document.createRange()
        const characters = []
        let unit = 0
        for (const character of text) {
          range.setStart(div.firstChild, unit)
          range.setEnd(div.firstChild, unit + character.length)
          const rect = [...range.getClientRects()].find(box => box.width > 0 || box.height > 0)
          characters.push(
            rect === undefined || character === '\n'
              ? '-'
              : `${Math.round((rect.top - origin.top) / 20)}:${Number((rect.left - origin.left).toFixed(3))}:${Number(rect.width.toFixed(3))}`,
          )
          unit += character.length
        }
        div.remove()
        return characters.join(' ')
      }),
    ROWS,
  )

  const rows = ROWS.map(([text, css, family], at) => ['line', family, JSON.stringify(text), css || '-', measured[at]].join('\t'))

  // A mark joins the letter before it, a format character has no box and a
  // space is a gap: none can be read this way, so none is swept.
  const points = []
  for (const [from, to, step] of SWEPT) {
    for (let point = from; point <= to; point += step) {
      if (!/[\p{M}\p{Cf}\p{Zs}\p{Cc}\p{Cs}\p{Co}\p{Cn}]/u.test(String.fromCodePoint(point))) points.push(point)
    }
  }
  const shared = await page.evaluate(
    ({ points, threshold }) => {
      const div = document.createElement('div')
      div.style.cssText =
        'position:absolute;top:0;left:0;font:16px serif;line-height:20px;width:400px;text-align:justify;text-align-last:justify;white-space:nowrap'
      const alone = document.createElement('span')
      alone.style.cssText = 'font:16px serif;white-space:pre'
      document.body.append(div, alone)
      const range = document.createRange()
      return points.map(point => {
        const character = String.fromCodePoint(point)
        div.textContent = `a${character}a${character}a`
        alone.textContent = character
        range.setStart(div.firstChild, 1)
        range.setEnd(div.firstChild, 1 + character.length)
        return range.getBoundingClientRect().width - alone.getBoundingClientRect().width > threshold
      })
    },
    { points, threshold: SHARED },
  )
  // Runs of swept points with one answer, so the table stays readable.
  const runs = []
  points.forEach((point, at) => {
    const last = runs[runs.length - 1]
    if (last !== undefined && last.shared === shared[at]) {
      last.to = point
      last.count += 1
    } else {
      runs.push({ from: point, to: point, shared: shared[at], count: 1 })
    }
  })
  const hex = point => point.toString(16).toUpperCase().padStart(4, '0')
  for (const run of runs) rows.push(['class', hex(run.from), hex(run.to), String(run.count), run.shared ? 'shares' : '-'].join('\t'))
  const header = [
    "# Chrome, through `just conformance`: where `text-align: justify` puts a line's slack, at 16px with a 20px",
    "# line in a 120px block unless `css` says otherwise, from each character's client rect.",
    '# Written by packages/meo-canvas/tools/conformance/justify.mjs.',
    '#',
    '# A `line` row: `face` is `Trim`, the cut of Noto Sans CJK SC 2.004 the text-spacing-trim table uses, or `Wide`, the cut',
    '# of Noto Sans SC the line-break table uses. `characters` has one entry per code point: `line:x:width`',
    '# against the block, or `-` for a newline or a character with no box. Texts are JSON strings.',
    '#',
    '# A `class` row is a run of swept code points with one answer: `from`, `to`, how many were swept between',
    '# them, and `shares` where each took a share of `aXaXa` justified on one 400px line, `-` where none did.',
    '# Marks, format characters, spaces and unassigned code points are not swept. Blocks of one script are',
    '# sampled: U+3400 every 97th, U+4E00 every 61st, U+AC00 every 127th, U+20000 every 1021st, U+30000 every 997th.',
    '#',
    '# kind\tface\ttext\tcss\tcharacters',
    '# kind\tfrom\tto\tswept\tshares',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    console.log(`wrote ${DESTINATION}`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
