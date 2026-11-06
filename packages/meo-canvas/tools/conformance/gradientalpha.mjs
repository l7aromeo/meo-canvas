// How a gradient mixes stops that are not opaque: CSS interpolates them with the
// alpha multiplied in, so a stop at alpha 0 adds no colour of its own. Read from a
// screenshot over a white page, since the question is what Chrome painted.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/gradient-alpha.tsv')

/** The box: wide and short for a linear ramp, square for the radial and conic ones. */
const LINE = { width: 100, height: 20 }
const SQUARE = { width: 100, height: 100 }

/** Every case, its box, and the page that draws it. */
const CASES = [
  ['opaque', LINE, background('linear-gradient(to right, rgb(255, 0, 0), rgb(0, 0, 255))')],
  ['zero same', LINE, background('linear-gradient(to right, rgb(255, 0, 0), rgba(255, 0, 0, 0))')],
  ['transparent', LINE, background('linear-gradient(to right, rgb(255, 0, 0), transparent)')],
  ['near zero', LINE, background('linear-gradient(to right, rgb(255, 0, 0), rgba(255, 0, 0, 0.01))')],
  ['zero other', LINE, background('linear-gradient(to right, rgb(255, 0, 0), rgba(0, 0, 255, 0))')],
  ['zero first', LINE, background('linear-gradient(to right, rgba(255, 0, 0, 0), rgb(0, 0, 255))')],
  ['through transparent', LINE, background('linear-gradient(to right, rgb(255, 0, 0), transparent, rgb(0, 0, 255))')],
  ['partial', LINE, background('linear-gradient(to right, rgb(255, 0, 0), rgba(0, 0, 255, 0.2))')],
  ['radial transparent', SQUARE, background('radial-gradient(rgb(255, 0, 0), transparent)')],
  ['conic transparent', SQUARE, background('conic-gradient(rgb(255, 0, 0), transparent)')],
  ['conic partial', SQUARE, background('conic-gradient(rgb(255, 0, 0), rgba(0, 0, 255, 0.2))')],
  // The root's background, which Chrome draws on the canvas from the root's box.
  ['root transparent', LINE, root('linear-gradient(to right, rgb(255, 0, 0), transparent)')],
  ['root partial', LINE, root('linear-gradient(to right, rgb(255, 0, 0), rgba(0, 0, 255, 0.2))')],
  // A red box under a gradient mask, which reads alpha alone: a control, since
  // either mixing gives the same alpha.
  ['mask partial', LINE, mask('linear-gradient(to right, rgb(0, 0, 0), rgba(0, 0, 255, 0.2))')],
  // An SVG gradient, which Chrome mixes straight, as a rect's fill, a line's
  // stroke and a radial fill: the stops as `<stop>` elements.
  ['svg transparent', LINE, svg('fill', ['#ff0000', 1], ['#000000', 0])],
  ['svg zero same', LINE, svg('fill', ['#ff0000', 1], ['#ff0000', 0])],
  ['svg zero first', LINE, svg('fill', ['#ff0000', 0], ['#0000ff', 1])],
  ['svg partial', LINE, svg('fill', ['#ff0000', 1], ['#0000ff', 0.2])],
  ['svg stroke zero same', LINE, svg('stroke', ['#ff0000', 1], ['#ff0000', 0])],
  ['svg stroke partial', LINE, svg('stroke', ['#ff0000', 1], ['#0000ff', 0.2])],
  ['svg radial zero same', SQUARE, svg('radial', ['#ff0000', 1], ['#ff0000', 0])],
  ['svg radial partial', SQUARE, svg('radial', ['#ff0000', 1], ['#0000ff', 0.2])],
]

/** The page around a body, white, with no margin unless `html` says otherwise. */
function page(body, html = '') {
  return `<!doctype html><style>html,body{margin:0;background:#fff}${html}</style>${body}`
}

/** A box at the origin with `css` as its background image. */
function background(css) {
  return box => page(`<div style="position:absolute;left:0;top:0;width:${box.width}px;height:${box.height}px;background-image:${css}"></div>`)
}

/** `css` as the root element's background, over white, the root the box's size. */
function root(css) {
  return box => page('', `html{width:${box.width}px;height:${box.height}px;background:${css} #fff}`)
}

/** A red box masked by `css`. */
function mask(css) {
  return box => page(`<div style="position:absolute;left:0;top:0;width:${box.width}px;height:${box.height}px;background:#f00;mask-image:${css}"></div>`)
}

/**
 * An inline SVG painted with a gradient of evenly spaced stops: a rect's fill
 * left to right, a line's stroke across the box, or a radial fill from the
 * centre out to the corners, CSS's `farthest-corner` on a square.
 */
function svg(kind, ...stops) {
  const at = stops.map(([color, opacity], index) => `<stop offset="${index / (stops.length - 1)}" stop-color="${color}" stop-opacity="${opacity}"/>`).join('')
  return box => {
    const { width: w, height: h } = box
    const defs =
      kind === 'radial'
        ? `<radialGradient id="g" gradientUnits="userSpaceOnUse" cx="${w / 2}" cy="${h / 2}" r="${Math.hypot(w, h) / 2}">${at}</radialGradient>`
        : `<linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" x2="${w}" y1="0" y2="0">${at}</linearGradient>`
    const shape =
      kind === 'stroke'
        ? `<line x1="0" y1="${h / 2}" x2="${w}" y2="${h / 2}" stroke="url(#g)" stroke-width="${h}"/>`
        : `<rect width="${w}" height="${h}" fill="url(#g)"/>`
    return page(`<svg width="${w}" height="${h}" style="display:block"><defs>${defs}</defs>${shape}</svg>`)
  }
}

/**
 * Where each box is read: across the middle row for a linear ramp, out from the
 * centre for a radial one, and around a circle of radius 30 for a conic one.
 */
function points(name, box) {
  if (box === LINE) return [10, 25, 50, 75, 90].map(x => [`x${x}`, x, box.height / 2])
  if (name.includes('radial')) return [10, 25, 35, 45].map(r => [`r${r}`, box.width / 2 + r, box.height / 2])
  return [45, 90, 180, 270].map(degrees => {
    const turn = (degrees * Math.PI) / 180
    return [`a${degrees}`, Math.round(box.width / 2 + 30 * Math.sin(turn)), Math.round(box.height / 2 - 30 * Math.cos(turn))]
  })
}

const browser = await open()
try {
  const rows = []
  for (const [name, box, draw] of CASES) {
    await browser.page.setViewportSize(box)
    await browser.page.setContent(draw(box))
    await settle(browser.page)
    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...box } }))
    if (shot.width !== box.width || shot.height !== box.height) {
      throw new Error(`${name}: the screenshot is ${shot.width}x${shot.height}, not ${box.width}x${box.height}`)
    }
    for (const [point, x, y] of points(name, box)) {
      const [r, g, b] = pixel(shot, x, y)
      rows.push([JSON.stringify(name), box.width, box.height, point, x, y, r, g, b, draw(box).replace(/\s+/g, ' ')].join('\t'))
    }
  }

  const written = table([
    '# Chrome, through `just conformance`. How a gradient mixes stops that are not opaque.',
    '# Written by packages/meo-canvas/tools/conformance/gradientalpha.mjs.',
    '#',
    '# Each box is a `background-image` over a white page, read from a screenshot.',
    '# CSS interpolates a gradient with the alpha multiplied in (CSS Images 4,',
    '# "Color Stop Lists"), so a stop at alpha 0 contributes no colour: red to',
    '# `transparent` and red to `rgba(255, 0, 0, 0)` are the same fade, and red to',
    '# `transparent` to blue passes through no grey.',
    '#',
    "# The `root` rows are the root element's background, the `mask` row a red box",
    '# under a gradient mask, which reads alpha alone. The `svg` rows are an SVG',
    '# gradient as a fill or a stroke, which Chrome mixes straight: there a stop at',
    '# opacity 0 keeps its own colour, and fading red to transparent black darkens it.',
    '#',
    '# Chrome dithers gradients (see `gradient-truth.tsv`), so a consumer carries',
    '# at least one unit of tolerance per channel.',
    '#',
    '# case\tw\th\tpoint\tx\ty\tr\tg\tb\tpage',
    ...rows,
  ])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`gradient alpha: ${CASES.length} cases, ${rows.length} samples -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
