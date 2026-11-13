// Which full-width punctuation Chrome narrows under its default
// `text-spacing-trim: normal`: every ordered pair of marks between ideographs,
// sentences, lines that end on a closing bracket, and min-content, in a face
// with `halt` and the same face without it. `chrome_text_spacing_trim.rs` reads
// the table.

import { readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/text-spacing-trim.tsv')
const FONTS = resolve(HERE, '../../../../crates/meo-canvas-core/tests/assets/fonts')

/**
 * Noto Sans CJK SC 2.004 cut to the characters below, with its layout features,
 * and the same cut without `halt`, `vhal`, `palt` or `vpal`: the face Chrome
 * narrows nothing in. Registered under the families the test uses.
 */
const FACES = {
  Trim: resolve(FONTS, 'NotoSansCJKsc-Trim.otf'),
  NoHalt: resolve(FONTS, 'NotoSansCJKsc-NoHalt.otf'),
}

/** The marks every ordered pair is drawn from: each class, and two that have none. */
const MARKS = [...'「『（〔［｛〈《【〖〘〝‘“｟」』）〕］｝〉》】〗〙〟’”｠、。，．：；・！？　']

/**
 * `lang` values the pairs are measured under. This renderer has no `lang`, so a
 * value that changed a pair would be a row it cannot match.
 */
const LANGS = ['', 'zh-Hans', 'zh-Hant', 'ja', 'ko']

/** Sentences laid out unwrapped, with each character's width. */
const SENTENCES = [
  '漢字、。」漢字',
  '「「漢字」」',
  '漢字」「漢字',
  '漢字・「漢字」・漢字',
  '漢字　「漢字」　漢字',
  '漢」 」漢',
  '漢」(漢',
  '「漢字」漢字',
  // Marks a neighbour narrows where they begin a line: at the start of the
  // text, and after a newline, which `white-space: pre` keeps.
  '」」漢字',
  '、。漢字',
  '漢\n」」漢',
  '漢\n、。漢',
  '漢\n「「漢',
]

/** Strings at a width where a closing mark ends a line, fits only narrowed, or fits whole. */
const WRAPPED = [
  ['漢字漢字漢、漢字', 96],
  ['漢字漢字漢字、漢字', 104],
  ['漢字漢字漢字、漢字', 112],
  ['漢字漢字漢字」漢字', 104],
  ['漢字漢字漢字」漢字', 112],
  ['漢字漢字漢字」', 104],
  ['漢字漢字漢字、」漢字', 104],
  ['漢字漢字漢字、」漢字', 111.99],
  ['漢字漢字漢字、」漢字', 112],
  ['漢字漢字漢字、」漢字', 120],
  ['漢字漢字漢字、」漢字', 128],
  ['漢字漢字漢字、」）漢字', 112],
  ['漢字漢字漢字、」）漢字', 120],
  ['漢字漢字漢字」「漢字', 104],
  ['漢字漢字漢字」「漢字', 112],
  ['漢字漢字漢字「漢字」', 96],
  ['漢字漢字漢」「漢字」', 96],
  ['漢字漢字漢字。漢字', 104],
  ['漢字漢字漢字）漢字', 104],
  ['漢字漢字漢字”漢字', 104],
  // The same where a break at a space begins the line.
  ['漢字 」」漢', 40],
  ['漢字 、。漢', 40],
  ['漢字 「「漢', 40],
]

/** Strings at `min-content`, where each piece between two opportunities is a line. */
const NARROWEST = ['漢「「漢', '漢」」漢', '漢、。漢', '漢」「漢', '漢」・漢', '漢字、。」漢字', '「漢字」「漢字」', '「「漢字」」']

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

  /**
   * Lays each text out at 16px with a 20px line, unwrapped when `width` is
   * null, and returns each character's width and the code-point indices lines
   * after the first start at, from each character's client rect.
   */
  const lay = (texts, family, lang, width) =>
    page.evaluate(
      ({ texts, family, lang, width }) =>
        texts.map(text => {
          const div = document.createElement('div')
          if (lang) div.lang = lang
          const size = width === null ? 'white-space:pre' : `width:${typeof width === 'number' ? `${width}px` : width}`
          div.style.cssText = `position:absolute;top:0;left:0;font:16px "${family}";line-height:20px;${size}`
          div.textContent = text
          document.body.append(div)
          const range = document.createRange()
          const widths = []
          const starts = []
          let unit = 0
          let index = 0
          let top = null
          for (const character of text) {
            range.setStart(div.firstChild, unit)
            range.setEnd(div.firstChild, unit + character.length)
            const rect = [...range.getClientRects()].find(box => box.width > 0 || box.height > 0)
            widths.push(rect === undefined ? 0 : Number(rect.width.toFixed(3)))
            if (character !== ' ' && rect !== undefined) {
              const at = Math.round(rect.top)
              if (top !== null && at > top) starts.push(index)
              top = at
            }
            unit += character.length
            index += 1
          }
          const box = div.getBoundingClientRect().width
          div.remove()
          return { widths, starts, box }
        }),
      { texts, family, lang, width },
    )

  const rows = []
  const cell = text => JSON.stringify(text)
  const list = values => values.join(',') || '-'

  for (const family of Object.keys(FACES)) {
    for (const lang of family === 'Trim' ? LANGS : ['']) {
      for (const first of MARKS) {
        const measured = await lay(
          MARKS.map(second => `漢${first}${second}漢`),
          family,
          lang,
          null,
        )
        const narrowed = measured.map(({ widths: [, a, b] }) => (a < 15.5 ? '1' : b < 15.5 ? '2' : '-')).join('')
        rows.push(['pairs', family, lang || '-', cell(first), cell(MARKS.join('')), narrowed].join('\t'))
      }
    }
    for (const [text, one] of (await lay(SENTENCES, family, '', null)).map((one, at) => [SENTENCES[at], one])) {
      rows.push(['line', family, '-', cell(text), '-', `${list(one.starts)};${list(one.widths)}`].join('\t'))
    }
    for (const [text, width] of WRAPPED) {
      const [one] = await lay([text], family, '', width)
      rows.push(['line', family, '-', cell(text), String(width), `${list(one.starts)};${list(one.widths)}`].join('\t'))
    }
    for (const [text, one] of (await lay(NARROWEST, family, '', 'min-content')).map((one, at) => [NARROWEST[at], one])) {
      rows.push(['min-content', family, '-', cell(text), one.box.toFixed(3), `${list(one.starts)};${list(one.widths)}`].join('\t'))
    }
  }

  const header = [
    '# Chrome, through `just conformance`: which full-width punctuation `text-spacing-trim: normal`, its default,',
    "# narrows, at 16px with a 20px line, from each character's client rect in an HTML block.",
    '# Written by packages/meo-canvas/tools/conformance/spacingtrim.mjs.',
    '#',
    '# `face` is `Trim`, Noto Sans CJK SC 2.004 cut to these characters with its layout features, or `NoHalt`,',
    "# the same cut without `halt`, `vhal`, `palt` or `vpal`. `lang` is the block's, `-` for none.",
    '#',
    '# `pairs` rows lay out `漢` + `first` + each of `tried` + `漢` unwrapped; `narrowed` has one character per',
    '# tried mark: `1` where `first` was narrowed, `2` where the tried mark was, `-` where neither.',
    '# `line` rows lay `text` out at `width`, unwrapped at `-`, and `min-content` rows at min-content, with',
    "# the block's width as `width`; `result` is the code-point indices lines after the first start at,",
    "# then `;`, then each character's width. Texts are JSON strings.",
    '#',
    '# kind\tface\tlang\ttext\twidth\tresult',
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
