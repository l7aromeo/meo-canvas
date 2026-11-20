// What a `filter` covers: the element's whole rendering, children, clipped
// overflow and box shadow included, filtered once as a group in its own
// coordinate space, before its mask and opacity. Read at points derived from the
// geometry, each beside the reason it separates a group from per-draw filtering.

import { readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { FONT, open, settle, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const ASSETS = resolve(HERE, '../../../../crates/meo-canvas/tests/assets')
const DESTINATION = resolve(ASSETS, 'chrome/filter-whole.tsv')

/** The page every case is drawn on, white, each case at its top-left corner. */
const PAGE = 240

/**
 * An 80x80 opaque blue disc on transparency, `filter-disc.png`: an image whose
 * own box clips what it draws, and whose silhouette a zero-blur shadow recolours.
 */
const DISC = 80

const BLUE = 'rgb(0, 0, 255)'
const GREEN = 'rgb(0, 160, 0)'

/** The disc as an `<img>`, with more inline style. */
const img = (src, style = '') => `<img src="${src}" style="display:block;width:${DISC}px;height:${DISC}px;${style}">`

/** A block of `colour`, with more inline style. */
const block = (width, height, colour, style = '') => `<div style="width:${width}px;height:${height}px;background:${colour};${style}"></div>`

/** An absolutely positioned block. */
const placed = (left, top, width, height, colour) => block(width, height, colour, `position:absolute;left:${left}px;top:${top}px`)

/**
 * The cases, by the name `chrome_filter_whole.rs` builds each one under, with
 * the points read and why each separates the two readings of `filter`. Every
 * point sits at least two pixels from any edge, so antialiasing reads nowhere.
 */
function cases(disc) {
  return [
    {
      name: 'image shadow past its box',
      html: `<div style="width:160px;height:80px">${img(disc, 'filter:drop-shadow(80px 0px 0px red)')}</div>`,
      points: [
        [40, 40, "the disc's centre"],
        [120, 40, "the shadow's centre, wholly outside the image box"],
      ],
    },
    {
      name: 'image shadow at its edge',
      html: `<div style="width:160px;height:80px">${img(disc, 'filter:drop-shadow(4px 0px 0px red)')}</div>`,
      points: [
        [82, 40, 'the shadow two pixels past the image box'],
        [86, 40, 'past the shadow'],
      ],
    },
    {
      name: 'box shadowing an image child',
      html: `<div style="width:160px;height:80px"><div style="width:80px;height:80px;filter:drop-shadow(80px 0px 0px red)">${img(disc)}</div></div>`,
      points: [[120, 40, "the child's shadow, outside the child's box"]],
    },
    {
      name: 'one shadow under both children',
      html:
        `<div style="position:relative;width:120px;height:100px;filter:drop-shadow(-30px -30px 0px red)">` +
        `${placed(0, 0, 60, 40, GREEN)}${placed(40, 40, 40, 40, BLUE)}</div>`,
      points: [
        [20, 20, "the first child, under the second's shadow: per draw it is red"],
        [15, 45, "the second child's shadow over nothing"],
      ],
    },
    {
      name: 'one opacity over both children',
      html:
        `<div style="position:relative;width:100px;height:60px;filter:opacity(0.5)">` + `${placed(0, 0, 60, 60, GREEN)}${placed(40, 0, 60, 60, BLUE)}</div>`,
      points: [
        [20, 30, 'the first child alone'],
        [50, 30, 'the overlap, the second child alone at half: per draw both show'],
      ],
    },
    {
      name: 'blur past an image child',
      html: `<div style="padding:20px;width:80px;height:80px"><div style="width:80px;height:80px;filter:blur(4px)">${img(disc)}</div></div>`,
      points: [
        [60, 60, "the disc's centre"],
        [102, 60, "two pixels past the child's box, which the blur reaches"],
      ],
    },
    {
      name: 'shadow of clipped overflow',
      html: `<div style="width:160px;height:80px"><div style="width:80px;height:80px;overflow:hidden;filter:drop-shadow(80px 0px 0px red)">${block(160, 80, BLUE)}</div></div>`,
      points: [
        [120, 40, 'the shadow of what the clip kept, outside the clip'],
        [158, 40, 'the shadow ends where the clip did'],
      ],
    },
    {
      name: 'shadow of a rounded clip',
      html: `<div style="width:160px;height:80px"><div style="width:80px;height:80px;overflow:hidden;border-radius:40px;filter:drop-shadow(80px 0px 0px red)">${block(80, 80, BLUE)}</div></div>`,
      points: [
        [120, 40, "the rounded shadow's centre"],
        [83, 3, "the shadow's rounded corner, clear"],
      ],
    },
    {
      name: 'opacity after the filter',
      html: `<div style="width:160px;height:80px"><div style="width:80px;height:80px;opacity:0.5;filter:drop-shadow(80px 0px 0px red)">${img(disc)}</div></div>`,
      points: [
        [40, 40, 'the disc at half'],
        [120, 40, 'its shadow at half too'],
      ],
    },
    {
      name: 'blend after the filter',
      html: `<div style="width:160px;height:80px;background:rgb(255,255,0)"><div style="width:80px;height:80px;mix-blend-mode:multiply;filter:drop-shadow(80px 0px 0px rgb(0,255,255))">${img(disc)}</div></div>`,
      points: [
        [40, 40, 'blue multiplied over yellow'],
        [120, 40, 'the cyan shadow multiplied over yellow'],
      ],
    },
    {
      name: 'shadow scaled with its box',
      html: `<div style="width:200px;height:100px"><div style="width:40px;height:40px;transform-origin:0 0;transform:scale(2);filter:drop-shadow(20px 0px 0px red)">${block(40, 40, BLUE)}</div></div>`,
      points: [
        [110, 40, 'the shadow forty page pixels over: twenty would end at 100'],
        [125, 40, 'past the shadow'],
      ],
    },
    {
      name: 'shadow turned with its box',
      html: `<div style="width:200px;height:200px"><div style="margin:60px;width:40px;height:40px;transform:rotate(90deg);filter:drop-shadow(30px 0px 0px red)">${block(40, 40, BLUE)}</div></div>`,
      points: [
        [80, 110, 'the shadow below: the offset turns with the box'],
        [110, 80, 'right of the box, where an unturned offset would put it'],
      ],
    },
    {
      name: 'blur scaled with its box',
      html: `<div style="width:200px;height:100px"><div style="width:40px;height:40px;transform-origin:0 0;transform:scale(2);filter:blur(5px)">${block(40, 40, BLUE)}</div></div>`,
      points: [[92, 40, 'twelve page pixels out, inside a ten-pixel standard deviation but not a five']],
    },
    {
      // The bar is 80x20 at 0,80; moved, turned and doubled about its middle it
      // covers 80..120 by 10..170, far outside the filtered box and its own.
      name: 'a transformed child inside the group',
      html: `<div style="width:240px;height:200px"><div style="margin-top:80px;width:40px;height:40px;filter:drop-shadow(100px 0px 0px red)">${block(80, 20, BLUE, 'transform:translate(60px, 0px) rotate(90deg) scale(2)')}</div></div>`,
      points: [
        [100, 25, "the bar's top end: only a moved, turned and doubled bar reaches it"],
        [200, 25, 'its shadow'],
        [100, 5, 'above the bar'],
        [130, 90, 'right of the bar, which an unturned doubled bar covers'],
      ],
    },
    {
      // One filtered box scaled to nothing and one off the page, then a block
      // after both, which a transform or save left open would hide or move.
      name: 'a filter that shows nothing',
      html: `<div style="position:relative;width:240px;height:80px"><div style="width:40px;height:40px;transform:scale(0);filter:drop-shadow(80px 0px 0px red)">${block(40, 40, BLUE)}</div>${block(40, 40, BLUE, 'position:absolute;left:-300px;top:0px;filter:drop-shadow(20px 0px 0px red)')}${placed(0, 40, 40, 40, GREEN)}</div>`,
      points: [
        [20, 20, 'the box scaled to nothing'],
        [100, 20, 'its shadow, scaled with it'],
        [20, 60, 'the block after both'],
      ],
    },
    {
      name: 'nested filters',
      html: `<div style="width:200px;height:80px"><div style="width:40px;height:40px;filter:drop-shadow(50px 0px 0px red)"><div style="width:40px;height:40px;filter:drop-shadow(0px 30px 0px ${GREEN})">${block(40, 40, BLUE)}</div></div></div>`,
      points: [
        [20, 50, 'the inner shadow'],
        [70, 20, 'the outer shadow of the box'],
        [70, 50, 'the outer shadow of the inner shadow'],
      ],
    },
    {
      name: 'shadow of a box shadow',
      html: `<div style="width:200px;height:80px">${block(40, 40, BLUE, `box-shadow:40px 0 0 ${GREEN};filter:drop-shadow(0px 30px 0px red)`)}</div>`,
      points: [[60, 50, 'the drop shadow of the box shadow']],
    },
    {
      name: "an ancestor's clip cuts the shadow",
      html: `<div style="width:100px;height:80px;overflow:hidden"><div style="width:80px;height:80px;filter:drop-shadow(80px 0px 0px red)">${img(disc)}</div></div>`,
      points: [
        [90, 40, 'the shadow inside the clip'],
        [110, 40, 'the shadow beyond it'],
      ],
    },
    {
      name: 'the mask after the filter',
      html: `<div style="width:160px;height:80px"><div style="width:160px;height:80px;-webkit-mask-image:linear-gradient(90deg, black 50%, transparent 50%);filter:drop-shadow(80px 0px 0px red)">${block(80, 80, BLUE)}</div></div>`,
      points: [
        [40, 40, 'the box, where the mask shows'],
        [120, 40, 'the shadow, where the mask hides'],
      ],
    },
    {
      name: 'the filter as a backdrop root',
      html: `<div style="position:relative;width:80px;height:80px;background:rgb(255,0,0)"><div style="position:absolute;inset:0;filter:brightness(1)">${block(80, 80, 'transparent', 'backdrop-filter:invert(1)')}</div></div>`,
      points: [[40, 40, 'the backdrop sees only what the filtered ancestor drew']],
    },
    {
      name: 'no backdrop root',
      html: `<div style="position:relative;width:80px;height:80px;background:rgb(255,0,0)"><div style="position:absolute;inset:0">${block(80, 80, 'transparent', 'backdrop-filter:invert(1)')}</div></div>`,
      points: [[40, 40, 'the backdrop sees the page']],
    },
    {
      name: 'icon tint',
      html: `<div style="width:80px;height:80px;overflow:hidden"><div style="position:relative;left:-80px;width:80px;height:80px;filter:drop-shadow(80px 0px 0px red)">${img(disc)}</div></div>`,
      points: [
        [40, 40, "the disc's silhouette, in the shadow's colour"],
        [3, 3, 'outside the disc, clear'],
      ],
    },
    {
      name: 'icon tint on the image',
      html: `<div style="width:80px;height:80px;overflow:hidden">${img(disc, 'position:relative;left:-80px;filter:drop-shadow(80px 0px 0px red)')}</div>`,
      points: [
        [40, 40, "the disc's silhouette, in the shadow's colour"],
        [3, 3, 'outside the disc, clear'],
      ],
    },
  ]
}

/** An `I` in the fixture face: one stem, wide enough to sample at its middle. */
const TEXT = { size: 80, offset: 100 }

/** The same `I` in a line box a quarter of an em tall, below a padding that keeps its ink on the page. */
const TIGHT = { lineHeight: 0.25, top: 60 }

const disc = `data:image/png;base64,${(await readFile(resolve(ASSETS, 'filter-disc.png'))).toString('base64')}`
const browser = await open()
try {
  await browser.page.setViewportSize({ width: PAGE, height: PAGE })
  const shoot = async html => {
    await browser.page.evaluate(html => {
      document.body.style.background = 'white'
      document.body.innerHTML = html
    }, html)
    await settle(browser.page)
    return read(await browser.page.screenshot({ clip: { x: 0, y: 0, width: PAGE, height: PAGE } }))
  }

  const rows = []
  for (const { name, html, points } of cases(disc)) {
    const shot = await shoot(html)
    for (const [x, y, why] of points) rows.push([name, x, y, ...pixel(shot, x, y), why].join('\t'))
  }

  // The text row's points come from the glyph: the middle of the stem's blue
  // ink on the line through the glyph's middle, and that again past the offset.
  const name = 'text'
  const shot = await shoot(
    `<div style="font:${TEXT.size}px ${FONT.family};line-height:1;color:${BLUE};filter:drop-shadow(${TEXT.offset}px 0px 0px red)">I</div>`,
  )
  const y = Math.floor(TEXT.size / 2)
  const stem = []
  for (let x = 0; x < PAGE; x++) {
    const [r, g, b] = pixel(shot, x, y)
    if (b === 255 && r === 0 && g === 0) stem.push(x)
  }
  if (stem.length < 5) throw new Error(`the stem inks ${stem.length} solid pixels at y ${y}; too narrow to sample`)
  const x = stem[Math.floor(stem.length / 2)]
  rows.push([name, x, y, ...pixel(shot, x, y), "the stem's middle"].join('\t'))
  rows.push([name, x + TEXT.offset, y, ...pixel(shot, x + TEXT.offset, y), "the stem's shadow"].join('\t'))

  // A line box shorter than the glyph, so the stem inks above the text's own
  // box, read at the stem's top beyond it: a group bounded by boxes alone
  // would cut it.
  const tight = 'text past its line box'
  const shotTight = await shoot(
    `<div style="padding-top:${TIGHT.top}px"><div id=t style="font:${TEXT.size}px ${FONT.family};line-height:${TIGHT.lineHeight};color:${BLUE};filter:drop-shadow(${TEXT.offset}px 0px 0px red)">I</div></div>`,
  )
  const boxTop = await browser.page.evaluate(() => document.getElementById('t').getBoundingClientRect().top)
  let inkTop = null
  for (let row = 0; row < PAGE && inkTop === null; row++) {
    const [r, g, b] = pixel(shotTight, x, row)
    if (b === 255 && r === 0 && g === 0) inkTop = row
  }
  const above = inkTop === null ? null : Math.floor((inkTop + boxTop) / 2)
  if (above === null || above < inkTop + 2 || above > boxTop - 2) {
    throw new Error(`the stem's ink starts at ${inkTop} and the box at ${boxTop}: no row of ink above the box to read`)
  }
  rows.push([tight, x, above, ...pixel(shotTight, x, above), 'the stem above its line box'].join('\t'))
  rows.push([tight, x + TEXT.offset, above, ...pixel(shotTight, x + TEXT.offset, above), 'its shadow, above the box too'].join('\t'))

  const header = [
    '# Chrome, through `just conformance`. What a `filter` covers.',
    '#',
    "# The element's whole rendering, filtered once as a group in its own",
    '# coordinate space, before its mask, opacity and blend.',
    '#',
    `# Each case is drawn at the top-left of a white ${PAGE}x${PAGE} page. The disc is`,
    `# filter-disc.png, ${DISC}x${DISC} opaque blue on transparency. \`text\` is one \`I\``,
    `# in the fixture face at ${TEXT.size}px, sampled at its stem's middle and ${TEXT.offset}px right.`,
    `# \`text past its line box\` sets it at \`line-height: ${TIGHT.lineHeight}\` under ${TIGHT.top}px of padding.`,
    '# The markup per case is in the tool; `why` says what each point separates.',
    '# Written by packages/meo-canvas/tools/conformance/filterwhole.mjs.',
    '#',
    '# case\tx\ty\tr\tg\tb\twhy',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`filter whole: ${rows.length} samples -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
