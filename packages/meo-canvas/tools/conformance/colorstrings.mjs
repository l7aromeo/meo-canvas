// What Chrome paints for a colour string, as a div's `background-color`, read
// from screenshot pixels. Each string follows a sentinel declaration in the same
// rule, so one Chrome refuses leaves the sentinel painted; each is drawn over
// white and over black, so its alpha is the difference between the two.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/color-strings.tsv')

/** The box, small: one pixel is read, from its middle. */
const BOX = { width: 4, height: 4 }

/**
 * Painted before the string under test. A colour no row resolves to, so a
 * refused string reads as this one over both pages.
 */
const SENTINEL = [1, 2, 3]

/** Every string, by why it is here. */
const GROUPS = {
  // The names, hex lengths and functional notations the renderer has always read.
  basic: [
    'black',
    'rebeccapurple',
    '#f00',
    '#ff0000',
    '#ff0000ff',
    '#ff000080',
    '#ff00',
    '#808080',
    '#f2aa4c',
    '#3366cc',
    'rgb(255, 0, 0)',
    'rgb(255 0 0)',
    'rgba(255, 0, 0, 1)',
    'rgba(0, 0, 0, 0.5)',
    'hsl(0 100% 50%)',
    // A channel exactly half way between two bytes: 25.5.
    'hsl(210 80% 50%)',
  ],
  // CSS Color 4's own lab() and lch() examples, each spelled in both, and
  // percentages, `none`, turns, a negative or out-of-range hue, chroma and
  // lightness, and the white whose conversion lands just past the gamut.
  lab: [
    'lab(29.2345% 39.3825 20.0664)',
    'lab(52.2345% 40.1645 59.9971)',
    'lab(60.2345 -5.3654 58.956)',
    'lab(62.2345% -34.9638 47.7721)',
    'lab(67.5345% -8.6911 -41.6019)',
    'lch(29.2345% 44.2 27)',
    'lch(52.2345% 72.2 56.2)',
    'lch(60.2345 59.2 95.2)',
    'lch(62.2345% 59.2 126.2)',
    'lch(67.5345% 42.5 258.2)',
    'lab(50 40% 30%)',
    'lab(none 40 30)',
    'lch(50 40 0.25turn)',
    'lch(50 40 -30)',
    'lch(50 200 30)',
    'lab(1e30 0 0)',
    'oklab(1 0 0)',
    'oklch(1 0 0)',
  ],
  // The modern hsl() and hwb() syntax reads a bare number as a percentage.
  hsl: ['hsl(120 1 0.5)', 'hsl(120 100 50)', 'hsl(30 60 40)', 'hsl(30 60% 40)', 'hsl(30 60 40%)', 'hwb(120 20 30)'],
  // color() in each space CSS Color 4 names, the relative form, and alpha.
  color: [
    'color(srgb 1 0 0)',
    'color(srgb 1.0 0.0 0.0 / 1)',
    'color(srgb 0 0 0 / 0.5)',
    'color(srgb 1.2 -0.1 0.5)',
    'color(srgb 100% 0% 0%)',
    'color(srgb none 0 0)',
    'color(srgb-linear 0.5 0.5 0.5)',
    'color(srgb-linear 0.214 0.214 0.214)',
    'color(srgb-linear 50% 50% 50%)',
    'color(xyz 0.4124 0.2126 0.0193)',
    'color(xyz 0.2 0.2 0.2)',
    'color(xyz-d65 0.2 0.2 0.2)',
    'color(xyz-d50 0.2 0.2 0.2)',
    'color(xyz 50% 50% 50%)',
    'color(display-p3 1 0 0)',
    'color(rec2020 1 0 0)',
    'color(a98-rgb 1 0 0)',
    'color(prophoto-rgb 1 0 0)',
    'color(from red srgb calc(r / 2) g b)',
    'color(from color(srgb 0.5 0.5 0.5) srgb r g b)',
    'rgb(from color(srgb 1 0 0) r g b)',
    'color(srgb 1 0 0 / 0)',
    'COLOR(xyz 1 0 0 / 0)',
    'color(srgb-linear 1 0 0/0)',
    'color(from red srgb r g b / 0)',
    'color(srgb 1 0 0 / 2)',
  ],
  // Strings Chrome refuses: malformed color(), a space no profile defines,
  // legacy commas or a fourth value where the syntax has none, and the
  // spellings outside CSS that other parsers read.
  refused: [
    'color(srgb 1 0 0 0)',
    'color(srgb 0.5 0.5 0.5 0.25)',
    'color(xyz 1 0 0 0)',
    'color(srgb-linear 1 0 0 0)',
    'color(srgb 1,0,0)',
    'color(xyz 0.2, 0.2, 0.2)',
    'color(srgb 1 0 0, 0.5)',
    'color(srgb 1 0 0 / 0.5 0.5)',
    'color (srgb 1 0 0)',
    'color(srgb 1 0)',
    'not-a-colour',
    'color(--profile 1 0 0)',
    'lab(50, 40, 30)',
    'rgb(1 2 3 4)',
    'hsl(120, 100, 50)',
    'ff0000',
    'f00',
    'hwba(120, 0%, 0%, 1)',
    'hsv(120, 100%, 100%)',
    'hsva(120, 100%, 100%, 1)',
  ],
  // A space Chrome paints that the reader does not convert.
  unread: ['color(display-p3-linear 1 0 0)'],
}

const browser = await open()
try {
  await browser.page.setViewportSize(BOX)
  const rows = []
  for (const [group, strings] of Object.entries(GROUPS)) {
    for (const css of strings) {
      const over = []
      for (const page of ['#ffffff', '#000000']) {
        await browser.page.setContent(
          `<!doctype html><style>html,body{margin:0;background:${page}}#m{position:absolute;inset:0;background-color:rgb(${SENTINEL.join(',')})}</style><div id="m"></div>`,
        )
        // Set through the style sheet rather than the element's style object,
        // whose setter would drop a refused string before the cascade sees it.
        await browser.page.addStyleTag({ content: `#m{background-color:rgb(${SENTINEL.join(',')});background-color:${css}}` })
        await settle(browser.page)
        const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))
        over.push(pixel(shot, 2, 2).slice(0, 3))
      }
      const refused = over.every(rgb => rgb.every((value, index) => value === SENTINEL[index]))
      rows.push([JSON.stringify(css), group, refused ? 'refused' : 'painted', ...over.flat()].join('\t'))
    }
  }

  const written = table([
    '# Chrome, through `just conformance`. What a colour string paints as a background.',
    '# Written by packages/meo-canvas/tools/conformance/colorstrings.mjs.',
    '#',
    '# Each string is a div’s `background-color`, after a sentinel declaration of',
    `# rgb(${SENTINEL.join(', ')}) in the same rule: a string Chrome refuses leaves the sentinel,`,
    '# and the row says `refused`. Each is drawn over a white page and over a black',
    '# one, so an opaque colour reads the same on both and an alpha is one minus',
    '# their difference over 255. Read from screenshot pixels.',
    '#',
    '# css\tgroup\tchrome\twhite r\tg\tb\tblack r\tg\tb',
    ...rows,
  ])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`colour strings: ${rows.length} strings -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
