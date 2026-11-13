// Where Chrome breaks a line of text with no spaces to break at: ideographs, kana,
// Latin beside them, punctuation, and Chrome's own rules for ASCII. A 0px block
// takes every break opportunity, so its lines are the opportunities themselves;
// wider blocks test the widths. `chrome_line_break.rs` reads the table.

import { readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/line-break.tsv')

/**
 * Noto Sans SC subset to the characters the width rows use, registered under the
 * family the test registers it under, so both measure the same file.
 */
const CJK = {
  family: 'Noto Sans SC',
  path: resolve(HERE, '../../../../crates/meo-canvas-core/tests/assets/fonts/NotoSansSC-Subset.ttf'),
}

/** Every printable ASCII character, `!` to `~`: the pairs Chrome's own table decides. */
const ASCII = Array.from({ length: 94 }, (_, at) => String.fromCharCode(0x21 + at))

/**
 * Characters of each line-breaking class in running text, with the CJK ones a
 * reader meets: small kana and `ー` (CJ, where strictness decides), iteration marks
 * (NS), full-width brackets and stops. Every ordered pair is measured.
 */
const CLASSES = [
  '漢',
  'あ',
  'ア', // ID
  'ぁ',
  'ッ',
  'ー',
  'ｯ', // CJ
  '々',
  'ゝ',
  'ヽ',
  '・',
  '‼', // NS
  '、',
  '。',
  '」',
  '）',
  '，',
  '］', // CL
  '「',
  '（',
  '［',
  '¿', // OP
  '！',
  '？', // EX
  '．',
  '：', // IS (full-width)
  '“',
  '”',
  '«',
  '»', // QU
  '¥',
  '￥',
  '€',
  '＄', // PR
  '％',
  '¢',
  '°', // PO
  '１',
  '٣', // NU
  'é',
  'Ω',
  'א', // AL, AL, HL
  '　',
  '–',
  '‐', // BA: ideographic space, en dash, hyphen
  '—', // B2
  ' ', // GL
  '…', // IN
  '​',
  '⁠', // ZW, WJ
  '́',
  '‍', // CM, ZWJ
  '👍',
  '🏽',
  '🇯', // EB, EM, RI
  '가',
  '각', // H2, H3
  '§',
  '´', // AI, BB
]

/**
 * Strings measured whole at 0px: sentences mixing scripts, where a rule needs more
 * context than a pair -- a number with its unit and currency, a word inside
 * brackets, Thai, which ICU breaks by dictionary.
 */
const SENTENCES = [
  '我们在Tokyo见面了，然后去了大阪看樱花。Hello世界こんにちは',
  '「漢字」は（かんじ）と読む。',
  '漢字(kanji)漢字[1]漢字',
  '価格は¥1,000円、税込￥1,100（10%）です。',
  '50%漢 ¥100漢 $100漢 100円漢 １２３漢',
  'well-known state-of-the-art long-term x86-64 C++ e.g. a/b',
  'what?yes http://x.y/z?q=1&r=2 (a b) ( a a )',
  'ペン・パイナップル・アップル・ペン',
  'ちょっと待ってください、ゃゅょ。',
  '一石二鳥……ですね——はい',
  '第1章 5月12日 東京-大阪 東京—大阪 東京/大阪',
  '한국어 텍스트를 한국어텍스트',
  'สวัสดีครับผมชื่อสมชาย',
  'Emoji👍🏽👍and🇯🇵flags',
  'a b 漢 漢 a　b 漢​漢 a⁠b',
]

/**
 * Strings measured at widths that leave a line part-full, with each line's start
 * recorded, and with the narrowest box that holds them -- `min-content`, which is
 * the widest piece between two opportunities. All of their characters are in the
 * subset, so the widths are the face's own.
 */
const WRAPPED = [
  '漢字'.repeat(20),
  'ひらがなカタカナ'.repeat(5),
  'アイウエオカキクケコサシスセソタチツテト',
  '我们在Tokyo见面了，然后去了大阪看樱花。Hello世界こんにちは',
  '漢字'.repeat(5) + '、。」' + '漢字'.repeat(2),
  '「漢字」「漢字」「漢字」「漢字」「漢字」「漢字」',
  'ちょっと待ってください。ちょっと待ってください。',
  'well-known state-of-the-art long-term',
  'Type-Token Ty-To L-T what?Yes x86-64',
  '价格是¥1,000（含税10%），共计100件商品。',
  'word '.repeat(20).trim(),
  // At 200px, twelve ideographs fill a line; the thirteenth character may not
  // begin the next, or may.
  '漢字'.repeat(6) + '。' + '漢字'.repeat(2),
  '漢字'.repeat(6) + '」' + '漢字'.repeat(2),
  '漢字'.repeat(5) + '漢「字' + '漢字'.repeat(2),
  '漢字'.repeat(6) + '々' + '漢字'.repeat(2),
  '漢字'.repeat(6) + 'ッ' + '漢字'.repeat(2),
  '漢字'.repeat(6) + 'ー' + '漢字'.repeat(2),
]

/** The widths a wrapped string is measured at, in px. */
const WIDTHS = [96, 200, 333]

/**
 * Strings whose face kerns across a break opportunity -- `アイ` sets 0.8px narrower
 * than `ア` and `イ` apart, `-T` 0.9px -- each measured at the width of a prefix
 * set whole plus a tenth of a pixel, where the prefix fits only with its kerning.
 */
const KERNED = [
  ['アイウエオカキクケコサシスセソタチツテト', [7, 13]],
  ['L-TL-TL-TL-TL-TL-TL-TL-TL-T', [11, 20]],
]

/** Both `text-spacing-trim` values: a row is kept only where they agree. See the header. */
const TRIMS = ['normal', 'space-all']

const browser = await open()
try {
  await browser.page.setViewportSize({ width: 1200, height: 800 })
  const bytes = await readFile(CJK.path)
  await browser.page.evaluate(
    async ({ family, data }) => {
      document.fonts.add(await new FontFace(family, `url(data:font/ttf;base64,${data})`).load())
    },
    { family: CJK.family, data: bytes.toString('base64') },
  )
  if (!(await browser.page.evaluate(family => document.fonts.check(`16px "${family}"`), CJK.family))) {
    throw new Error(`${CJK.family} did not load`)
  }
  await settle(browser.page)

  /**
   * Lays `text` out in a block `width` wide (a number, or `min-content`) and
   * returns the block's width and the code-point index each line after the first
   * starts at, read from each character's client rect. Spaces are skipped: one
   * that ends a line hangs on it.
   */
  const lay = (texts, width, trim) =>
    browser.page.evaluate(
      ({ texts, width, trim, family }) =>
        texts.map(text => {
          const div = document.createElement('div')
          div.style.cssText = `position:absolute;top:0;left:0;font:16px "${family}";line-height:20px;text-spacing-trim:${trim};width:${typeof width === 'number' ? `${width}px` : width}`
          div.textContent = text
          document.body.append(div)
          const node = div.firstChild
          const range = document.createRange()
          const starts = []
          let unit = 0
          let index = 0
          let top = null
          for (const character of text) {
            range.setStart(node, unit)
            range.setEnd(node, unit + character.length)
            const rect = [...range.getClientRects()].find(box => box.width > 0 || box.height > 0)
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
          return { starts, box }
        }),
      { texts, width, trim, family: CJK.family },
    )

  /** Measures with both trims and returns the first, or `null` where they disagree. */
  const agreed = async (texts, width) => {
    const [normal, spaced] = await Promise.all(TRIMS.map(trim => lay(texts, width, trim)))
    return normal.map((one, at) => {
      const other = spaced[at]
      const same = one.starts.join() === other.starts.join() && Math.abs(one.box - other.box) < 0.001
      return same ? one : null
    })
  }

  const rows = []
  let dropped = 0
  const cell = text => JSON.stringify(text)

  for (const set of [ASCII, CLASSES]) {
    const name = set === ASCII ? 'ascii' : 'class'
    for (const first of set) {
      const pairs = set.map(second => first + second)
      const measured = await agreed(pairs, 0)
      const breaks = []
      const tried = []
      measured.forEach((one, at) => {
        if (one === null) {
          dropped += 1
          return
        }
        tried.push(set[at])
        if (one.starts.length > 0) breaks.push(set[at])
      })
      rows.push([name, cell(first), cell(tried.join('')), cell(breaks.join(''))].join('\t'))
    }
  }

  for (const [text, one] of (await agreed(SENTENCES, 0)).map((one, at) => [SENTENCES[at], one])) {
    if (one === null) {
      dropped += 1
      continue
    }
    rows.push(['opportunity', cell(text), '0', one.starts.join(',') || '-'].join('\t'))
  }

  for (const width of WIDTHS) {
    for (const [text, one] of (await agreed(WRAPPED, width)).map((one, at) => [WRAPPED[at], one])) {
      if (one === null) {
        dropped += 1
        continue
      }
      rows.push(['wrap', cell(text), String(width), one.starts.join(',') || '-'].join('\t'))
    }
  }
  for (const [text, prefixes] of KERNED) {
    for (const length of prefixes) {
      const prefix = [...text].slice(0, length).join('')
      const set = await browser.page.evaluate(
        ({ prefix, family }) => {
          const span = document.createElement('span')
          span.style.cssText = `font:16px "${family}";white-space:pre`
          span.textContent = prefix
          document.body.append(span)
          const width = span.getBoundingClientRect().width
          span.remove()
          return width
        },
        { prefix, family: CJK.family },
      )
      const width = Number((set + 0.1).toFixed(3))
      const [one] = await agreed([text], width)
      if (one === null) {
        dropped += 1
        continue
      }
      rows.push(['wrap', cell(text), String(width), one.starts.join(',') || '-'].join('\t'))
    }
  }
  for (const [text, one] of (await agreed(WRAPPED, 'min-content')).map((one, at) => [WRAPPED[at], one])) {
    if (one === null) {
      dropped += 1
      continue
    }
    rows.push(['min-content', cell(text), one.box.toFixed(3), one.starts.join(',') || '-'].join('\t'))
  }

  const header = [
    '# Chrome, through `just conformance`: where a line of text breaks, at 16px in Noto Sans SC subset to',
    "# the width rows, line-height 20px, from each character's client rect in an HTML block.",
    '# Written by packages/meo-canvas/tools/conformance/linebreak.mjs.',
    '#',
    '# `ascii` and `class` rows lay out every two-character string `first` + one of `tried` in a 0px',
    '# block, which takes every break opportunity; `breaks` holds the seconds that went to a new line.',
    '# `opportunity` rows do the same for whole strings, and `wrap` rows at `width`; their `breaks` are',
    "# the code-point indices lines start at. `min-content` rows give the block's width as `width`.",
    '# Texts are JSON strings.',
    '#',
    "# Each row is measured at `text-spacing-trim: normal`, Chrome's default, and `space-all`, and",
    `# kept only where both agree: trimming narrows full-width punctuation, which is not breaking. ${dropped} dropped.`,
    '#',
    '# kind\ttext\twidth\tbreaks',
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
